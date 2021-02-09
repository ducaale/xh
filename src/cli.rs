use std::path::Path;
use std::str::FromStr;

use regex::Regex;
use structopt::clap::AppSettings;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

use crate::{Body, Buffer};

// Following doc comments were copy-pasted from HTTPie
#[derive(StructOpt, Debug)]
#[structopt(name = "ht", setting = AppSettings::DeriveDisplayOrder)]
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

    /// Create, or reuse and update a session. By default, sessions are stored in ~/.config/ht/sessions/.
    #[structopt(long = "session", parse(try_from_str = validate_session_name))]
    pub session: Option<String>,

    /// The request URL, preceded by an optional HTTP method.
    #[structopt(name = "[METHOD] URL", parse(try_from_str = parse_method_url))]
    pub method_url: (Option<Method>, String),

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    pub request_items: Vec<RequestItem>,
}

impl Cli {
    pub fn from_args() -> Self {
        Cli::from_iter(std::env::args())
    }

    pub fn from_iter(iter: impl IntoIterator<Item = String>) -> Self {
        let mut args = vec![];
        let mut method = None;
        // Merge `method` and `url` entries from std::env::args()
        for arg in iter {
            if arg.parse::<Method>().is_ok() {
                method = Some(arg)
            } else if let Some(method_value) = method {
                args.push(format!("{} {}", method_value, arg));
                method = None;
            } else {
                args.push(arg);
            }
        }

        StructOpt::from_iter(args)
    }
}

#[derive(Debug, Clone, Copy)]
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

fn parse_method_url(s: &str) -> Result<(Option<Method>, String)> {
    let parts = s.split_whitespace().collect::<Vec<_>>();
    if parts.len() == 1 {
        Ok((None, parts[0].to_string()))
    } else {
        Ok((Some(parts[0].parse()?), parts[1].to_string()))
    }
}

fn validate_session_name(s: &str) -> Result<String> {
    let path = Path::new(s);
    match path.to_str() {
        Some(identifier) => Ok(identifier.to_string()),
        None => {
            return Err(Error::with_description(
                &format!("{:?} contains invalid characters", s),
                ErrorKind::InvalidValue,
            ))
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

#[derive(Debug, Clone)]
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
        lazy_static::lazy_static! {
            static ref RE1: Regex = Regex::new(r"^(.+?)@(.+?);type=(.+?)$").unwrap();
        }
        lazy_static::lazy_static! {
            static ref RE2: Regex = Regex::new(r"^(.+?)(==|:=|=|@|:)(.+)$").unwrap();
        }
        lazy_static::lazy_static! {
            static ref RE3: Regex = Regex::new(r"^(.+?)(:|;)$").unwrap();
        }

        if let Some(caps) = RE1.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[2].to_string();
            let file_type = caps[3].to_string();
            Ok(RequestItem::FormFile(key, value, Some(file_type)))
        } else if let Some(caps) = RE2.captures(request_item) {
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
        } else if let Some(caps) = RE3.captures(request_item) {
            let key = caps[1].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeaderToUnset(key)),
                ";" => Ok(RequestItem::HttpHeader(key, "".into())),
                _ => unreachable!(),
            }
        } else {
            Err(Error::with_description(
                &format!("{:?} is not a valid value", request_item),
                ErrorKind::InvalidValue,
            ))
        }
    }
}
