use std::io::{self, Write};

use syntect::dumps::from_binary;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use termcolor::WriteColor;

use crate::{buffer::Buffer, cli::Theme};

pub fn get_json_formatter(indent_level: usize) -> jsonxf::Formatter {
    let mut fmt = jsonxf::Formatter::pretty_printer();
    fmt.indent = " ".repeat(indent_level);
    fmt.record_separator = String::from("\n\n");
    fmt.eager_record_separators = true;
    fmt
}

static TS: once_cell::sync::Lazy<ThemeSet> = once_cell::sync::Lazy::new(|| {
    from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/themepack.themedump"
    )))
});
static PS_BASIC: once_cell::sync::Lazy<SyntaxSet> = once_cell::sync::Lazy::new(|| {
    from_binary(include_bytes!(concat!(env!("OUT_DIR"), "/basic.packdump")))
});
static PS_LARGE: once_cell::sync::Lazy<SyntaxSet> = once_cell::sync::Lazy::new(|| {
    from_binary(include_bytes!(concat!(env!("OUT_DIR"), "/large.packdump")))
});

pub struct Highlighter<'a> {
    highlighter: HighlightLines<'static>,
    syntax_set: &'static SyntaxSet,
    out: &'a mut Buffer,
}

/// A wrapper around a [`Buffer`] to add syntax highlighting when printing.
impl<'a> Highlighter<'a> {
    pub fn new(syntax: &'static str, theme: Theme, out: &'a mut Buffer) -> Self {
        let syntax_set: &SyntaxSet = match syntax {
            "json" | "http" => &PS_BASIC,
            _ => &PS_LARGE,
        };
        let syntax = syntax_set
            .find_syntax_by_extension(syntax)
            .expect("syntax not found");
        Self {
            highlighter: HighlightLines::new(syntax, &TS.themes[theme.as_str()]),
            syntax_set,
            out,
        }
    }

    /// Write a single piece of highlighted text.
    /// May return a [`io::ErrorKind::Other`] when there is a problem
    /// during highlighting.
    pub fn highlight(&mut self, text: &str) -> io::Result<()> {
        for line in LinesWithEndings::from(text) {
            for (style, component) in self
                .highlighter
                .highlight_line(line, self.syntax_set)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            {
                self.out.set_color(&convert_style(style))?;
                write!(self.out, "{}", component)?;
            }
        }
        Ok(())
    }

    pub fn highlight_bytes(&mut self, line: &[u8]) -> io::Result<()> {
        self.highlight(&String::from_utf8_lossy(line))
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

impl Drop for Highlighter<'_> {
    fn drop(&mut self) {
        // This is just a best-effort attempt to restore the terminal, failure can be ignored
        let _ = self.out.reset();
    }
}

fn convert_style(style: syntect::highlighting::Style) -> termcolor::ColorSpec {
    use syntect::highlighting::FontStyle;
    let mut spec = termcolor::ColorSpec::new();
    spec.set_fg(convert_color(style.foreground))
        .set_underline(style.font_style.contains(FontStyle::UNDERLINE));
    spec
}

// https://github.com/sharkdp/bat/blob/3a85fd767bd1f03debd0a60ac5bc08548f95bc9d/src/terminal.rs
fn convert_color(color: syntect::highlighting::Color) -> Option<termcolor::Color> {
    use termcolor::Color;

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
            0x05 => Some(Color::Magenta),
            0x06 => Some(Color::Cyan),
            // The 8th color is white. Themes use it as the default foreground
            // color, but that looks wrong on terminals with a light background.
            // So keep that text uncolored instead.
            0x07 => None,
            // For all other colors, produce escape sequences using
            // codes 38;5 (foreground) and 48;5 (background). For example,
            // bright red foreground is \x1b[38;5;9m. This only works on
            // terminals with 256-color support.
            n => Some(Color::Ansi256(n)),
        }
    } else {
        Some(Color::Rgb(color.r, color.g, color.b))
    }
}
