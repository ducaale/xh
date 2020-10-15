use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH,
    CONTENT_TYPE, HOST,
};
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod cli;
mod printer;

use cli::{Opt, Pretty, RequestItem, Theme};
use printer::Printer;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let printer = Printer::new(&opt);

    let url = opt.url;
    let mut query = vec![];
    let mut headers = HeaderMap::new();
    let mut body = serde_json::Map::new();

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
                body.insert(key, serde_json::Value::String(value));
            }
            RequestItem::RawDataField(key, value) => {
                body.insert(key, value);
            }
        };
    }

    let body = if body.len() > 0 {
        if opt.form {
            serde_urlencoded::to_string(&body).unwrap()
        } else {
            serde_json::to_string(&body).unwrap()
        }
    } else {
        String::from("")
    };

    if body.len() > 0 {
        if opt.form {
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
        } else {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        let content_length = HeaderValue::from_str(&body.len().to_string())?;
        headers.insert(CONTENT_LENGTH, content_length);
    }

    let client = Client::new();
    let request = client
        .request(opt.method.clone().into(), url)
        .query(&query)
        .headers(headers)
        .body(body)
        .build()?;

    print!("\n");

    if opt.verbose {
        printer.print_request_headers(&request);
        printer.print_request_body(&request);
    }

    if !opt.offline {
        let response = client.execute(request)?;
        printer.print_response_headers(&response);
        printer.print_response_body(response);
    }
    Ok(())
}
