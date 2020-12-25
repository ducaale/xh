use std::str::FromStr;

use regex::Regex;
use structopt::clap::AppSettings;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

// Following doc comments were copy-pasted from HTTPie
/// Yet another HTTPie clone
#[derive(StructOpt, Debug)]
#[structopt(name = "yahc", setting = AppSettings::DeriveDisplayOrder)]
pub struct Opt {
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

    // TODO: save to file even when download flag is not being used
    /// Save output to FILE instead of stdout.
    #[structopt(short, long)]
    pub output: Option<String>,

    #[structopt(short = "d", long)]
    pub download: bool,

    /// String specifying what the output should contain
    #[structopt(short = "p", long)]
    pub print: Option<Print>,

    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Controls output processing.
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true)]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long = "style", possible_values = &Theme::variants(), case_insensitive = true)]
    pub theme: Option<Theme>,

    /// The default scheme to use if not specified in the URL.
    #[structopt(long = "default-scheme")]
    pub default_scheme: Option<String>,

    /// The HTTP method to be used for the request.
    #[structopt(name = "METHOD", possible_values = &Method::variants(), case_insensitive = true)]
    pub method: Method,

    #[structopt(name = "URL")]
    pub url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    pub request_items: Vec<RequestItem>,
}

// TODO: add remaining methods
arg_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Method {
        GET,
        POST,
        PUT,
        PATCH,
        DELETE
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
