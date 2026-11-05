use std::fs;
use std::process::Command;
use unstruct::config::parse;
use unstruct::transform::{extract_rows, extract_rows_with_diagnostics, LookupDiagnostics};

#[test]
fn absent_elements_and_attributes_are_null_while_present_empty_values_are_strings() {
    let config = parse(r#"{ <record> code = "record/@code" { value = "value" } }"#).unwrap();
    let xml = r#"<root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
        <record code=""><value></value></record>
        <record><value/></record>
        <record code="0"/>
        <record><value> </value></record>
        <record><value>001</value></record>
        <record><value>null</value></record>
        <record><value xsi:nil="true"/></record>
    </root>"#;
    let rows = extract_rows(&config, xml).unwrap();
    let actual: Vec<_> = rows
        .iter()
        .map(|row| (row["code"].as_deref(), row["value"].as_deref()))
        .collect();
    assert_eq!(
        actual,
        [
            (Some(""), Some("")),
            (None, Some("")),
            (Some("0"), None),
            (None, Some(" ")),
            (None, Some("001")),
            (None, Some("null")),
            (None, Some(""))
        ]
    );
    assert!(extract_rows(&config, "<root/>").unwrap().is_empty());
}

#[test]
fn missing_plain_selector_never_matches_an_empty_string_filter() {
    let config =
        parse(r#"{ <record> id = "record/@id" { ["value" = ""] value = "value" } }"#).unwrap();
    let rows = extract_rows(
        &config,
        r#"<root><record id="missing"/><record id="empty"><value/></record></root>"#,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"].as_deref(), Some("empty"));
    assert_eq!(rows[0]["value"].as_deref(), Some(""));
}

#[test]
fn tolerant_lookups_preserve_empty_names_and_empty_keys() {
    let config = parse(
        r#"
        <root> dictionary d { lenient entry = "type" key = "@p" value = "." }
        { <r> name = d["r/@p"] value = "r" }
    "#,
    )
    .unwrap();
    let mut diagnostics = LookupDiagnostics::default();
    let rows = extract_rows_with_diagnostics(
        &config,
        r#"<root>
        <r p="1"/><r p="999">orphan</r><r>no key</r><r p="">empty key</r>
        <type p="1"/><type p="">blank key</type>
    </root>"#,
        &mut diagnostics,
    )
    .unwrap();
    let actual: Vec<_> = rows.iter().map(|row| row["name"].as_deref()).collect();
    assert_eq!(actual, [Some(""), None, None, Some("blank key")]);
    assert_eq!(rows[1]["value"].as_deref(), Some("orphan"));
    assert_eq!(diagnostics.missing_lookups, 2);
}

#[test]
fn nulls_are_inherited_and_preserved_in_sibling_products() {
    let config = parse(
        r#"
        <root> inherited = "root/@missing"
        { <left> a = "left/@value" <right> b = "right/@value" }
    "#,
    )
    .unwrap();
    let rows = extract_rows(
        &config,
        r#"<root><left/><left value=""/><right value="0"/><right/></root>"#,
    )
    .unwrap();
    let actual: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["inherited"].as_deref(),
                row["a"].as_deref(),
                row["b"].as_deref(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (None, None, Some("0")),
            (None, None, None),
            (None, Some(""), Some("0")),
            (None, Some(""), None)
        ]
    );
}

#[test]
fn tsv_null_marker_keeps_empty_strings_and_default_output_compatible() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    fs::write(
        dir.join("input.xml"),
        r#"<root><r id="missing"/><r id="empty" value=""/><r id="literal" value="null"/></root>"#,
    )
    .unwrap();
    fs::write(
        dir.join("config.parser"),
        r#"{ <r> id = "r/@id" value = "r/@value" }"#,
    )
    .unwrap();
    for (extra, expected) in [
        (vec![], "id\tvalue\nmissing\t\nempty\t\nliteral\tnull\n"),
        (
            vec!["--null-marker", r"\N"],
            "id\tvalue\nmissing\t\\N\nempty\t\nliteral\tnull\n",
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_unstruct"))
            .arg("--filename")
            .arg(dir.join("input.xml"))
            .arg("--parser")
            .arg(dir.join("config.parser"))
            .arg("--outfile")
            .arg(dir.join("out.tsv"))
            .arg("--quiet")
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(dir.join("out.tsv")).unwrap(), expected);
    }
}

#[test]
fn quiet_cli_reports_bounded_tolerant_warnings_across_files_without_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    fs::write(
        dir.join("config.parser"),
        r#"
        <root> dictionary d { entry = "type" key = "@p" value = "." lenient }
        { <r> [d["r/@p"] = "wanted"] value = "r" }
    "#,
    )
    .unwrap();
    for name in ["a.xml", "b.xml"] {
        fs::write(
            dir.join(name),
            format!("<root>{}</root>", "<r p=\"999\"/>".repeat(20)),
        )
        .unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_unstruct"))
        .arg("--filename")
        .arg(dir.join("*.xml"))
        .arg("--parser")
        .arg(dir.join("config.parser"))
        .arg("--outfile")
        .arg(dir.join("out.tsv"))
        .arg("--quiet")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("40 unresolved dictionary lookup evaluations"),
        "{stderr}"
    );
    assert!(
        stderr.contains("a.xml") && stderr.contains("999"),
        "{stderr}"
    );
    assert_eq!(stderr.lines().count(), 6);
    assert_eq!(fs::read_to_string(dir.join("out.tsv")).unwrap(), "value\n");
}
