use std::io::Result;

use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, StatusCode, Version,
};
use syntect::highlighting::Theme;
use termcolor::WriteColor;
use url::Url;

use crate::utils::HeaderValueExt;

super::palette::palette! {
    struct HeaderPalette {
        http_keyword: ["keyword.other.http"],
        http_separator: ["punctuation.separator.http"],
        http_version: ["constant.numeric.http"],
        method: ["keyword.control.http"],
        path: ["const.language.http"],
        status_code: ["constant.numeric.http"],
        status_reason: ["keyword.reason.http"],
        header_name: ["source.http", "http.requestheaders", "support.variable.http"],
        header_colon: ["source.http", "http.requestheaders", "punctuation.separator.http"],
        header_value: ["source.http", "http.requestheaders", "string.other.http"],
        error: ["error"],
    }
}

macro_rules! set_color {
    ($self:ident, $color:ident) => {
        if let Some(ref palette) = $self.palette {
            $self.output.set_color(&palette.$color)
        } else {
            Ok(())
        }
    };
}

pub(crate) struct HeaderFormatter<'a, W: WriteColor> {
    output: &'a mut W,
    palette: Option<HeaderPalette>,
    is_terminal: bool,
    sort_headers: bool,
}

impl<'a, W: WriteColor> HeaderFormatter<'a, W> {
    pub(crate) fn new(
        output: &'a mut W,
        theme: Option<&Theme>,
        is_terminal: bool,
        sort_headers: bool,
    ) -> Self {
        Self {
            palette: theme.map(HeaderPalette::from),
            output,
            is_terminal,
            sort_headers,
        }
    }

    fn print(&mut self, text: &str) -> Result<()> {
        self.output.write_all(text.as_bytes())
    }

    fn print_plain(&mut self, text: &str) -> Result<()> {
        set_color!(self, default)?;
        self.print(text)
    }

    pub(crate) fn print_request_headers(
        &mut self,
        method: &Method,
        url: &Url,
        version: Version,
        headers: &HeaderMap,
    ) -> Result<()> {
        set_color!(self, method)?;
        self.print(method.as_str())?;

        self.print_plain(" ")?;

        set_color!(self, path)?;
        self.print(url.path())?;
        if let Some(query) = url.query() {
            self.print("?")?;
            self.print(query)?;
        }

        self.print_plain(" ")?;
        self.print_http_version(version)?;

        self.print_plain("\n")?;
        self.print_headers(headers, version)?;

        if self.palette.is_some() {
            self.output.reset()?;
        }
        Ok(())
    }

    pub(crate) fn print_response_headers(
        &mut self,
        version: Version,
        status: StatusCode,
        reason_phrase: &str,
        headers: &HeaderMap,
    ) -> Result<()> {
        self.print_http_version(version)?;

        self.print_plain(" ")?;

        set_color!(self, status_code)?;
        self.print(status.as_str())?;

        self.print_plain(" ")?;

        set_color!(self, status_reason)?;
        self.print(reason_phrase)?;

        self.print_plain("\n")?;

        self.print_headers(headers, version)?;

        if self.palette.is_some() {
            self.output.reset()?;
        }
        Ok(())
    }

    fn print_http_version(&mut self, version: Version) -> Result<()> {
        let version = format!("{version:?}");
        let version = version.strip_prefix("HTTP/").unwrap_or(&version);

        set_color!(self, http_keyword)?;
        self.print("HTTP")?;
        set_color!(self, http_separator)?;
        self.print("/")?;
        set_color!(self, http_version)?;
        self.print(version)?;

        Ok(())
    }

    fn print_headers(&mut self, headers: &HeaderMap, version: Version) -> Result<()> {
        let as_titlecase = match version {
            Version::HTTP_09 | Version::HTTP_10 | Version::HTTP_11 => true,
            Version::HTTP_2 | Version::HTTP_3 => false,
            _ => false,
        };
        let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
        if self.sort_headers {
            headers.sort_by_key(|(name, _)| name.as_str());
        }

        let mut namebuf = String::with_capacity(64);
        for (name, value) in headers {
            let key = if as_titlecase {
                titlecase_header(name, &mut namebuf)
            } else {
                name.as_str()
            };

            set_color!(self, header_name)?;
            self.print(key)?;
            set_color!(self, header_colon)?;
            self.print(":")?;
            self.print_plain(" ")?;

            match value.to_ascii_or_latin1() {
                Ok(ascii) => {
                    set_color!(self, header_value)?;
                    self.print(ascii)?;
                }
                Err(bad) => {
                    const FAQ_URL: &str =
                        "https://github.com/ducaale/xh/blob/master/FAQ.md#header-value-encoding";

                    let mut latin1 = bad.latin1();
                    if self.is_terminal {
                        latin1 = sanitize_header_value(&latin1);
                    }
                    set_color!(self, error)?;
                    self.print(&latin1)?;

                    if let Some(utf8) = bad.utf8() {
                        set_color!(self, default)?;
                        if self.palette.is_some() && super::supports_hyperlinks() {
                            self.print(" (")?;
                            self.print(&super::create_hyperlink("UTF-8", FAQ_URL))?;
                            self.print(": ")?;
                        } else {
                            self.print(" (UTF-8: ")?;
                        }

                        set_color!(self, header_value)?;
                        // We could escape these as well but latin1 has a much higher chance
                        // to contain control characters because:
                        // - ~14% of the possible latin1 codepoints are control characters,
                        //   versus <0.1% for UTF-8.
                        // - The latin1 text may not be intended as latin1, but if it's valid
                        //   as UTF-8 then chances are that it really is UTF-8.
                        // We should revisit this if we come up with a general policy for
                        // escaping control characters, not just in headers.
                        self.print(utf8)?;
                        self.print_plain(")")?;
                    }
                }
            }
            self.print_plain("\n")?;
        }

        Ok(())
    }
}

fn titlecase_header<'b>(name: &HeaderName, buffer: &'b mut String) -> &'b str {
    let name = name.as_str();
    buffer.clear();
    buffer.reserve(name.len());
    // Ought to be equivalent to how hyper does it
    // https://github.com/hyperium/hyper/blob/f46b175bf71b202fbb907c4970b5743881b891e1/src/proto/h1/role.rs#L1332
    // Header names are ASCII so operating on char or u8 is equivalent
    let mut prev = '-';
    for mut c in name.chars() {
        if prev == '-' {
            c.make_ascii_uppercase();
        }
        buffer.push(c);
        prev = c;
    }
    buffer
}

/// Escape control characters. Firefox uses Unicode replacement characters,
/// that seems like a good choice.
///
/// Header values can't contain ASCII control characters (like newlines)
/// but if misencoded they frequently contain latin1 control characters.
/// What we do here might not make sense for other strings.
fn sanitize_header_value(value: &str) -> String {
    const REPLACEMENT_CHARACTER: &str = "\u{FFFD}";
    value.replace(char::is_control, REPLACEMENT_CHARACTER)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn test_header_casing() {
        let mut headers = HeaderMap::new();
        headers.insert("ab-cd", "0".parse().unwrap());
        headers.insert("-cd", "0".parse().unwrap());
        headers.insert("-", "0".parse().unwrap());
        headers.insert("ab-%c", "0".parse().unwrap());
        headers.insert("A-b--C", "0".parse().unwrap());

        let mut buf = termcolor::Ansi::new(Vec::new());
        let mut formatter = HeaderFormatter::new(&mut buf, None, false, false);
        formatter.print_headers(&headers, Version::HTTP_11).unwrap();
        let buf = buf.into_inner();
        assert_eq!(
            buf,
            indoc! {b"
                Ab-Cd: 0
                -Cd: 0
                -: 0
                Ab-%c: 0
                A-B--C: 0
                "
            }
        );

        let mut buf = termcolor::Ansi::new(Vec::new());
        let mut formatter = HeaderFormatter::new(&mut buf, None, false, false);
        formatter.print_headers(&headers, Version::HTTP_2).unwrap();
        let buf = buf.into_inner();
        assert_eq!(
            buf,
            indoc! {b"
                ab-cd: 0
                -cd: 0
                -: 0
                ab-%c: 0
                a-b--c: 0
                "
            }
        );
    }
}
