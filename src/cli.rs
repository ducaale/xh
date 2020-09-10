use regex::Regex;
use reqwest::Url;
use structopt::clap::{arg_enum, Error, ErrorKind, Result};
use structopt::StructOpt;

/// Yet another HTTPie clone
#[derive(StructOpt, Debug)]
#[structopt(name = "yahc")]
pub struct Opt {
    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Construct HTTP requests without sending them anywhere.
    #[structopt(short, long)]
    pub offline: bool,

    #[structopt(short = "a", long)]
    pub auth: Option<String>,

    /// Controls output processing.
    #[structopt(short, long, possible_values = &Pretty::variants(), case_insensitive = true)]
    pub pretty: Option<Pretty>,

    /// Specify the auth mechanism.
    #[structopt(short = "A", long = "auth-type")]
    pub auth_type: Option<String>,

    /// The HTTP method to be used for the request.
    #[structopt(name = "METHOD", possible_values = &Method::variants(), case_insensitive = true)]
    pub method: Method,

    #[structopt(name = "URL", parse(from_str = parse_url))]
    pub url: Url,

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    pub request_items: Vec<RequestItem>,
}

arg_enum! {
    #[derive(Debug, Clone)]
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

fn parse_url(url: &str) -> Url {
    let re = Regex::new("[a-zA-Z]://.+").unwrap();

    if url.starts_with(":") {
        let url = String::from("http://localhost") + url;
        Url::parse(&url).unwrap()
    } else if !re.is_match(url) {
        let url = String::from("http://") + url;
        Url::parse(&url).unwrap()
    } else {
        Url::parse(url).unwrap()
    }
}

#[derive(Debug)]
pub enum RequestItem {
    HttpHeader(String, String),
    UrlParam(String, String),
    DataField(String, String),
}

impl std::str::FromStr for RequestItem {
    type Err = Error;
    fn from_str(request_item: &str) -> Result<RequestItem> {
        let re = Regex::new(r"^(.+?)(==|=|:)(.+)$").unwrap();
        if let Some(caps) = re.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[3].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField(key, value)),
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

// TODO: rename this to format?
arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Pretty {
        All, Colors, Format, None
    }
}
