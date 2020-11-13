use std::fmt::Write;

use ansi_term::Color::{self, Fixed, RGB};
use ansi_term::{self, Style};
use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::{SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

use crate::{Opt, Pretty, Theme};

pub struct Printer {
    indent_json: bool,
    color: bool,
    theme: Theme,
    sort_headers: bool,
}

impl Printer {
    pub fn new(opt: &Opt) -> Printer {
        let pretty = opt.pretty.as_ref().unwrap_or(&Pretty::All);
        let theme = opt.style.as_ref().unwrap_or(&Theme::Auto);

        match pretty {
            Pretty::All => Printer {
                indent_json: true,
                color: true,
                theme: theme.clone(),
                sort_headers: true,
            },
            Pretty::Colors => Printer {
                indent_json: false,
                color: true,
                theme: theme.clone(),
                sort_headers: false,
            },
            Pretty::Format => Printer {
                indent_json: true,
                color: false,
                theme: theme.clone(),
                sort_headers: true,
            },
            Pretty::None => Printer {
                indent_json: false,
                color: false,
                theme: theme.clone(),
                sort_headers: false,
            },
        }
    }

    fn print_json(&self, text: &str) {
        match (self.indent_json, self.color) {
            (true, true) => colorize(&indent_json(text), "json", &self.theme)
                .for_each(|line| print!("{}", line)),
            (false, true) => {
                colorize(text, "json", &self.theme).for_each(|line| print!("{}", line))
            }
            (true, false) => print!("{}", indent_json(text)),
            (false, false) => print!("{}", text),
        }
        println!("\x1b[0m");
    }

    fn print_xml(&self, text: &str) {
        if self.color {
            colorize(text, "xml", &self.theme).for_each(|line| print!("{}", line))
        } else {
            print!("{}", text)
        }
        println!("\x1b[0m");
    }

    fn print_html(&self, text: &str) {
        if self.color {
            colorize(text, "html", &self.theme).for_each(|line| print!("{}", line))
        } else {
            print!("{}", text)
        }
        println!("\x1b[0m");
    }

    pub fn print_request_headers(&self, request: &Request) {
        let method = request.method();
        let url = request.url();
        let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
        let version = reqwest::Version::HTTP_11;
        let mut headers = request.headers().clone();

        // See https://github.com/seanmonstar/reqwest/issues/1030
        if let Some(body) = request.body().and_then(|body| body.as_bytes()) {
            let content_length = HeaderValue::from_str(&body.len().to_string()).unwrap();
            headers.insert(CONTENT_LENGTH, content_length);
        }

        let request_line = format!("{} {}{} {:?}\n", method, url.path(), query_string, version);
        let headers = &headers_to_string(&headers, self.sort_headers);

        if self.color {
            colorize(&(request_line + &headers), "http", &self.theme)
                .for_each(|line| print!("{}", line));
            println!("\x1b[0m");
        } else {
            println!("{}", &(request_line + &headers));
        }
    }

    pub fn print_response_headers(&self, response: &Response) {
        let version = response.version();
        let status = response.status();
        let headers = response.headers();

        let status_line = format!(
            "{:?} {} {}\n",
            version,
            status.as_str(),
            status.canonical_reason().unwrap()
        );
        let headers = headers_to_string(headers, self.sort_headers);

        if self.color {
            colorize(&(status_line + &headers), "http", &self.theme)
                .for_each(|line| print!("{}", line));
            println!("\x1b[0m");
        } else {
            println!("{}", &(status_line + &headers));
        }
    }

    fn print_binary_suppressor(&self) {
        print!("\n\n");
        println!("+-----------------------------------------+");
        println!("| NOTE: binary data not shown in terminal |");
        println!("+-----------------------------------------+");
        print!("\n\n");
    }

    fn print_multipart_suppressor(&self) {
        print!("\n\n");
        println!("+--------------------------------------------+");
        println!("| NOTE: multipart data not shown in terminal |");
        println!("+--------------------------------------------+");
        print!("\n\n");
    }

    pub fn print_request_body(&self, request: &Request) {
        let content_type = match get_content_type(&request.headers()) {
            Some(content_type) => content_type,
            None => return,
        };

        if let Some(body) = request.body() {
            if content_type.contains("multipart") {
                self.print_multipart_suppressor();
            } else if content_type.contains("json") {
                let body = &String::from_utf8(body.as_bytes().unwrap().into()).unwrap();
                self.print_json(body);
            } else {
                let body = &String::from_utf8(body.as_bytes().unwrap().into()).unwrap();
                println!("{}", body);
            }
        }
        print!("\n");
    }

    pub fn print_response_body(&self, response: Response) {
        let content_type = match get_content_type(&response.headers()) {
            Some(content_type) => content_type,
            None => return,
        };

        if !content_type.contains("application") && !content_type.contains("text") {
            self.print_binary_suppressor();
        } else if content_type.contains("json") {
            self.print_json(&response.text().unwrap());
        } else if content_type.contains("xml") {
            self.print_xml(&response.text().unwrap());
        } else if content_type.contains("html") {
            self.print_html(&response.text().unwrap());
        } else {
            println!("{}", &response.text().unwrap());
        }
        print!("\n");
    }
}

// https://github.com/sharkdp/bat/blob/3a85fd767bd1f03debd0a60ac5bc08548f95bc9d/src/terminal.rs
fn to_ansi_color(color: syntect::highlighting::Color) -> ansi_term::Color {
    if color.a == 0 {
        // Themes can specify one of the user-configurable terminal colors by
        // encoding them as #RRGGBBAA with AA set to 00 (transparent) and RR set
        // to the 8-bit color palette number. The built-in themes ansi-light,
        // ansi-dark, base16, and base16-256 use this.
        match color.r {
            // For the first 8 colors, use the Color enum to produce ANSI escape
            // sequences using codes 30-37 (foreground) and 40-47 (background).
            // For example, red foreground is \x1b[31m. This works on terminals
            // without 256-color support.
            0x00 => Color::Black,
            0x01 => Color::Red,
            0x02 => Color::Green,
            0x03 => Color::Yellow,
            0x04 => Color::Blue,
            0x05 => Color::Purple,
            0x06 => Color::Cyan,
            0x07 => Color::White,
            // For all other colors, use Fixed to produce escape sequences using
            // codes 38;5 (foreground) and 48;5 (background). For example,
            // bright red foreground is \x1b[38;5;9m. This only works on
            // terminals with 256-color support.
            //
            // TODO: When ansi_term adds support for bright variants using codes
            // 90-97 (foreground) and 100-107 (background), we should use those
            // for values 0x08 to 0x0f and only use Fixed for 0x10 to 0xff.
            n => Fixed(n),
        }
    } else {
        RGB(color.r, color.g, color.b)
    }
}

fn colorize<'a>(text: &'a str, syntax: &str, theme: &Theme) -> impl Iterator<Item = String> + 'a {
    lazy_static! {
        static ref TS: ThemeSet = ThemeSet::load_from_folder("assets").unwrap();
        static ref PS: SyntaxSet = {
            let mut ps = SyntaxSetBuilder::new();
            ps.add_from_folder("assets", true).unwrap();
            ps.build()
        };
    }
    let syntax = PS.find_syntax_by_extension(syntax).unwrap();
    let mut h = match theme {
        Theme::Auto => HighlightLines::new(syntax, &TS.themes["ansi"]),
        Theme::Solarized => HighlightLines::new(syntax, &TS.themes["solarized"]),
    };

    LinesWithEndings::from(text).map(move |line| {
        let mut s: String = String::new();
        let highlights = h.highlight(line, &PS);
        for (style, component) in highlights {
            let mut color = Style::from(to_ansi_color(style.foreground));
            if style.font_style.contains(FontStyle::UNDERLINE) {
                color = color.underline();
            }
            write!(s, "{}", &color.paint(component)).unwrap();
        }
        s
    })
}

fn indent_json(text: &str) -> String {
    let mut fmt = jsonxf::Formatter::pretty_printer();
    fmt.indent = String::from("    ");
    fmt.format(text).unwrap()
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

// TODO: return enum
fn get_content_type(headers: &HeaderMap) -> Option<&str> {
    headers.get(CONTENT_TYPE)?.to_str().ok()
}
