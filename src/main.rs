use reqwest::blocking::Client;
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod auth;
mod cli;
mod printer;
mod request_items;
mod url;
mod utils;

use auth::Auth;
use cli::{AuthType, Opt, Pretty, RequestItem, Theme};
use printer::Printer;
use request_items::{Body, RequestItems};
use url::Url;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let printer = Printer::new(&opt);
    let request_items = RequestItems::new(opt.request_items);

    let url = Url::new(opt.url, opt.default_scheme);
    let method = opt.method.into();
    let auth = Auth::new(opt.auth, opt.auth_type, &url);
    let query = request_items.query();
    let headers = request_items.headers(&url);
    let body = request_items.body(opt.form)?;

    let client = Client::new();
    let request = {
        let mut request_builder = client.request(method, url.0).query(&query).headers(headers);

        request_builder = match body {
            Some(Body::Json(body)) => request_builder.json(&body),
            Some(Body::Form(body)) => request_builder.form(&body),
            Some(Body::Multipart(body)) => request_builder.multipart(body),
            None => request_builder,
        };

        request_builder = match auth {
            Some(Auth::Bearer(token)) => request_builder.bearer_auth(token),
            Some(Auth::Basic(username, password)) => request_builder.basic_auth(username, password),
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
