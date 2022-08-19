use std::fs;
use roxmltree;
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

fn main() {
    // read the config containing the mapping between elements and columns
    let configuration = read_to_string("unstruct.parser").unwrap();
    let (matcher, header) = parse(&configuration);
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
                for element in doc.descendants() {
                    if element.is_element() {
                        println!("Looking for: {}", &element.tag_name().name().to_string());
                        match matcher.get(&element.tag_name().name().to_string()) {
                            Some(column) => {
                                // if we parse something again, write out what we have
                                if parsed.get(&column.column_name).is_some() {
                                    parsed = HashSet::default();
                                    let mut peekable_header = header.iter().peekable();
                                    while let Some(head) = peekable_header.next() {
                                        println!("FINDING: {}", head);
                                        match result.get(head).unwrap() {
                                            Match::Value(column_value) => {
                                                println!("WRITING: {}", column_value);
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
                                }
                                // println!("{} = {}", column.column_name, element.text().unwrap());
                                result.insert(
                                    column.column_name.to_owned(), 
                                    Match::Value(element.text().unwrap().to_owned())
                                );
                                parsed.insert(column.column_name.to_owned());
                            }
                            None => ()
                        };
                        for attribute in element.attributes() {
                            let path = element.tag_name().name().to_string() + "/@" + attribute.name();
                            println!("Looking for: {}", &path);
                            match matcher.get(&path) {
                                Some(column) => {
                                    // if we parse something again, write out what we have
                                    if parsed.get(&column.column_name).is_some() {
                                        parsed = HashSet::default();
                                        let mut peekable_header = header.iter().peekable();
                                        while let Some(head) = peekable_header.next() {
                                            println!("FINDING: {}", head);
                                            match result.get(head).unwrap() {
                                                Match::Value(column_value) => {
                                                    println!("WRITING: {}", column_value);
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
                                    }
                                    // println!("{} = {}", column.column_name, element.text().unwrap());
                                    result.insert(
                                        column.column_name.to_owned(), 
                                        Match::Value(attribute.value().to_owned())
                                    );
                                    parsed.insert(column.column_name.to_owned());
                                }
                                None => ()
                            };
                        }
                    }
                }
                // println!("HERE!");
                if parsed.len() > 0 {
                    let mut peekable_header = header.iter().peekable();
                    while let Some(head) = peekable_header.next() {
                        // println!("FINDING: {}", directive.column_name);
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
                }
            },
            Err(e) => println!("{:?}", e),
        }
    }

    println!("All done!");
    // println!("{:?}", result);
}
