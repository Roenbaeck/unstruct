use clap::Parser;
use glob::glob;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tempfile::NamedTempFile;
use unstruct::config::parse;
use unstruct::output::TsvWriter;

#[derive(Parser, Debug)]
#[clap(author, version)]
struct Args {
    /// The name of the input xml file or matching files if wildcards are used
    #[clap(short, long)]
    filename: String,

    /// The name of the text file into which the results of the parsing will be output
    #[clap(short, long)]
    outfile: String,

    /// The configuration file specifying the parsing rules
    #[clap(short, long, default_value = "unstruct.parser")]
    parser: String,

    /// Add metadata columns and values in the output file
    #[clap(short, long)]
    metadata: bool,

    /// Do not write any info to output
    #[clap(short, long)]
    quiet: bool,
}

fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut common = paths.first()?.parent()?.to_path_buf();
    while paths.iter().any(|path| !path.starts_with(&common)) {
        if !common.pop() {
            return None;
        }
    }
    Some(common)
}

fn metadata_path(path: &Path, common: Option<&Path>) -> String {
    common
        .and_then(|common| path.strip_prefix(common).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn comparable_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path)
            .map_err(|error| format!("Could not resolve path {}: {error}", path.display()));
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "Could not resolve output directory {}: {error}",
            parent.display()
        )
    })?;
    let name = path
        .file_name()
        .ok_or_else(|| format!("Output path must name a file: {}", path.display()))?;
    Ok(parent.join(name))
}

fn run() -> Result<(), String> {
    let Args {
        filename,
        outfile,
        parser,
        metadata,
        quiet,
    } = Args::parse();

    let config_source = fs::read_to_string(&parser)
        .map_err(|error| format!("Could not read parser config {parser:?}: {error}"))?;
    let configuration = parse(&config_source)
        .map_err(|error| format!("Could not parse parser config {parser:?}: {error}"))?;

    if !quiet {
        println!("Finding files matching: {filename}");
        println!("Results are stored in: {outfile}");
    }

    let mut matching_files = glob(&filename)
        .map_err(|error| format!("Failed to read glob pattern: {error}"))?
        .map(|entry| entry.map_err(|error| format!("Failed to read matched path: {error}")))
        .collect::<Result<Vec<_>, _>>()?;
    matching_files.sort();
    if matching_files.is_empty() {
        return Err(format!("No files matched input pattern: {filename}"));
    }
    let common = common_parent(&matching_files);

    let output_path = PathBuf::from(&outfile);
    let resolved_output = comparable_path(&output_path)?;
    for input in &matching_files {
        if comparable_path(input)? == resolved_output {
            return Err(format!(
                "Output path {} is also an input XML file",
                output_path.display()
            ));
        }
    }
    if comparable_path(Path::new(&parser))? == resolved_output {
        return Err(format!(
            "Output path {} is also the parser config file",
            output_path.display()
        ));
    }

    let output_parent = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output = NamedTempFile::new_in(output_parent).map_err(|error| {
        format!(
            "Could not create temporary output in {}: {error}",
            output_parent.display()
        )
    })?;
    let mut output = TsvWriter::new(output, &configuration, metadata).map_err(|error| {
        format!(
            "Could not initialize output file {}: {error}",
            output_path.display()
        )
    })?;

    for path in matching_files {
        let relative_path = metadata_path(&path, common.as_deref());
        if !quiet {
            println!("Parsing the file: {relative_path}");
        }
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read XML file {}: {error}", path.display()))?;
        output
            .write_xml(
                &configuration,
                &contents,
                metadata.then_some(relative_path.as_str()),
            )
            .map_err(|error| {
                format!(
                    "Could not process XML file {} or write output {}: {error}",
                    path.display(),
                    output_path.display()
                )
            })?;
    }

    let output = output.finish().map_err(|error| {
        format!(
            "Could not finalize output file {}: {error}",
            output_path.display()
        )
    })?;
    output.persist(&output_path).map_err(|error| {
        format!(
            "Could not publish output file {}: {}",
            output_path.display(),
            error.error
        )
    })?;
    if !quiet {
        println!("All done!");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
