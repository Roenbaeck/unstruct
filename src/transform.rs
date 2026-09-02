use crate::config::{Configuration, Recording, Selector};
use roxmltree::{Document, Node};
use std::collections::HashMap;

pub type Row = HashMap<String, String>;

fn depth(node: Node<'_, '_>) -> usize {
    node.ancestors().filter(Node::is_element).count()
}

fn qualified_name(node: Node<'_, '_>, namespaces: &HashMap<String, String>) -> String {
    let name = node.tag_name().name();
    match node
        .tag_name()
        .namespace()
        .and_then(|namespace| namespaces.get(namespace))
    {
        Some(prefix) => format!("{prefix}:{name}"),
        None => name.to_owned(),
    }
}

fn selector_value(
    selector: &Selector,
    node: Node<'_, '_>,
    namespaces: &HashMap<String, String>,
) -> Option<String> {
    if !node.is_element()
        || !selector
            .path
            .iter()
            .rev()
            .zip(node.ancestors().filter(Node::is_element))
            .all(|(expected, candidate)| qualified_name(candidate, namespaces) == *expected)
        || selector.path.len() > node.ancestors().filter(Node::is_element).count()
    {
        return None;
    }

    match &selector.attribute {
        Some(attribute) => node.attribute(attribute.as_str()).map(str::to_owned),
        None => Some(node.text().unwrap_or("").to_owned()),
    }
}

fn scope_matches(
    recording: &Recording,
    node: Node<'_, '_>,
    namespaces: &HashMap<String, String>,
) -> bool {
    recording.filters.iter().all(|filter| {
        node.descendants().any(|candidate| {
            depth(candidate) == filter.depth
                && selector_value(&filter.selector, candidate, namespaces).as_deref()
                    == Some(filter.expected.as_str())
        })
    })
}

fn extract_scope(
    recording: &Recording,
    node: Node<'_, '_>,
    namespaces: &HashMap<String, String>,
) -> Vec<Row> {
    if !scope_matches(recording, node, namespaces) {
        return Vec::new();
    }

    let mut base = Row::new();
    for column in &recording.columns {
        let value = node
            .descendants()
            .find_map(|candidate| {
                (depth(candidate) == column.depth)
                    .then(|| selector_value(&column.selector, candidate, namespaces))
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
                    && selector_value(&child.selector, *candidate, namespaces).is_some()
            })
            .flat_map(|candidate| extract_scope(child, candidate, namespaces))
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
    let root = document.root_element();
    let namespaces: HashMap<String, String> = root
        .namespaces()
        .filter_map(|namespace| {
            namespace
                .name()
                .map(|name| (namespace.uri().to_owned(), name.to_owned()))
        })
        .collect();

    Ok(configuration
        .recordings
        .iter()
        .flat_map(|recording| {
            document
                .descendants()
                .filter(|node| {
                    depth(*node) == recording.depth
                        && selector_value(&recording.selector, *node, &namespaces).is_some()
                })
                .flat_map(|node| extract_scope(recording, node, &namespaces))
                .collect::<Vec<_>>()
        })
        .collect())
}
