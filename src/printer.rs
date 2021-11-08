use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};

use encoding_rs::{Encoding, UTF_8};
use encoding_rs_io::DecodeReaderBytesBuilder;
use mime::Mime;
use reqwest::blocking::{Request, Response};
use reqwest::cookie::CookieStore;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HOST,
};
use reqwest::Version;
use termcolor::WriteColor;

use crate::{
    buffer::Buffer,
    cli::{Pretty, Print, Theme},
    formatting::{get_json_formatter, Highlighter},
    utils::{copy_largebuf, test_mode, BUFFER_SIZE},
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
    pub print: Print,
    indent_json: bool,
    color: bool,
    theme: Theme,
    sort_headers: bool,
    stream: bool,
    buffer: Buffer,
}

impl Printer {
    pub fn new(
        print: Print,
        pretty: Pretty,
        theme: Option<Theme>,
        stream: bool,
        buffer: Buffer,
    ) -> Self {
        let theme = theme.unwrap_or(Theme::auto);

        Printer {
            print,
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

    fn headers_to_string(&self, headers: &HeaderMap, version: Version) -> String {
        let as_titlecase = match version {
            Version::HTTP_09 | Version::HTTP_10 | Version::HTTP_11 => true,
            Version::HTTP_2 | Version::HTTP_3 => false,
            _ => false,
        };
        let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
        if self.sort_headers {
            headers.sort_by_key(|(name, _)| name.as_str());
        }

        let mut header_string = String::new();
        for (key, value) in headers {
            if as_titlecase {
                // Ought to be equivalent to how hyper does it
                // https://github.com/hyperium/hyper/blob/f46b175bf71b202fbb907c4970b5743881b891e1/src/proto/h1/role.rs#L1332
                // Header names are ASCII so it's ok to operate on char instead of u8
                let mut prev = '-';
                for mut c in key.as_str().chars() {
                    if prev == '-' {
                        c.make_ascii_uppercase();
                    }
                    header_string.push(c);
                    prev = c;
                }
            } else {
                header_string.push_str(key.as_str());
            }
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

    // Each of the print_* functions adds an extra line separator at the end
    // except for print_response_body. We are using this function when we have
    // something to print after the response body.
    pub fn print_separator(&mut self) -> io::Result<()> {
        if self.print.response_body {
            self.buffer.print("\n")?;
        }
        Ok(())
    }

    pub fn print_request_headers<T>(&mut self, request: &Request, cookie_jar: &T) -> io::Result<()>
    where
        T: CookieStore,
    {
        if !self.print.request_headers {
            return Ok(());
        }

        let method = request.method();
        let url = request.url();
        let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
        let version = request.version();
        let mut headers = request.headers().clone();

        headers
            .entry(ACCEPT)
            .or_insert_with(|| HeaderValue::from_static("*/*"));

        if let Some(cookie) = cookie_jar.cookies(url) {
            headers.insert(COOKIE, cookie);
        }

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
        let headers = self.headers_to_string(&headers, version);

        self.print_headers(&(request_line + &headers))?;
        self.buffer.print("\n\n")?;
        Ok(())
    }

    pub fn print_response_headers(&mut self, response: &Response) -> io::Result<()> {
        if !self.print.response_headers {
            return Ok(());
        }

        let version = response.version();
        let status = response.status();
        let headers = response.headers();

        let status_line = format!("{:?} {}\n", version, status);
        let headers = self.headers_to_string(headers, version);

        self.print_headers(&(status_line + &headers))?;
        self.buffer.print("\n\n")?;
        Ok(())
    }

    pub fn print_request_body(&mut self, request: &mut Request) -> anyhow::Result<()> {
        if !self.print.request_body {
            return Ok(());
        }

        let content_type = get_content_type(request.headers());
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

    pub fn print_response_body(
        &mut self,
        mut response: Response,
        encoding: Option<&'static Encoding>,
        mime: Option<&str>,
    ) -> anyhow::Result<()> {
        if !self.print.response_body {
            return Ok(());
        }

        let content_type = mime
            .map(ContentType::from)
            .unwrap_or_else(|| get_content_type(response.headers()));
        let encoding = encoding.unwrap_or_else(|| guess_encoding(&response));

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
                    self.print_body_stream(
                        content_type,
                        &mut decode_stream(&mut response, encoding),
                    )?;
                } else {
                    let bytes = response.bytes()?;
                    let (text, _, _) = encoding.decode(&bytes);
                    self.print_body_text(content_type, &text)?;
                }
            } else if self.stream {
                copy_largebuf(&mut response, &mut self.buffer)?;
            } else {
                let body = response.bytes()?;
                self.buffer.print(&body)?;
            }
        } else if self.stream {
            match self.print_body_stream(content_type, &mut decode_stream(&mut response, encoding))
            {
                Ok(_) => {
                    self.buffer.print("\n")?;
                }
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                    self.buffer.print(BINARY_SUPPRESSOR)?;
                }
                Err(err) => return Err(err.into()),
            }
        } else {
            // Note that .decode() behaves like String::from_utf8_lossy()
            let bytes = response.bytes()?;
            let (text, _, _) = encoding.decode(&bytes);
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

pub enum ContentType {
    Json,
    Html,
    Xml,
    JavaScript,
    Css,
    Text,
    UrlencodedForm,
    Multipart,
    Unknown,
}

impl ContentType {
    pub fn is_text(&self) -> bool {
        !matches!(
            self,
            ContentType::Unknown | ContentType::UrlencodedForm | ContentType::Multipart
        )
    }
}

impl From<&str> for ContentType {
    fn from(content_type: &str) -> Self {
        if content_type.contains("json") {
            ContentType::Json
        } else if content_type.contains("html") {
            ContentType::Html
        } else if content_type.contains("xml") {
            ContentType::Xml
        } else if content_type.contains("multipart") {
            ContentType::Multipart
        } else if content_type.contains("x-www-form-urlencoded") {
            ContentType::UrlencodedForm
        } else if content_type.contains("javascript") {
            ContentType::JavaScript
        } else if content_type.contains("css") {
            ContentType::Css
        } else if content_type.contains("text") {
            // We later check if this one's JSON
            // HTTPie checks for "json", "javascript" and "text" in one place:
            // https://github.com/httpie/httpie/blob/a32ad344dd/httpie/output/formatters/json.py#L14
            // We have it more spread out but it behaves more or less the same
            ContentType::Text
        } else {
            ContentType::Unknown
        }
    }
}

pub fn get_content_type(headers: &HeaderMap) -> ContentType {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ContentType::from)
        .unwrap_or(ContentType::Unknown)
}

pub fn valid_json(text: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
}

/// Decode a streaming response in a way that matches `.text()`.
///
/// Note that in practice this seems to behave like String::from_utf8_lossy(),
/// but it makes no guarantees about outputting valid UTF-8 if the input is
/// invalid UTF-8 (claiming to be UTF-8). So only pass data through here
/// that's going to the terminal, and don't trust its output.
fn decode_stream<'a>(response: &'a mut Response, encoding: &'static Encoding) -> impl Read + 'a {
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
    use indoc::indoc;

    use super::*;
    use crate::utils::random_string;
    use crate::{buffer::Buffer, cli::Cli, vec_of_strings};
    use assert_matches::assert_matches;

    fn run_cmd(args: impl IntoIterator<Item = String>, is_stdout_tty: bool) -> Printer {
        let args = Cli::from_iter_safe(args).unwrap();
        let buffer =
            Buffer::new(args.download, args.output.as_deref(), is_stdout_tty, None).unwrap();
        let pretty = args.pretty.unwrap_or_else(|| buffer.guess_pretty());
        Printer::new("hHbB".parse().unwrap(), pretty, args.style, false, buffer)
    }

    fn temp_path() -> String {
        let mut dir = std::env::temp_dir();
        let filename = random_string();
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
        let output = temp_path();
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-o", output], true);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn redirect_mode_with_output_file() {
        let output = temp_path();
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
        let output = temp_path();
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn redirect_mode_download_with_output_file() {
        let output = temp_path();
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr(..));
    }

    #[test]
    fn test_header_casing() {
        let p = Printer {
            print: "hHbB".parse().unwrap(),
            indent_json: false,
            color: false,
            theme: Theme::auto,
            sort_headers: false,
            stream: false,
            buffer: Buffer::new(false, None, false, Some(Pretty::none)).unwrap(),
        };

        let mut headers = HeaderMap::new();
        headers.insert("ab-cd", "0".parse().unwrap());
        headers.insert("-cd", "0".parse().unwrap());
        headers.insert("-", "0".parse().unwrap());
        headers.insert("ab-%c", "0".parse().unwrap());
        headers.insert("A-b--C", "0".parse().unwrap());

        assert_eq!(
            p.headers_to_string(&headers, reqwest::Version::HTTP_11),
            indoc! {"
                Ab-Cd: 0
                -Cd: 0
                -: 0
                Ab-%c: 0
                A-B--C: 0"
            }
        );

        assert_eq!(
            p.headers_to_string(&headers, reqwest::Version::HTTP_2),
            indoc! {"
                ab-cd: 0
                -cd: 0
                -: 0
                ab-%c: 0
                a-b--c: 0"
            }
        );
    }
}
