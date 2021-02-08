use std::env;

use atty::Stream;
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, RANGE, USER_AGENT,
};
use reqwest::{Client, StatusCode};

mod auth;
mod buffer;
mod cli;
mod download;
mod printer;
mod request_items;
mod url;
mod utils;

use anyhow::{anyhow, Result};
use auth::Auth;
use buffer::Buffer;
use cli::{AuthType, Cli, Method, Pretty, Print, RequestItem, Theme};
use download::{download_file, get_file_size};
use printer::Printer;
use request_items::{Body, RequestItems};
use reqwest::redirect::Policy;
use url::Url;
use utils::body_from_stdin;

fn get_user_agent() -> &'static str {
    // Hard-coded user agent for the benefit of tests
    // In integration tests the binary isn't compiled with cfg(test), so we
    // use an environment variable
    if cfg!(test) || env::var_os("HT_TEST_MODE").is_some() {
        "ht/0.0.0 (test mode)"
    } else {
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::from_args();

    let request_items = RequestItems::new(args.request_items);
    let query = request_items.query();
    let (headers, headers_to_unset) = request_items.headers()?;
    #[allow(clippy::eval_order_dependence)]
    let body = match (
        request_items.body(args.form, args.multipart).await?,
        // TODO: can we give an error before reading all of stdin?
        body_from_stdin(args.ignore_stdin).await?,
    ) {
        (Some(_), Some(_)) => {
            return Err(anyhow!(
                "Request body (from stdin) and Request data (key=value) cannot be mixed"
            ))
        }
        (Some(body), None) | (None, Some(body)) => Some(body),
        (None, None) => None,
    };

    let (method, url) = args.method_url;
    let url = Url::new(url, args.default_scheme)?;
    let host = url.host().ok_or_else(|| anyhow!("Missing hostname"))?;
    let method = method.unwrap_or_else(|| Method::from(&body)).into();
    let auth = Auth::new(args.auth, args.auth_type, &host)?;
    let redirect = match args.follow {
        true => Policy::limited(args.max_redirects.unwrap_or(10)),
        false => Policy::none(),
    };

    let client = Client::builder().redirect(redirect).build()?;
    let request = {
        let mut request_builder = client
            .request(method, url.0)
            .header(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"))
            .header(CONNECTION, HeaderValue::from_static("keep-alive"))
            .header(USER_AGENT, get_user_agent());

        request_builder = match body {
            Some(Body::Form(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("*/*"))
                .form(&body),
            Some(Body::Multipart(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("*/*"))
                .multipart(body),
            Some(Body::Json(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("application/json, */*"))
                .json(&body),
            Some(Body::Raw(body)) => request_builder
                .header(ACCEPT, HeaderValue::from_static("application/json, */*"))
                .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                .body(body),
            None => request_builder.header(ACCEPT, HeaderValue::from_static("*/*")),
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

    let buffer = Buffer::new(args.download, &args.output, atty::is(Stream::Stdout))?;
    let print = match args.print {
        Some(print) => print,
        None => Print::new(
            args.verbose,
            args.headers,
            args.body,
            args.quiet,
            args.offline,
            &buffer,
        ),
    };
    let mut printer = Printer::new(args.pretty, args.theme, args.stream, buffer);

    if print.request_headers {
        printer.print_request_headers(&request)?;
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
            let resume = response.status() == StatusCode::PARTIAL_CONTENT;
            download_file(response, args.output, resume, args.quiet).await?;
        } else if print.response_body {
            printer.print_response_body(response).await?;
        }
    }
    Ok(())
}
