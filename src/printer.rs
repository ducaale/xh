use std::io::{self, Read};

use encoding_rs::{Encoding, UTF_8};
use encoding_rs_io::DecodeReaderBytesBuilder;
use mime::Mime;
use reqwest::blocking::{Request, Response};
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, HOST,
};

use crate::{
    formatting::{get_json_formatter, Highlighter},
    utils::{copy_largebuf, get_content_type, test_mode, valid_json, ContentType},
};
use crate::{Buffer, Pretty, Theme};

const MULTIPART_SUPPRESSOR: &str = concat!(
    "+--------------------------------------------+\n",
    "| NOTE: multipart data not shown in terminal |\n",
    "+--------------------------------------------+\n",
    "\n"
);

const BINARY_SUPPRESSOR: &str = concat!(
    "+-----------------------------------------+\n",
    "| NOTE: binary data not shown in terminal |\n",
    "+-----------------------------------------+\n",
    "\n"
);

pub struct Printer {
    indent_json: bool,
    color: bool,
    theme: Theme,
    sort_headers: bool,
    stream: bool,
    buffer: Buffer,
}

impl Printer {
    pub fn new(pretty: Option<Pretty>, theme: Option<Theme>, stream: bool, buffer: Buffer) -> Self {
        let pretty = pretty.unwrap_or_else(|| Pretty::from(&buffer));
        let theme = theme.unwrap_or(Theme::auto);

        Printer {
            indent_json: pretty.format(),
            sort_headers: pretty.format(),
            color: pretty.color(),
            stream,
            theme,
            buffer,
        }
    }

    /// Run a piece of code with a [`Highlighter`] instance. After the code runs
    /// successfully, [`Highlighter::finish`] will be called to properly terminate.
    ///
    /// That way you don't have to remember to call it manually, and errors
    /// can still be handled (unlike an implementation of [`Drop`]).
    ///
    /// This version of the method does not check for null bytes.
    fn with_unguarded_highlighter(
        &mut self,
        syntax: &'static str,
        code: impl FnOnce(&mut Highlighter) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut highlighter =
            Highlighter::new(syntax, self.theme, Box::new(self.buffer.unguarded()));
        code(&mut highlighter)?;
        highlighter.finish()
    }

    fn print_text(&mut self, text: &str) -> io::Result<()> {
        self.buffer.unguarded().write_all(text.as_bytes())
    }

    fn print_colorized_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        self.with_unguarded_highlighter(syntax, |highlighter| highlighter.highlight(text))
    }

    fn print_syntax_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        if self.color {
            self.print_colorized_text(text, syntax)
        } else {
            self.print_text(text)
        }
    }

    fn print_json_text(&mut self, text: &str, check_valid: bool) -> io::Result<()> {
        if !self.indent_json {
            // We don't have to do anything specialized, so fall back to the generic version
            return self.print_syntax_text(text, "json");
        }

        if check_valid && !valid_json(text) {
            // JSONXF may mess up the text, e.g. by removing whitespace
            // This is somewhat common as application/json is the default
            // content type for requests
            return self.print_syntax_text(text, "json");
        }

        if self.color {
            let mut buf = Vec::new();
            get_json_formatter().format_stream_unbuffered(&mut text.as_bytes(), &mut buf)?;
            // in principle, buf should already be valid UTF-8,
            // because JSONXF doesn't mangle it
            let text = String::from_utf8_lossy(&buf);
            self.print_colorized_text(&text, "json")
        } else {
            get_json_formatter()
                .format_stream_unbuffered(&mut text.as_bytes(), &mut self.buffer.unguarded())
        }
    }

    fn print_javascript_text(&mut self, text: &str) -> io::Result<()> {
        if valid_json(text) {
            self.print_json_text(text, false)
        } else {
            self.print_syntax_text(text, "js")
        }
    }

    fn print_body_text(&mut self, content_type: Option<ContentType>, body: &str) -> io::Result<()> {
        match content_type {
            Some(ContentType::Json) => self.print_json_text(body, true),
            Some(ContentType::Xml) => self.print_syntax_text(body, "xml"),
            Some(ContentType::Html) => self.print_syntax_text(body, "html"),
            Some(ContentType::Css) => self.print_syntax_text(body, "css"),
            Some(ContentType::JavaScript) => self.print_javascript_text(body),
            // In HTTPie part of this behavior is gated behind the --json flag
            // But it does JSON formatting even without that flag, so doing
            // this check unconditionally is fine
            Some(ContentType::Text) if valid_json(body) => self.print_json_text(body, false),
            _ => self.buffer.print(body),
        }
    }

    /// Variant of `with_unguarded_highlighter` to use for text that has not
    /// yet been checked for null bytes.
    fn with_guarded_highlighter(
        &mut self,
        syntax: &'static str,
        code: impl FnOnce(&mut Highlighter) -> io::Result<()>,
    ) -> io::Result<()> {
        let theme = self.theme; // To avoid borrowing self
        self.buffer.with_guard(|guard| {
            let mut highlighter = Highlighter::new(syntax, theme, Box::new(guard));
            code(&mut highlighter)?;
            highlighter.finish()
        })
    }

    fn print_stream(&mut self, reader: &mut impl Read) -> io::Result<()> {
        self.buffer
            .with_guard(|mut guard| copy_largebuf(reader, &mut guard))
    }

    fn print_colorized_stream(
        &mut self,
        stream: &mut impl Read,
        syntax: &'static str,
    ) -> io::Result<()> {
        self.with_guarded_highlighter(syntax, |highlighter| {
            copy_largebuf(stream, &mut highlighter.linewise())?;
            Ok(())
        })
    }

    fn print_syntax_stream(
        &mut self,
        stream: &mut impl Read,
        syntax: &'static str,
    ) -> io::Result<()> {
        if self.color {
            self.print_colorized_stream(stream, syntax)
        } else {
            self.print_stream(stream)
        }
    }

    fn print_json_stream(&mut self, stream: &mut impl Read) -> io::Result<()> {
        if !self.indent_json {
            // We don't have to do anything specialized, so fall back to the generic version
            self.print_syntax_stream(stream, "json")
        } else if self.color {
            self.with_guarded_highlighter("json", |highlighter| {
                get_json_formatter().format_stream_unbuffered(stream, &mut highlighter.linewise())
            })
        } else {
            self.buffer.with_guard(|mut guard| {
                get_json_formatter().format_stream_unbuffered(stream, &mut guard)
            })
        }
    }

    fn print_body_stream(
        &mut self,
        content_type: Option<ContentType>,
        body: &mut impl Read,
    ) -> io::Result<()> {
        match content_type {
            Some(ContentType::Json) => self.print_json_stream(body),
            Some(ContentType::Xml) => self.print_syntax_stream(body, "xml"),
            Some(ContentType::Html) => self.print_syntax_stream(body, "html"),
            Some(ContentType::Css) => self.print_syntax_stream(body, "css"),
            // print_body_text() has fancy JSON detection, but we can't do that here
            Some(ContentType::JavaScript) => self.print_syntax_stream(body, "js"),
            _ => self.print_stream(body),
        }
    }

    fn print_headers(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.print_colorized_text(text, "http")
        } else {
            self.buffer.print(text)
        }
    }

    fn headers_to_string(&self, headers: &HeaderMap, sort: bool) -> String {
        let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
        if sort {
            headers.sort_by_key(|(name, _)| name.as_str());
        }

        let mut header_string = String::new();
        for (key, value) in headers {
            header_string.push_str(key.as_str());
            header_string.push_str(": ");
            match value.to_str() {
                Ok(value) => header_string.push_str(value),
                Err(_) => header_string.push_str(&format!("{:?}", value)),
            }
            header_string.push('\n');
        }
        header_string.pop();

        header_string
    }

    pub fn print_request_headers(&mut self, request: &Request) -> io::Result<()> {
        let method = request.method();
        let url = request.url();
        let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
        let version = reqwest::Version::HTTP_11;
        let mut headers = request.headers().clone();

        headers
            .entry(ACCEPT)
            .or_insert_with(|| HeaderValue::from_static("*/*"));

        // See https://github.com/seanmonstar/reqwest/issues/1030
        // reqwest and hyper add certain headers, but only in the process of
        // sending the request, which we haven't done yet
        if let Some(body) = request.body().and_then(|body| body.as_bytes()) {
            // Added at https://github.com/seanmonstar/reqwest/blob/e56bd160ba/src/blocking/request.rs#L132
            headers
                .entry(CONTENT_LENGTH)
                .or_insert_with(|| body.len().into());
        }
        if let Some(host) = request.url().host_str() {
            // This is incorrect in case of HTTP/2, but we're already assuming
            // HTTP/1.1 anyway
            headers.entry(HOST).or_insert_with(|| {
                // Added at https://github.com/hyperium/hyper/blob/dfa1bb291d/src/client/client.rs#L237
                if test_mode() {
                    HeaderValue::from_str("http.mock")
                } else if let Some(port) = request.url().port() {
                    HeaderValue::from_str(&format!("{}:{}", host, port))
                } else {
                    HeaderValue::from_str(host)
                }
                .expect("hostname should already be validated/parsed")
            });
        }

        let request_line = format!("{} {}{} {:?}\n", method, url.path(), query_string, version);
        let headers = &self.headers_to_string(&headers, self.sort_headers);

        self.print_headers(&(request_line + &headers))?;
        self.buffer.print("\n\n")?;
        Ok(())
    }

    pub fn print_response_headers(&mut self, response: &Response) -> io::Result<()> {
        let version = response.version();
        let status = response.status();
        let headers = response.headers();

        let status_line = format!("{:?} {}\n", version, status);
        let headers = self.headers_to_string(headers, self.sort_headers);

        self.print_headers(&(status_line + &headers))?;
        self.buffer.print("\n\n")?;
        Ok(())
    }

    pub fn print_request_body(&mut self, request: &Request) -> io::Result<()> {
        match get_content_type(&request.headers()) {
            Some(ContentType::Multipart) => {
                self.buffer.print(MULTIPART_SUPPRESSOR)?;
            }
            content_type => {
                if let Some(body) = request.body().and_then(|b| b.as_bytes()) {
                    if body.contains(&b'\0') {
                        self.buffer.print(BINARY_SUPPRESSOR)?;
                    } else {
                        self.print_body_text(content_type, &String::from_utf8_lossy(body))?;
                        self.buffer.print("\n")?;
                    }
                    // Breathing room between request and response
                    self.buffer.print("\n")?;
                }
            }
        }
        Ok(())
    }

    pub fn print_response_body(&mut self, mut response: Response) -> anyhow::Result<()> {
        let content_type = get_content_type(&response.headers());
        if !self.buffer.is_terminal() {
            // No trailing newlines, no decoding, direct streaming
            self.print_body_stream(content_type, &mut response)?;
        } else if self.stream {
            match self.print_body_stream(content_type, &mut decode_stream(&mut response)) {
                Ok(_) => {
                    self.buffer.print("\n")?;
                }
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                    if self.color {
                        self.buffer.print("\x1b[0m")?;
                    }
                    self.buffer.print(BINARY_SUPPRESSOR)?;
                }
                Err(err) => return Err(err.into()),
            }
        } else {
            // Note that .text() behaves like String::from_utf8_lossy()
            let text = response.text()?;
            if text.contains('\0') {
                self.buffer.print(BINARY_SUPPRESSOR)?;
                return Ok(());
            }
            self.print_body_text(content_type, &text)?;
            self.buffer.print("\n")?;
        }
        Ok(())
    }
}

/// Decode a streaming response in a way that matches `.text()`.
///
/// Note that in practice this seems to behave like String::from_utf8_lossy(),
/// but it makes no guarantees about outputting valid UTF-8 if the input is
/// invalid UTF-8 (claiming to be UTF-8). So only pass data through here
/// that's going to the terminal, and don't trust its output.
///
/// `reqwest` doesn't provide an API for this, so we have to roll our own. It
/// doesn't even provide an API to detect the response's encoding, so that
/// logic is copied here.
///
/// See https://github.com/seanmonstar/reqwest/blob/2940740493/src/async_impl/response.rs#L172
fn decode_stream(response: &mut Response) -> impl Read + '_ {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok());
    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
        .unwrap_or("utf-8");
    let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);

    DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding))
        .build(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{buffer::Buffer, cli::Cli, vec_of_strings};
    use assert_matches::assert_matches;

    fn run_cmd(args: impl IntoIterator<Item = String>, is_stdout_tty: bool) -> Printer {
        let args = Cli::from_iter_safe(args).unwrap();
        let buffer = Buffer::new(args.download, &args.output, is_stdout_tty).unwrap();
        Printer::new(args.pretty, args.theme, false, buffer)
    }

    fn temp_path(filename: &str) -> String {
        let mut dir = std::env::temp_dir();
        dir.push(filename);
        dir.to_str().unwrap().to_owned()
    }

    #[test]
    fn test_1() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stdout(..));
    }

    #[test]
    fn test_2() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], false);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::Redirect(..));
    }

    #[test]
    fn test_3() {
        let output = temp_path("temp3");
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-o", output], true);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn test_4() {
        let output = temp_path("temp4");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-o", output],
            false,
        );
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn test_5() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn test_6() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], false);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn test_7() {
        let output = temp_path("temp7");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn test_8() {
        let output = temp_path("temp8");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }
}
