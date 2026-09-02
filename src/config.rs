use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;
use std::collections::{HashMap, HashSet};

#[derive(Parser)]
#[grammar = "parser.pest"]
struct UnstructParser;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct XmlName {
    pub namespace: Option<String>,
    pub local_name: String,
}

impl XmlName {
    fn parse(value: &str, namespaces: &HashMap<String, String>) -> Result<Self, String> {
        let (namespace, local_name) = match value.split_once(':') {
            Some((prefix, local_name))
                if !prefix.is_empty() && !local_name.is_empty() && !local_name.contains(':') =>
            {
                let namespace = namespaces
                    .get(prefix)
                    .ok_or_else(|| format!("Undeclared namespace prefix {prefix:?}"))?;
                (Some(namespace.clone()), local_name)
            }
            Some(_) => return Err(format!("Invalid qualified XML name: {value:?}")),
            None if value.is_empty() => return Err("XML names cannot be empty".to_owned()),
            None => (None, value),
        };

        Ok(Self {
            namespace,
            local_name: local_name.to_owned(),
        })
    }

    fn display(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{{{namespace}}}{}", self.local_name),
            None => self.local_name.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Selector {
    pub path: Vec<XmlName>,
    pub attribute: Option<XmlName>,
}

impl Selector {
    fn parse(value: &str, namespaces: &HashMap<String, String>) -> Result<Self, String> {
        let (element, attribute) = match value.split_once("/@") {
            Some((element, attribute)) if !attribute.is_empty() && !attribute.contains("/@") => {
                (element, Some(XmlName::parse(attribute, namespaces)?))
            }
            Some(_) => return Err(format!("Invalid selector: {value:?}")),
            None => (value, None),
        };
        let path = element
            .split('/')
            .map(|name| XmlName::parse(name, namespaces))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { path, attribute })
    }

    fn display(&self) -> String {
        let element = self
            .path
            .iter()
            .map(XmlName::display)
            .collect::<Vec<_>>()
            .join("/");
        match &self.attribute {
            Some(attribute) => format!("{element}/@{}", attribute.display()),
            None => element,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub selector: Selector,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub selector: Selector,
    pub expected: String,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct Recording {
    pub selector: Selector,
    pub depth: usize,
    pub columns: Vec<Column>,
    pub filters: Vec<Filter>,
    pub children: Vec<Recording>,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    pub header: Vec<String>,
    pub recordings: Vec<Recording>,
}

#[derive(Debug)]
struct RecordingBuilder {
    selector: Selector,
    depth: usize,
    parent: Option<usize>,
    columns: Vec<Column>,
    filters: Vec<Filter>,
    children: Vec<usize>,
}

fn namespace_parts(pair: Pair<Rule>) -> Result<(String, String), String> {
    let mut prefix = None;
    let mut uri = None;
    for part in pair.into_inner() {
        match part.as_rule() {
            Rule::namespace_prefix => prefix = Some(part.as_str().to_owned()),
            Rule::namespace_uri => uri = Some(part.as_str().to_owned()),
            _ => return Err(format!("Malformed namespace component: {part:?}")),
        }
    }

    match (prefix, uri) {
        (Some(prefix), Some(uri)) => Ok((prefix, uri)),
        _ => Err("Namespace declaration must contain a prefix and URI".to_owned()),
    }
}

fn directive_parts(
    pair: Pair<Rule>,
    namespaces: &HashMap<String, String>,
) -> Result<(String, Selector), String> {
    let mut column = None;
    let mut selector = None;
    for part in pair.into_inner() {
        match part.as_rule() {
            Rule::column_name => column = Some(part.as_str().to_owned()),
            Rule::xml_name => selector = Some(Selector::parse(part.as_str(), namespaces)?),
            _ => return Err(format!("Malformed directive component: {part:?}")),
        }
    }

    match (column, selector) {
        (Some(column), Some(selector)) => Ok((column, selector)),
        _ => Err("Directive must contain a column and selector".to_owned()),
    }
}

fn filter_parts(
    pair: Pair<Rule>,
    namespaces: &HashMap<String, String>,
) -> Result<(Selector, String), String> {
    let mut selector = None;
    let mut expected = None;
    for part in pair.into_inner() {
        match part.as_rule() {
            Rule::xml_name => selector = Some(Selector::parse(part.as_str(), namespaces)?),
            Rule::value => expected = Some(part.as_str().to_owned()),
            _ => return Err(format!("Malformed filter component: {part:?}")),
        }
    }

    match (selector, expected) {
        (Some(selector), Some(expected)) => Ok((selector, expected)),
        _ => Err("Filter must contain a selector and expected value".to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_group(
    pairs: Pairs<Rule>,
    depth: usize,
    parent_scope: Option<usize>,
    builders: &mut Vec<RecordingBuilder>,
    roots: &mut Vec<usize>,
    header: &mut Vec<String>,
    column_names: &mut HashSet<String>,
    namespaces: &mut HashMap<String, String>,
) -> Result<(), String> {
    let mut local_scope = parent_scope;

    for pair in pairs {
        match pair.as_rule() {
            Rule::xml_namespace => {
                let (prefix, uri) = namespace_parts(pair)?;
                if namespaces.insert(prefix.clone(), uri).is_some() {
                    return Err(format!("Duplicate namespace prefix: {prefix}"));
                }
            }
            Rule::element => {
                let value = pair.as_str();
                let value = value
                    .strip_prefix('<')
                    .and_then(|value| value.strip_suffix('>'))
                    .ok_or_else(|| format!("Malformed recording element: {value:?}"))?;
                let selector = Selector::parse(value, namespaces)?;
                if selector.attribute.is_some() || selector.path.len() != 1 {
                    return Err("Recording elements must name one XML element".to_owned());
                }
                if builders.iter().any(|recording| {
                    recording.parent == parent_scope
                        && recording.depth == depth
                        && recording.selector == selector
                }) {
                    return Err(format!(
                        "Duplicate recording selector at depth {depth}: {}",
                        selector.display()
                    ));
                }

                let index = builders.len();
                builders.push(RecordingBuilder {
                    selector,
                    depth,
                    parent: parent_scope,
                    columns: Vec::new(),
                    filters: Vec::new(),
                    children: Vec::new(),
                });
                if let Some(parent) = parent_scope {
                    builders[parent].children.push(index);
                } else {
                    roots.push(index);
                }
                local_scope = Some(index);
            }
            Rule::directive => {
                let scope = local_scope.ok_or_else(|| {
                    format!("Directive at XML depth {depth} is outside a recording scope")
                })?;
                let (name, selector) = directive_parts(pair, namespaces)?;
                if !column_names.insert(name.clone()) {
                    return Err(format!("Duplicate output column: {name}"));
                }
                if builders[scope]
                    .columns
                    .iter()
                    .any(|column| column.depth == depth && column.selector == selector)
                {
                    return Err(format!(
                        "Duplicate selector in recording <{}> at depth {depth}: {}",
                        builders[scope].selector.display(),
                        selector.display()
                    ));
                }
                builders[scope].columns.push(Column {
                    name: name.clone(),
                    selector,
                    depth,
                });
                header.push(name);
            }
            Rule::filter => {
                let scope = local_scope.ok_or_else(|| {
                    format!("Filter at XML depth {depth} is outside a recording scope")
                })?;
                let (selector, expected) = filter_parts(pair, namespaces)?;
                if builders[scope]
                    .filters
                    .iter()
                    .any(|filter| filter.depth == depth && filter.selector == selector)
                {
                    return Err(format!(
                        "Duplicate filter in recording <{}> at depth {depth}: {}",
                        builders[scope].selector.display(),
                        selector.display()
                    ));
                }
                builders[scope].filters.push(Filter {
                    selector,
                    expected,
                    depth,
                });
            }
            Rule::block => compile_group(
                pair.into_inner(),
                depth + 1,
                local_scope,
                builders,
                roots,
                header,
                column_names,
                namespaces,
            )?,
            Rule::EOI => {}
            _ => return Err(format!("Unsupported configuration item: {pair:?}")),
        }
    }

    Ok(())
}

fn build_recording(index: usize, builders: &[RecordingBuilder]) -> Recording {
    let builder = &builders[index];
    Recording {
        selector: builder.selector.clone(),
        depth: builder.depth,
        columns: builder.columns.clone(),
        filters: builder.filters.clone(),
        children: builder
            .children
            .iter()
            .map(|child| build_recording(*child, builders))
            .collect(),
    }
}

pub fn parse(configuration: &str) -> Result<Configuration, String> {
    let mut parsed = UnstructParser::parse(Rule::config, configuration)
        .map_err(|error| format!("Could not parse the config file:\n{error}"))?;
    let config = parsed
        .next()
        .ok_or_else(|| "Configuration is empty".to_owned())?;
    let mut builders = Vec::new();
    let mut roots = Vec::new();
    let mut header = Vec::new();
    let mut column_names = HashSet::new();
    let mut namespaces = HashMap::new();
    compile_group(
        config.into_inner(),
        1,
        None,
        &mut builders,
        &mut roots,
        &mut header,
        &mut column_names,
        &mut namespaces,
    )?;

    if roots.is_empty() {
        return Err("Configuration must declare at least one recording element".to_owned());
    }
    if header.is_empty() {
        return Err("Configuration must declare at least one output column".to_owned());
    }

    Ok(Configuration {
        header,
        recordings: roots
            .into_iter()
            .map(|root| build_recording(root, &builders))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_complete_configuration() {
        let configuration = r#"
            <record>
            {
                identifier = "id"
                ["active" = "true"]
            }
        "#;

        assert!(parse(configuration).is_ok());
    }

    #[test]
    fn rejects_trailing_invalid_content_with_location() {
        let error = parse("identifier = \"id\"\nthis is not valid").unwrap_err();

        assert!(error.contains("2:"), "missing line location: {error}");
        assert!(
            error.contains("this is not valid"),
            "missing source line: {error}"
        );
    }

    #[test]
    fn rejects_empty_configuration() {
        assert!(parse(" \n\t").is_err());
    }

    #[test]
    fn rejects_duplicate_output_columns() {
        let error = parse(
            r#"
                <record> {
                    value = "first"
                    value = "second"
                }
            "#,
        )
        .unwrap_err();

        assert!(error.contains("Duplicate output column: value"));
    }

    #[test]
    fn rejects_duplicate_selectors_in_one_scope() {
        let error = parse(
            r#"
                <record> {
                    first = "value"
                    second = "value"
                }
            "#,
        )
        .unwrap_err();

        assert!(error.contains("Duplicate selector in recording <record>"));
    }

    #[test]
    fn allows_identical_selectors_in_different_scopes() {
        let configuration = parse(
            r#"
                <root> {
                    <left> { first = "value" }
                    <right> { second = "value" }
                }
            "#,
        );

        assert!(configuration.is_ok());
    }

    #[test]
    fn rejects_duplicate_filters_in_one_scope() {
        let error = parse(
            r#"
                <record>
                ["record/@active" = "true"]
                ["record/@active" = "false"]
                { value = "value" }
            "#,
        )
        .unwrap_err();

        assert!(error.contains("Duplicate filter in recording <record>"));
    }

    #[test]
    fn rejects_duplicate_recording_selectors() {
        let error = parse(
            r#"
                <root> {
                    <item> { first = "value" }
                    <item> { second = "other" }
                }
            "#,
        )
        .unwrap_err();

        assert!(error.contains("Duplicate recording selector"));
    }

    #[test]
    fn rejects_directives_outside_recording_scopes() {
        let error = parse("value = \"value\"").unwrap_err();

        assert!(error.contains("outside a recording scope"));
    }

    #[test]
    fn rejects_configurations_without_directives() {
        let error = parse("<record>").unwrap_err();

        assert!(error.contains("at least one output column"));
    }

    #[test]
    fn resolves_declared_namespace_aliases() {
        let configuration = parse(
            r#"
                namespace cfg = "urn:unstruct:test"
                <cfg:record> { value = "cfg:value" }
            "#,
        );

        assert!(configuration.is_ok());
    }

    #[test]
    fn rejects_undeclared_namespace_prefixes() {
        let error = parse("<cfg:record> { value = \"cfg:value\" }").unwrap_err();

        assert!(error.contains("Undeclared namespace prefix \"cfg\""));
    }

    #[test]
    fn rejects_duplicate_namespace_prefixes() {
        let error = parse(
            r#"
                namespace cfg = "urn:first"
                namespace cfg = "urn:second"
                <record> { value = "value" }
            "#,
        )
        .unwrap_err();

        assert!(error.contains("Duplicate namespace prefix: cfg"));
    }
}
