use pest::iterators::Pairs;
use pest::Parser;
use pest_derive::Parser;
use std::{collections::HashMap, hash::Hash};

pub const LEVEL: &str = "|";

#[derive(Parser)]
#[grammar = "parser.pest"] // path relative to src
struct UnstructParser;

pub fn block_recurse(
    remainder: Pairs<Rule>,
    matcher: &mut HashMap<String, String>,
    filters: &mut HashMap<String, String>,
    header: &mut Vec<String>,
    elements: &mut HashMap<String, Vec<String>>,
    levels: &mut Vec<usize>,
    current_element: String,
    level: usize,
) {
    let mut local_element = current_element;
    // println!("Level: {}", level);
    // println!("Remainder: {:?}", remainder);
    while levels.len() < level {
        levels.push(0);
    }
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
                        }
                        _ => {
                            println!("The directive is malformed: {:?}", column_or_xml);
                        }
                    }
                }
                // println!("{} = {}", column_name.as_ref().unwrap(), xml_name.as_ref().unwrap());
                matcher.insert(
                    {
                        let element = xml_name.as_ref().unwrap();
                        format!("{}{}{}", element, LEVEL, level)
                    },
                    column_name.as_ref().unwrap().to_owned(),
                );
                header.push(column_name.as_ref().unwrap().to_owned());
                levels[level-1] = levels[level-1] + 1;
                if let Some(partial_header) = elements.get_mut(&local_element) {
                    partial_header.push(column_name.as_ref().unwrap().to_owned());
                }
            }
            Rule::filter => {
                let mut xml_name: Option<String> = None;
                let mut value: Option<String> = None;
                for xml_or_value in parsed.into_inner() {
                    match xml_or_value.as_rule() {
                        Rule::xml_name => {
                            xml_name = Some(xml_or_value.as_str().to_owned());
                        }
                        Rule::value => {
                            value = Some(xml_or_value.as_str().to_owned());
                        }
                        _ => {
                            println!("The directive is malformed: {:?}", xml_or_value);
                            std::process::exit(1);
                        }
                    }
                }
                filters.insert(
                    {
                        let element = xml_name.as_ref().unwrap();
                        format!("{}{}{}", element, LEVEL, level)
                    },
                    value.as_ref().unwrap().to_owned(),
                );
            }
            Rule::block => {
                block_recurse(
                    parsed.into_inner(),
                    matcher,
                    filters,
                    header,
                    elements,
                    levels,
                    local_element.to_owned(),
                    level + 1,
                );
            }
            _ => {
                println!("No parsing rule matches: {:?}", parsed);
                std::process::exit(1);
            }
        }
    }
}

pub fn parse(
    configuration: &str,
) -> (
    HashMap<String, String>,
    HashMap<String, String>,
    Vec<String>,
    HashMap<String, Vec<String>>,
    Vec<usize>,
) {
    // println!("The configuration is:\n{}", configuration);
    match UnstructParser::parse(Rule::config, configuration.trim()) {
        Result::Ok(mut remainder) => {
            let mut matcher: HashMap<String, String> = HashMap::default();
            let mut filters: HashMap<String, String> = HashMap::default();
            let mut header: Vec<String> = Vec::default();
            let mut elements: HashMap<String, Vec<String>> = HashMap::default();
            let mut levels: Vec<usize> = Vec::default();
            block_recurse(
                remainder.next().unwrap().into_inner(),
                &mut matcher,
                &mut filters,
                &mut header,
                &mut elements,
                &mut levels,
                "".to_owned(),
                1,
            );
            /*  
            println!("matcher: {:?}", &matcher);
            println!("filters: {:?}", &filters);
            println!("header: {:?}", &header);
            println!("elements: {:?}", &elements);
            println!("levels: {:?}", &levels);
            */
            (matcher, filters, header, elements, levels)        
        }, 
        Result::Err(error) => {
            println!("Could not parse the config file: {:?}", error);
            std::process::exit(1);
        }
    }
}
