use crate::config::{Configuration, Recording, RelativeSelector, Selector, XmlName};
use roxmltree::{Document, Node};
use std::collections::HashMap;

pub type Row = HashMap<String, String>;

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

fn scope_matches(recording: &Recording, node: Node<'_, '_>) -> bool {
    recording.filters.iter().all(|filter| {
        node.descendants().any(|candidate| {
            depth(candidate) == filter.depth
                && selector_value(&filter.selector, candidate).as_deref()
                    == Some(filter.expected.as_str())
        })
    })
}

type Dictionaries = HashMap<String, HashMap<String, String>>;

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
        dictionaries.insert(dictionary.name.clone(), entries);
    }
    Ok(dictionaries)
}

fn extract_scope(
    recording: &Recording,
    node: Node<'_, '_>,
    dictionaries: &mut Vec<Dictionaries>,
) -> Result<Vec<Row>, String> {
    if !scope_matches(recording, node) {
        return Ok(Vec::new());
    }
    let context = |error| {
        format!(
            "{error} in <{}> at {}",
            node.tag_name().name(),
            node.document().text_pos_at(node.range().start)
        )
    };
    dictionaries.push(build_dictionaries(recording, node).map_err(context)?);
    let result = extract_scope_rows(recording, node, dictionaries).map_err(context);
    dictionaries.pop();
    result
}

fn extract_scope_rows(
    recording: &Recording,
    node: Node<'_, '_>,
    dictionaries: &mut Vec<Dictionaries>,
) -> Result<Vec<Row>, String> {
    let mut base = Row::new();
    for column in &recording.columns {
        let value = node.descendants().find_map(|candidate| {
            (depth(candidate) == column.depth)
                .then(|| selector_value(&column.selector, candidate))
                .flatten()
        });
        let value = match &column.dictionary {
            Some(name) => {
                let key = value.ok_or_else(|| {
                    format!(
                        "Missing lookup key for dictionary {name:?}, column {:?}",
                        column.name
                    )
                })?;
                dictionaries
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name))
                    .and_then(|entries| entries.get(&key))
                    .cloned()
                    .ok_or_else(|| {
                        format!(
                            "Missing key {key:?} in dictionary {name:?}, column {:?}",
                            column.name
                        )
                    })?
            }
            None => value.unwrap_or_default(),
        };
        base.insert(column.name.clone(), value);
    }

    let mut rows = vec![base];
    for child in &recording.children {
        let mut child_rows = Vec::new();
        for candidate in node.descendants().filter(|candidate| {
            depth(*candidate) == child.depth && element_matches(&child.selector, *candidate)
        }) {
            child_rows.extend(extract_scope(child, candidate, dictionaries)?);
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

pub fn extract_rows(configuration: &Configuration, xml: &str) -> Result<Vec<Row>, String> {
    let document =
        Document::parse(xml).map_err(|error| format!("Could not parse the xml: {error}"))?;

    let mut rows = Vec::new();
    let mut dictionaries = Vec::new();
    for recording in &configuration.recordings {
        for node in document.descendants().filter(|node| {
            depth(*node) == recording.depth && element_matches(&recording.selector, *node)
        }) {
            rows.extend(extract_scope(recording, node, &mut dictionaries)?);
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
                    row["source"].as_str(),
                    row["group"].as_str(),
                    row["item"].as_str(),
                    row["value"].as_str(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("shared", "A", "1", "first"),
                ("shared", "A", "", ""),
                ("shared", "B", "2", "second"),
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
                    row["group"].as_str(),
                    row["left"].as_str(),
                    row["right"].as_str(),
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
        let actual: Vec<_> = rows.iter().map(|row| row["value"].as_str()).collect();
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

        assert_eq!(rows[0]["value"], "left+cdataright");
    }

    #[test]
    fn preserves_direct_formatting_whitespace() {
        let configuration = parse("{ <record> { value = \"value\" } }").unwrap();
        let rows = extract_rows(
            &configuration,
            "<root><record><value>  value\n</value></record></root>",
        )
        .unwrap();

        assert_eq!(rows[0]["value"], "  value\n");
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

        assert_eq!(rows[0]["value"], "matched");
    }
}
