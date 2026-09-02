use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static OUTPUT_ID: AtomicUsize = AtomicUsize::new(0);

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn run_unstruct(filename: &Path, parser: &Path) -> (Output, PathBuf) {
    let output_path = std::env::temp_dir().join(format!(
        "unstruct-regression-{}-{}.tsv",
        std::process::id(),
        OUTPUT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let output = run_unstruct_to(filename, parser, &output_path);

    (output, output_path)
}

fn run_unstruct_to(filename: &Path, parser: &Path, output_path: &Path) -> Output {
    run_command(filename, parser, output_path, false)
}

fn run_command(filename: &Path, parser: &Path, output_path: &Path, metadata: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_unstruct"));
    command
        .arg("--filename")
        .arg(filename)
        .arg("--outfile")
        .arg(output_path)
        .arg("--parser")
        .arg(parser)
        .arg("--quiet");
    if metadata {
        command.arg("--metadata");
    }
    command.output().expect("failed to run unstruct")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "unstruct failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "--quiet emitted informational output:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn filters_attributes_namespaces_nested_records_and_missing_values() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("features.xml"),
        &fixtures.join("features.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let expected = fs::read(fixtures.join("features.txt")).expect("failed to read golden output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, expected);
}

#[test]
fn identical_selectors_under_different_parents_do_not_collide() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("context_aware.xml"),
        &fixtures.join("context_aware.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let expected =
        fs::read(fixtures.join("context_aware.txt")).expect("failed to read golden output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, expected);
}

#[test]
fn qualified_paths_distinguish_same_depth_sibling_branches() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("qualified_paths.xml"),
        &fixtures.join("qualified_paths.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let expected =
        fs::read(fixtures.join("qualified_paths.txt")).expect("failed to read golden output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, expected);
}

#[test]
fn outer_context_is_broadcast_regardless_of_sibling_order() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("sibling_context.xml"),
        &fixtures.join("sibling_context.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let expected =
        fs::read(fixtures.join("sibling_context.txt")).expect("failed to read golden output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, expected);
}

#[test]
fn structural_characters_are_quoted_and_unicode_is_preserved() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("unsafe_values.xml"),
        &fixtures.join("unsafe_values.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(
        actual,
        "value\n\"left\tmiddle\nright\r\"\"räksmörgås\"\"\"\n".as_bytes()
    );
}

#[test]
fn malformed_configuration_fails() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("features.xml"),
        &fixtures.join("malformed.parser"),
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Could not parse the config file"));
    assert!(stderr.contains("malformed.parser"));
    assert!(!output_path.exists());
}

#[test]
fn malformed_xml_fails() {
    let fixtures = fixtures();
    let malformed = fixtures.join("malformed.xml");
    let (output, output_path) = run_unstruct(&malformed, &fixtures.join("features.parser"));

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Could not parse the xml"));
    assert!(stderr.contains(&malformed.display().to_string()));
    assert!(!output_path.exists());
}

#[test]
fn output_cannot_be_an_input_file() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let input_path = directory.path().join("input.xml");
    let input = fs::read(fixtures.join("features.xml")).expect("failed to read fixture");
    fs::write(&input_path, &input).expect("failed to write input");

    let output = run_unstruct_to(&input_path, &fixtures.join("features.parser"), &input_path);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("also an input XML file"));
    assert_eq!(fs::read(input_path).expect("failed to reread input"), input);
}

#[test]
fn failed_transform_does_not_replace_existing_output() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let output_path = directory.path().join("result.tsv");
    fs::write(&output_path, b"existing output\n").expect("failed to write existing output");

    let output = run_unstruct_to(
        &fixtures.join("malformed.xml"),
        &fixtures.join("features.parser"),
        &output_path,
    );

    assert!(!output.status.success());
    assert_eq!(
        fs::read(output_path).expect("failed to read existing output"),
        b"existing output\n"
    );
}

#[test]
fn successful_transform_atomically_replaces_existing_output() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let output_path = directory.path().join("result.tsv");
    fs::write(&output_path, b"existing output\n").expect("failed to write existing output");

    let output = run_unstruct_to(
        &fixtures.join("unsafe_values.xml"),
        &fixtures.join("unsafe_values.parser"),
        &output_path,
    );
    assert_success(&output);

    assert_eq!(
        fs::read(output_path).expect("failed to read replaced output"),
        "value\n\"left\tmiddle\nright\r\"\"räksmörgås\"\"\"\n".as_bytes()
    );
}

#[test]
fn failed_batch_does_not_publish_rows_from_earlier_files() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    fs::write(
        directory.path().join("a-valid.xml"),
        "<root><Record><value>valid</value></Record></root>",
    )
    .expect("failed to write valid XML");
    fs::write(directory.path().join("b-invalid.xml"), "<root>")
        .expect("failed to write malformed XML");
    let output_path = directory.path().join("result.tsv");
    fs::write(&output_path, b"existing output\n").expect("failed to write existing output");

    let output = run_unstruct_to(
        &directory.path().join("*.xml"),
        &fixtures.join("unsafe_values.parser"),
        &output_path,
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("b-invalid.xml"));
    assert_eq!(
        fs::read(output_path).expect("failed to read existing output"),
        b"existing output\n"
    );
}

#[test]
fn invalid_glob_fails_without_creating_output() {
    let fixtures = fixtures();
    let (output, output_path) =
        run_unstruct(Path::new("["), &fixtures.join("unsafe_values.parser"));

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to read glob pattern"));
    assert!(!output_path.exists());
}

#[test]
fn unreadable_input_fails_without_creating_output() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let (output, output_path) =
        run_unstruct(directory.path(), &fixtures.join("unsafe_values.parser"));

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not read XML file"));
    assert!(!output_path.exists());
}

#[test]
fn unwritable_output_location_fails_without_creating_output() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let output_path = directory.path().join("missing").join("result.tsv");

    let output = run_unstruct_to(
        &fixtures.join("unsafe_values.xml"),
        &fixtures.join("unsafe_values.parser"),
        &output_path,
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not resolve output directory"));
    assert!(!output_path.exists());
}

#[test]
fn metadata_paths_are_sorted_relative_to_the_common_parent() {
    let fixtures = fixtures();
    let directory = tempfile::tempdir().expect("failed to create test directory");
    let foo = directory.path().join("foo");
    let foobar = directory.path().join("foobar");
    fs::create_dir(&foo).expect("failed to create foo directory");
    fs::create_dir(&foobar).expect("failed to create foobar directory");
    fs::write(
        foo.join("input.xml"),
        "<root><Record><value>one</value></Record></root>",
    )
    .expect("failed to write foo XML");
    fs::write(
        foobar.join("input.xml"),
        "<root><Record><value>two</value></Record></root>",
    )
    .expect("failed to write foobar XML");
    let output_path = directory.path().join("result.tsv");

    let output = run_command(
        &directory.path().join("foo*").join("*.xml"),
        &fixtures.join("unsafe_values.parser"),
        &output_path,
        true,
    );
    assert_success(&output);

    assert_eq!(
        fs::read(output_path).expect("failed to read generated output"),
        b"value\t_path\none\tfoo/input.xml\ntwo\tfoobar/input.xml\n"
    );
}

#[test]
fn metadata_rejects_reserved_path_column_before_creating_output() {
    let fixtures = fixtures();
    let output_path = std::env::temp_dir().join(format!(
        "unstruct-reserved-metadata-{}.tsv",
        std::process::id()
    ));
    let output = run_command(
        &fixtures.join("features.xml"),
        &fixtures.join("reserved_metadata.parser"),
        &output_path,
        true,
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("_path is reserved"));
    assert!(!output_path.exists());
}

#[test]
fn empty_glob_fails_without_creating_output() {
    let fixtures = fixtures();
    let pattern = fixtures.join("does-not-exist-*.xml");
    let (output, output_path) = run_unstruct(&pattern, &fixtures.join("features.parser"));

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No files matched input pattern"));
    assert!(stderr.contains(&pattern.display().to_string()));
    assert!(!output_path.exists());
}
