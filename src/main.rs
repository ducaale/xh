use reqwest::blocking::Client;
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod cli;
mod printer;
mod request_items;

use cli::{Opt, Pretty, Theme};
use printer::Printer;
use request_items::{RequestItems, Body};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let printer = Printer::new(&opt);

    let url = opt.url.clone();
    let method = opt.method.clone().into();
    let request_items = RequestItems::new(opt.request_items);
    let query = request_items.query();
    let headers = request_items.headers(&url);
    let body = request_items.body(opt.form);

    let client = Client::new();
    let request = {
        let mut request_builder = client.request(method, url).query(&query).headers(headers);

        request_builder = match body {
            Some(Body::Json(body)) => request_builder.json(&body),
            Some(Body::Form(body)) => request_builder.form(&body),
            Some(Body::Multipart(body)) => request_builder.multipart(body),
            None => request_builder,
        };

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
