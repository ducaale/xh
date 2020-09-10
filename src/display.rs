use crate::Pretty;
use ansi_term::Colour;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{StatusCode, Url, Version};
use serde::Serialize;
use serde_json::Value;

pub fn print_json(text: &str, options: &Pretty) {
    // TODO: replace colored_json with syntec
    use colored_json::{Color, ColoredFormatter, CompactFormatter, PrettyFormatter, Styler};
    let style = Styler {
        key: Color::Blue.bold(),
        string_value: Color::Yellow.normal(),
        integer_value: Color::Blue.normal(),
        float_value: Color::Blue.normal(),
        nil_value: Color::Blue.normal(),
        ..Default::default()
    };
    let data: Value = serde_json::from_str(&text).unwrap();

    match options {
        Pretty::All => {
            let f = ColoredFormatter::with_styler(PrettyFormatter::with_indent(b"    "), style);
            println!("{}", f.to_colored_json_auto(&data).unwrap());
        }
        Pretty::Colors => {
            // don't format response in this case
            let f = ColoredFormatter::with_styler(CompactFormatter {}, style);
            println!("{}", f.to_colored_json_auto(&data).unwrap());
        }
        Pretty::Format => {
            // https://stackoverflow.com/a/49087292/5915221
            let buf = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut ser = serde_json::Serializer::with_formatter(buf, formatter);
            data.serialize(&mut ser).unwrap();
            println!("{}", String::from_utf8(ser.into_inner()).unwrap());
        }
        Pretty::None => {
            println!("{}", text);
        }
    }
    print!("\n");
}

fn http_version_to_number(version: Version) -> f64 {
    match version {
        Version::HTTP_09 => 0.9,
        Version::HTTP_10 => 1.0,
        Version::HTTP_11 => 1.1,
        Version::HTTP_2 => 2.0,
        Version::HTTP_3 => 3.0,
        _ => -1.0,
    }
}

pub fn print_request_line(version: Version, method: &crate::Method, url: &Url, pretty: &Pretty) {
    let query = url.query().map_or(String::from(""), |q| ["?", q].concat());
    let version_number = http_version_to_number(version);
    match pretty {
        Pretty::All | Pretty::Colors => {
            println!(
                "{} {}{} {}/{}",
                Colour::Green.paint(method.to_string()),
                Colour::Cyan.underline().paint(url.path()),
                Colour::Cyan.underline().paint(query),
                Colour::Blue.paint("HTTP"),
                Colour::Blue.paint(version_number.to_string())
            );
        }
        Pretty::Format | Pretty::None => {
            println!("{} {}{} {:?}", method, url.path(), query, version);
        }
    }
}

pub fn print_status_line(version: Version, status: StatusCode, pretty: &Pretty) {
    let version_number = http_version_to_number(version);
    match pretty {
        Pretty::All | Pretty::Colors => {
            println!(
                "{}/{} {} {}",
                Colour::Blue.paint("HTTP"),
                Colour::Blue.paint(version_number.to_string()),
                Colour::Blue.paint(status.as_str()),
                Colour::Cyan.paint(status.canonical_reason().unwrap())
            );
        }
        Pretty::Format | Pretty::None => {
            println!(
                "{}/{} {} {}",
                "HTTP",
                version_number.to_string(),
                status.as_str(),
                status.canonical_reason().unwrap()
            );
        }
    }
}

pub fn print_headers(headers: &HeaderMap, pretty: &Pretty) {
    let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
    if *pretty != Pretty::None {
        headers.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()))
    }

    for (key, value) in headers {
        let key = key.to_string();
        let value = value.to_str().unwrap();
        match pretty {
            Pretty::All | Pretty::Colors => {
                println!("{}: {}", Colour::Cyan.paint(key), value);
            }
            Pretty::Format | Pretty::None => {
                println!("{}: {}", key, value);
            }
        }
    }
    println!("");
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
        } else if content_type.contains("application/json") {
            print_json(&body(), &pretty)
        } else {
            println!("{}", body())
        }
    }
}
