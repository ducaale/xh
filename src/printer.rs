use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};

use encoding_rs::{Encoding, UTF_8};
use encoding_rs_io::DecodeReaderBytesBuilder;
use mime::Mime;
use reqwest::blocking::{Request, Response};
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, HOST,
};
use termcolor::WriteColor;

use crate::{
    buffer::Buffer,
    cli::{Pretty, Theme},
    formatting::{get_json_formatter, Highlighter},
    utils::{copy_largebuf, get_content_type, test_mode, valid_json, ContentType, BUFFER_SIZE},
};

const BINARY_SUPPRESSOR: &str = concat!(
    "+-----------------------------------------+\n",
    "| NOTE: binary data not shown in terminal |\n",
    "+-----------------------------------------+\n",
    "\n"
);

/// A wrapper around a reader that reads line by line, (optionally) returning
/// an error if the line appears to be binary.
///
/// This is meant for streaming output. `checked` should typically be
/// set to buffer.is_terminal(), but if you need neither checking nor
/// highlighting then you may not need a `BinaryGuard` at all.
///
/// This reader does not validate UTF-8.
struct BinaryGuard<'a, T: Read> {
    reader: BufReader<&'a mut T>,
    buffer: Vec<u8>,
    checked: bool,
}

impl<'a, T: Read> BinaryGuard<'a, T> {
    fn new(reader: &'a mut T, checked: bool) -> Self {
        Self {
            reader: BufReader::with_capacity(BUFFER_SIZE, reader),
            buffer: Vec::new(),
            checked,
        }
    }

    fn read_line(&mut self) -> io::Result<Option<&[u8]>> {
        self.buffer.clear();
        self.reader.read_until(b'\n', &mut self.buffer)?;
        if self.buffer.is_empty() {
            return Ok(None);
        }
        if self.checked && self.buffer.contains(&b'\0') {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Found binary data",
            ))
        } else {
            Ok(Some(&self.buffer))
        }
    }
}

pub struct Printer {
    indent_json: bool,
    color: bool,
    theme: Theme,
    sort_headers: bool,
    stream: bool,
    buffer: Buffer,
}

impl Printer {
    pub fn new(pretty: Pretty, theme: Option<Theme>, stream: bool, buffer: Buffer) -> Self {
        let theme = theme.unwrap_or(Theme::auto);

        Printer {
            indent_json: pretty.format(),
            sort_headers: pretty.format(),
            color: pretty.color() && (cfg!(test) || buffer.supports_color()),
            stream,
            theme,
            buffer,
        }
    }

    fn get_highlighter(&mut self, syntax: &'static str) -> Highlighter<'_> {
        Highlighter::new(syntax, self.theme, &mut self.buffer)
    }

    fn print_colorized_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        // This could perhaps be optimized
        // syntect processes the whole buffer at once, doing it line by line might
        // let us start printing earlier (but can decrease quality since regexes
        // can't look ahead)
        // A buffered writer could improve performance, but we'd have to use a
        // BufferedStandardStream instead of a StandardStream, which is slightly tricky
        // (wrapping a BufWriter around a Buffer wouldn't preserve syntax coloring)
        self.get_highlighter(syntax).highlight(text)
    }

    fn print_syntax_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        if self.color {
            self.print_colorized_text(text, syntax)
        } else {
            self.buffer.print(text)
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
            get_json_formatter().format_buf(text.as_bytes(), &mut buf)?;
            // in principle, buf should already be valid UTF-8,
            // because JSONXF doesn't mangle it
            let text = String::from_utf8_lossy(&buf);
            self.print_colorized_text(&text, "json")
        } else {
            let mut out = BufWriter::new(&mut self.buffer);
            get_json_formatter().format_buf(text.as_bytes(), &mut out)?;
            out.flush()
        }
    }

    fn print_body_text(&mut self, content_type: ContentType, body: &str) -> io::Result<()> {
        match content_type {
            ContentType::Json => self.print_json_text(body, true),
            ContentType::Xml => self.print_syntax_text(body, "xml"),
            ContentType::Html => self.print_syntax_text(body, "html"),
            ContentType::Css => self.print_syntax_text(body, "css"),
            // In HTTPie part of this behavior is gated behind the --json flag
            // But it does JSON formatting even without that flag, so doing
            // this check unconditionally is fine
            ContentType::Text | ContentType::JavaScript if valid_json(body) => {
                self.print_json_text(body, false)
            }
            ContentType::JavaScript => self.print_syntax_text(body, "js"),
            _ => self.buffer.print(body),
        }
    }

    fn print_stream(&mut self, reader: &mut impl Read) -> io::Result<()> {
        if !self.buffer.is_terminal() {
            return copy_largebuf(reader, &mut self.buffer);
        }
        let mut guard = BinaryGuard::new(reader, true);
        while let Some(line) = guard.read_line()? {
            self.buffer.print(line)?;
        }
        Ok(())
    }

    fn print_colorized_stream(
        &mut self,
        stream: &mut impl Read,
        syntax: &'static str,
    ) -> io::Result<()> {
        let mut guard = BinaryGuard::new(stream, self.buffer.is_terminal());
        let mut highlighter = self.get_highlighter(syntax);
        while let Some(line) = guard.read_line()? {
            highlighter.highlight_bytes(line)?;
        }
        Ok(())
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
            let mut guard = BinaryGuard::new(stream, self.buffer.is_terminal());
            let mut formatter = get_json_formatter();
            let mut highlighter = self.get_highlighter("json");
            let mut buf = Vec::new();
            while let Some(line) = guard.read_line()? {
                formatter.format_buf(line, &mut buf)?;
                highlighter.highlight_bytes(&buf)?;
                buf.clear();
            }
            Ok(())
        } else {
            let mut formatter = get_json_formatter();
            if !self.buffer.is_terminal() {
                return formatter.format_stream_unbuffered(stream, &mut self.buffer);
            }
            let mut guard = BinaryGuard::new(stream, true);
            while let Some(line) = guard.read_line()? {
                formatter.format_buf(line, &mut self.buffer)?;
            }
            Ok(())
        }
    }

    fn print_body_stream(
        &mut self,
        content_type: ContentType,
        body: &mut impl Read,
    ) -> io::Result<()> {
        match content_type {
            ContentType::Json => self.print_json_stream(body),
            ContentType::Xml => self.print_syntax_stream(body, "xml"),
            ContentType::Html => self.print_syntax_stream(body, "html"),
            ContentType::Css => self.print_syntax_stream(body, "css"),
            // print_body_text() has fancy JSON detection, but we can't do that here
            ContentType::JavaScript => self.print_syntax_stream(body, "js"),
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
        let version = request.version();
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

    pub fn print_request_body(&mut self, request: &mut Request) -> anyhow::Result<()> {
        let content_type = get_content_type(&request.headers());
        if let Some(body) = request.body_mut() {
            let body = body.buffer()?;
            if body.contains(&b'\0') {
                self.buffer.print(BINARY_SUPPRESSOR)?;
            } else {
                self.print_body_text(content_type, &String::from_utf8_lossy(body))?;
                self.buffer.print("\n")?;
            }
            // Breathing room between request and response
            self.buffer.print("\n")?;
        }
        Ok(())
    }

    pub fn print_response_body(&mut self, mut response: Response) -> anyhow::Result<()> {
        let content_type = get_content_type(&response.headers());
        if !self.buffer.is_terminal() {
            if (self.color || self.indent_json) && content_type.is_text() {
                // The user explicitly asked for formatting even though this is
                // going into a file, and the response is at least supposed to be
                // text, so decode it

                // TODO: HTTPie re-encodes output in the original encoding, we don't
                // encoding_rs::Encoder::encode_from_utf8_to_vec_without_replacement()
                // and guess_encoding() may help, but it'll require refactoring

                // The current design is a bit unfortunate because there's no way to
                // force UTF-8 output without coloring or formatting
                // Unconditionally decoding is not an option because the body
                // might not be text at all
                if self.stream {
                    self.print_body_stream(content_type, &mut decode_stream(&mut response))?;
                } else {
                    let text = response.text()?;
                    self.print_body_text(content_type, &text)?;
                }
            } else if self.stream {
                copy_largebuf(&mut response, &mut self.buffer)?;
            } else {
                let body = response.bytes()?;
                self.buffer.print(&body)?;
            }
        } else if self.stream {
            match self.print_body_stream(content_type, &mut decode_stream(&mut response)) {
                Ok(_) => {
                    self.buffer.print("\n")?;
                }
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {
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
fn decode_stream(response: &mut Response) -> impl Read + '_ {
    let encoding = guess_encoding(response);

    DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding))
        .build(response)
}

/// Guess the response's encoding, with UTF-8 as the default.
///
/// reqwest doesn't provide an API for this, so the logic is copied here.
///
/// See https://github.com/seanmonstar/reqwest/blob/2940740493/src/async_impl/response.rs#L172
fn guess_encoding(response: &Response) -> &'static Encoding {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok());
    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
        .unwrap_or("utf-8");
    Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{buffer::Buffer, cli::Cli, vec_of_strings};
    use assert_matches::assert_matches;

    fn run_cmd(args: impl IntoIterator<Item = String>, is_stdout_tty: bool) -> Printer {
        let args = Cli::from_iter_safe(args).unwrap();
        let buffer =
            Buffer::new(args.download, args.output.as_deref(), is_stdout_tty, None).unwrap();
        let pretty = args.pretty.unwrap_or_else(|| buffer.guess_pretty());
        Printer::new(pretty, args.style, false, buffer)
    }

    fn temp_path(filename: &str) -> String {
        let mut dir = std::env::temp_dir();
        dir.push(filename);
        dir.to_str().unwrap().to_owned()
    }

    #[test]
    fn terminal_mode() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stdout(..));
    }

    #[test]
    fn redirect_mode() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], false);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::Redirect(..));
    }

    #[test]
    fn terminal_mode_with_output_file() {
        let output = temp_path("temp3");
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-o", output], true);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn redirect_mode_with_output_file() {
        let output = temp_path("temp4");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-o", output],
            false,
        );
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn terminal_mode_download() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn redirect_mode_download() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], false);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn terminal_mode_download_with_output_file() {
        let output = temp_path("temp7");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn redirect_mode_download_with_output_file() {
        let output = temp_path("temp8");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }
}
