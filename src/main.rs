use atty::Stream;
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST, RANGE,
};
use reqwest::{Client, StatusCode};
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
use cli::{AuthType, Cli, Pretty, Print, RequestItem, Theme};
use download::{download_file, get_file_size};
use printer::{Buffer, Printer};
use request_items::{Body, RequestItems};
use url::Url;
use utils::body_from_stdin;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();

    let request_items = RequestItems::new(args.request_items);

    let url = Url::new(args.url, args.default_scheme);
    let host = url.host().unwrap();
    let method = args.method.into();
    let auth = Auth::new(args.auth, args.auth_type, &host);
    let query = request_items.query();
    let (headers, headers_to_unset) = request_items.headers();
    let body = match (
        request_items.body(args.form, args.multipart).await?,
        body_from_stdin(args.ignore_stdin),
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
            .header(HOST, HeaderValue::from_str(&host)?);

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

        request_builder = match get_file_size(&args.output) {
            Some(r) if args.download && args.resume => {
                request_builder.header(RANGE, HeaderValue::from_str(&format!("bytes={}", r))?)
            }
            _ => request_builder,
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

    let buffer = match &args.output {
        Some(output) if !args.download => Buffer::File(Box::new(std::fs::File::create(&output)?)),
        _ if atty::isnt(Stream::Stdout) => Buffer::Redirect(Box::new(std::io::stdout())),
        Some(_) => Buffer::Terminal(Box::new(std::io::stdout())),
        None => Buffer::Terminal(Box::new(std::io::stdout())),
    };
    let print = args.print.unwrap_or(
        if args.verbose {
            Print::new(true, true, true, true)
        } else if args.quiet {
            Print::new(false, false, false, false)
        } else if args.offline {
            Print::new(true, true, false, false)
        } else if !matches!(&buffer, Buffer::Terminal(_)) {
            Print::new(false, false, false, true)
        } else {
            Print::new(false, false, true, true)
        }
    );
    let mut printer = Printer::new(args.pretty, args.theme, buffer);

    if print.request_headers {
        printer.print_request_headers(&request);
    }
    if print.request_body {
        printer.print_request_body(&request)?;
    }
    if !args.offline {
        let response = client.execute(request).await?;
        if print.response_headers {
            printer.print_response_headers(&response)?;
        }
        if args.download {
            let resume = &response.status() == &StatusCode::OK;
            download_file(response, args.output, resume, args.quiet).await;
        } else if print.response_body {
            printer.print_response_body(response).await?;
        }
    }
    Ok(())
}
