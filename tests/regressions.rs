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
    let output = Command::new(env!("CARGO_BIN_EXE_unstruct"))
        .arg("--filename")
        .arg(filename)
        .arg("--outfile")
        .arg(&output_path)
        .arg("--parser")
        .arg(parser)
        .arg("--quiet")
        .output()
        .expect("failed to run unstruct");

    (output, output_path)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "unstruct failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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
fn tabs_and_newlines_in_xml_values_are_written_verbatim() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("unsafe_values.xml"),
        &fixtures.join("unsafe_values.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    let expected =
        fs::read(fixtures.join("unsafe_values.txt")).expect("failed to read golden output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, expected);
}

#[test]
fn malformed_configuration_fails() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("features.xml"),
        &fixtures.join("malformed.parser"),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not parse the config file"));
    assert!(!output_path.exists());
}

#[test]
fn malformed_xml_fails() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("malformed.xml"),
        &fixtures.join("features.parser"),
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Could not parse the xml"));
    fs::remove_file(output_path).expect("failed to remove partial output");
}

#[test]
fn empty_glob_succeeds_with_header_only_output() {
    let fixtures = fixtures();
    let (output, output_path) = run_unstruct(
        &fixtures.join("does-not-exist-*.xml"),
        &fixtures.join("features.parser"),
    );
    assert_success(&output);

    let actual = fs::read(&output_path).expect("failed to read generated output");
    fs::remove_file(output_path).expect("failed to remove generated output");
    assert_eq!(actual, b"id\tkind\tvalue\tunit\tmissing\n");
}
