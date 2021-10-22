#![allow(clippy::bool_assert_comparison)]
mod auth;
mod buffer;
mod cli;
mod download;
mod formatting;
mod printer;
mod redirect;
mod request_items;
mod session;
mod to_curl;
mod utils;
mod vendored;

use std::env;
use std::fs::File;
use std::io::{stdin, Read};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use atty::Stream;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_TYPE, COOKIE, RANGE,
    USER_AGENT,
};

use crate::auth::{auth_from_netrc, parse_auth, read_netrc};
use crate::buffer::Buffer;
use crate::cli::{BodyType, Cli, HttpVersion, Print, Proxy, Verify};
use crate::download::{download_file, get_file_size};
use crate::printer::Printer;
use crate::request_items::{Body, FORM_CONTENT_TYPE, JSON_ACCEPT, JSON_CONTENT_TYPE};
use crate::session::Session;
use crate::utils::{test_mode, test_pretend_term};
use crate::vendored::reqwest_cookie_store;

fn get_user_agent() -> &'static str {
    if test_mode() {
        // Hard-coded user agent for the benefit of tests
        "xh/0.0.0 (test mode)"
    } else {
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
    }
}

fn main() {
    let args = Cli::parse();
    let bin_name = args.bin_name.clone();
    let url = args.url.clone();
    let native_tls = args.native_tls;

    match run(args) {
        Ok(exit_code) => {
            process::exit(exit_code);
        }
        Err(err) => {
            eprintln!("{}: error: {:?}", bin_name, err);
            if !native_tls && err.root_cause().to_string() == "invalid dnsname" {
                eprintln!();
                if utils::url_requires_native_tls(&url) {
                    eprintln!("rustls does not support HTTPS for IP addresses.");
                } else {
                    // Maybe we went to https://<IP> after a redirect?
                    eprintln!(
                        "This may happen because rustls does not support HTTPS for IP addresses."
                    );
                }
                if cfg!(feature = "native-tls") {
                    eprintln!("Try using the --native-tls flag.");
                } else {
                    eprintln!("Consider building with the `native-tls` feature enabled.");
                }
            }
            process::exit(1);
        }
    }
}

fn run(args: Cli) -> Result<i32> {
    if args.curl {
        to_curl::print_curl_translation(args)?;
        return Ok(0);
    }

    let warn = |msg| eprintln!("{}: warning: {}", args.bin_name, msg);

    let (mut headers, headers_to_unset) = args.request_items.headers()?;

    let ignore_stdin = args.ignore_stdin || atty::is(Stream::Stdin) || test_pretend_term();
    let body_type = args.request_items.body_type;
    let mut body = args.request_items.body()?;
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

    let mut client = Client::builder()
        .http1_title_case_headers()
        .use_rustls_tls()
        .http2_adaptive_window(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout);

    #[cfg(feature = "native-tls")]
    if args.native_tls {
        client = client.use_native_tls();
    } else if utils::url_requires_native_tls(&args.url) {
        // We should be loud about this to prevent confusion
        warn("rustls does not support HTTPS for IP addresses. native-tls will be enabled. Use --native-tls to silence this warning.");
        client = client.use_native_tls();
    }

    #[cfg(not(feature = "native-tls"))]
    if args.native_tls {
        return Err(anyhow!("This binary was built without native-tls support"));
    }

    let mut exit_code: i32 = 0;
    let mut resume: Option<u64> = None;

    if args.url.scheme() == "https" {
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
                if args.native_tls {
                    // This is not a hard error in case it gets fixed upstream
                    // https://github.com/seanmonstar/reqwest/issues/1260
                    warn("Custom CA bundles with native-tls are broken");
                }

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
            if args.native_tls {
                // Unlike the --verify case this is advertised to not work, so it's
                // not an outright bug, but it's still imaginable that it'll start working
                warn("Client certificates are not supported for native-tls")
            }

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

            // We may fail here if we can't parse it but also if we don't have the key
            let identity = reqwest::Identity::from_pem(&buffer)
                .context("Failed to load the cert/cert key files")?;
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

    if matches!(
        args.http_version,
        Some(HttpVersion::Http10) | Some(HttpVersion::Http11)
    ) {
        client = client.http1_only();
    }

    let cookie_jar = Arc::new(reqwest_cookie_store::CookieStoreMutex::default());
    client = client.cookie_provider(cookie_jar.clone());

    let client = client.build()?;

    let mut session = match &args.session {
        Some(name_or_path) => Some(
            Session::load_session(&args.url, name_or_path.clone(), args.is_session_read_only)
                .with_context(|| {
                    format!("couldn't load session {:?}", name_or_path.to_string_lossy())
                })?,
        ),
        None => None,
    };

    if let Some(ref mut s) = session {
        for (key, value) in s.headers()?.iter() {
            headers.entry(key).or_insert_with(|| value.clone());
        }
        if let Some(auth) = s.auth()? {
            headers
                .entry(AUTHORIZATION)
                .or_insert(HeaderValue::from_str(&auth)?);
        }
        s.save_headers(&headers)?;

        let mut cookie_jar = cookie_jar.lock().unwrap();
        for cookie in s.cookies() {
            match cookie_jar.insert_raw(&cookie, &args.url) {
                Ok(..) | Err(cookie_store::CookieError::Expired) => {}
                Err(err) => return Err(err.into()),
            }
        }
        if let Some(cookie) = headers.remove(COOKIE) {
            for cookie in cookie.to_str()?.split(';') {
                cookie_jar.insert_raw(&cookie.parse()?, &args.url)?;
            }
        }
    }

    let mut request = {
        let mut request_builder = client
            .request(method, args.url.clone())
            .header(
                ACCEPT_ENCODING,
                HeaderValue::from_static("gzip, deflate, br"),
            )
            .header(USER_AGENT, get_user_agent());

        if matches!(
            args.http_version,
            Some(HttpVersion::Http10) | Some(HttpVersion::Http11) | None
        ) {
            request_builder =
                request_builder.header(CONNECTION, HeaderValue::from_static("keep-alive"));
        }

        request_builder = match args.http_version {
            Some(HttpVersion::Http10) => request_builder.version(reqwest::Version::HTTP_10),
            Some(HttpVersion::Http11) => request_builder.version(reqwest::Version::HTTP_11),
            Some(HttpVersion::Http2) => request_builder.version(reqwest::Version::HTTP_2),
            None => request_builder,
        };

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
            Body::Raw(body) => match body_type {
                BodyType::Json => request_builder
                    .header(ACCEPT, HeaderValue::from_static(JSON_ACCEPT))
                    .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE)),
                BodyType::Form => request_builder
                    .header(CONTENT_TYPE, HeaderValue::from_static(FORM_CONTENT_TYPE)),
                BodyType::Multipart => unreachable!(),
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
            let (username, password) = parse_auth(auth, args.url.host_str().unwrap_or("<host>"))?;
            if let Some(ref mut s) = session {
                s.save_basic_auth(username.clone(), password.clone());
            }
            request_builder = request_builder.basic_auth(username, password);
        } else if !args.ignore_netrc {
            if let Some(host) = args.url.host_str() {
                if let Some(netrc) = read_netrc() {
                    if let Some((username, password)) = auth_from_netrc(host, &netrc) {
                        request_builder = request_builder.basic_auth(username, password);
                    }
                }
            }
        }
        if let Some(token) = args.bearer {
            if let Some(ref mut s) = session {
                s.save_bearer_auth(token.clone())
            }
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
    }

    let buffer = Buffer::new(
        args.download,
        args.output.as_deref(),
        atty::is(Stream::Stdout) || test_pretend_term(),
        args.pretty,
    )?;
    let is_output_redirected = buffer.is_redirect();
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
    let mut printer = Printer::new(print.clone(), pretty, args.style, args.stream, buffer);

    printer.print_request_headers(&request, &*cookie_jar)?;
    printer.print_request_body(&mut request)?;

    if !args.offline {
        let response = if args.follow {
            let mut client =
                redirect::RedirectFollower::new(&client, args.max_redirects.unwrap_or(10));
            if let Some(history_print) = args.history_print {
                printer.print = history_print;
            }
            if args.all {
                client.on_redirect(|prev_response, next_request| {
                    printer.print_response_headers(&prev_response)?;
                    printer.print_response_body(prev_response)?;
                    printer.print_separator()?;
                    printer.print_request_headers(next_request, &*cookie_jar)?;
                    printer.print_request_body(next_request)?;
                    Ok(())
                });
            }
            client.execute(request)?
        } else {
            client.execute(request)?
        };

        let status = response.status();
        if args.check_status.unwrap_or(!args.httpie_compat_mode) {
            exit_code = match status.as_u16() {
                300..=399 if !args.follow => 3,
                400..=499 => 4,
                500..=599 => 5,
                _ => 0,
            }
        }
        if is_output_redirected && exit_code != 0 {
            warn(&format!("HTTP {}", status));
        }

        printer.print = print;
        printer.print_response_headers(&response)?;
        if args.download {
            if exit_code == 0 {
                download_file(
                    response,
                    args.output,
                    &args.url,
                    resume,
                    pretty.color(),
                    args.quiet,
                )?;
            }
        } else {
            printer.print_response_body(response)?;
        }
    }

    if let Some(ref mut s) = session {
        let cookie_jar = cookie_jar.lock().unwrap();
        s.save_cookies(
            cookie_jar
                .matches(&args.url)
                .into_iter()
                .map(|c| cookie_crate::Cookie::from(c.clone()))
                .collect(),
        );
        s.persist()
            .with_context(|| format!("couldn't persist session {}", s.path.display()))?;
    }

    Ok(exit_code)
}
