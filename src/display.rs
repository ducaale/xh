use std::fmt::Write;

use crate::Pretty;
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxSet, SyntaxSetBuilder};
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

fn colorize<'a>(text: &'a str, syntax: &str) -> impl Iterator<Item = String> + 'a {
    lazy_static! {
        // static ref PS: SyntaxSet = SyntaxSet::load_defaults_newlines();
        static ref TS: ThemeSet = ThemeSet::load_defaults();
        static ref PS: SyntaxSet = {
            let mut ps = SyntaxSetBuilder::new();
            ps.add_from_folder("assets", true).unwrap();
            ps.build()
        };
    }
    let syntax = PS.find_syntax_by_extension(syntax).unwrap();
    let mut h = HighlightLines::new(syntax, &TS.themes["Solarized (dark)"]);
    LinesWithEndings::from(text).map(move |line| {
        let ranges: Vec<(Style, &str)> = h.highlight(line, &PS);
        as_24_bit_terminal_escaped(&ranges[..], false)
    })
}

fn indent_json(text: &str) -> String {
    let data: Value = serde_json::from_str(&text).unwrap();
    let buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
    data.serialize(&mut ser).unwrap();
    String::from_utf8(ser.into_inner()).unwrap()
}

pub fn print_json(text: &str, options: &Pretty) {
    match options {
        Pretty::All => {
            colorize(&indent_json(text), "json").for_each(|line| print!("{}", line));
        }
        Pretty::Colors => {
            colorize(text, "json").for_each(|line| print!("{}", line));
        }
        Pretty::Format => println!("{}", indent_json(text)),
        Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

pub fn print_xml(text: &str, options: &Pretty) {
    match options {
        Pretty::All | Pretty::Colors => colorize(text, "xml").for_each(|line| print!("{}", line)),
        Pretty::Format | Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

pub fn print_html(text: &str, options: &Pretty) {
    match options {
        Pretty::All | Pretty::Colors => colorize(text, "html").for_each(|line| print!("{}", line)),
        Pretty::Format | Pretty::None => println!("{}", text),
    }
    println!("\x1b[0m");
}

fn headers_to_string(headers: &HeaderMap, sort: bool) -> String {
    let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
    if sort {
        headers.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()))
    }

    let mut header_string = String::new();
    for (key, value) in headers {
        let key = key.to_string();
        let value = value.to_str().unwrap();
        writeln!(&mut header_string, "{}: {}", key, value).unwrap();
    }

    header_string
}

pub fn print_request_headers(request: &Request) {
    let method = request.method();
    let url = request.url();
    let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
    let version = reqwest::Version::HTTP_11;
    let headers = request.headers();

    let request_line = format!("{} {}{} {:?}\n", method, url.path(), query_string, version);
    let headers = &headers_to_string(headers, true);

    for line in colorize(&(request_line + &headers), "http") {
        print!("{}", line)
    }
    println!("\x1b[0m");
}

pub fn print_response_headers(response: &Response) {
    let version = response.version();
    let status = response.status();
    let headers = response.headers();

    let status_line = format!(
        "{:?} {} {}\n",
        version,
        status.as_str(),
        status.canonical_reason().unwrap()
    );
    let headers = headers_to_string(headers, true);

    for line in colorize(&(status_line + &headers), "http") {
        print!("{}", line)
    }
    println!("\x1b[0m");
}

// TODO: support pretty printing more response types
pub fn print_body(body: Box<dyn FnOnce() -> String>, content_type: Option<&str>, pretty: &Pretty) {
    if let Some(content_type) = content_type {
        if !content_type.contains("application") && !content_type.contains("text") {
            print!("\n\n");
            println!("+-----------------------------------------+");
            println!("| NOTE: binary data not shown in terminal |");
            println!("+-----------------------------------------+");
            print!("\n\n");
        } else if content_type.contains("json") {
            print_json(&body(), &pretty)
        } else if content_type.contains("xml") {
            print_xml(&body(), &pretty)
        } else if content_type.contains("html") {
            print_html(&body(), &pretty)
        }
    }
}
