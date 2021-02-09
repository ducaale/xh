use std::io;

use reqwest::blocking::{Request, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, HOST};

use crate::utils::{get_content_type, get_json_formatter, test_mode, ContentType, Highlighter};
use crate::{Buffer, Pretty, Theme};

const MULTIPART_SUPPRESSOR: &str = concat!(
    "+--------------------------------------------+\n",
    "| NOTE: multipart data not shown in terminal |\n",
    "+--------------------------------------------+\n",
    "\n"
);

pub(crate) const BINARY_SUPPRESSOR: &str = concat!(
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
        let theme = theme.unwrap_or(Theme::Auto);

        Printer {
            indent_json: matches!(pretty, Pretty::All | Pretty::Format),
            sort_headers: matches!(pretty, Pretty::All | Pretty::Format),
            color: matches!(pretty, Pretty::All | Pretty::Colors),
            stream: matches!(pretty, Pretty::None) || stream,
            theme,
            buffer,
        }
    }

    fn get_highlighter(&mut self, syntax: &'static str) -> Highlighter {
        Highlighter::new(syntax, self.theme, &mut self.buffer)
    }

    fn colorize_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        self.get_highlighter(syntax).highlight(text)
    }

    fn dump(&mut self, reader: &mut Response) -> io::Result<()> {
        io::copy(reader, &mut self.buffer)?;
        Ok(())
    }

    fn print_json_text(&mut self, text: &str) -> io::Result<()> {
        if !self.indent_json {
            self.print_syntax_text(text, "json")
        } else if self.color {
            let mut buf = Vec::new();
            get_json_formatter().format_stream(&mut text.as_bytes(), &mut buf)?;
            let text = String::from_utf8_lossy(&buf);
            self.colorize_text(&text, "json")
        } else {
            get_json_formatter().format_stream(&mut text.as_bytes(), &mut self.buffer)
        }
    }

    fn print_json_stream(&mut self, stream: &mut Response) -> io::Result<()> {
        if !self.indent_json {
            self.print_syntax_stream(stream, "json")
        } else if self.color {
            let mut highlighter = self.get_highlighter("json");
            get_json_formatter().format_stream(stream, &mut highlighter.linewise())?;
            highlighter.finish()
        } else {
            get_json_formatter().format_stream(stream, &mut self.buffer)
        }
    }

    fn print_syntax_text(&mut self, text: &str, syntax: &'static str) -> io::Result<()> {
        if self.color {
            self.colorize_text(text, syntax)
        } else {
            self.buffer.print(text)
        }
    }

    fn print_syntax_stream(
        &mut self,
        stream: &mut Response,
        syntax: &'static str,
    ) -> io::Result<()> {
        if self.color {
            let mut highlighter = self.get_highlighter(syntax);
            io::copy(stream, &mut highlighter.linewise())?;
            highlighter.finish()
        } else {
            self.dump(stream)
        }
    }

    fn print_headers(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            self.colorize_text(text, "http")
        } else {
            self.buffer.print(text)
        }
    }

    fn headers_to_string(&self, headers: &HeaderMap, sort: bool) -> String {
        let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
        if sort {
            headers.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()))
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

    fn print_body_text(&mut self, content_type: Option<ContentType>, body: &str) -> io::Result<()> {
        match content_type {
            Some(ContentType::Json) => self.print_json_text(body),
            Some(ContentType::Xml) => self.print_syntax_text(body, "xml"),
            Some(ContentType::Html) => self.print_syntax_text(body, "html"),
            _ => self.buffer.print(body),
        }
    }

    fn print_body_stream(
        &mut self,
        content_type: Option<ContentType>,
        body: &mut Response,
    ) -> io::Result<()> {
        match content_type {
            Some(ContentType::Json) => self.print_json_stream(body),
            Some(ContentType::Xml) => self.print_syntax_stream(body, "xml"),
            Some(ContentType::Html) => self.print_syntax_stream(body, "html"),
            _ => self.dump(body),
        }
    }

    pub fn print_request_body(&mut self, request: &Request) -> io::Result<()> {
        match get_content_type(&request.headers()) {
            Some(ContentType::Multipart) => {
                self.buffer.print(MULTIPART_SUPPRESSOR)?;
            }
            // TODO: Should this print BINARY_SUPPRESSOR?
            content_type => {
                if let Some(body) = request
                    .body()
                    .and_then(|b| b.as_bytes())
                    .filter(|b| !b.contains(&b'\0'))
                    .and_then(|b| String::from_utf8(b.into()).ok())
                {
                    self.print_body_text(content_type, &body)?;
                    self.buffer.print("\n\n")?;
                }
            }
        }
        Ok(())
    }

    pub fn print_response_body(&mut self, mut response: Response) -> anyhow::Result<()> {
        if self.stream {
            self.print_body_stream(get_content_type(&response.headers()), &mut response)?;
            self.buffer.print("\n")?;
        } else {
            let content_type = get_content_type(&response.headers());
            let text = match response.text() {
                Ok(text) => text,
                Err(err) if err.is_decode() => {
                    self.buffer.print(BINARY_SUPPRESSOR)?;
                    return Ok(());
                }
                Err(err) => return Err(err.into()),
            };
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{buffer::BufferKind, cli::Cli, vec_of_strings};
    use assert_matches::assert_matches;

    fn run_cmd(args: impl IntoIterator<Item = String>, is_stdout_tty: bool) -> Printer {
        let args = Cli::from_iter(args).unwrap();
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
        assert_matches!(p.buffer.kind, BufferKind::Stdout);
    }

    #[test]
    fn test_2() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], false);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer.kind, BufferKind::Redirect);
    }

    #[test]
    fn test_3() {
        let output = temp_path("temp3");
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-o", output], true);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer.kind, BufferKind::File(_));
    }

    #[test]
    fn test_4() {
        let output = temp_path("temp4");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-o", output],
            false,
        );
        assert_eq!(p.color, false);
        assert_matches!(p.buffer.kind, BufferKind::File(_));
    }

    #[test]
    fn test_5() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer.kind, BufferKind::Stderr);
    }

    #[test]
    fn test_6() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], false);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer.kind, BufferKind::Stderr);
    }

    #[test]
    fn test_7() {
        let output = temp_path("temp7");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer.kind, BufferKind::Stderr);
    }

    #[test]
    fn test_8() {
        let output = temp_path("temp8");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer.kind, BufferKind::Stderr);
    }
}
