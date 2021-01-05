use std::str::FromStr;

use regex::Regex;
use structopt::clap::AppSettings;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

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

    #[structopt(short = "d", long)]
    pub download: bool,

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
    #[structopt(name = "[METHOD] URL", parse(try_from_str = parse_method_url))]
    pub method_url: (Option<Method>, String),

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    pub request_items: Vec<RequestItem>,
}

impl Cli {
    pub fn from_args() -> Self {
        let mut args = vec![];
        let mut method = None;
        // Merge `method` and `url` entries from std::env::args()
        for arg in std::env::args() {
            if arg.parse::<Method>().is_ok() {
                method = Some(arg)
            } else if method.is_some() {
                args.push(format!("{} {}", method.unwrap(), arg));
                method = None;
            } else {
                args.push(arg);
            }
        }

        Cli::from_iter(args)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Method {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE
}

impl FromStr for Method {
    type Err = Error;
    fn from_str(s: &str) -> Result<Method> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "PATCH" => Ok(Method::PATCH),
            "DELETE" => Ok(Method::DELETE),
            method => {
                return Err(Error::with_description(
                    &format!("unknown http method {}", method),
                    ErrorKind::InvalidValue,
                ))
            }
        }
    }
}

impl From<Method> for reqwest::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::GET => reqwest::Method::GET,
            Method::POST => reqwest::Method::POST,
            Method::PUT => reqwest::Method::PUT,
            Method::PATCH => reqwest::Method::PATCH,
            Method::DELETE => reqwest::Method::DELETE,
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

arg_enum! {
    #[derive(Debug)]
    pub enum AuthType {
        Basic, Bearer
    }
}

// TODO: rename this to format?
arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Pretty {
        All, Colors, Format, None
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
        request_headers: bool,
        request_body: bool,
        response_headers: bool,
        response_body: bool,
    ) -> Print {
        Print {
            request_headers,
            request_body,
            response_headers,
            response_body,
        }
    }
}

impl FromStr for Print {
    type Err = Error;
    fn from_str(s: &str) -> Result<Print> {
        let mut p = Print::new(false, false, false, false);

        for char in s.chars() {
            match char {
                'H' => p.request_headers = true,
                'B' => p.request_body = true,
                'h' => p.response_headers = true,
                'b' => p.response_body = true,
                char => {
                    return Err(Error::with_description(
                        &format!("{:?} is not a valid value", char),
                        ErrorKind::InvalidValue,
                    ))
                }
            }
        }

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
    FormFile(String, String),
}

impl FromStr for RequestItem {
    type Err = Error;
    fn from_str(request_item: &str) -> Result<RequestItem> {
        let re1 = Regex::new(r"^(.+?)(==|:=|=|@|:)(.+)$").unwrap();
        let re2 = Regex::new(r"^(.+?)(:|;)$").unwrap();
        if let Some(caps) = re1.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[3].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField(key, value)),
                ":=" => Ok(RequestItem::JSONField(
                    key,
                    serde_json::from_str(&value).unwrap(),
                )),
                "@" => Ok(RequestItem::FormFile(key, value)),
                _ => unreachable!(),
            }
        } else if let Some(caps) = re2.captures(request_item) {
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
