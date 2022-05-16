use std::fs;
use roxmltree;
use clap::Parser;
use std::collections::HashMap;
use glob::glob;
use unstruct::config;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    filename: String,
}

fn main() {
    // read the config containing the mapping between elements and columns



    // parse the arguments to get the filename glob pattern
    let args = Args::parse();
    let filename = args.filename;
    println!("Finding files matching: {}", &filename);

    // use the glob to find matching files
    for entry in glob(&filename).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("Parsing the file: {:?}", &path.display());
                let contents = fs::read_to_string(&path).expect("Something went wrong reading the file");
                let doc = roxmltree::Document::parse(&contents).expect("Could not parse the xml");
                for element in doc.descendants() {
                    match map.get(&element.tag_name().name().to_string()) {
                        Some(column) => {
                            result.insert(column, element.text().unwrap().to_owned());
                            println!("Hit: {} = {}", column, result.get(column).unwrap());
                        }
                        None => ()
                    };
                }
            },
            Err(e) => println!("{:?}", e),
        }
    }

    println!("All done!");
    println!("{:?}", result);
}
