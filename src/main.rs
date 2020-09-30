use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH,
    CONTENT_TYPE, HOST,
};
use serde_json::{Map, Value};
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod cli;
mod display;

use cli::{Method, Opt, Pretty, Theme, RequestItem};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let format_option = opt.pretty.unwrap_or(Pretty::All);
    let theme = opt.style.unwrap_or(Theme::Auto);

    let url = opt.url;
    let mut query = vec![];
    let mut headers = HeaderMap::new();
    let mut body = Map::new();

    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    headers.insert(
        HOST,
        HeaderValue::from_str(&url.host().unwrap().to_string())?,
    );

    for item in opt.request_items {
        match item {
            RequestItem::HttpHeader(key, value) => {
                let key = HeaderName::from_bytes(&key.as_bytes())?;
                let value = HeaderValue::from_str(&value)?;
                headers.insert(key, value);
            }
            RequestItem::UrlParam(key, value) => {
                query.push((key, value));
            }
            RequestItem::DataField(key, value) => {
                body.insert(key, Value::String(value));
            }
        };
    }

    let client = Client::new();
    let request = match opt.method {
        Method::PUT | Method::POST | Method::PATCH if body.len() > 0 => {
            let body = Value::Object(body.clone()).to_string();
            let content_length = HeaderValue::from_str(&body.len().to_string())?;
            headers.insert(CONTENT_LENGTH, content_length);
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            client
                .request(opt.method.clone().into(), url)
                .query(&query)
                .headers(headers)
                .body(body.clone())
                .build()?
        }
        _ => client
            .request(opt.method.clone().into(), url)
            .query(&query)
            .headers(headers)
            .build()?,
    };

    print!("\n");

    if opt.verbose {
        display::print_request_headers(&request, &format_option, &theme);
        if let Some(body) = request.body() {
            display::print_json(
                &String::from_utf8(body.as_bytes().unwrap().into())?,
                &format_option,
                &theme
            );
        }
    }

    if !opt.offline {
        let response = client.execute(request)?;
        display::print_response_headers(&response, &format_option, &theme);
        display::print_response_body(response, &format_option, &theme);
    }
    Ok(())
}
