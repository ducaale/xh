use std::io::{self, LineWriter, Write};

use ansi_term::Color::{self, Fixed, RGB};
use ansi_term::{self, Style};
use syntect::dumps::from_binary;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::buffer::Buffer;
use crate::cli::Theme;

pub fn get_json_formatter() -> jsonxf::Formatter {
    let mut fmt = jsonxf::Formatter::pretty_printer();
    fmt.indent = String::from("    ");
    fmt
}

lazy_static::lazy_static! {
    static ref TS: ThemeSet = from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/themepack.themedump"
    )));
    static ref PS: SyntaxSet = from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/syntax.packdump"
    )));
}

pub struct Highlighter<'a> {
    inner: HighlightLines<'static>,
    out: &'a mut Buffer,
}

impl<'a> Highlighter<'a> {
    pub fn new(syntax: &'static str, theme: Theme, out: &'a mut Buffer) -> Self {
        let syntax = PS
            .find_syntax_by_extension(syntax)
            .expect("syntax not found");
        Self {
            inner: HighlightLines::new(syntax, &TS.themes[theme.as_str()]),
            out,
        }
    }

    pub fn highlight_line(&mut self, line: &str) -> io::Result<()> {
        let highlights = self.inner.highlight(line, &PS);
        for (style, component) in highlights {
            let mut color = Style {
                foreground: to_ansi_color(style.foreground),
                ..Style::default()
            };
            if style.font_style.contains(FontStyle::UNDERLINE) {
                color = color.underline();
            }
            write!(self.out, "{}", color.paint(component))?;
        }
        Ok(())
    }

    pub fn finish(self) -> io::Result<()> {
        write!(self.out, "\x1b[0m")?;
        Ok(())
    }

    pub fn highlight(mut self, text: &str) -> io::Result<()> {
        for line in LinesWithEndings::from(text) {
            self.highlight_line(line)?;
        }
        self.finish()
    }

    pub fn linewise(&mut self) -> LineWriter<&mut Self> {
        LineWriter::new(self)
    }
}

impl<'a> Write for Highlighter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.highlight_line(&String::from_utf8_lossy(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

// https://github.com/sharkdp/bat/blob/3a85fd767bd1f03debd0a60ac5bc08548f95bc9d/src/terminal.rs
fn to_ansi_color(color: syntect::highlighting::Color) -> Option<ansi_term::Color> {
    if color.a == 0 {
        // Themes can specify one of the user-configurable terminal colors by
        // encoding them as #RRGGBBAA with AA set to 00 (transparent) and RR set
        // to the 8-bit color palette number. The built-in themes ansi-light,
        // ansi-dark, base16, and base16-256 use this.
        match color.r {
            // For the first 7 colors, use the Color enum to produce ANSI escape
            // sequences using codes 30-37 (foreground) and 40-47 (background).
            // For example, red foreground is \x1b[31m. This works on terminals
            // without 256-color support.
            0x00 => Some(Color::Black),
            0x01 => Some(Color::Red),
            0x02 => Some(Color::Green),
            0x03 => Some(Color::Yellow),
            0x04 => Some(Color::Blue),
            0x05 => Some(Color::Purple),
            0x06 => Some(Color::Cyan),
            // The 8th color is white. Themes use it as the default foreground
            // color, but that looks wrong on terminals with a light background.
            // So keep that text uncolored instead.
            0x07 => None,
            // For all other colors, use Fixed to produce escape sequences using
            // codes 38;5 (foreground) and 48;5 (background). For example,
            // bright red foreground is \x1b[38;5;9m. This only works on
            // terminals with 256-color support.
            //
            // TODO: When ansi_term adds support for bright variants using codes
            // 90-97 (foreground) and 100-107 (background), we should use those
            // for values 0x08 to 0x0f and only use Fixed for 0x10 to 0xff.
            n => Some(Fixed(n)),
        }
    } else {
        Some(RGB(color.r, color.g, color.b))
    }
}
