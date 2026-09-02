use crate::config::{Configuration, Recording, Selector, XmlName};
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

fn selector_value(selector: &Selector, node: Node<'_, '_>) -> Option<String> {
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

fn extract_scope(recording: &Recording, node: Node<'_, '_>) -> Vec<Row> {
    if !scope_matches(recording, node) {
        return Vec::new();
    }

    let mut base = Row::new();
    for column in &recording.columns {
        let value = node
            .descendants()
            .find_map(|candidate| {
                (depth(candidate) == column.depth)
                    .then(|| selector_value(&column.selector, candidate))
                    .flatten()
            })
            .unwrap_or_default();
        base.insert(column.name.clone(), value);
    }

    let mut rows = vec![base];
    for child in &recording.children {
        let child_rows: Vec<Row> = node
            .descendants()
            .filter(|candidate| {
                depth(*candidate) == child.depth
                    && selector_value(&child.selector, *candidate).is_some()
            })
            .flat_map(|candidate| extract_scope(child, candidate))
            .collect();
        if child_rows.is_empty() {
            return Vec::new();
        }

        rows = rows
            .into_iter()
            .flat_map(|row| {
                child_rows.iter().map(move |child_row| {
                    let mut combined = row.clone();
                    combined.extend(child_row.clone());
                    combined
                })
            })
            .collect();
    }

    rows
}

pub fn extract_rows(configuration: &Configuration, xml: &str) -> Result<Vec<Row>, String> {
    let document =
        Document::parse(xml).map_err(|error| format!("Could not parse the xml: {error}"))?;

    Ok(configuration
        .recordings
        .iter()
        .flat_map(|recording| {
            document
                .descendants()
                .filter(|node| {
                    depth(*node) == recording.depth
                        && selector_value(&recording.selector, *node).is_some()
                })
                .flat_map(|node| extract_scope(recording, node))
                .collect::<Vec<_>>()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::extract_rows;
    use crate::config::parse;

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
