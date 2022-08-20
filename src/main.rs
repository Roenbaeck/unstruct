use std::fs;
use roxmltree::{self, Node};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use glob::glob;
use unstruct::config::{parse, Directive};
use std::fs::{File, read_to_string};
use std::io::Write;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    filename: String,
    #[clap(short, long)]
    outfile: String,
}

#[derive(Debug)]
enum Match {
    Value(String),
    Nothing
}

const DELIMITER: char = '\t';
const TERMINATOR: char = '\n';

fn traverse(
        nodes: Vec<Node>,     
        matcher: &HashMap<String, Directive>, 
        header: &Vec<String>,
        parsed: &mut HashSet<String>, 
        result: &mut HashMap<String, Match>, 
        output: &mut File, 
        depth: usize
    ) {
    let mut child_nodes = Vec::default();
    for element in nodes {
        if element.is_element() {
            if element.has_children() {
                for child in element.children() {
                    child_nodes.push(child);
                }
            }
            manage(&element.tag_name().name().to_string(), &element.text().unwrap().to_owned(), matcher, header, parsed, result, output, depth);
            for attribute in element.attributes() {
                let xml_name = element.tag_name().name().to_string() + "/@" + attribute.name();
                let xml_value = attribute.value().to_owned();
                println!("Looking for ({}): {}", depth, xml_name);
                manage(&xml_name, &xml_value, matcher, header, parsed, result, output, depth);
            }
            // ------------------------------------------------------------------------------------------
            let mut peekable_header = header.iter().peekable();
            while let Some(head) = peekable_header.next() {
                // println!("FINDING: {}", head);
                match result.get(head).unwrap() {
                    Match::Value(column_value) => {
                        // println!("WRITING: {}", column_value);
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
    if !child_nodes.is_empty() {
        traverse(child_nodes, matcher, header, parsed, result, output, depth + 1);
    } 
}

fn manage(
        xml_name: &str, 
        xml_value: &str,     
        matcher: &HashMap<String, Directive>, 
        header: &Vec<String>,
        parsed: &mut HashSet<String>, 
        result: &mut HashMap<String, Match>, 
        output: &mut File, 
        depth: usize
    ) {
    
    match matcher.get(&xml_name.to_owned()) {
        Some(column) => {
            if column.level == depth {
                result.insert(
                    column.column_name.to_owned(), 
                    Match::Value(xml_value.to_owned())
                );
            }

            parsed.insert(column.column_name.to_owned());
         
        }
        None => ()
    };
}

fn main() {
    // read the config containing the mapping between elements and columns
    let configuration = read_to_string("unstruct.parser").unwrap();
    let (matcher, header) = parse(&configuration);
    println!("Matcher: {:?}", matcher);
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
                let mut peekable_header = header.iter().peekable();
                while let Some(head) = peekable_header.next() {
                    result.insert(head.to_owned(), Match::Nothing);
                }
                let mut parsed: HashSet<String> = HashSet::default();
                let root = doc.root_element();
                let mut nodes = Vec::default();
                nodes.push(root);

                traverse(
                    nodes,                    
                    &matcher, 
                    &header, 
                    &mut parsed, 
                    &mut result,
                    &mut output, 
                    0
                );

            },
            Err(e) => println!("{:?}", e),
        }
    }

    println!("All done!");
    // println!("{:?}", result);
}
