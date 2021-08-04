#![allow(clippy::bool_assert_comparison)]
mod auth;
mod buffer;
mod cli;
mod download;
mod formatting;
mod printer;
mod request_items;
mod to_curl;
mod url;
mod utils;

use std::env;
use std::fs::File;
use std::io::{stdin, Read};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use atty::Stream;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, RANGE, USER_AGENT,
};
use reqwest::redirect::Policy;

use crate::auth::{auth_from_netrc, parse_auth, read_netrc};
use crate::buffer::Buffer;
use crate::cli::{Cli, Print, Proxy, RequestType, Verify};
use crate::download::{download_file, get_file_size};
use crate::printer::Printer;
use crate::request_items::{Body, RequestItems, FORM_CONTENT_TYPE, JSON_ACCEPT, JSON_CONTENT_TYPE};
use crate::url::construct_url;
use crate::utils::{test_mode, test_pretend_term};

fn get_user_agent() -> &'static str {
    if test_mode() {
        // Hard-coded user agent for the benefit of tests
        "xh/0.0.0 (test mode)"
    } else {
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
    }
}

#[exit_status::main]
fn main() -> Result<i32> {
    let args = Cli::parse();

    if args.curl {
        to_curl::print_curl_translation(args)?;
        return Ok(0);
    }

    let request_items = RequestItems::new(args.request_items);
    let query = request_items.query();
    let (headers, headers_to_unset) = request_items.headers()?;
    let url = construct_url(&args.url, args.default_scheme.as_deref(), query)?;

    let ignore_stdin = args.ignore_stdin || atty::is(Stream::Stdin) || test_pretend_term();
    let mut body = request_items.body(args.request_type)?;
    if !ignore_stdin {
        if !body.is_empty() {
            if body.is_multipart() {
                return Err(anyhow!("Cannot build a multipart request body from stdin"));
            } else {
                return Err(anyhow!(
                    "Request body (from stdin) and request data (key=value) cannot be mixed. \
                    Pass --ignore-stdin to ignore standard input."
                ));
            }
        }
        let mut buffer = Vec::new();
        stdin().read_to_end(&mut buffer)?;
        body = Body::Raw(buffer);
    }

    let method = args.method.unwrap_or_else(|| body.pick_method());
    let timeout = args.timeout.and_then(|t| t.as_duration());
    let redirect = match args.follow {
        true => Policy::limited(args.max_redirects.unwrap_or(10)),
        false => Policy::none(),
    };

    let mut client = Client::builder()
        .http1_title_case_headers()
        .http2_adaptive_window(true)
        .timeout(timeout)
        .redirect(redirect);

    let mut resume: Option<u64> = None;

    if url.scheme() == "https" {
        let verify = args.verify.unwrap_or_else(|| {
            // requests library which is used by HTTPie checks for both
            // REQUESTS_CA_BUNDLE and CURL_CA_BUNDLE environment variables.
            // See https://docs.python-requests.org/en/master/user/advanced/#ssl-cert-verification
            if let Some(path) = env::var_os("REQUESTS_CA_BUNDLE") {
                Verify::CustomCaBundle(PathBuf::from(path))
            } else if let Some(path) = env::var_os("CURL_CA_BUNDLE") {
                Verify::CustomCaBundle(PathBuf::from(path))
            } else {
                Verify::Yes
            }
        });
        client = match verify {
            Verify::Yes => client,
            Verify::No => client.danger_accept_invalid_certs(true),
            Verify::CustomCaBundle(path) => {
                let mut buffer = Vec::new();
                let mut file = File::open(&path).with_context(|| {
                    format!("Failed to open the custom CA bundle: {}", path.display())
                })?;
                file.read_to_end(&mut buffer).with_context(|| {
                    format!("Failed to read the custom CA bundle: {}", path.display())
                })?;

                client = client.tls_built_in_root_certs(false);
                for pem in pem::parse_many(buffer) {
                    let certificate = reqwest::Certificate::from_pem(pem::encode(&pem).as_bytes())
                        .with_context(|| {
                            format!("Failed to load the custom CA bundle: {}", path.display())
                        })?;
                    client = client.add_root_certificate(certificate);
                }
                client
            }
        };

        if let Some(cert) = args.cert {
            let mut buffer = Vec::new();
            let mut file = File::open(&cert)
                .with_context(|| format!("Failed to open the cert file: {}", cert.display()))?;
            file.read_to_end(&mut buffer)
                .with_context(|| format!("Failed to read the cert file: {}", cert.display()))?;

            if let Some(cert_key) = args.cert_key {
                buffer.push(b'\n');

                let mut file = File::open(&cert_key).with_context(|| {
                    format!("Failed to open the cert key file: {}", cert_key.display())
                })?;
                file.read_to_end(&mut buffer).with_context(|| {
                    format!("Failed to read the cert key file: {}", cert_key.display())
                })?;
            }

            let identity = reqwest::Identity::from_pem(&buffer)
                .context("Failed to parse the cert/cert key files")?;
            client = client.identity(identity);
        };
    }

    for proxy in args.proxy.into_iter().rev() {
        client = client.proxy(match proxy {
            Proxy::Http(url) => reqwest::Proxy::http(url),
            Proxy::Https(url) => reqwest::Proxy::https(url),
            Proxy::All(url) => reqwest::Proxy::all(url),
        }?);
    }

    let client = client.build()?;

    let mut request = {
        let mut request_builder = client
            .request(method, url.clone())
            .header(
                ACCEPT_ENCODING,
                HeaderValue::from_static("gzip, deflate, br"),
            )
            .header(CONNECTION, HeaderValue::from_static("keep-alive"))
            .header(USER_AGENT, get_user_agent());

        request_builder = match body {
            Body::Form(body) => request_builder.form(&body),
            Body::Multipart(body) => request_builder.multipart(body),
            Body::Json(body) => {
                // An empty JSON body would produce "{}" instead of "", so
                // this is the one kind of body that needs an is_empty() check
                if !body.is_empty() {
                    request_builder
                        .header(ACCEPT, HeaderValue::from_static(JSON_ACCEPT))
                        .json(&body)
                } else if args.json {
                    request_builder
                        .header(ACCEPT, HeaderValue::from_static(JSON_ACCEPT))
                        .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE))
                } else {
                    // We're here because this is the default request type
                    // There's nothing to do
                    request_builder
                }
            }
            Body::Raw(body) => match args.request_type {
                RequestType::Json => request_builder
                    .header(ACCEPT, HeaderValue::from_static(JSON_ACCEPT))
                    .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE)),
                RequestType::Form => request_builder
                    .header(CONTENT_TYPE, HeaderValue::from_static(FORM_CONTENT_TYPE)),
                RequestType::Multipart => unreachable!(),
            }
            .body(body),
            Body::File {
                file_name,
                file_type,
                // We could turn this into a Content-Disposition header, but
                // that has no effect, so just ignore it
                // (Additional precedent: HTTPie ignores file_type here)
                file_name_header: _,
            } => request_builder.body(File::open(file_name)?).header(
                CONTENT_TYPE,
                file_type.unwrap_or_else(|| HeaderValue::from_static(JSON_CONTENT_TYPE)),
            ),
        };

        if args.resume {
            if let Some(file_size) = get_file_size(args.output.as_deref()) {
                request_builder = request_builder.header(RANGE, format!("bytes={}-", file_size));
                resume = Some(file_size);
            }
        }

        if let Some(auth) = args.auth {
            let (username, password) = parse_auth(auth, url.host_str().unwrap_or("<host>"))?;
            request_builder = request_builder.basic_auth(username, password);
        } else if !args.ignore_netrc {
            if let Some(host) = url.host_str() {
                if let Some(netrc) = read_netrc() {
                    if let Some((username, password)) = auth_from_netrc(host, &netrc) {
                        request_builder = request_builder.basic_auth(username, password);
                    }
                }
            }
        }
        if let Some(token) = args.bearer {
            request_builder = request_builder.bearer_auth(token);
        }

        let mut request = request_builder.headers(headers).build()?;

        headers_to_unset.iter().for_each(|h| {
            request.headers_mut().remove(h);
        });

        request
    };

    if args.download {
        request
            .headers_mut()
            .insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    };

    let buffer = Buffer::new(
        args.download,
        args.output.as_deref(),
        atty::is(Stream::Stdout) || test_pretend_term(),
        args.pretty,
    )?;
    let is_redirect = buffer.is_redirect();
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
    let pretty = args.pretty.unwrap_or_else(|| buffer.guess_pretty());
    let mut printer = Printer::new(pretty, args.style, args.stream, buffer);

    if print.request_headers {
        printer.print_request_headers(&request)?;
    }
    if print.request_body {
        printer.print_request_body(&mut request)?;
    }
    if !args.offline {
        let orig_url = request.url().clone();
        let response = client.execute(request)?;
        if print.response_headers {
            printer.print_response_headers(&response)?;
        }
        let status = response.status();
        let check_status = args.check_status.unwrap_or(!args.httpie_compat_mode);
        let exit_code: i32 = match status.as_u16() {
            _ if !(check_status) => 0,
            300..=399 if !args.follow => 3,
            400..=499 => 4,
            500..=599 => 5,
            _ => 0,
        };
        if is_redirect && exit_code != 0 {
            eprintln!("\n{}: warning: HTTP {}\n", env!("CARGO_PKG_NAME"), status);
        }
        if args.download {
            if exit_code == 0 {
                download_file(
                    response,
                    args.output,
                    &orig_url,
                    resume,
                    pretty.color(),
                    args.quiet,
                )?;
            }
        } else if print.response_body {
            printer.print_response_body(response)?;
        }
        Ok(exit_code)
    } else {
        Ok(0)
    }
}
