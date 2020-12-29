use std::fmt::Write as FmtWrite;
use std::io::Write as IoWrite;

use atty::Stream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH};
use reqwest::{Request, Response};

use crate::utils::{colorize, get_content_type, indent_json, ContentType};
use crate::{Pretty, Theme};


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
    buffer: Box<dyn IoWrite>
}

impl Printer {
    pub fn new(pretty: Option<Pretty>, theme: Option<Theme>, output: &Option<String>) -> Printer {
        let pretty = pretty.unwrap_or(if atty::isnt(Stream::Stdout) {
            Pretty::None
        } else if output.is_some() {
            Pretty::None
        } else {
            Pretty::All
        });
        let theme = theme.unwrap_or(Theme::Auto);
        let buffer: Box<dyn std::io::Write> = if let Some(output) = output {
            Box::new(std::fs::File::create(output).unwrap())
        } else {
            Box::new(std::io::stdout())
        };

        match pretty {
            Pretty::All => Printer {
                indent_json: true,
                color: true,
                theme: theme,
                sort_headers: true,
                buffer
            },
            Pretty::Colors => Printer {
                indent_json: false,
                color: true,
                theme: theme,
                sort_headers: false,
                buffer
            },
            Pretty::Format => Printer {
                indent_json: true,
                color: false,
                theme: theme,
                sort_headers: true,
                buffer
            },
            Pretty::None => Printer {
                indent_json: false,
                color: false,
                theme: theme,
                sort_headers: false,
                buffer
            },
        }
    }

    fn print_json(&mut self, text: &str) -> std::io::Result<()> {
        match (self.indent_json, self.color) {
            (true, true) => {
                for line in colorize(&indent_json(text), "json", &self.theme) {
                    write!(self.buffer, "{}", line)?;
                }
                write!(self.buffer, "\x1b[0m")?;
            }
            (false, true) => {
                for line in colorize(text, "json", &self.theme) {
                    write!(self.buffer, "{}", line)?;
                }
                write!(self.buffer, "\x1b[0m")?;
            }
            (true, false) => write!(self.buffer, "{}", indent_json(text))?,
            (false, false) => write!(self.buffer, "{}", text)?,
        }
        Ok(())
    }

    fn print_xml(&mut self, text: &str) -> std::io::Result<()> {
        if self.color {
            for line in colorize(text, "xml", &self.theme) {
                write!(self.buffer, "{}", line)?;
            }
            write!(self.buffer, "\x1b[0m")?;
        } else {
            write!(self.buffer, "{}", text)?;
        }
        Ok(())
    }

    fn print_html(&mut self, text: &str) -> std::io::Result<()> {
        if self.color {
            for line in colorize(text, "html", &self.theme) {
                write!(self.buffer, "{}", line)?;
            }
            write!(self.buffer, "\x1b[0m")?;
        } else {
            write!(self.buffer, "{}", text)?;
        }
        Ok(())
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

        if self.color {
            colorize(&(request_line + &headers), "http", &self.theme)
                .for_each(|line| write!(self.buffer, "{}", line).unwrap());
            write!(self.buffer, "\x1b[0m").unwrap();
        } else {
            write!(self.buffer, "{}", &(request_line + &headers)).unwrap();
        }

        write!(self.buffer, "\n\n").unwrap();
    }

    pub fn print_response_headers(&mut self, response: &Response) -> std::io::Result<()> {
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

        if self.color {
            for line in colorize(&(status_line + &headers), "http", &self.theme) {
                write!(self.buffer, "{}", line)?
            }
            write!(self.buffer, "\x1b[0m").unwrap();
        } else {
            write!(self.buffer, "{}", &(status_line + &headers))?;
        }

        write!(self.buffer, "\n\n")?;
        Ok(())
    }

    pub fn print_request_body(&mut self, request: &Request) -> std::io::Result<()> {
        let get_body = || {
            request
                .body()
                .and_then(|b| b.as_bytes())
                .and_then(|b| String::from_utf8(b.into()).ok())
        };

        match get_content_type(&request.headers()) {
            Some(ContentType::Multipart) => {
                write!(self.buffer, "{}", MULTIPART_SUPPRESSOR)?;
                write!(self.buffer, "\n\n")?;
            }
            Some(ContentType::Json) => {
                if let Some(body) = get_body() {
                    self.print_json(&body)?;
                    write!(self.buffer, "\n\n")?;
                }
            }
            Some(ContentType::UrlencodedForm) | _ => {
                if let Some(body) = get_body() {
                    write!(self.buffer, "{}", body)?;
                    write!(self.buffer, "\n\n")?;
                }
            }
        };
        Ok(())
    }

    pub async fn print_response_body(&mut self, response: Response) -> std::io::Result<()> {
        match get_content_type(&response.headers()) {
            Some(ContentType::Json) => self.print_json(&response.text().await.unwrap())?,
            Some(ContentType::Xml) => self.print_xml(&response.text().await.unwrap())?,
            Some(ContentType::Html) => self.print_html(&response.text().await.unwrap())?,
            _ => {
                let bytes = response.bytes().await.unwrap();
                if atty::is(Stream::Stdout) && bytes.contains(&b'\0') {
                    write!(self.buffer, "{}", BINARY_SUPPRESSOR)?;
                } else {
                    self.buffer.write(&bytes).unwrap();
                }
            }
        };

        self.buffer.flush().unwrap();
        Ok(())
    }
}
