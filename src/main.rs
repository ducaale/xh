#![allow(clippy::bool_assert_comparison)]
mod auth;
mod buffer;
mod cli;
mod download;
mod formatting;
mod middleware;
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
use redirect::RedirectFollower;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, COOKIE, RANGE, USER_AGENT,
};
use reqwest::tls;

use crate::auth::{read_netrc, Auth, DigestAuthMiddleware};
use crate::buffer::Buffer;
use crate::cli::{BodyType, Cli, HttpVersion, Print, Proxy, Verify};
use crate::download::{download_file, get_file_size};
use crate::middleware::ClientWithMiddleware;
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
            let msg = err.root_cause().to_string();
            if !native_tls && msg == "invalid dnsname" {
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
            if native_tls && msg == "invalid minimum TLS version for backend" {
                eprintln!();
                eprintln!("Try running without the --native-tls flag.");
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

    let warn = {
        let bin_name = &args.bin_name;
        move |msg| eprintln!("{}: warning: {}", bin_name, msg)
    };

    let (mut headers, headers_to_unset) = args.request_items.headers()?;

    let ignore_stdin = args.ignore_stdin || atty::is(Stream::Stdin) || test_pretend_term();
    let body_type = args.request_items.body_type;
    let body_from_request_items = args.request_items.body()?;

    let body = match (!ignore_stdin, args.raw, !body_from_request_items.is_empty()) {
        (true, None, false) => {
            let mut buffer = Vec::new();
            stdin().read_to_end(&mut buffer)?;
            Body::Raw(buffer)
        }
        (false, Some(raw), false) => Body::Raw(raw.as_bytes().to_vec()),
        (false, None, _) => body_from_request_items,

        (true, Some(_), _) => {
            return Err(anyhow!(
                "Request body from stdin and --raw cannot be mixed. \
                Pass --ignore-stdin to ignore standard input."
            ))
        }
        (true, _, true) => {
            if args.multipart {
                return Err(anyhow!("Cannot build a multipart request body from stdin"));
            } else {
                return Err(anyhow!(
                    "Request body (from stdin) and request data (key=value) cannot be mixed. \
                    Pass --ignore-stdin to ignore standard input."
                ));
            }
        }
        (_, Some(_), true) => {
            if args.multipart {
                return Err(anyhow!("Cannot build a multipart request body from --raw"));
            } else {
                return Err(anyhow!(
                    "Request body (from --raw) and request data (key=value) cannot be mixed."
                ));
            }
        }
    };

    let method = args.method.unwrap_or_else(|| body.pick_method());
    let timeout = args.timeout.and_then(|t| t.as_duration());

    let mut client = Client::builder()
        .http1_title_case_headers()
        .use_rustls_tls()
        .http2_adaptive_window(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout);

    if let Some(Some(tls_version)) = args.ssl {
        client = client
            .min_tls_version(tls_version)
            .max_tls_version(tls_version);

        #[cfg(feature = "native-tls")]
        if !args.native_tls && tls_version < tls::Version::TLS_1_2 {
            warn("rustls does not support older TLS versions. native-tls will be enabled. Use --native-tls to silence this warning.");
            client = client.use_native_tls();
        }

        #[cfg(not(feature = "native-tls"))]
        if tls_version < tls::Version::TLS_1_2 {
            warn("rustls does not support older TLS versions. Consider building with the `native-tls` feature enabled.");
        }
    }

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
    let mut auth = None;
    let mut save_auth_in_session = true;

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
        auth = s.auth()?;
        for (key, value) in s.headers()?.iter() {
            headers.entry(key).or_insert_with(|| value.clone());
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

        let auth_type = args.auth_type.unwrap_or_default();
        if let Some(auth_from_arg) = args.auth {
            auth = Some(Auth::from_str(
                &auth_from_arg,
                auth_type,
                args.url.host_str().unwrap_or("<host>"),
            )?);
        } else if !args.ignore_netrc {
            if let (Some(host), Some(netrc)) = (args.url.host_str(), read_netrc()) {
                auth = Auth::from_netrc(&netrc, auth_type, host);
                save_auth_in_session = false;
            }
        }

        if let Some(auth) = &auth {
            if let Some(ref mut s) = session {
                if save_auth_in_session {
                    s.save_auth(auth);
                }
            }
            request_builder = match auth {
                Auth::Basic(username, password) => {
                    request_builder.basic_auth(username, password.as_ref())
                }
                Auth::Bearer(token) => request_builder.bearer_auth(token),
                Auth::Digest(..) => request_builder,
            }
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
    let mut printer = Printer::new(pretty, args.style, args.stream, buffer);

    let response_charset = args.response_charset;
    let response_mime = args.response_mime.as_deref();

    if print.request_headers {
        printer.print_request_headers(&request, &*cookie_jar)?;
    }
    if print.request_body {
        printer.print_request_body(&mut request)?;
    }

    if !args.offline {
        let response = {
            let history_print = args.history_print.unwrap_or(print);
            let mut client = ClientWithMiddleware::new(&client);
            if args.all {
                client = client.with_printer(|prev_response, next_request| {
                    if history_print.response_headers {
                        printer.print_response_headers(&prev_response)?;
                    }
                    if history_print.response_body {
                        printer.print_response_body(
                            prev_response,
                            response_charset,
                            response_mime,
                        )?;
                        printer.print_separator()?;
                    }
                    if history_print.request_headers {
                        printer.print_request_headers(next_request, &*cookie_jar)?;
                    }
                    if history_print.request_body {
                        printer.print_request_body(next_request)?;
                    }
                    Ok(())
                });
            }
            if args.follow {
                client = client.with(RedirectFollower::new(args.max_redirects.unwrap_or(10)));
            }
            if let Some(Auth::Digest(username, password)) = &auth {
                client = client.with(DigestAuthMiddleware::new(username, password));
            }
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

        if print.response_headers {
            printer.print_response_headers(&response)?;
        }
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
        } else if print.response_body {
            printer.print_response_body(response, response_charset, response_mime)?;
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
