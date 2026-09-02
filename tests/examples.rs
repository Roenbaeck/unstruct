use std::fs;
use std::path::PathBuf;
use std::process::Command;

const EXAMPLES: [&str; 4] = ["bag", "cdr", "ftp", "sts"];

fn assert_example_matches_golden(example: &str, metadata: bool) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("example_{example}"));
    let output_path = std::env::temp_dir().join(format!(
        "unstruct-{}-{example}-{}.tsv",
        std::process::id(),
        if metadata { "metadata" } else { "default" }
    ));

    let mut command = Command::new(env!("CARGO_BIN_EXE_unstruct"));
    command
        .current_dir(&root)
        .arg("--filename")
        .arg(directory.join("*.xml"))
        .arg("--outfile")
        .arg(&output_path)
        .arg("--parser")
        .arg(directory.join(format!("{example}.parser")))
        .arg("--quiet");
    if metadata {
        command.arg("--metadata");
    }

    let output = command.output().expect("failed to run unstruct");
    assert!(
        output.status.success(),
        "{example} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let golden_name = if metadata {
        "result_metadata.txt"
    } else {
        "result.txt"
    };
    let expected = fs::read(directory.join(golden_name)).expect("failed to read golden output");
    fs::remove_file(&output_path).expect("failed to remove generated output");

    assert_eq!(actual, expected, "{example} {golden_name} differs");
}

#[test]
fn examples_match_default_golden_files() {
    for example in EXAMPLES {
        assert_example_matches_golden(example, false);
    }
}

#[test]
fn examples_match_metadata_golden_files() {
    for example in EXAMPLES {
        assert_example_matches_golden(example, true);
    }
}
