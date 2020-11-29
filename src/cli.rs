use std::str::FromStr;

use regex::Regex;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

// Following doc comments were copy-pasted from HTTPie
/// Yet another HTTPie clone
#[derive(StructOpt, Debug)]
#[structopt(name = "yahc")]
pub struct Opt {
    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Construct HTTP requests without sending them anywhere.
    #[structopt(long)]
    pub offline: bool,

    /// (default) Data items from the command line are serialized as a JSON object.
    #[structopt(short = "j", long)]
    pub json: bool,

    /// Data items from the command line are serialized as form fields.
    #[structopt(short = "f", long)]
    pub form: bool,

    #[structopt(short, long = "ignore-stdin")]
    pub ignore_stdin: bool,

    /// Specify the auth mechanism.
    #[structopt(short = "A", long = "auth-type", possible_values = &AuthType::variants(), case_insensitive = true)]
    pub auth_type: Option<AuthType>,

    #[structopt(short = "a", long)]
    pub auth: Option<String>,

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

#[derive(Debug, Clone)]
pub enum RequestItem {
    HttpHeader(String, String),
    UrlParam(String, String),
    DataField(String, String),
    JSONField(String, serde_json::Value),
    FormFile(String, String),
}

impl FromStr for RequestItem {
    type Err = Error;
    fn from_str(request_item: &str) -> Result<RequestItem> {
        let re = Regex::new(r"^(.+?)(==|:=|=|@|:)(.+)$").unwrap();
        if let Some(caps) = re.captures(request_item) {
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
        } else {
            Err(Error::with_description(
                &format!("{:?} is not a valid value", request_item),
                ErrorKind::InvalidValue,
            ))
        }
    }
}
