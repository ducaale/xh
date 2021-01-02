use atty::Stream;
use reqwest::header::{HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST};
use reqwest::Client;
use structopt::StructOpt;
#[macro_use]
extern crate lazy_static;

mod auth;
mod cli;
mod download;
mod printer;
mod request_items;
mod url;
mod utils;

use auth::Auth;
use cli::{AuthType, Opt, Pretty, Print, RequestItem, Theme};
use download::download_file;
use printer::Printer;
use request_items::{Body, RequestItems};
use url::Url;
use utils::body_from_stdin;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let request_items = RequestItems::new(opt.request_items);

    let url = Url::new(opt.url, opt.default_scheme);
    let host = url.host().unwrap();
    let method = opt.method.into();
    let auth = Auth::new(opt.auth, opt.auth_type, &host);
    let query = request_items.query();
    let (headers, headers_to_unset) = request_items.headers();
    let body = match (
        request_items.body(opt.form, opt.multipart).await?,
        body_from_stdin(opt.ignore_stdin),
    ) {
        (Some(_), Some(_)) => {
            return Err(
                "Request body (from stdin) and Request data (key=value) cannot be mixed".into(),
            )
        }
        (Some(body), None) | (None, Some(body)) => Some(body),
        (None, None) => None,
    };

    let client = Client::new();
    let request = {
        let mut request_builder = client
            .request(method, url.0)
            .header(ACCEPT, HeaderValue::from_static("*/*"))
            .header(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"))
            .header(CONNECTION, HeaderValue::from_static("keep-alive"))
            .header(HOST, HeaderValue::from_str(&host).unwrap());

        request_builder = match body {
            Some(Body::Form(body)) => request_builder.form(&body),
            Some(Body::Multipart(body)) => request_builder.multipart(body),
            Some(Body::Json(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("application/json, */*"))
                .json(&body),
            Some(Body::Raw(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("application/json, */*"))
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(body),
            None => request_builder,
        };

        request_builder = match auth {
            Some(Auth::Bearer(token)) => request_builder.bearer_auth(token),
            Some(Auth::Basic(username, password)) => request_builder.basic_auth(username, password),
            None => request_builder,
        };

        let mut request = request_builder.query(&query).headers(headers).build()?;

        headers_to_unset.iter().for_each(|h| {
            request.headers_mut().remove(h);
        });

        request
    };

    let mut printer = Printer::new(opt.pretty, opt.theme, &opt.output);

    let print = opt.print.unwrap_or(
        match (opt.verbose, opt.quiet, opt.offline, atty::isnt(Stream::Stdout), &opt.output, opt.download) {
            (true, _, _, _, _, _) => Print::new(true, true, true, true),
            (_, true, _, _, _, _) => Print::new(false, false, false, false),
            (_, _, true, _, _, _) => Print::new(true, true, false, false),
            (_, _, _, true, _, _) => Print::new(false, false, false, true),
            (_, _, _, _, Some(_), false) => Print::new(false, false, false, true),
            (_, _, _, _, _, _) => Print::new(false, false, true, true)
        }
    );

    if print.request_headers {
        printer.print_request_headers(&request);
    }
    if print.request_body {
        printer.print_request_body(&request)?;
    }
    if !opt.offline {
        let response = client.execute(request).await?;
        if print.response_headers {
            printer.print_response_headers(&response)?;
        }
        if opt.download {
            download_file(response, opt.output, opt.quiet).await;
        } else if print.response_body {
            printer.print_response_body(response).await?;
        }
    }
    Ok(())
}
