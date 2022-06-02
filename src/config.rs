use pest::Parser;
use pest_derive::Parser;
use pest::iterators::Pair;

#[derive(Parser)]
#[grammar = "parser.pest"] // relative to src
struct UnstructParser;

pub fn parse(configuration: &str) {
    println!("The configuration is:\n{}", configuration);
    let parser = UnstructParser::parse(Rule::block, configuration.trim()).expect("Parsing error");
    for directive in parser {
        println!("{:?}", &directive);
        let mut column_name: Option<String> = None; 
        let mut xml_name: Option<String> = None; 
        for column_or_xml in directive.into_inner() {
            match column_or_xml.as_rule() {
                Rule::column_name => {
                    column_name = Some(column_or_xml.as_str().to_owned());
                }
                Rule::xml_name => { 
                    xml_name = Some(column_or_xml.as_str().to_owned());
                }, 
                _ => {
                    println!("Parsing error: {:?}", column_or_xml);
                }
            }
        }
        println!("{} = {}", column_name.as_ref().unwrap(), xml_name.as_ref().unwrap());
    }
}