use unstruct::config::parse;
use unstruct::transform::{extract_rows, extract_rows_with_diagnostics, LookupDiagnostics};

const CONFIG: &str = r#"
    namespace m = "urn:measurements"
    { <m:info> group = "m:info/@id"
      dictionary counters { entry = "m:type" key = "@p" value = "." }
      { <m:r> [counters["m:r/@p"] = "wanted"]
        counter = counters["m:r/@p"] value = "m:r"
      }
    }
"#;

#[test]
fn documented_downtime_parser_selects_only_the_three_matching_cells() {
    let config = parse(include_str!("../example_3gpp/downtime.parser")).unwrap();
    let rows = extract_rows(&config, include_str!("../example_3gpp/sample-node.xml")).unwrap();
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["counter"].as_deref(), Some("pmCellDowntimeAuto"));
        assert_eq!(row["downtime"].as_deref(), Some("0"));
        assert_eq!(
            row["meas_info"].as_deref(),
            Some("PM=1001,PmGroup=EUtranCellFDD")
        );
        assert_eq!(
            row["object"].as_deref(),
            Some(format!("ManagedElement=example-node-001,ENodeBFunction=1001,EUtranCellFDD=example-eutrancellfdd-{:03}", index + 1).as_str())
        );
    }
    assert!(
        extract_rows(&config, include_str!("../example_3gpp/measurements.xml"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lookup_filters_resolve_scoped_names_instead_of_counter_numbers() {
    let config = parse(CONFIG).unwrap();
    let xml = r#"<root xmlns="urn:measurements">
        <info id="A"><r p="14">001</r><r p="99">discard</r><type p="14">wanted</type><type p="99">other</type></info>
        <info id="B"><type p="14">other</type><type p="99">wanted</type><r p="14">discard</r><r p="99">0,1,2</r></info>
    </root>"#;
    let rows = extract_rows(&config, xml).unwrap();
    let actual: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row["group"].as_deref(),
                row["counter"].as_deref(),
                row["value"].as_deref(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (Some("A"), Some("wanted"), Some("001")),
            (Some("B"), Some("wanted"), Some("0,1,2"))
        ]
    );
    assert!(extract_rows(&config, &xml.replace("wanted", "different"))
        .unwrap()
        .is_empty());
}

#[test]
fn local_dictionary_is_built_before_its_filter_even_when_declared_later() {
    let config = parse(
        r#"
        namespace n = "urn:keys"
        { <record> [names["record/@n:p"] = "wanted"] value = "record"
          dictionary names { entry = "type" key = "@n:p" value = "." }
        }
    "#,
    )
    .unwrap();
    let rows = extract_rows(
        &config,
        r#"<root xmlns:k="urn:keys">
        <record k:p="1">kept<type k:p="1">wanted</type></record>
        <record k:p="1">discarded<type k:p="1">other</type></record>
    </root>"#,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["value"].as_deref(), Some("kept"));
}

#[test]
fn filters_keep_existential_matching_and_combine_as_and() {
    let config = parse(
        r#"
        <root> dictionary d { entry = "type" key = "@p" value = "." }
        { ["tag" = "yes"] [d["r/@p"] = "wanted"] value = "value" }
    "#,
    )
    .unwrap();
    let xml = r#"<root><tag>no</tag><tag>yes</tag><value>keep</value><r p="1"/><r p="2"/>
        <type p="1">other</type><type p="2">wanted</type></root>"#;
    assert_eq!(
        extract_rows(&config, xml).unwrap()[0]["value"].as_deref(),
        Some("keep")
    );
    assert!(extract_rows(&config, &xml.replace("<tag>yes</tag>", ""))
        .unwrap()
        .is_empty());
}

#[test]
fn strict_lookup_filters_fail_on_unresolved_keys() {
    let config = parse(CONFIG).unwrap();
    for (result, expected) in [
        (r#"<r p="999"/>"#, "Missing key \"999\""),
        ("<r/>", "Missing lookup key"),
    ] {
        let xml = format!("<root xmlns=\"urn:measurements\"><info>{result}</info></root>");
        assert!(extract_rows(&config, &xml).unwrap_err().contains(expected));
    }
}

#[test]
fn tolerant_lookup_null_does_not_match_empty_string_but_resolved_empty_does() {
    let source = CONFIG
        .replace("value = \".\"", "value = \".\" lenient")
        .replace("= \"wanted\"]", "= \"\"]");
    let config = parse(&source).unwrap();
    let mut diagnostics = LookupDiagnostics::default();
    let rows = extract_rows_with_diagnostics(
        &config,
        r#"<root xmlns="urn:measurements">
        <info><r p="999">unknown</r><r>absent</r><r p="1">empty name</r><type p="1"/></info>
    </root>"#,
        &mut diagnostics,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["counter"].as_deref(), Some(""));
    assert_eq!(rows[0]["value"].as_deref(), Some("empty name"));
    assert_eq!(diagnostics.missing_lookups, 2);
}

#[test]
fn lookup_filters_use_nearest_dictionary_and_do_not_fall_back() {
    let config = parse(
        r#"
        <root> dictionary d { entry = "type" key = "@p" value = "." }
        { <record> [d["record/@p"] = "wanted"] value = "record/@p"
          dictionary d { entry = "type" key = "@p" value = "." lenient }
        }
    "#,
    )
    .unwrap();
    let xml = r#"<root><type p="1">wanted</type>
        <record p="1"/><record p="1"><type p="1">other</type></record>
        <record p="2"><type p="2">wanted</type></record>
    </root>"#;
    let mut diagnostics = LookupDiagnostics::default();
    let rows = extract_rows_with_diagnostics(&config, xml, &mut diagnostics).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["value"].as_deref(), Some("2"));
    assert_eq!(diagnostics.missing_lookups, 1);
}

#[test]
fn invalid_lookup_filters_and_flags_fail_during_configuration() {
    for (source, message) in [
        (
            r#"<root> [unknown["root/@p"] = "x"] value = "root""#,
            "Unknown dictionary",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" value = "." lenient lenient } value = "root""#,
            "Duplicate lenient flag",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" value = "." missing = "rabbit" } value = "root""#,
            "Could not parse",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" value = "." lenient = "true" } value = "root""#,
            "Could not parse",
        ),
        (
            r#"<root> dictionary d { entry = "type" key = "@p" value = "." } [d["root/@p"] = "a"] [d["root/@p"] = "b"] value = "root""#,
            "Duplicate filter",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn diagnostics_are_bounded_and_count_filtered_missing_lookups() {
    let config = parse(&CONFIG.replace("value = \".\"", "value = \".\" lenient")).unwrap();
    let xml = format!(
        "<root xmlns=\"urn:measurements\"><info>{}</info></root>",
        "<r p=\"999\"/>".repeat(100)
    );
    let mut batch = LookupDiagnostics::default();
    for file in ["a.xml", "b.xml"] {
        let mut diagnostics = LookupDiagnostics::default();
        assert!(
            extract_rows_with_diagnostics(&config, &xml, &mut diagnostics)
                .unwrap()
                .is_empty()
        );
        assert_eq!(diagnostics.missing_lookups, 100);
        assert_eq!(diagnostics.examples.len(), 5);
        batch.merge(diagnostics, file);
    }
    assert_eq!(batch.missing_lookups, 200);
    assert_eq!(batch.examples.len(), 5);
    assert!(batch.examples[0].contains("a.xml") && batch.examples[0].contains("999"));
}

#[test]
fn lenient_never_hides_invalid_dictionary_entries() {
    let config = parse(&CONFIG.replace("value = \".\"", "value = \".\" lenient")).unwrap();
    for entries in [
        r#"<type p="1">one</type><type p="1">two</type>"#,
        "<type>missing key</type>",
    ] {
        let xml =
            format!("<root xmlns=\"urn:measurements\"><info>{entries}<r p=\"1\"/></info></root>");
        assert!(extract_rows(&config, &xml).is_err());
    }
}
