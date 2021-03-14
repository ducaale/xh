//! This is a vendored version of JSONXF: https://github.com/gamache/jsonxf
//!
//! It has been slightly modified for xh's use case. It may be unvendored
//! if https://github.com/gamache/jsonxf/pull/8 is merged.
//!
//! Copyright 2017 Pete Gamache, see LICENSE-JSONXF.txt.

use std::io::prelude::*;
use std::io::Error;
use std::io::ErrorKind;

use crate::utils::BUFFER_SIZE;

const C_CR: u8 = b'\r';
const C_LF: u8 = b'\n';
const C_TAB: u8 = b'\t';
const C_SPACE: u8 = b' ';

const C_COMMA: u8 = b',';
const C_COLON: u8 = b':';
const C_QUOTE: u8 = b'"';
const C_BACKSLASH: u8 = b'\\';

const C_LEFT_BRACE: u8 = b'{';
const C_LEFT_BRACKET: u8 = b'[';
const C_RIGHT_BRACE: u8 = b'}';
const C_RIGHT_BRACKET: u8 = b']';

/// `Formatter` allows customizable pretty-printing, minimizing,
/// and other formatting tasks on JSON-encoded UTF-8 data in
/// string or stream format.
///
/// Example:
///
/// ```
/// let mut fmt = jsonxf::Formatter::pretty_printer();
/// fmt.line_separator = String::from("\r\n");
/// assert_eq!(
///     fmt.format("{\"a\":1}").unwrap(),
///     "{\r\n  \"a\": 1\r\n}"
/// );
/// ```
pub struct Formatter {
    /// Used for beginning-of-line indentation in arrays and objects.
    pub indent: String,

    /// Used inside arrays and objects.
    pub line_separator: String,

    /// Used between root-level arrays and objects.
    pub record_separator: String,

    /// Used after a colon inside objects.
    pub after_colon: String,

    /// Used at very end of output.
    pub trailing_output: String,

    /// Add a record_separator as soon as a record ends, before seeing a
    /// subsequent record. Useful when there's a long time between records.
    pub eager_record_separators: bool,

    // private mutable state
    depth: usize,       // current nesting depth
    in_string: bool,    // is the next byte part of a string?
    in_backslash: bool, // does the next byte follow a backslash in a string?
    empty: bool,        // is the next byte in an empty object or array?
    first: bool,        // is this the first byte of input?
}

impl Formatter {
    fn default() -> Formatter {
        Formatter {
            indent: String::from("  "),
            line_separator: String::from("\n"),
            record_separator: String::from("\n"),
            after_colon: String::from(" "),
            trailing_output: String::from(""),
            eager_record_separators: false,
            depth: 0,
            in_string: false,
            in_backslash: false,
            empty: false,
            first: true,
        }
    }

    /// Returns a Formatter set up for pretty-printing.
    /// Defaults to using two spaces of indentation,
    /// Unix newlines, and no whitespace at EOF.
    ///
    /// # Example:
    ///
    /// ```
    /// assert_eq!(
    ///     jsonxf::Formatter::pretty_printer().format("{\"a\":1}").unwrap(),
    ///     "{\n  \"a\": 1\n}"
    /// );
    /// ```
    pub fn pretty_printer() -> Formatter {
        Formatter::default()
    }

    /// Formats a stream of JSON-encoded data without buffering.
    ///
    /// # Example:
    ///
    /// ```no_run
    /// let mut fmt = jsonxf::Formatter::pretty_printer();
    /// let mut stdin = std::io::stdin();
    /// let mut stdout = std::io::stdout();
    /// fmt.format_stream_unbuffered(&mut stdin, &mut std::io::LineWriter::new(stdout))
    ///     .unwrap();
    /// ```
    pub fn format_stream_unbuffered(
        &mut self,
        input: &mut impl Read,
        output: &mut impl Write,
    ) -> Result<(), Error> {
        let mut buf = [0_u8; BUFFER_SIZE];
        loop {
            match input.read(&mut buf) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    self.format_buf(&buf[0..n], output)?;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        output.write_all(self.trailing_output.as_bytes())?;
        Ok(())
    }

    /* Formats the contents of `buf` into `writer`. */
    pub fn format_buf(&mut self, buf: &[u8], writer: &mut impl Write) -> Result<(), Error> {
        let mut n = 0;
        while n < buf.len() {
            let b = buf[n];

            if self.in_string {
                if self.in_backslash {
                    writer.write_all(&buf[n..n + 1])?;
                    self.in_backslash = false;
                } else {
                    match memchr::memchr2(C_QUOTE, C_BACKSLASH, &buf[n..]) {
                        None => {
                            // The whole rest of buf is part of the string
                            writer.write_all(&buf[n..])?;
                            break;
                        }
                        Some(index) => {
                            let length = index + 1;
                            writer.write_all(&buf[n..n + length])?;
                            if buf[n + index] == C_QUOTE {
                                // End of string
                                self.in_string = false;
                            } else {
                                // Backslash
                                self.in_backslash = true;
                            }
                            n += length;
                            continue;
                        }
                    }
                }
            } else {
                match b {
                    C_SPACE | C_LF | C_CR | C_TAB => {
                        // skip whitespace
                    }

                    C_LEFT_BRACKET | C_LEFT_BRACE => {
                        if self.first {
                            self.first = false;
                            writer.write_all(&buf[n..n + 1])?;
                        } else if self.empty {
                            writer.write_all(self.line_separator.as_bytes())?;
                            for _ in 0..self.depth {
                                writer.write_all(self.indent.as_bytes())?;
                            }
                            writer.write_all(&buf[n..n + 1])?;
                        } else if !self.eager_record_separators && self.depth == 0 {
                            writer.write_all(self.record_separator.as_bytes())?;
                            writer.write_all(&buf[n..n + 1])?;
                        } else {
                            writer.write_all(&buf[n..n + 1])?;
                        }
                        self.depth += 1;
                        self.empty = true;
                    }

                    C_RIGHT_BRACKET | C_RIGHT_BRACE => {
                        self.depth = self.depth.saturating_sub(1);
                        if self.empty {
                            self.empty = false;
                            writer.write_all(&buf[n..n + 1])?;
                        } else {
                            writer.write_all(self.line_separator.as_bytes())?;
                            for _ in 0..self.depth {
                                writer.write_all(self.indent.as_bytes())?;
                            }
                            writer.write_all(&buf[n..n + 1])?;
                        }
                        if self.eager_record_separators && self.depth == 0 {
                            writer.write_all(self.record_separator.as_bytes())?;
                        }
                    }

                    C_COMMA => {
                        writer.write_all(&buf[n..n + 1])?;
                        writer.write_all(self.line_separator.as_bytes())?;
                        for _ in 0..self.depth {
                            writer.write_all(self.indent.as_bytes())?;
                        }
                    }

                    C_COLON => {
                        writer.write_all(&buf[n..n + 1])?;
                        writer.write_all(self.after_colon.as_bytes())?;
                    }

                    _ => {
                        if self.empty {
                            writer.write_all(self.line_separator.as_bytes())?;
                            for _ in 0..self.depth {
                                writer.write_all(self.indent.as_bytes())?;
                            }
                            self.empty = false;
                        }
                        if b == C_QUOTE {
                            self.in_string = true;
                        }
                        writer.write_all(&buf[n..n + 1])?;
                    }
                };
            };
            n += 1;
        }

        Ok(())
    }
}
