use pest::Parser;
use pest_derive::Parser;
use pest::iterators::{Pairs};
use std::{collections::{HashMap}, hash::Hash};

pub const LEVEL: &str = "|";

#[derive(Parser)]
#[grammar = "parser.pest"] // path relative to src
struct UnstructParser;

pub fn block_recurse(
        remainder: Pairs<Rule>,
        matcher: &mut HashMap<String, String>, 
        header: &mut Vec<String>, 
        elements: &mut HashMap<String, Vec<String>>,
        current_element: String,
        level: usize) 
    {
    let mut local_element = current_element;
    for parsed in remainder {
        match parsed.as_rule() {
            Rule::element => {
                let element = {
                    let element = parsed.as_str();
                    let one_before_last = element.len() - 1;
                    let trimmed = &element[1..one_before_last];
                    format!("{}{}{}", trimmed, LEVEL, level)
                };
                elements.insert(element.clone(), Vec::default());
                local_element = element;
            }
            Rule::directive => {
                let mut column_name: Option<String> = None; 
                let mut xml_name: Option<String> = None; 
                for column_or_xml in parsed.into_inner() {
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
                        let el = xml_name.as_ref().unwrap();
                        let one_before_last = el.len();
                        let trimmed = &el[1..one_before_last];
                        format!("{}{}{}", trimmed, LEVEL, level)
                    }, 
                    column_name.as_ref().unwrap().to_owned()
                );
                header.push(column_name.as_ref().unwrap().to_owned());
                if let Some(partial_header) = elements.get_mut(&local_element) {
                    partial_header.push(column_name.as_ref().unwrap().to_owned());
                }
            }
            Rule::block => {
                block_recurse(parsed.into_inner(), matcher, header, elements, local_element.to_owned(), level + 1);
            }
            _ => {
                println!("Parsing error: {:?}", parsed);
            }
        }
    }
}

pub fn parse(configuration: &str) -> (HashMap<String, String>, Vec<String>, HashMap<String, Vec<String>>) {
    // println!("The configuration is:\n{}", configuration);
    let remainder = UnstructParser::parse(Rule::block, configuration.trim()).expect("Parsing error");
    let mut matcher: HashMap<String, String> = HashMap::default();
    let mut header: Vec<String> = Vec::default();
    let mut elements: HashMap<String, Vec<String>> = HashMap::default();
    block_recurse(remainder, &mut matcher, &mut header, &mut elements, "".to_owned(), 1);
    // println!("matcher: {:?}", &matcher);
    // println!("header: {:?}", &header);
    // println!("elements: {:?}", &elements);
    (matcher, header, elements)
}
