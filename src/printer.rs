use std::io::{self, Write};

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, HOST};
use reqwest::{Request, Response};

use crate::utils::{colorize, get_content_type, indent_json, test_mode, ContentType};
use crate::{Buffer, Pretty, Theme};

const MULTIPART_SUPPRESSOR: &str = concat!(
    "+--------------------------------------------+\n",
    "| NOTE: multipart data not shown in terminal |\n",
    "+--------------------------------------------+"
);

const BINARY_SUPPRESSOR: &str = concat!(
    "+-----------------------------------------+\n",
    "| NOTE: binary data not shown in terminal |\n",
    "+-----------------------------------------+"
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
            indent_json: matches!(pretty, Pretty::all | Pretty::format),
            sort_headers: matches!(pretty, Pretty::all | Pretty::format),
            color: matches!(pretty, Pretty::all | Pretty::colors),
            stream: matches!(pretty, Pretty::none) || stream,
            theme,
            buffer,
        }
    }

    fn print_json(&mut self, text: &str) -> io::Result<()> {
        // This code is a little thorny because of ownership issues.
        // We have to keep the indented text alive until the end of the function.
        let indent_result = match self.indent_json {
            true => Some(indent_json(text)),
            false => None,
        };
        let text = match &indent_result {
            Some(Ok(result)) => result.as_str(),
            _ => text,
        };
        if self.color {
            colorize(text, "json", &self.theme, &mut self.buffer)
        } else {
            self.buffer.print(text)
        }
    }

    fn print_xml(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            colorize(text, "xml", &self.theme, &mut self.buffer)
        } else {
            self.buffer.print(text)
        }
    }

    fn print_html(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            colorize(text, "html", &self.theme, &mut self.buffer)
        } else {
            self.buffer.print(text)
        }
    }

    fn print_headers(&mut self, text: &str) -> io::Result<()> {
        if self.color {
            colorize(text, "http", &self.theme, &mut self.buffer)
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

    pub fn print_request_body(&mut self, request: &Request) -> io::Result<()> {
        let get_body = || {
            request
                .body()
                .and_then(|b| b.as_bytes())
                .and_then(|b| String::from_utf8(b.into()).ok())
        };

        match get_content_type(&request.headers()) {
            Some(ContentType::Multipart) => {
                self.buffer.print(MULTIPART_SUPPRESSOR)?;
                self.buffer.print("\n\n")?;
            }
            Some(ContentType::Json) => {
                if let Some(body) = get_body() {
                    self.print_json(&body)?;
                    self.buffer.print("\n\n")?;
                }
            }
            _ => {
                if let Some(body) = get_body() {
                    self.buffer.print(&body)?;
                    self.buffer.print("\n\n")?;
                }
            }
        };
        Ok(())
    }

    pub async fn print_response_body(&mut self, mut response: Response) -> Result<()> {
        match get_content_type(&response.headers()) {
            Some(ContentType::Json) if self.stream => {
                while let Some(bytes) = response.chunk().await? {
                    self.print_json(&String::from_utf8_lossy(&bytes))?;
                }
            }
            Some(ContentType::Xml) if self.stream => {
                while let Some(bytes) = response.chunk().await? {
                    self.print_xml(&String::from_utf8_lossy(&bytes))?;
                }
            }
            Some(ContentType::Html) if self.stream => {
                while let Some(bytes) = response.chunk().await? {
                    self.print_html(&String::from_utf8_lossy(&bytes))?;
                }
            }
            Some(ContentType::Json) => self.print_json(&response.text().await?)?,
            Some(ContentType::Xml) => self.print_xml(&response.text().await?)?,
            Some(ContentType::Html) => self.print_html(&response.text().await?)?,
            _ => {
                let mut is_first_chunk = true;
                while let Some(bytes) = response.chunk().await? {
                    if is_first_chunk
                        && matches!(self.buffer, Buffer::Stdout | Buffer::Stderr)
                        && bytes.contains(&b'\0')
                    {
                        self.buffer.print(BINARY_SUPPRESSOR)?;
                        break;
                    }
                    is_first_chunk = false;

                    self.buffer.write_all(&bytes)?;
                }
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cli::Cli, vec_of_strings};
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
        assert_matches!(p.buffer, Buffer::Stdout);
    }

    #[test]
    fn test_2() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get"], false);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::Redirect);
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
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_6() {
        let p = run_cmd(vec_of_strings!["xh", "httpbin.org/get", "-d"], false);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_7() {
        let output = temp_path("temp7");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_8() {
        let output = temp_path("temp8");
        let p = run_cmd(
            vec_of_strings!["xh", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }
}
