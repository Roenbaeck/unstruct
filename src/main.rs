use std::fs;
use roxmltree;
use clap::Parser;
use std::collections::HashMap;
use glob::glob;
use unstruct::config::parse;
use std::fs::{File, read_to_string};
use std::io::Write;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    filename: String,
    #[clap(short, long)]
    outfile: String
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
    let matcher = parse(&configuration);
    let mut result: HashMap<String, Match> = HashMap::default();
    let mut header: Vec<String> = Vec::default(); // repeatable list, since .values is not
    for directive in matcher.values() {
        header.push(directive.column_name.to_owned());
    }            

    // parse the arguments to get the filename glob pattern
    let args = Args::parse();
    let filename = args.filename;
    let outfile = args.outfile;
    println!("Finding files matching: {}", &filename);
    println!("Results are stored in: {}", &outfile);

    let mut output = File::create(outfile).unwrap();
    for column_header in &header {
        write!(output, "{}", &column_header).expect("Cannot write to output file");
        write!(output, "{}", DELIMITER).expect("Cannot write to output file");
    }            
    write!(output, "{}", TERMINATOR).expect("Cannot write to output file");

    // use the glob to find matching files
    for entry in glob(&filename).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("Parsing the file: {:?}", &path.display());
                let contents = fs::read_to_string(&path).expect("Something went wrong reading the file");
                let doc = roxmltree::Document::parse(&contents).expect("Could not parse the xml");
                // clear the results between each file
                for column_header in &header {
                    result.insert(column_header.to_owned(), Match::Nothing);
                }            
                for element in doc.descendants() {
                    if element.is_element() {
                        // println!("Looking for: {}", &element.tag_name().name().to_string());
                        match matcher.get(&element.tag_name().name().to_string()) {
                            Some(column) => {
                                result.insert(
                                    column.column_name.to_owned(), 
                                    Match::Value(element.text().unwrap().to_owned())
                                );
                            }
                            None => ()
                        };
                    }
                }
                for column_header in &header {
                    match result.get(column_header).unwrap() {
                        Match::Value(column_value) => {
                            write!(output, "{}", column_value).expect("Cannot write to output file");
                            write!(output, "{}", DELIMITER).expect("Cannot write to output file");                    
                        }
                        Match::Nothing => {
                            write!(output, "{}", DELIMITER).expect("Cannot write to output file");                    
                        }
                    };
                }            
                write!(output, "{}", TERMINATOR).expect("Cannot write to output file");
            },
            Err(e) => println!("{:?}", e),
        }
    }

    println!("All done!");
    // println!("{:?}", result);
}
