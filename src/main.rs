use clap::Parser;
use glob::glob;
use roxmltree::{self, Node};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::{read_to_string, File};
use std::io::Write;
use std::rc::Rc;
use unstruct::config::{parse, LEVEL};

/// Unstruct is a program that parses simple xml files into text files,
/// suitable for bulk inserts into a relational database
#[derive(Parser, Debug)]
#[clap(author, version)]
struct Args {
    /// The name of the input xml file or matching files if wildcards are used
    #[clap(short, long)]
    filename: String,

    /// The name of the text file into which the results of the parsing will be output
    #[clap(short, long)]
    outfile: String,

    /// The configuration file specifying the parsing rules
    #[clap(short, long, default_value = "unstruct.parser")]
    parser: String,

    /// Add metadata columns and values in the output file
    #[clap(short, long)]
    metadata: bool,

    /// Do not write any info to output
    #[clap(short, long)]
    quiet: bool,
}

#[derive(Debug)]
enum Match {
    Value(String),
    Nothing,
}

const DELIMITER: char = '\t';
const TERMINATOR: char = '\n';

fn traverse(
    nodes: Vec<Rc<Node>>,
    matcher: &HashMap<String, String>,
    filters: &HashMap<String, String>,
    header: &Vec<String>,
    elements: &HashMap<String, Vec<String>>,
    levels: &Vec<usize>,
    namespaces: &HashMap<String, String>,
    parsed: &mut HashMap<String, HashSet<String>>,
    result: &mut HashMap<String, Match>,
    output: &mut File,
    recording: Option<String>,
    siblings: bool,
    depth: usize,
) {
    // let current_element = recording.clone();
    // println!("Parsing depth: {}", depth);
    if depth > levels.len() {
        return;
    }
    let mut siblings = siblings;
    let mut found: usize = 0;
    let mut skip: bool;
    if siblings {
        for element in nodes.into_iter().filter(|el| el.is_element()) {
            skip = false;
            let mut nodes_to_search = Vec::default();
            if element.has_children() {
                siblings = false;
                nodes_to_search.extend(element.children().map(Rc::new));
            }  
            if recording.is_some() {
                let mut xml_name = element.tag_name().name().to_string();
                let schema_name = element.tag_name().namespace();
                if schema_name.is_some() {
                    let namespace = namespaces.get(schema_name.unwrap());
                    if namespace.is_some() {
                        xml_name = format!("{}:{}", namespace.unwrap(), xml_name);
                    }
                }
                let qualified_element_name = format!("{}{}{}", xml_name, LEVEL, depth);
                if let Some(partial_header) = elements.get(&qualified_element_name) {
                    result.extend(
                        partial_header
                            .iter()
                            .map(|head| (head.to_owned(), Match::Nothing)),
                    );
                }
                let mut xml_value = element.text().unwrap_or("").to_owned();
                let qualified_name = format!("{}{}{}", xml_name, LEVEL, depth);
                let value_filter = filters.get(&qualified_name);
                if value_filter.is_some() && xml_value.ne(value_filter.unwrap()) {
                    skip = true;
                }
                //println!("Filtering: {} = {:?} (siblings = {})", &qualified_name, filters.get(&qualified_name), siblings);
                found = found + record(&xml_name, &xml_value, recording.as_ref().unwrap(), matcher, parsed, result, depth);
                for attribute in element.attributes() {
                    let xml_attribute = format!("{}{}{}", xml_name, "/@", attribute.name());
                    xml_value = attribute.value().to_owned();
                    let qualified_name = format!("{}{}{}", xml_attribute, LEVEL, depth);
                    let value_filter = filters.get(&qualified_name);
                    if value_filter.is_some() && xml_value.ne(value_filter.unwrap()) {
                        skip = true;
                        break;
                    }
                    //println!("Filtering: {} = {:?} (siblings = {})", &qualified_name, filters.get(&qualified_name), siblings);
                    //println!("Attribute found: {} (on {} with {} found)", xml_value, recording.as_ref().unwrap(), found);
                    found = found + record(&xml_attribute, &xml_value, recording.as_ref().unwrap(), matcher, parsed, result, depth);
                }
                // println!("Number found: {}", found);
            } 
            if !nodes_to_search.is_empty() && !skip {
                traverse(
                    nodes_to_search,
                    matcher,
                    filters,
                    header,
                    elements,
                    levels,
                    namespaces,
                    parsed,
                    result,
                    output,
                    recording.to_owned(),
                    siblings,
                    if siblings { depth } else { depth + 1 },
                );
            }
        }
    } else {
        let mut nodes_to_search = Vec::default();
        skip = false;
        for element in nodes.into_iter().filter(|el| el.is_element()) {
            let mut xml_name = element.tag_name().name().to_string();
            let schema_name = element.tag_name().namespace();
            if schema_name.is_some() {
                let namespace = namespaces.get(schema_name.unwrap());
                if namespace.is_some() {
                    xml_name = format!("{}:{}", namespace.unwrap(), xml_name);
                }
            }
            let qualified_element_name = format!("{}{}{}", xml_name, LEVEL, depth);
            if elements.contains_key(&qualified_element_name) {
                // println!("Element to record: {}", qualified_element_name);
                let mut siblings_to_search = Vec::default();
                siblings_to_search.extend(element.next_siblings().filter(|el| el.has_tag_name(element.tag_name())).map(Rc::new));
                if !siblings_to_search.is_empty() {
                    traverse(
                        siblings_to_search,
                        matcher,
                        filters,
                        header,
                        elements,
                        levels,
                        namespaces,
                        parsed,
                        result,
                        output,
                        Some(qualified_element_name),
                        true,
                        depth,
                    );
                }
                break;
            } else if element.has_children() {
                nodes_to_search.extend(element.children().map(Rc::new));
            }  
            if recording.is_some() && found < levels[depth-1] {
                let mut xml_value = element.text().unwrap_or("").to_owned();
                let qualified_name = format!("{}{}{}", xml_name, LEVEL, depth);
                let value_filter = filters.get(&qualified_name);
                if value_filter.is_some() && xml_value.ne(value_filter.unwrap()) {
                    skip = true;
                    break;
                }
                // println!("Filtering: {} = {:?} (siblings = {})", &qualified_name, filters.get(&qualified_name), siblings);
                found = found + record(&xml_name, &xml_value, recording.as_ref().unwrap(), matcher, parsed, result, depth);
                for attribute in element.attributes() {
                    let xml_attribute = format!("{}{}{}", xml_name, "/@", attribute.name());
                    xml_value = attribute.value().to_owned();
                    let qualified_name = format!("{}{}{}", xml_attribute, LEVEL, depth);
                    let value_filter = filters.get(&qualified_name);
                    if value_filter.is_some() && xml_value.ne(value_filter.unwrap()) {
                        skip = true;
                        break;
                    }
                    //println!("Filtering: {} = {:?} (siblings = {})", &qualified_name, filters.get(&qualified_name), siblings);
                    //println!("Attribute found: {} (on {} with {} found)", xml_value, recording.as_ref().unwrap(), found);
                    found = found + record(&xml_attribute, &xml_value, recording.as_ref().unwrap(), matcher, parsed, result, depth);
                }
                // println!("Number found: {}", found);    
            } 
        }
        if !nodes_to_search.is_empty() && !skip {
            traverse(
                nodes_to_search,
                matcher,
                filters,
                header,
                elements,
                levels,
                namespaces,
                parsed,
                result,
                output,
                recording,
                siblings,
                depth + 1,
            );
        }
    }
    if elements.keys().all(|key| parsed.contains_key(key)) {
        if !parsed.keys().all(|key| parsed.get(key).unwrap().is_empty()) {
            // println!("Parsed <{}>: {:?}", current_element.unwrap(), parsed);
            // println!("Result: {:?}", result);
            for values in parsed.values_mut() {
                values.clear();
            }
            // ------------------------------------------------------------------------------------------
            let mut peekable_header = header.iter().peekable();
            while let Some(head) = peekable_header.next() {
                if let Some(Match::Value(column_value)) = result.get(head) {
                    write!(output, "{}", column_value).expect("Cannot write to output file");
                }
                if peekable_header.peek().is_none() {
                    write!(output, "{}", TERMINATOR).expect("Cannot write to output file");
                } else {
                    write!(output, "{}", DELIMITER).expect("Cannot write to output file");
                }
            }
            // ------------------------------------------------------------------------------------------
        }
    }
}

fn record(
    xml_name: &str,
    xml_value: &str,
    recording: &String,
    matcher: &HashMap<String, String>,
    parsed: &mut HashMap<String, HashSet<String>>,
    result: &mut HashMap<String, Match>,
    depth: usize,
) -> usize {
    let element = format!("{}{}{}", xml_name, LEVEL, depth);
    let mut found: usize = 0;
    // println!("Looking for: {} in <{}>", &element, recording);
    if let Some(column) = matcher.get(&element) {
        // println!("Found on level {}: {} = {}", depth, &element, xml_value);
        result.insert(column.to_owned(), Match::Value(xml_value.to_owned()));
        let values = parsed.entry(recording.to_owned()).or_insert(HashSet::new());        
        values.insert(column.to_owned());
        found = 1;
    }
    found
}

fn main() {
    let Args {
        filename,
        outfile,
        parser,
        metadata,
        quiet,
    } = Args::parse();

    // read the config containing the mapping between elements and columns
    let configuration = read_to_string(parser);
    match configuration {
        Ok(config) => {
            let (
                matcher, 
                filters,
                mut header, 
                elements, 
                levels
            ) = parse(&config);
            let mut result: HashMap<String, Match> = HashMap::default();
            if metadata {
                header.push("_path".to_owned());
            }

            // parse the arguments to get the filename glob pattern
            if !quiet {
                println!("Finding files matching: {}", &filename);
                println!("Results are stored in: {}", &outfile);
            }
            let mut output = File::create(outfile).unwrap();
            let mut peekable_header = header.iter().peekable();
            while let Some(head) = peekable_header.next() {
                write!(output, "{}", head).expect("Cannot write to output file");
                if peekable_header.peek().is_none() {
                    write!(output, "{}", TERMINATOR).expect("Cannot write to output file");
                } else {
                    write!(output, "{}", DELIMITER).expect("Cannot write to output file");
                }
            }

            // use the glob to find matching files
            for entry in glob(&filename).expect("Failed to read glob pattern") {
                match entry {
                    Ok(path) => {
                        if !quiet {
                            println!("Parsing the file: {:?}", &path.display());
                        }
                        let contents = fs::read_to_string(&path)
                            .expect("Something went wrong reading the file");
                        let doc =
                            roxmltree::Document::parse(&contents).expect("Could not parse the xml");
                        result.extend(header.iter().map(|head| (head.to_owned(), Match::Nothing)));
                        if metadata {
                            result.insert("_path".to_owned(), Match::Value(path.display().to_string()));
                        }
                        let mut parsed: HashMap<String, HashSet<String>> = HashMap::default();
                        let root = doc.root_element();
                        let mut namespaces: HashMap<String, String> = HashMap::default();
                        for namespace in root.namespaces() {
                            match namespace.name() {
                                Some(name) => {
                                    namespaces.insert(namespace.uri().to_owned(), name.to_owned());
                                },
                                None => ()
                            };
                        }
                        //println!("namespaces: {:?}", &namespaces);
                        let nodes = vec![Rc::new(root)];

                        traverse(
                            nodes,
                            &matcher,
                            &filters,
                            &header,
                            &elements,
                            &levels,
                            &namespaces,
                            &mut parsed,
                            &mut result,
                            &mut output,
                            None,
                            false,
                            1,
                        );
                    }
                    Err(e) => println!("{:?}", e),
                }
            }
            if !quiet {
                println!("All done!");
            }
        }
        Err(_) => {
            println!("You need to specify an existing parser config file.");
        }
    };
}
