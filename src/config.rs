use pest::Parser;
use pest_derive::Parser;
use pest::iterators::{Pair, Pairs};
use std::{collections::HashMap, hash::Hash};

#[derive(Parser)]
#[grammar = "parser.pest"] // relative to src
struct UnstructParser;

#[derive(Debug)]
pub struct Directive {
    pub level: usize, 
    pub column_name: String
}

pub fn block_recurse(remainder: Pairs<Rule>, matcher: &mut HashMap<String, Directive>, header: &mut Vec<String>, level: usize) {
    for directive_or_block in remainder {
        match directive_or_block.as_rule() {
            Rule::directive => {
                let mut column_name: Option<String> = None; 
                let mut xml_name: Option<String> = None; 
                for column_or_xml in directive_or_block.into_inner() {
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
                // println!("{} = {}", column_name.as_ref().unwrap(), xml_name.as_ref().unwrap());
                matcher.insert(
                    {
                        let mut chars = xml_name.as_ref().unwrap().chars();
                        chars.next();
                        chars.next_back();
                        chars.as_str().to_owned()
                    }, 
                    Directive {
                        level, 
                        column_name: column_name.as_ref().unwrap().to_owned()
                    }
                );
                header.push(column_name.as_ref().unwrap().to_owned());
            }
            Rule::block => {
                block_recurse(directive_or_block.into_inner(), matcher, header, level + 1);
            }
            _ => {
                println!("Parsing error: {:?}", directive_or_block);
            }
        }
    }
}

pub fn parse(configuration: &str) -> (HashMap<String, Directive>, Vec<String>) {
    // println!("The configuration is:\n{}", configuration);
    let remainder = UnstructParser::parse(Rule::block, configuration.trim()).expect("Parsing error");
    let mut matcher: HashMap<String, Directive> = HashMap::default();
    let mut header: Vec<String> = Vec::default();
    block_recurse(remainder, &mut matcher, &mut header, 0);
    // println!("{:?}", &matcher);
    (matcher, header)
}
