use std::fmt::Write;
use std::io::{self, Read};
use std::path::Path;

use ansi_term::Color::{self, Fixed, RGB};
use ansi_term::{self, Style};
use atty::Stream;
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use syntect::dumps::from_binary;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::Body;
use crate::Theme;

pub enum ContentType {
    Json,
    Html,
    Xml,
    UrlencodedForm,
    Multipart,
}

pub fn get_content_type(headers: &HeaderMap) -> Option<ContentType> {
    headers
        .get(CONTENT_TYPE)?
        .to_str()
        .ok()
        .and_then(|content_type| {
            if content_type.contains("json") {
                Some(ContentType::Json)
            } else if content_type.contains("html") {
                Some(ContentType::Html)
            } else if content_type.contains("xml") {
                Some(ContentType::Xml)
            } else if content_type.contains("multipart") {
                Some(ContentType::Multipart)
            } else if content_type.contains("x-www-form-urlencoded") {
                Some(ContentType::UrlencodedForm)
            } else {
                None
            }
        })
}

// https://github.com/seanmonstar/reqwest/issues/646#issuecomment-616985015
pub async fn body_to_file(path: impl AsRef<Path>) -> reqwest::Body {
    let file = File::open(&path).await.unwrap();
    reqwest::Body::wrap_stream(FramedRead::new(file, BytesCodec::new()))
}

pub fn body_from_stdin(ignore_stdin: bool) -> Option<Body> {
    if atty::is(Stream::Stdin) || ignore_stdin {
        None
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap();
        Some(Body::Raw(buffer))
    }
}

pub fn indent_json(text: &str) -> String {
    let mut fmt = jsonxf::Formatter::pretty_printer();
    fmt.indent = String::from("    ");
    fmt.format(text).unwrap()
}

pub fn colorize<'a>(
    text: &'a str,
    syntax: &str,
    theme: &Theme,
) -> impl Iterator<Item = String> + 'a {
    lazy_static::lazy_static! {
        static ref TS: ThemeSet = ThemeSet::from(from_binary(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/themepack.themedump"
        ))));
        static ref PS: SyntaxSet = SyntaxSet::from(from_binary(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/syntax.packdump"
        ))));
    }
    let syntax = PS.find_syntax_by_extension(syntax).unwrap();
    let mut h = match theme {
        Theme::Auto => HighlightLines::new(syntax, &TS.themes["ansi"]),
        Theme::Solarized => HighlightLines::new(syntax, &TS.themes["solarized"]),
    };

    LinesWithEndings::from(text)
        .map(move |line| {
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
        .chain(std::iter::once("\x1b[0m".into()))
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

// https://stackoverflow.com/a/45145246/5915221
#[macro_export]
macro_rules! vec_of_strings {
    ($($str:expr),*) => ({
        vec![$(String::from($str),)*] as Vec<String>
    });
}
