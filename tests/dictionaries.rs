use unstruct::{config::parse, transform::extract_rows};

const CONFIG: &str = r#"
    namespace m = "urn:measurements"
    { <m:info>
      dictionary counters { entry = "m:type" key = "@p" value = "." }
      { <m:object> object = "m:object/@id"
        { <m:r> counter = counters["m:r/@p"] value = "m:r" }
      }
    }
"#;

#[test]
fn documented_3gpp_parser_resolves_counters_and_inherits_period_and_object() {
    let config = parse(include_str!("../example_3gpp/measurements.parser")).unwrap();
    let rows = extract_rows(&config, include_str!("../example_3gpp/measurements.xml")).unwrap();
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(row["node"].as_deref(), Some("example-node"));
        assert_eq!(row["period_start"].as_deref(), Some("2026-01-01T10:00:00Z"));
        assert_eq!(row["period_end"].as_deref(), Some("2026-01-01T10:15:00Z"));
        assert_eq!(row["duration"].as_deref(), Some("PT900S"));
        assert_eq!(row["meas_info"].as_deref(), Some("example-group"));
    }
    assert_eq!(rows[0]["counter"].as_deref(), Some("packetsReceived"));
    assert_eq!(rows[0]["value"].as_deref(), Some("001"));
    assert_eq!(rows[1]["counter"].as_deref(), Some("packetDistribution"));
    assert_eq!(rows[1]["value"].as_deref(), Some("0,1,2"));
    assert_eq!(rows[2]["counter"].as_deref(), Some("packetsLost"));
    assert_eq!(rows[2]["object"].as_deref(), Some("cell-B"));
    assert_eq!(rows[2]["job"].as_deref(), Some("second-job"));
}

#[test]
fn resolves_forward_entries_and_isolates_identical_ids_and_files() {
    let config = parse(CONFIG).unwrap();
    let xml = r#"<root xmlns="urn:measurements">
      <info id="same"><object id="A"><r p="14">001</r></object><type p="14">first</type></info>
      <info id="same"><type p="14">second</type><object id="B"><r p="14">0,1,2</r></object></info>
    </root>"#;
    let rows = extract_rows(&config, xml).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0]["object"].as_deref(),
            rows[0]["counter"].as_deref(),
            rows[0]["value"].as_deref()
        ),
        (Some("A"), Some("first"), Some("001"))
    );
    assert_eq!(
        (
            rows[1]["object"].as_deref(),
            rows[1]["counter"].as_deref(),
            rows[1]["value"].as_deref()
        ),
        (Some("B"), Some("second"), Some("0,1,2"))
    );
    let another_file = xml.replace("first", "third");
    assert_eq!(
        extract_rows(&config, &another_file).unwrap()[0]["counter"].as_deref(),
        Some("third")
    );
    let missing =
        r#"<root xmlns="urn:measurements"><info><object><r p="14">1</r></object></info></root>"#;
    assert!(extract_rows(&config, missing)
        .unwrap_err()
        .contains("Missing key \"14\""));
}

#[test]
fn duplicate_or_missing_keys_fail_with_scope_context() {
    let config = parse(CONFIG).unwrap();
    for (contents, expected) in [
        (
            r#"<type p="1">a</type><type p="1">a</type>"#,
            "Duplicate key \"1\"",
        ),
        ("<type>missing</type>", "missing attribute \"p\""),
        (r#"<object><r p="99">1</r></object>"#, "Missing key \"99\""),
        ("<object><r>1</r></object>", "Missing lookup key"),
    ] {
        let xml = format!("<root xmlns=\"urn:measurements\"><info>{contents}</info></root>");
        let error = extract_rows(&config, &xml).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("in <info> at"), "{error}");
        assert!(error.contains("counters"), "{error}");
    }
}

#[test]
fn resolves_relative_paths_namespaced_attributes_and_direct_text() {
    let config = parse(
        r#"
      namespace m = "urn:measurements"
      <root>
      dictionary names { entry = "types/type" key = "id/@m:p" value = "label" }
      { <r> raw = "r/@p" resolved = names["r/@p"] }
    "#,
    )
    .unwrap();
    let xml = r#"<root xmlns:other="urn:measurements"><r p="01"/><types><type>
        <id other:p="01"/><label> left<![CDATA[+text]]><ignored>skip</ignored> right </label>
        </type></types></root>"#;
    let rows = extract_rows(&config, xml).unwrap();
    assert_eq!(rows[0]["raw"].as_deref(), Some("01"));
    assert_eq!(rows[0]["resolved"].as_deref(), Some(" left+text right "));
    let ambiguous = xml.replace("<label>", "<label>extra</label><label>");
    assert!(extract_rows(&config, &ambiguous)
        .unwrap_err()
        .contains("found 2"));
    let missing = xml.replace("<id other:p=\"01\"/>", "");
    assert!(extract_rows(&config, &missing)
        .unwrap_err()
        .contains("found 0"));
}

#[test]
fn declaration_order_and_nested_shadowing_are_lexical() {
    let config = parse(
        r#"
      <root>
      outer = names["root/@p"]
      dictionary names { entry = "type" key = "@p" value = "." }
      { <child>
        inner = names["child/@p"]
        dictionary names { entry = "type" key = "@p" value = "." }
      }
    "#,
    )
    .unwrap();
    let xml = r#"<root p="1"><child p="1"><type p="1">inner</type></child><type p="1">outer</type></root>"#;
    let rows = extract_rows(&config, xml).unwrap();
    assert_eq!(rows[0]["outer"].as_deref(), Some("outer"));
    assert_eq!(rows[0]["inner"].as_deref(), Some("inner"));
    let no_fallback = xml.replace("<type p=\"1\">inner</type>", "");
    assert!(extract_rows(&config, &no_fallback).is_err());
}

#[test]
fn sibling_scopes_do_not_share_dictionary_entries() {
    let config = parse(CONFIG).unwrap();
    let xml = r#"<root xmlns="urn:measurements">
      <info><type p="1">first</type><object><r p="1">1</r></object></info>
      <info><object><r p="1">2</r></object></info>
    </root>"#;
    assert!(extract_rows(&config, xml)
        .unwrap_err()
        .contains("Missing key \"1\""));
}

#[test]
fn invalid_dictionary_configurations_fail_before_reading_xml() {
    for (source, expected) in [
        (r#"<root> x = missing["root/@p"]"#, "Unknown dictionary"),
        (
            r#"dictionary d { entry = "type" key = "@p" value = "." } <root> x = "root""#,
            "outside a recording scope",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" } x = "root""#,
            "Missing value",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" key = "." value = "." } x = "root""#,
            "Duplicate key field",
        ),
        (
            r#"<root> dictionary d { entry = "." key = "@p" value = "." } x = "root""#,
            "child element path",
        ),
        (
            r#"<root> dictionary d { entry = "type/@p" key = "@p" value = "." } x = "root""#,
            "child element path",
        ),
        (
            r#"<root> dictionary d { entry = "../type" key = "@p" value = "." } x = "root""#,
            "Invalid relative selector",
        ),
        (
            r#"<root> dictionary d { entry = "m:type" key = "@p" value = "." } x = "root""#,
            "Undeclared namespace",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" value = "." } dictionary d { entry = "other" key = "@p" value = "." } x = "root""#,
            "Duplicate dictionary",
        ),
        (
            r#"<root> { <left> dictionary d { entry = "type" key = "@p" value = "." } <right> x = d["right/@p"] }"#,
            "Unknown dictionary",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.contains(expected), "{source}: {error}");
    }
}
