use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, HOST,
};
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod cli;
mod request_body;
mod printer;

use cli::{Opt, Pretty, RequestItem, Theme};
use request_body::RequestBody;
use printer::Printer;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let printer = Printer::new(&opt);

    let url = opt.url.clone();
    let mut query = vec![];
    let mut headers = HeaderMap::new();
    let mut body = RequestBody::new(&opt);

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
            request_item => body.insert(request_item)
        };
    }

    let client = Client::new();
    let request = {
        let mut request_builder = client
            .request(opt.method.clone().into(), url)
            .query(&query)
            .headers(headers);

        if let Some(json) = body.json() {
            request_builder = request_builder.json(json)
        }
        if let Some(form) = body.form() {
            request_builder = request_builder.form(form)
        }
        if let Some(multipart) = body.multipart() {
            request_builder = request_builder.multipart(multipart)
        }

        request_builder.build()?
    };

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
