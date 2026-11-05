use crate::config::{Configuration, Expression, Recording, RelativeSelector, Selector, XmlName};
use roxmltree::{Document, Node};
use std::collections::HashMap;

/// None means no selected value; Some("") is present but empty.
pub type Row = HashMap<String, Option<String>>;

/// Counts unresolved lookup evaluations in lenient dictionaries, including filters.
/// Retains at most five bounded examples; short-circuited expressions are not counted.
#[derive(Debug, Default)]
pub struct LookupDiagnostics {
    pub missing_lookups: u64,
    pub examples: Vec<String>,
}

fn abbreviated(value: &str) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

impl LookupDiagnostics {
    fn record(&mut self, dictionary: &str, key: Option<&str>, node: Node<'_, '_>) {
        self.missing_lookups += 1;
        if self.examples.len() < 5 {
            let key = key.map(abbreviated);
            self.examples.push(format!(
                "dictionary {:?}, key {key:?}, <{}> at {}",
                abbreviated(dictionary),
                abbreviated(node.tag_name().name()),
                node.document().text_pos_at(node.range().start)
            ));
        }
    }

    pub fn merge(&mut self, other: Self, source: &str) {
        self.missing_lookups += other.missing_lookups;
        for example in other
            .examples
            .into_iter()
            .take(5usize.saturating_sub(self.examples.len()))
        {
            self.examples
                .push(format!("{:?}: {example}", abbreviated(source)));
        }
    }
}

fn depth(node: Node<'_, '_>) -> usize {
    node.ancestors().filter(Node::is_element).count()
}

fn name_matches(expected: &XmlName, local_name: &str, namespace: Option<&str>) -> bool {
    expected.local_name == local_name && expected.namespace.as_deref() == namespace
}

fn direct_text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(Node::is_text)
        .filter_map(|child| child.text())
        .collect()
}

// Match only the element path. Recording discovery does not need its text or attributes.
fn element_matches(selector: &Selector, node: Node<'_, '_>) -> bool {
    if !node.is_element()
        || !selector
            .path
            .iter()
            .rev()
            .zip(node.ancestors().filter(Node::is_element))
            .all(|(expected, candidate)| {
                name_matches(
                    expected,
                    candidate.tag_name().name(),
                    candidate.tag_name().namespace(),
                )
            })
        || selector.path.len() > node.ancestors().filter(Node::is_element).count()
    {
        return false;
    }
    true
}

fn selector_value(selector: &Selector, node: Node<'_, '_>) -> Option<String> {
    if !element_matches(selector, node) {
        return None;
    }
    match &selector.attribute {
        Some(attribute) => node
            .attributes()
            .find(|candidate| name_matches(attribute, candidate.name(), candidate.namespace()))
            .map(|attribute| attribute.value().to_owned()),
        None => Some(direct_text(node)),
    }
}

struct DictionaryValues {
    values: HashMap<String, String>,
    lenient: bool,
}

type Dictionaries = HashMap<String, DictionaryValues>;

struct Context<'a> {
    dictionaries: Vec<Dictionaries>,
    diagnostics: &'a mut LookupDiagnostics,
}

fn evaluate(
    expression: &Expression,
    value: Option<String>,
    node: Node<'_, '_>,
    context: &mut Context<'_>,
) -> Result<Option<String>, String> {
    let Some(name) = &expression.dictionary else {
        return Ok(value);
    };
    let dictionary = context
        .dictionaries
        .iter()
        .rev()
        .find_map(|scope| scope.get(name))
        .ok_or_else(|| format!("Unknown dictionary {name:?}"))?;
    if let Some(resolved) = value.as_ref().and_then(|key| dictionary.values.get(key)) {
        return Ok(Some(resolved.clone()));
    }
    if dictionary.lenient {
        context.diagnostics.record(name, value.as_deref(), node);
        return Ok(None);
    }
    Err(match value {
        Some(key) => format!("Missing key {key:?} in dictionary {name:?}"),
        None => format!("Missing lookup key for dictionary {name:?}"),
    })
}

fn scope_matches(
    recording: &Recording,
    node: Node<'_, '_>,
    context: &mut Context<'_>,
) -> Result<bool, String> {
    for filter in &recording.filters {
        let mut matched = false;
        for candidate in node.descendants().filter(|candidate| {
            depth(*candidate) == filter.depth
                && element_matches(&filter.expression.selector, *candidate)
        }) {
            let value = evaluate(
                &filter.expression,
                selector_value(&filter.expression.selector, candidate),
                candidate,
                context,
            )?;
            if value.as_deref() == Some(filter.expected.as_str()) {
                matched = true;
                break;
            }
        }
        if !matched {
            return Ok(false);
        }
    }
    Ok(true)
}

fn relative_nodes<'a, 'input>(
    selector: &RelativeSelector,
    node: Node<'a, 'input>,
) -> Vec<Node<'a, 'input>> {
    let mut nodes = vec![node];
    for name in &selector.path {
        nodes = nodes
            .into_iter()
            .flat_map(|parent| parent.children())
            .filter(|child| {
                child.is_element()
                    && name_matches(name, child.tag_name().name(), child.tag_name().namespace())
            })
            .collect();
    }
    nodes
}

fn relative_value(selector: &RelativeSelector, node: Node<'_, '_>) -> Result<String, String> {
    let nodes = relative_nodes(selector, node);
    if nodes.len() != 1 {
        return Err(format!(
            "expected exactly one matching element, found {}",
            nodes.len()
        ));
    }
    let node = nodes[0];
    match &selector.attribute {
        Some(name) => node
            .attributes()
            .find(|attr| name_matches(name, attr.name(), attr.namespace()))
            .map(|attr| attr.value().to_owned())
            .ok_or_else(|| format!("missing attribute {:?}", name.local_name)),
        None => Ok(direct_text(node)),
    }
}

fn build_dictionaries(recording: &Recording, node: Node<'_, '_>) -> Result<Dictionaries, String> {
    let mut dictionaries = Dictionaries::new();
    for dictionary in &recording.dictionaries {
        let mut entries = HashMap::new();
        for entry in relative_nodes(&dictionary.entry, node) {
            let key = relative_value(&dictionary.key, entry)
                .map_err(|error| format!("Dictionary {:?} key: {error}", dictionary.name))?;
            let value = relative_value(&dictionary.value, entry).map_err(|error| {
                format!(
                    "Dictionary {:?} value for key {key:?}: {error}",
                    dictionary.name
                )
            })?;
            if entries.insert(key.clone(), value).is_some() {
                return Err(format!(
                    "Duplicate key {key:?} in dictionary {:?}",
                    dictionary.name
                ));
            }
        }
        dictionaries.insert(
            dictionary.name.clone(),
            DictionaryValues {
                values: entries,
                lenient: dictionary.lenient,
            },
        );
    }
    Ok(dictionaries)
}

fn extract_scope(
    recording: &Recording,
    node: Node<'_, '_>,
    context: &mut Context<'_>,
) -> Result<Vec<Row>, String> {
    let with_location = |error| {
        format!(
            "{error} in <{}> at {}",
            node.tag_name().name(),
            node.document().text_pos_at(node.range().start)
        )
    };
    // Build local dictionaries before filters so forward references and local shadowing work.
    context
        .dictionaries
        .push(build_dictionaries(recording, node).map_err(with_location)?);
    let result = extract_scope_rows(recording, node, context).map_err(with_location);
    context.dictionaries.pop();
    result
}

fn extract_scope_rows(
    recording: &Recording,
    node: Node<'_, '_>,
    context: &mut Context<'_>,
) -> Result<Vec<Row>, String> {
    if !scope_matches(recording, node, context)? {
        return Ok(Vec::new());
    }
    let mut base = Row::new();
    for column in &recording.columns {
        let selected = node.descendants().find_map(|candidate| {
            (depth(candidate) == column.depth)
                .then(|| selector_value(&column.expression.selector, candidate))
                .flatten()
                .map(|value| (value, candidate))
        });
        let (value, location) = match selected {
            Some((value, candidate)) => (Some(value), candidate),
            None => (None, node),
        };
        let value = evaluate(&column.expression, value, location, context)
            .map_err(|error| format!("{error}, column {:?}", column.name))?;
        base.insert(column.name.clone(), value);
    }

    let mut rows = vec![base];
    for child in &recording.children {
        let mut child_rows = Vec::new();
        for candidate in node.descendants().filter(|candidate| {
            depth(*candidate) == child.depth && element_matches(&child.selector, *candidate)
        }) {
            child_rows.extend(extract_scope(child, candidate, context)?);
        }
        if child_rows.is_empty() {
            return Ok(Vec::new());
        }

        rows = combine_rows(rows, child_rows);
    }

    Ok(rows)
}

fn combine_rows(parents: Vec<Row>, mut children: Vec<Row>) -> Vec<Row> {
    if let [parent] = parents.as_slice() {
        // Broadcast inherited fields without copying the completed child rows.
        for child in &mut children {
            for (name, value) in parent {
                child.entry(name.clone()).or_insert_with(|| value.clone());
            }
        }
        return children;
    }

    // Repeated sibling recordings require every parent/child combination,
    // in parent-major order. Child fields retain precedence in both paths.
    parents
        .into_iter()
        .flat_map(|parent| {
            children.iter().map(move |child| {
                let mut combined = parent.clone();
                combined.extend(child.clone());
                combined
            })
        })
        .collect()
}

/// Extract values without retaining tolerant-lookup diagnostics.
/// Use extract_rows_with_diagnostics when the caller needs to report missing keys.
pub fn extract_rows(configuration: &Configuration, xml: &str) -> Result<Vec<Row>, String> {
    extract_rows_with_diagnostics(configuration, xml, &mut LookupDiagnostics::default())
}

pub fn extract_rows_with_diagnostics(
    configuration: &Configuration,
    xml: &str,
    diagnostics: &mut LookupDiagnostics,
) -> Result<Vec<Row>, String> {
    let document =
        Document::parse(xml).map_err(|error| format!("Could not parse the xml: {error}"))?;

    let mut rows = Vec::new();
    let mut context = Context {
        dictionaries: Vec::new(),
        diagnostics,
    };
    for recording in &configuration.recordings {
        for node in document.descendants().filter(|node| {
            depth(*node) == recording.depth && element_matches(&recording.selector, *node)
        }) {
            rows.extend(extract_scope(recording, node, &mut context)?);
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::extract_rows;
    use crate::config::parse;

    #[test]
    fn nested_recordings_inherit_fields_and_keep_empty_rows() {
        let config = parse(
            r#"
            <root> source = "root/@source"
            { <group> group = "group/@id"
              { <item> item = "item/@id" value = "item" }
            }
        "#,
        )
        .unwrap();
        let rows = extract_rows(
            &config,
            r#"
            <root source="shared">
              <group id="A"><item id="1">first</item><item/></group>
              <group id="B"><item id="2">second</item></group>
              <group id="no-items"/>
            </root>
        "#,
        )
        .unwrap();
        let actual: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row["source"].as_deref().unwrap(),
                    row["group"].as_deref().unwrap(),
                    row["item"].as_deref(),
                    row["value"].as_deref().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("shared", "A", Some("1"), "first"),
                ("shared", "A", None, ""),
                ("shared", "B", Some("2"), "second"),
            ]
        );
    }

    #[test]
    fn repeated_sibling_recordings_preserve_combination_order_and_filters() {
        let config = parse(
            r#"
            { <group> group = "group/@id"
              { <left> left = "left"
                <right> right = "right" ["right/@keep" = "yes"]
              }
            }
        "#,
        )
        .unwrap();
        let rows = extract_rows(
            &config,
            r#"
            <root>
              <group id="multiple">
                <right keep="yes">1</right><left>A</left><left>B</left>
                <right keep="no">discarded</right><right keep="yes">2</right>
              </group>
              <group id="single">
                <left>C</left><right keep="yes">3</right><right keep="yes">4</right>
              </group>
              <group id="no-right"><left>D</left></group>
            </root>
        "#,
        )
        .unwrap();
        let actual: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row["group"].as_deref().unwrap(),
                    row["left"].as_deref().unwrap(),
                    row["right"].as_deref().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("multiple", "A", "1"),
                ("multiple", "A", "2"),
                ("multiple", "B", "1"),
                ("multiple", "B", "2"),
                ("single", "C", "3"),
                ("single", "C", "4"),
            ]
        );
    }

    #[test]
    fn recording_matching_respects_namespaces_and_preserves_direct_text() {
        let config = parse(
            r#"
            namespace m = "urn:records"
            <m:root> { <m:item> value = "m:item" }
        "#,
        )
        .unwrap();
        let xml = r#"<root xmlns="urn:records">
            <item/><item>left<![CDATA[+text]]><nested>ignored</nested>right</item>
            <item xmlns="urn:other">wrong namespace</item>
        </root>"#;
        let rows = extract_rows(&config, xml).unwrap();
        let actual: Vec<_> = rows
            .iter()
            .map(|row| row["value"].as_deref().unwrap())
            .collect();
        assert_eq!(actual, ["", "left+textright"]);
        assert!(
            extract_rows(&config, "<root><item>unqualified</item></root>")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn concatenates_direct_text_and_cdata_but_excludes_nested_text() {
        let configuration = parse("{ <record> { value = \"value\" } }").unwrap();
        let rows = extract_rows(
            &configuration,
            "<root><record><value>left<![CDATA[+cdata]]><nested>ignored</nested>right</value></record></root>",
        )
        .unwrap();

        assert_eq!(rows[0]["value"].as_deref(), Some("left+cdataright"));
    }

    #[test]
    fn preserves_direct_formatting_whitespace() {
        let configuration = parse("{ <record> { value = \"value\" } }").unwrap();
        let rows = extract_rows(
            &configuration,
            "<root><record><value>  value\n</value></record></root>",
        )
        .unwrap();

        assert_eq!(rows[0]["value"].as_deref(), Some("  value\n"));
    }

    #[test]
    fn matches_namespaced_attributes_by_uri() {
        let configuration = parse(
            r#"
                namespace cfg = "urn:unstruct:attribute"
                { <record> value = "record/@cfg:value" }
            "#,
        )
        .unwrap();
        let rows = extract_rows(
            &configuration,
            r#"<root><record xmlns:document="urn:unstruct:attribute" document:value="matched"/></root>"#,
        )
        .unwrap();

        assert_eq!(rows[0]["value"].as_deref(), Some("matched"));
    }
}
