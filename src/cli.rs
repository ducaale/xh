use std::mem;
use std::str::FromStr;

use structopt::clap::AppSettings;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

use crate::{regex, Body, Buffer};

// Following doc comments were copy-pasted from HTTPie
#[derive(StructOpt, Debug)]
#[structopt(name = "xh", setting = AppSettings::DeriveDisplayOrder)]
pub struct Cli {
    /// Construct HTTP requests without sending them anywhere.
    #[structopt(long)]
    pub offline: bool,

    /// (default) Data items from the command line are serialized as a JSON object.
    #[structopt(short = "j", long)]
    pub json: bool,

    /// Data items from the command line are serialized as form fields.
    #[structopt(short = "f", long)]
    pub form: bool,

    /// Similar to --form, but always sends a multipart/form-data request (i.e., even without files).
    #[structopt(short, long)]
    pub multipart: bool,

    /// Do not attempt to read stdin.
    #[structopt(short = "I", long = "ignore-stdin")]
    pub ignore_stdin: bool,

    /// Specify the auth mechanism.
    #[structopt(short = "A", long = "auth-type", possible_values = &AuthType::variants(), case_insensitive = true)]
    pub auth_type: Option<AuthType>,

    #[structopt(short = "a", long)]
    pub auth: Option<String>,

    /// Save output to FILE instead of stdout.
    #[structopt(short, long)]
    pub output: Option<String>,

    /// Do follow redirects.
    #[structopt(short = "F", long = "follow")]
    pub follow: bool,

    /// Number of redirects to follow, only respected if `follow` is set.
    #[structopt(long = "max-redirects")]
    pub max_redirects: Option<usize>,

    #[structopt(short = "d", long)]
    pub download: bool,

    /// Print only the response headers, shortcut for --print=h.
    #[structopt(short = "h", long)]
    pub headers: bool,

    /// Print only the response body, Shortcut for --print=b.
    #[structopt(short = "b", long)]
    pub body: bool,

    /// Resume an interrupted download.
    #[structopt(short = "c", long = "continue")]
    pub resume: bool,

    /// String specifying what the output should contain.
    #[structopt(short = "p", long)]
    pub print: Option<Print>,

    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Do not print to stdout or stderr.
    #[structopt(short = "q", long)]
    pub quiet: bool,

    /// Always stream the response body
    #[structopt(short = "S", long)]
    pub stream: bool,

    /// Controls output processing.
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true)]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long = "style", possible_values = &Theme::variants(), case_insensitive = true)]
    pub theme: Option<Theme>,

    /// The default scheme to use if not specified in the URL.
    #[structopt(long = "default-scheme")]
    pub default_scheme: Option<String>,

    /// The request URL, preceded by an optional HTTP method.
    #[structopt(name = "[METHOD] URL")]
    raw_method_or_url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    raw_rest_args: Vec<String>,

    /// The HTTP method, if supplied.
    #[structopt(skip)]
    pub method: Option<Method>,

    /// The request URL.
    #[structopt(skip)]
    pub url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(skip)]
    pub request_items: Vec<RequestItem>,
}

impl Cli {
    pub fn from_args() -> Result<Self> {
        Cli::from_iter(std::env::args())
    }

    pub fn from_iter(iter: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut cli: Self = StructOpt::from_iter(iter);
        let mut rest_args = mem::take(&mut cli.raw_rest_args).into_iter();
        match cli.raw_method_or_url.parse::<Method>() {
            Ok(method) => {
                cli.method = Some(method);
                cli.url = rest_args.next().ok_or_else(|| {
                    Error::with_description("Missing URL", ErrorKind::MissingRequiredArgument)
                })?;
            }
            Err(_) => {
                cli.method = None;
                cli.url = mem::take(&mut cli.raw_method_or_url);
            }
        }
        for request_item in rest_args {
            cli.request_items.push(request_item.parse()?);
        }
        Ok(cli)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    PATCH,
    DELETE,
    OPTIONS,
}

impl FromStr for Method {
    type Err = Error;
    fn from_str(s: &str) -> Result<Method> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "HEAD" => Ok(Method::HEAD),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "PATCH" => Ok(Method::PATCH),
            "DELETE" => Ok(Method::DELETE),
            "OPTIONS" => Ok(Method::OPTIONS),
            method => Err(Error::with_description(
                &format!("unknown http method {}", method),
                ErrorKind::InvalidValue,
            )),
        }
    }
}

impl From<Method> for reqwest::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::GET => reqwest::Method::GET,
            Method::HEAD => reqwest::Method::HEAD,
            Method::POST => reqwest::Method::POST,
            Method::PUT => reqwest::Method::PUT,
            Method::PATCH => reqwest::Method::PATCH,
            Method::DELETE => reqwest::Method::DELETE,
            Method::OPTIONS => reqwest::Method::OPTIONS,
        }
    }
}

impl From<&Option<Body>> for Method {
    fn from(body: &Option<Body>) -> Self {
        match body {
            Some(_) => Method::POST,
            None => Method::GET,
        }
    }
}

arg_enum! {
    #[derive(Debug)]
    pub enum AuthType {
        Basic, Bearer
    }
}

arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Pretty {
        All, Colors, Format, None
    }
}

impl From<&Buffer> for Pretty {
    fn from(b: &Buffer) -> Self {
        match b {
            Buffer::File(_) | Buffer::Redirect => Pretty::None,
            Buffer::Stdout | Buffer::Stderr => Pretty::All,
        }
    }
}

arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Theme {
        Auto, Solarized
    }
}

#[derive(Debug)]
pub struct Print {
    pub request_headers: bool,
    pub request_body: bool,
    pub response_headers: bool,
    pub response_body: bool,
}

impl Print {
    pub fn new(
        verbose: bool,
        headers: bool,
        body: bool,
        quiet: bool,
        offline: bool,
        buffer: &Buffer,
    ) -> Self {
        if verbose {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: true,
                response_body: true,
            }
        } else if quiet {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: false,
            }
        } else if offline {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: false,
                response_body: false,
            }
        } else if headers {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: false,
            }
        } else if body || matches!(buffer, Buffer::Redirect | Buffer::File(_)) {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: true,
            }
        } else {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: true,
            }
        }
    }
}

impl FromStr for Print {
    type Err = Error;
    fn from_str(s: &str) -> Result<Print> {
        let mut request_headers = false;
        let mut request_body = false;
        let mut response_headers = false;
        let mut response_body = false;

        for char in s.chars() {
            match char {
                'H' => request_headers = true,
                'B' => request_body = true,
                'h' => response_headers = true,
                'b' => response_body = true,
                char => {
                    return Err(Error::with_description(
                        &format!("{:?} is not a valid value", char),
                        ErrorKind::InvalidValue,
                    ))
                }
            }
        }

        let p = Print {
            request_headers,
            request_body,
            response_headers,
            response_body,
        };
        Ok(p)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestItem {
    HttpHeader(String, String),
    HttpHeaderToUnset(String),
    UrlParam(String, String),
    DataField(String, String),
    JSONField(String, serde_json::Value),
    FormFile(String, String, Option<String>),
}

impl FromStr for RequestItem {
    type Err = Error;
    fn from_str(request_item: &str) -> Result<RequestItem> {
        regex!(FORM_FILE_TYPED = r"^(.+?)@(.+?);type=(.+?)$");
        regex!(PARAM = r"^(.+?)(==|:=|=|@|:)((?s).+)$");
        regex!(NO_HEADER = r"^(.+?)(:|;)$");

        if let Some(caps) = FORM_FILE_TYPED.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[2].to_string();
            let file_type = caps[3].to_string();
            Ok(RequestItem::FormFile(key, value, Some(file_type)))
        } else if let Some(caps) = PARAM.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[3].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField(key, value)),
                ":=" => Ok(RequestItem::JSONField(
                    key,
                    serde_json::from_str(&value).map_err(|err| {
                        Error::with_description(
                            &format!("{:?}: {}", request_item, err),
                            ErrorKind::InvalidValue,
                        )
                    })?,
                )),
                "@" => Ok(RequestItem::FormFile(key, value, None)),
                _ => unreachable!(),
            }
        } else if let Some(caps) = NO_HEADER.captures(request_item) {
            let key = caps[1].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeaderToUnset(key)),
                ";" => Ok(RequestItem::HttpHeader(key, "".into())),
                _ => unreachable!(),
            }
        } else {
            // TODO: We can also end up here if the method couldn't be parsed
            // and was interpreted as a URL, making the actual URL a request
            // item
            Err(Error::with_description(
                &format!("{:?} is not a valid request item", request_item),
                ErrorKind::InvalidValue,
            ))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli> {
        Cli::from_iter(
            Some("xh".to_string())
                .into_iter()
                .chain(args.iter().map(|s| s.to_string())),
        )
    }

    #[test]
    fn implicit_method() {
        let cli = parse(&["example.org"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url, "example.org");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn explicit_method() {
        let cli = parse(&["get", "example.org"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url, "example.org");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn missing_url() {
        parse(&["get"]).unwrap_err();
    }

    #[test]
    fn space_in_url() {
        let cli = parse(&["post", "example.org/foo bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::POST));
        assert_eq!(cli.url, "example.org/foo bar");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn request_items() {
        let cli = parse(&["get", "example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url, "example.org");
        assert_eq!(
            cli.request_items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn request_items_implicit_method() {
        let cli = parse(&["example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url, "example.org");
        assert_eq!(
            cli.request_items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn superfluous_arg() {
        parse(&["get", "example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn superfluous_arg_implicit_method() {
        parse(&["example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn multiple_methods() {
        parse(&["get", "post", "example.org"]).unwrap_err();
    }
}
