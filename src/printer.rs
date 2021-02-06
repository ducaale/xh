use std::fmt::Write;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH};
use reqwest::{Request, Response};

use crate::utils::{colorize, get_content_type, indent_json, ContentType};
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
        let pretty = pretty.unwrap_or(Pretty::from(&buffer));
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

    fn print_json(&mut self, text: &str) {
        match (self.indent_json, self.color) {
            (true, true) => colorize(&indent_json(text), "json", &self.theme)
                .for_each(|line| self.buffer.write(&line)),
            (false, true) => {
                colorize(text, "json", &self.theme).for_each(|line| self.buffer.write(&line));
            }
            (true, false) => self.buffer.write(&indent_json(text)),
            (false, false) => self.buffer.write(text),
        }
    }

    fn print_xml(&mut self, text: &str) {
        if self.color {
            colorize(text, "xml", &self.theme).for_each(|line| self.buffer.write(&line));
        } else {
            self.buffer.write(text);
        }
    }

    fn print_html(&mut self, text: &str) {
        if self.color {
            colorize(text, "html", &self.theme).for_each(|line| self.buffer.write(&line));
        } else {
            self.buffer.write(text);
        }
    }

    fn print_headers(&mut self, text: &str) {
        if self.color {
            colorize(text, "http", &self.theme).for_each(|line| self.buffer.write(&line));
        } else {
            self.buffer.write(text);
        }
    }

    fn headers_to_string(&self, headers: &HeaderMap, sort: bool) -> String {
        let mut headers: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
        if sort {
            headers.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()))
        }

        let mut header_string = String::new();
        for (key, value) in headers {
            let key = key.to_string();
            let value = value.to_str().unwrap();
            writeln!(&mut header_string, "{}: {}", key, value).unwrap();
        }
        header_string.pop();

        header_string
    }

    pub fn print_request_headers(&mut self, request: &Request) {
        let method = request.method();
        let url = request.url();
        let query_string = url.query().map_or(String::from(""), |q| ["?", q].concat());
        let version = reqwest::Version::HTTP_11;
        let mut headers = request.headers().clone();

        // See https://github.com/seanmonstar/reqwest/issues/1030
        if let Some(body) = request.body().and_then(|body| body.as_bytes()) {
            let content_length = HeaderValue::from_str(&body.len().to_string()).unwrap();
            headers.insert(CONTENT_LENGTH, content_length);
        }

        let request_line = format!("{} {}{} {:?}\n", method, url.path(), query_string, version);
        let headers = &self.headers_to_string(&headers, self.sort_headers);

        self.print_headers(&(request_line + &headers));
        self.buffer.write("\n\n");
    }

    pub fn print_response_headers(&mut self, response: &Response) {
        let version = response.version();
        let status = response.status();
        let headers = response.headers();

        let status_line = format!(
            "{:?} {} {}\n",
            version,
            status.as_str(),
            status.canonical_reason().unwrap()
        );
        let headers = self.headers_to_string(headers, self.sort_headers);

        self.print_headers(&(status_line + &headers));
        self.buffer.write("\n\n");
    }

    pub fn print_request_body(&mut self, request: &Request) {
        let get_body = || {
            request
                .body()
                .and_then(|b| b.as_bytes())
                .and_then(|b| String::from_utf8(b.into()).ok())
        };

        match get_content_type(&request.headers()) {
            Some(ContentType::Multipart) => {
                self.buffer.write(MULTIPART_SUPPRESSOR);
                self.buffer.write("\n\n");
            }
            Some(ContentType::Json) => {
                if let Some(body) = get_body() {
                    self.print_json(&body);
                    self.buffer.write("\n\n");
                }
            }
            Some(ContentType::UrlencodedForm) | _ => {
                if let Some(body) = get_body() {
                    self.buffer.write(&body);
                    self.buffer.write("\n\n");
                }
            }
        };
    }

    pub async fn print_response_body(&mut self, mut response: Response) {
        match get_content_type(&response.headers()) {
            Some(ContentType::Json) if self.stream => {
                while let Some(bytes) = response.chunk().await.unwrap() {
                    self.print_json(&String::from_utf8_lossy(&bytes));
                    self.buffer.write("\n");
                }
            }
            Some(ContentType::Xml) if self.stream => {
                while let Some(bytes) = response.chunk().await.unwrap() {
                    self.print_xml(&String::from_utf8_lossy(&bytes));
                }
            }
            Some(ContentType::Html) if self.stream => {
                while let Some(bytes) = response.chunk().await.unwrap() {
                    self.print_html(&String::from_utf8_lossy(&bytes));
                }
            }
            Some(ContentType::Json) => self.print_json(&response.text().await.unwrap()),
            Some(ContentType::Xml) => self.print_xml(&response.text().await.unwrap()),
            Some(ContentType::Html) => self.print_html(&response.text().await.unwrap()),
            _ => {
                let mut is_first_chunk = true;
                while let Some(bytes) = response.chunk().await.unwrap() {
                    if is_first_chunk
                        && matches!(self.buffer, Buffer::Stdout | Buffer::Stderr)
                        && bytes.contains(&b'\0')
                    {
                        self.buffer.write(BINARY_SUPPRESSOR);
                        break;
                    }
                    is_first_chunk = false;

                    self.buffer.write_bytes(&bytes);
                }
            }
        };
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{cli::Cli, vec_of_strings};
    use assert_matches::assert_matches;

    fn run_cmd(args: impl IntoIterator<Item = String>, is_stdout_tty: bool) -> Printer {
        let args = Cli::from_iter(args);
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
        let p = run_cmd(vec_of_strings!["ht", "httpbin.org/get"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stdout);
    }

    #[test]
    fn test_2() {
        let p = run_cmd(vec_of_strings!["ht", "httpbin.org/get"], false);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::Redirect);
    }

    #[test]
    fn test_3() {
        let output = temp_path("temp3");
        let p = run_cmd(vec_of_strings!["ht", "httpbin.org/get", "-o", output], true);
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn test_4() {
        let output = temp_path("temp4");
        let p = run_cmd(
            vec_of_strings!["ht", "httpbin.org/get", "-o", output],
            false,
        );
        assert_eq!(p.color, false);
        assert_matches!(p.buffer, Buffer::File(_));
    }

    #[test]
    fn test_5() {
        let p = run_cmd(vec_of_strings!["ht", "httpbin.org/get", "-d"], true);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_6() {
        let p = run_cmd(vec_of_strings!["ht", "httpbin.org/get", "-d"], false);
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_7() {
        let output = temp_path("temp7");
        let p = run_cmd(
            vec_of_strings!["ht", "httpbin.org/get", "-d", "-o", output],
            true,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }

    #[test]
    fn test_8() {
        let output = temp_path("temp8");
        let p = run_cmd(
            vec_of_strings!["ht", "httpbin.org/get", "-d", "-o", output],
            false,
        );
        assert_eq!(p.color, true);
        assert_matches!(p.buffer, Buffer::Stderr);
    }
}
