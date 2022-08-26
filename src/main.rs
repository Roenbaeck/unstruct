use std::fs;
use std::rc::Rc;
use roxmltree::{self, Node};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use glob::glob;
use unstruct::config::{parse, LEVEL};
use std::fs::{File, read_to_string};
use std::io::Write;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = "Unstruct is a program that parses simple xml files into text files, suitable for bulk inserts into a relational database.")]
struct Args {
    #[clap(short, long, help = "The name of the input xml file or matching files if wildcards are used")]
    filename: String,
    #[clap(short, long, help = "The name of the text file into which the results of the parsing will be output")]
    outfile: String,
    #[clap(short, long, help = "The configuration file specifying the parsing rules", default_value="unstruct.parser")]
    parser: String,
}

#[derive(Debug)]
enum Match {
    Value(String),
    Nothing
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
        depth: usize
    ) {
    let mut recording = recording;
    let mut siblings = siblings;
    if siblings {
        for element in nodes {
            let mut nodes_to_search = Vec::default();
            if element.is_element() {
                if element.has_children() {
                    siblings = false;
                    for child in element.children() {
                        nodes_to_search.push(Rc::new(child));
                    }
                }                    
                if recording {
                    let mut xml_name = element.tag_name().name().to_string();
                    let qualified_element_name = xml_name.to_owned() + LEVEL + &depth.to_string();
                    match elements.get(&qualified_element_name) {
                        Some(partial_header) => {
                            for head in partial_header {
                                result.insert(head.to_owned(), Match::Nothing);
                            }                
                        },
                        None => ()
                    }
                    let mut xml_value = match element.text() {
                        Some(text) => text,
                        None => ""
                    }.to_owned();
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
                        if siblings { depth } else { depth + 1 }
                    );
                }
            }
        }
    }
    else {
        let mut nodes_to_search = Vec::default();
        for element in nodes {
            if element.is_element() {
                let qualified_element_name = element.tag_name().name().to_string() + LEVEL + &depth.to_string();
                if elements.contains_key(&qualified_element_name) {
                    recording = true;
                    siblings = true;
                    for sibling in element.next_siblings() {
                        nodes_to_search.push(Rc::new(sibling));
                    }
                    break;
                }
                else {
                    if element.has_children() {
                        for child in element.children() {
                            nodes_to_search.push(Rc::new(child));
                        }
                    }                    
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
                if siblings { depth } else { depth + 1 }
            );
        }
    }
    if !parsed.is_empty() {
        parsed.clear();
        // ------------------------------------------------------------------------------------------
        let mut peekable_header = header.iter().peekable();
        while let Some(head) = peekable_header.next() {
            match result.get(head).unwrap() {
                Match::Value(column_value) => {
                    write!(output, "{}", column_value).expect("Cannot write to output file");
                }
                Match::Nothing => ()
            };
            if peekable_header.peek().is_none() {
                write!(output, "{}", TERMINATOR).expect("Cannot write to output file");
            }
            else {
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
        depth: usize
    ) {
    let element = xml_name.to_owned() + LEVEL + &depth.to_string();
    // println!("Looking for: {}", &element);
    match matcher.get(&element) {
        Some(column) => {
            result.insert(
                column.to_owned(), 
                Match::Value(xml_value.to_owned())
            );
            parsed.insert(column.to_owned());
        }
        None => ()
    };
}

fn main() {
    // read the config containing the mapping between elements and columns
    let configuration = read_to_string("unstruct.parser").unwrap();
    let (matcher, header, elements) = parse(&configuration);
    let mut result: HashMap<String, Match> = HashMap::default();      

    // parse the arguments to get the filename glob pattern
    let args = Args::parse();
    let filename = args.filename;
    let outfile = args.outfile;
    println!("Finding files matching: {}", &filename);
    println!("Results are stored in: {}", &outfile);

    let mut output = File::create(outfile).unwrap();
    let mut peekable_header = header.iter().peekable();
    while let Some(head) = peekable_header.next() {
        write!(output, "{}", head).expect("Cannot write to output file");
        if peekable_header.peek().is_none() {
            write!(output, "{}", TERMINATOR).expect("Cannot write to output file");
        }
        else {
            write!(output, "{}", DELIMITER).expect("Cannot write to output file");
        }
    }

    // use the glob to find matching files
    for entry in glob(&filename).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("Parsing the file: {:?}", &path.display());
                let contents = fs::read_to_string(&path).expect("Something went wrong reading the file");
                let doc = roxmltree::Document::parse(&contents).expect("Could not parse the xml");
                for head in &header {
                    result.insert(head.to_owned(), Match::Nothing);
                }
                let mut parsed: HashSet<String> = HashSet::default();
                let root = doc.root_element();
                let mut nodes = Vec::default();
                nodes.push(Rc::new(root));

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
                    1
                );

            },
            Err(e) => println!("{:?}", e),
        }
    }

    println!("All done!");
}
