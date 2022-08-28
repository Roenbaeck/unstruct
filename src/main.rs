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
    header: &Vec<String>,
    elements: &HashMap<String, Vec<String>>,
    parsed: &mut HashSet<String>,
    result: &mut HashMap<String, Match>,
    output: &mut File,
    recording: bool,
    siblings: bool,
    depth: usize,
) {
    let mut recording = recording;
    let mut siblings = siblings;
    if siblings {
        for element in nodes.into_iter().filter(|el| el.is_element()) {
            let mut nodes_to_search = Vec::default();
            if element.has_children() {
                siblings = false;
                nodes_to_search.extend(element.children().map(Rc::new));
            }
            if recording {
                let mut xml_name = element.tag_name().name().to_string();
                let qualified_element_name = format!("{}{}{}", xml_name, LEVEL, depth);
                if let Some(partial_header) = elements.get(&qualified_element_name) {
                    result.extend(
                        partial_header
                            .iter()
                            .map(|head| (head.to_owned(), Match::Nothing)),
                    );
                }
                let mut xml_value = element.text().unwrap_or("").to_owned();
                record(&xml_name, &xml_value, matcher, parsed, result, depth);
                for attribute in element.attributes() {
                    xml_name = element.tag_name().name().to_string() + "/@" + attribute.name();
                    xml_value = attribute.value().to_owned();
                    record(&xml_name, &xml_value, matcher, parsed, result, depth);
                }
            }
            if !nodes_to_search.is_empty() {
                traverse(
                    nodes_to_search,
                    matcher,
                    header,
                    elements,
                    parsed,
                    result,
                    output,
                    recording,
                    siblings,
                    if siblings { depth } else { depth + 1 },
                );
            }
        }
    } else {
        let mut nodes_to_search = Vec::default();
        for element in nodes.into_iter().filter(|el| el.is_element()) {
            let qualified_element_name = format!("{}{}{}", element.tag_name().name(), LEVEL, depth);
            if elements.contains_key(&qualified_element_name) {
                recording = true;
                siblings = true;
                nodes_to_search.extend(element.next_siblings().map(Rc::new));
                break;
            } else if element.has_children() {
                nodes_to_search.extend(element.children().map(Rc::new));
            }
            if recording {
                let mut xml_name = element.tag_name().name().to_string();
                let mut xml_value = element.text().unwrap().to_owned();
                record(&xml_name, &xml_value, matcher, parsed, result, depth);
                for attribute in element.attributes() {
                    xml_name = element.tag_name().name().to_string() + "/@" + attribute.name();
                    xml_value = attribute.value().to_owned();
                    record(&xml_name, &xml_value, matcher, parsed, result, depth);
                }
            }
        }
        if !nodes_to_search.is_empty() {
            traverse(
                nodes_to_search,
                matcher,
                header,
                elements,
                parsed,
                result,
                output,
                recording,
                siblings,
                if siblings { depth } else { depth + 1 },
            );
        }
    }
    if !parsed.is_empty() {
        parsed.clear();
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

fn record(
    xml_name: &str,
    xml_value: &str,
    matcher: &HashMap<String, String>,
    parsed: &mut HashSet<String>,
    result: &mut HashMap<String, Match>,
    depth: usize,
) {
    let element = format!("{}{}{}", xml_name, LEVEL, depth);
    // println!("Looking for: {}", &element);
    if let Some(column) = matcher.get(&element) {
        result.insert(column.to_owned(), Match::Value(xml_value.to_owned()));
        parsed.insert(column.to_owned());
    }
}

fn main() {
    let Args {
        filename,
        outfile,
        parser,
        quiet,
    } = Args::parse();

    // read the config containing the mapping between elements and columns
    let configuration = read_to_string(parser);
    match configuration {
        Ok(config) => {
            let (matcher, header, elements) = parse(&config);
            let mut result: HashMap<String, Match> = HashMap::default();

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

                        let mut parsed: HashSet<String> = HashSet::default();
                        let root = doc.root_element();
                        let nodes = vec![Rc::new(root)];

                        traverse(
                            nodes,
                            &matcher,
                            &header,
                            &elements,
                            &mut parsed,
                            &mut result,
                            &mut output,
                            false,
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
