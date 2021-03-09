use std::io::{self, LineWriter, Write};

use ansi_term::Color::{self, Fixed, RGB};
use syntect::dumps::from_binary;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::cli::Theme;
use crate::vendored::jsonxf;

pub fn get_json_formatter() -> jsonxf::Formatter {
    let mut fmt = jsonxf::Formatter::pretty_printer();
    fmt.indent = String::from("    ");
    fmt.eager_record_separators = true;
    fmt
}

lazy_static::lazy_static! {
    static ref TS: ThemeSet = from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/themepack.themedump"
    )));
    static ref PS_BASIC: SyntaxSet = from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/basic.packdump"
    )));
    static ref PS_LARGE: SyntaxSet = from_binary(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/large.packdump"
    )));
}

pub struct Highlighter<'a> {
    inner: HighlightLines<'static>,
    syntax_set: &'static SyntaxSet,
    out: Box<dyn Write + 'a>,
    buffer: Vec<u8>, // For use by HighlightWriter
}

/// A wrapper around a [`Buffer`] to add syntax highlighting when printing.
impl<'a> Highlighter<'a> {
    pub fn new(syntax: &'static str, theme: Theme, out: Box<dyn Write + 'a>) -> Self {
        let syntax_set: &SyntaxSet = match syntax {
            "json" | "http" => &PS_BASIC,
            _ => &PS_LARGE,
        };
        let syntax = syntax_set
            .find_syntax_by_extension(syntax)
            .expect("syntax not found");
        Self {
            inner: HighlightLines::new(syntax, &TS.themes[theme.as_str()]),
            syntax_set,
            out,
            buffer: Vec::new(),
        }
    }

    /// Write a single piece of highlighted text.
    pub fn highlight(&mut self, line: &str) -> io::Result<()> {
        write_style(&mut self.inner, self.syntax_set, line, &mut self.out)
    }

    fn highlight_buffer(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let text = String::from_utf8_lossy(&self.buffer);
            // Can't call .highlight() because text references self.buffer
            write_style(&mut self.inner, self.syntax_set, &text, &mut self.out)?;
            self.buffer.clear();
        }
        Ok(())
    }

    /// Write out any remaining text, reset the color, and flush the buffer.
    ///
    /// This must be called when you're done, if no errors happened. Otherwise
    /// data may be dropped.
    ///
    /// See [`with_highlighter`](`crate::printer::Printer::with_highlighter`).
    pub fn finish(mut self) -> io::Result<()> {
        self.highlight_buffer()?;
        write!(self.out, "\x1b[0m")?;
        self.out.flush()?;
        Ok(())
    }

    /// Return an instance of [`Write`] to write highlighted text.
    ///
    /// This does some special handling to ensure lines aren't printed until
    /// they're complete.
    pub fn linewise<'b>(&'b mut self) -> LineWriter<HighlightWriter<'a, 'b>> {
        LineWriter::new(HighlightWriter(self))
    }
}

/// A [`Write`] implementation that accepts writes and turns them into
/// full highlighted lines. Incomplete lines will be saved up, no matter how
/// long they get, to ensure UTF-8 isn't mangled and highlighting isn't
/// interrupted.
///
/// Must be used through a [`LineWriter`] or it won't work properly.
/// See [`Highlighter::linewise`].
///
/// Ideally this type would be private to `linewise` and returned as an
/// anonymous `impl` type, but there's no easy way to do that with the
/// multiple lifetime parameters.
pub struct HighlightWriter<'a, 'b>(&'b mut Highlighter<'a>);

impl Write for HighlightWriter<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.last().copied() == Some(b'\n') {
            if self.0.buffer.is_empty() {
                self.0.highlight(&String::from_utf8_lossy(buf))?;
            } else {
                self.0.buffer.extend(buf);
                self.0.highlight_buffer()?;
            };
        } else {
            self.0.buffer.extend(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.highlight_buffer()?;
        self.0.out.flush()
    }
}

fn write_style(
    highlighter: &mut HighlightLines,
    syntax_set: &SyntaxSet,
    text: &str,
    out: &mut impl Write,
) -> io::Result<()> {
    // TODO: this text may contain multiple lines, is that ok?
    // If not, try syntect::util::LinesWithEndings
    for (style, component) in highlighter.highlight(text, syntax_set) {
        write!(out, "{}", convert_style(style).paint(component))?;
    }
    Ok(())
}

fn convert_style(style: syntect::highlighting::Style) -> ansi_term::Style {
    ansi_term::Style {
        foreground: to_ansi_color(style.foreground),
        is_underline: style
            .font_style
            .contains(syntect::highlighting::FontStyle::UNDERLINE),
        ..Default::default()
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
