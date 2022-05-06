use std::fs;
use roxmltree;
use config;
use clap::Parser;
use std::collections::HashMap;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[clap(short, long)]
    filename: String,
}

fn main() {
    let args = Args::parse();
    // read from command line later
    let filename = args.filename;
    println!("Filename: {}", &filename);

    let cfg = config::Config::builder().add_source(config::File::with_name("unstruct.ini")).build().unwrap();
    let map = cfg.try_deserialize::<HashMap<String, String>>().unwrap();
    // println!("Map: {:?}", map);

    let contents = fs::read_to_string(filename)
        .expect("Something went wrong reading the file");
    let doc = roxmltree::Document::parse(&contents).unwrap();

    for element in doc.descendants() {
        match map.get(&element.tag_name().name().to_string()) {
            Some(mapping) => {
                println!("Hit: {} = {}",&mapping, &element.text().unwrap());
            }
            None => ()
        };
    }
    // test
    //let elem = doc.descendants().find(|n| n.has_tag_name("CDR")).unwrap();
    //let elem = elem.descendants().find(|n| n.has_tag_name("recordType")).unwrap();

    println!("All done!");
}
