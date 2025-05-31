#![allow(clippy::bool_assert_comparison)]
mod auth;
mod buffer;
mod cli;
mod content_disposition;
mod decoder;
mod download;
mod error_reporting;
mod formatting;
mod generation;
mod jwt;
mod middleware;
mod nested_json;
mod netrc;
mod printer;
mod redacted;
mod redirect;
mod request_items;
mod session;
mod to_curl;
mod utils;

use std::env;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write as _};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cookie_store::{CookieStore, RawCookie};
use flate2::write::ZlibEncoder;
use hyper::header::CONTENT_ENCODING;
use redirect::RedirectFollower;
use reqwest::blocking::{Body as ReqwestBody, Client};
use reqwest::header::{
    HeaderValue, ACCEPT, ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, COOKIE, RANGE, USER_AGENT,
};
use reqwest::tls;
use url::{Host, Url};
use utils::reason_phrase;

use crate::auth::{Auth, DigestAuthMiddleware};
use crate::buffer::Buffer;
use crate::cli::{Cli, FormatOptions, HttpVersion, Print, Proxy, Verify};
use crate::download::{download_file, get_file_size};
use crate::middleware::ClientWithMiddleware;
use crate::printer::Printer;
use crate::request_items::{Body, FORM_CONTENT_TYPE, JSON_ACCEPT, JSON_CONTENT_TYPE};
use crate::session::Session;
use crate::utils::{test_mode, test_pretend_term, url_with_query};

#[cfg(not(any(feature = "native-tls", feature = "rustls")))]
compile_error!("Either native-tls or rustls feature must be enabled!");

fn get_user_agent() -> &'static str {
    if test_mode() {
        // Hard-coded user agent for the benefit of tests
        "xh/0.0.0 (test mode)"
    } else {
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
    }
}

fn main() -> ExitCode {
    let args = Cli::parse();

    if args.debug {
        setup_backtraces();
    }
    args.logger_config().init();
    // HTTPie also prints the language version, library versions, and OS version.
    // But those are harder to access for us (and perhaps less likely to cause quirks).
    log::debug!("xh {} {}", env!("CARGO_PKG_VERSION"), env!("XH_FEATURES"));
    log::debug!("{args:#?}");

    let native_tls = args.native_tls;
    let bin_name = args.bin_name.clone();

    match run(args) {
        Ok(exit_code) => exit_code,
        Err(err) => {
            log::debug!("{err:#?}");
            eprintln!("{bin_name}: error: {err:?}");

            for message in error_reporting::additional_messages(&err, native_tls) {
                eprintln!();
                eprintln!("{message}");
            }

            error_reporting::exit_code(&err)
        }
    }
}

fn run(args: Cli) -> Result<ExitCode> {
    if let Some(generate) = args.generate {
        generation::generate(&args.bin_name, generate);
        return Ok(ExitCode::SUCCESS);
    }

    if args.curl {
        to_curl::print_curl_translation(args)?;
        return Ok(ExitCode::SUCCESS);
    }

    // Handle JWT token operations
    if args.jwt_list {
        return handle_jwt_list();
    }

    if let Some(token_name) = &args.jwt_delete {
        return handle_jwt_delete(token_name);
    }

    if let Some(token_name) = &args.jwt_show {
        return handle_jwt_show(token_name);
    }

    if let Some(token_name) = &args.jwt_refresh {
        return handle_jwt_refresh(token_name, &args);
    }

    if let Some(jwt_request) = &args.jwt_request {
        return handle_jwt_request(jwt_request, &args);
    }

    let (mut headers, headers_to_unset) = args.request_items.headers()?;
    let url = url_with_query(args.url, &args.request_items.query()?);
    log::debug!("Complete URL: {url}");

    let use_stdin = !(args.ignore_stdin || io::stdin().is_terminal() || test_pretend_term());

    let body = if use_stdin {
        if !args.request_items.is_body_empty() {
            if args.multipart {
                // Multipart bodies are never "empty", so we can get here without request items
                return Err(anyhow!("Cannot build a multipart request body from stdin"));
            } else {
                return Err(anyhow!(
                    "Request body (from stdin) and request data (key=value) cannot be mixed. \
                    Pass --ignore-stdin to ignore standard input."
                ));
            }
        }
        if args.raw.is_some() {
            return Err(anyhow!(
                "Request body from stdin and --raw cannot be mixed. \
                Pass --ignore-stdin to ignore standard input."
            ));
        }
        let mut buffer = Vec::new();
        io::stdin().read_to_end(&mut buffer)?;
        Body::Raw(buffer)
    } else if let Some(raw) = args.raw {
        Body::Raw(raw.into_bytes())
    } else {
        args.request_items.body()?
    };

    let method = args.method.unwrap_or_else(|| body.pick_method());
    log::debug!("HTTP method: {method}");

    let mut client = Client::builder()
        .http1_title_case_headers()
        .http2_adaptive_window(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(args.timeout.and_then(|t| t.as_duration()))
        .no_gzip()
        .no_deflate()
        .no_brotli();

    #[cfg(feature = "rustls")]
    if !args.native_tls {
        client = client.use_rustls_tls();
    }

    if let Some(tls_version) = args.ssl.and_then(Into::into) {
        client = client
            .min_tls_version(tls_version)
            .max_tls_version(tls_version);

        #[cfg(feature = "native-tls")]
        if !args.native_tls && tls_version < tls::Version::TLS_1_2 {
            log::warn!("rustls does not support older TLS versions. native-tls will be enabled. Use --native-tls to silence this warning.");
            client = client.use_native_tls();
        }

        #[cfg(not(feature = "native-tls"))]
        if tls_version < tls::Version::TLS_1_2 {
            log::warn!("rustls does not support older TLS versions. Consider building with the `native-tls` feature enabled.");
        }
    }

    #[cfg(feature = "native-tls")]
    if args.native_tls {
        client = client.use_native_tls();
    }

    #[cfg(not(feature = "native-tls"))]
    if args.native_tls {
        return Err(anyhow!("This binary was built without native-tls support"));
    }

    let mut failure_code = None;
    let mut resume: Option<u64> = None;
    let mut auth = None;
    let mut save_auth_in_session = true;

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
                log::warn!("Custom CA bundles with native-tls are broken");
            }

            let mut buffer = Vec::new();
            let mut file = File::open(&path).with_context(|| {
                format!("Failed to open the custom CA bundle: {}", path.display())
            })?;
            file.read_to_end(&mut buffer).with_context(|| {
                format!("Failed to read the custom CA bundle: {}", path.display())
            })?;

            client = client.tls_built_in_root_certs(false);
            for pem in pem::parse_many(buffer)? {
                let certificate = reqwest::Certificate::from_pem(pem::encode(&pem).as_bytes())
                    .with_context(|| {
                        format!("Failed to load the custom CA bundle: {}", path.display())
                    })?;
                client = client.add_root_certificate(certificate);
            }
            client
        }
    };

    #[cfg(feature = "rustls")]
    if let Some(cert) = args.cert {
        if args.native_tls {
            // Unlike the --verify case this is advertised to not work, so it's
            // not an outright bug, but it's still imaginable that it'll start working
            log::warn!("Client certificates are not supported for native-tls");
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
    }
    #[cfg(not(feature = "rustls"))]
    if args.cert.is_some() {
        // Unlike the --verify case this is advertised to not work, so it's
        // not an outright bug, but it's still imaginable that it'll start working
        log::warn!("Client certificates are not supported for native-tls and this binary was built without rustls support");
    }

    for proxy in args.proxy.into_iter().rev() {
        client = client.proxy(match proxy {
            Proxy::Http(url) => reqwest::Proxy::http(url),
            Proxy::Https(url) => reqwest::Proxy::https(url),
            Proxy::All(url) => reqwest::Proxy::all(url),
        }?);
    }

    client = match args.http_version {
        Some(HttpVersion::Http10 | HttpVersion::Http11) => client.http1_only(),
        Some(HttpVersion::Http2PriorKnowledge) => client.http2_prior_knowledge(),
        Some(HttpVersion::Http2) => client,
        None => client,
    };

    let cookie_jar = Arc::new(reqwest_cookie_store::CookieStoreMutex::default());
    client = client.cookie_provider(cookie_jar.clone());

    client = match (args.ipv4, args.ipv6) {
        (true, false) => client.local_address(IpAddr::from(Ipv4Addr::UNSPECIFIED)),
        (false, true) => client.local_address(IpAddr::from(Ipv6Addr::UNSPECIFIED)),
        _ => client,
    };

    if let Some(name_or_ip) = &args.interface {
        if let Ok(ip_addr) = IpAddr::from_str(name_or_ip) {
            client = client.local_address(ip_addr);
        } else {
            #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
            {
                client = client.interface(name_or_ip);
            }

            #[cfg(not(any(target_os = "android", target_os = "fuchsia", target_os = "linux")))]
            {
                #[cfg(not(feature = "network-interface"))]
                return Err(anyhow!(
                    "This binary was built without support for binding to interfaces. Enable the `network-interface` feature."
                ));

                #[cfg(feature = "network-interface")]
                {
                    use network_interface::{NetworkInterface, NetworkInterfaceConfig};
                    let ip_addr = NetworkInterface::show()?
                        .iter()
                        .find_map(|interface| {
                            if &interface.name == name_or_ip {
                                if let Some(addr) = interface.addr.first() {
                                    return Some(addr.ip());
                                }
                            }
                            None
                        })
                        .with_context(|| format!("Couldn't bind to {:?}", name_or_ip))?;
                    log::debug!("Resolved {name_or_ip:?} to {ip_addr:?}");
                    client = client.local_address(ip_addr);
                }
            }
        };
    }

    for resolve in args.resolve {
        client = client.resolve(&resolve.domain, SocketAddr::new(resolve.addr, 0));
    }

    log::trace!("Finalizing reqwest client");
    log::trace!("{client:#?}");
    let client = client.build()?;

    let mut session = match &args.session {
        Some(name_or_path) => Some(
            Session::load_session(url.clone(), name_or_path.clone(), args.is_session_read_only)
                .with_context(|| {
                    format!("couldn't load session {:?}", name_or_path.to_string_lossy())
                })?,
        ),
        None => None,
    };

    if let Some(ref mut s) = session {
        auth = s.auth()?;

        headers = {
            let mut session_headers = s.headers()?;
            session_headers.extend(headers);
            session_headers
        };
        s.save_headers(&headers)?;

        let mut cookie_jar = cookie_jar.lock().unwrap();
        *cookie_jar = CookieStore::from_cookies(s.cookies(), false)
            .context("Failed to load cookies from session file")?;

        if let Some(cookie) = headers.remove(COOKIE) {
            for cookie in RawCookie::split_parse(cookie.to_str()?) {
                cookie_jar.insert_raw(&cookie?, &url)?;
            }
        }
    }

    let mut request = {
        let mut request_builder = client
            .request(method, url.clone())
            .header(
                ACCEPT_ENCODING,
                HeaderValue::from_static("gzip, deflate, br, zstd"),
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
            Some(HttpVersion::Http2 | HttpVersion::Http2PriorKnowledge) => {
                request_builder.version(reqwest::Version::HTTP_2)
            }
            None => request_builder,
        };

        request_builder = match body {
            Body::Form(body) => request_builder.form(&body),
            Body::Multipart(body) => request_builder.multipart(body),
            Body::Json(body) => {
                // An empty JSON body would produce null instead of "", so
                // this is the one kind of body that needs an is_null() check
                if !body.is_null() {
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
            Body::Raw(body) => {
                if args.form {
                    request_builder
                        .header(CONTENT_TYPE, HeaderValue::from_static(FORM_CONTENT_TYPE))
                } else {
                    request_builder
                        .header(ACCEPT, HeaderValue::from_static(JSON_ACCEPT))
                        .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE))
                }
            }
            .body(body),
            Body::File {
                file_name,
                file_type,
                file_name_header,
            } => {
                if file_name_header.is_some() {
                    // Content-Disposition headers aren't allowed in this context (only responses
                    // and multipart request parts), so just ignore it
                    // (Additional precedent: HTTPie ignores file_type here)
                    log::warn!(
                        "Ignoring ;filename= tag for single-file body. Consider --multipart."
                    );
                }
                request_builder.body(File::open(file_name)?).header(
                    CONTENT_TYPE,
                    file_type.unwrap_or_else(|| HeaderValue::from_static(JSON_CONTENT_TYPE)),
                )
            }
        };

        if args.resume {
            if headers.contains_key(RANGE) {
                // There are no good options here, and `--continue` works on a
                // best-effort basis, so give up with a warning.
                //
                // - HTTPie:
                //   - If the file does not exist, errors when the response has
                //     an apparently incorrect content range, as though it sent
                //     `Range: bytes=0-`.
                //   - If the file already exists, ignores the manual header
                //     and downloads what's probably the wrong data.
                // - wget gives priority to the manual header and keeps failing
                //   and retrying the download (with or without existing file).
                // - curl gives priority to the manual header and reports that
                //   the server does not support partial downloads. It also has
                //   a --range CLI option which is mutually exclusive with its
                //   --continue-at option.
                log::warn!(
                    "--continue can't be used with a 'Range:' header. --continue will be disabled."
                );
            } else if let Some(file_size) = get_file_size(args.output.as_deref()) {
                request_builder = request_builder.header(RANGE, format!("bytes={}-", file_size));
                resume = Some(file_size);
            }
        }

        let auth_type = args.auth_type.unwrap_or_default();
        
        // Handle JWT token authentication first
        if let Some(jwt_token_name) = &args.jwt_token {
            let mut store = jwt::JwtTokenStore::load()?;
            match store.get_token(jwt_token_name) {
                Some(jwt_token) => {
                    // Check if token needs refresh and attempt it
                    let needs_refresh = jwt_token.is_expired() || jwt_token.expires_within(300); // 5 minutes
                    
                    if needs_refresh && jwt_token.refresh_token.is_some() {
                        log::info!("JWT token '{}' is expired or expiring soon, attempting refresh", jwt_token_name);
                        
                        // Try to refresh using the provided refresh URL
                        let refresh_url = args.jwt_refresh_url.as_deref();
                        if let Err(e) = store.refresh_token_if_needed(jwt_token_name, &client, refresh_url) {
                            log::warn!("Failed to refresh JWT token '{}': {}", jwt_token_name, e);
                        }
                        
                        // Save the potentially updated token store
                        if let Err(e) = store.save() {
                            log::warn!("Failed to save updated token store: {}", e);
                        }
                    } else if needs_refresh {
                        log::warn!("JWT token '{}' is expired but no refresh token available", jwt_token_name);
                    }
                    
                    // Get the (potentially refreshed) token
                    let final_token = store.get_token(jwt_token_name)
                        .ok_or_else(|| anyhow!("Token disappeared during refresh"))?;
                    
                    auth = Some(Auth::Bearer(final_token.token.clone()));
                    save_auth_in_session = false; // JWT tokens are managed separately
                }
                None => {
                    return Err(anyhow!("JWT token '{}' not found. Use --jwt-list to see available tokens or --jwt-request to create a new one.", jwt_token_name));
                }
            }
        } else if let Some(auth_from_arg) = args.auth {
            auth = Some(Auth::from_str(
                &auth_from_arg,
                auth_type,
                url.host_str().unwrap_or("<host>"),
            )?);
        } else if !args.ignore_netrc {
            // I don't know if it's possible for host() to return None
            // But if it does we still want to use the default entry, if there is one
            let host = url.host().unwrap_or(Host::Domain(""));
            if let Some(entry) = netrc::find_entry(host) {
                auth = Auth::from_netrc(auth_type, entry);
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

        if args.compress >= 1 {
            if request.headers().contains_key(CONTENT_ENCODING) {
                // HTTPie overrides the original Content-Encoding header in this case
                log::warn!("--compress can't be used with a 'Content-Encoding:' header. --compress will be disabled.");
            } else if let Some(body) = request.body_mut() {
                // TODO: Compress file body (File) without buffering
                let body_bytes = body.buffer()?;
                let mut encoder = ZlibEncoder::new(Vec::new(), Default::default());
                encoder.write_all(body_bytes)?;
                let output = encoder.finish()?;
                if output.len() < body_bytes.len() || args.compress >= 2 {
                    *body = ReqwestBody::from(output);
                    request
                        .headers_mut()
                        .insert(CONTENT_ENCODING, HeaderValue::from_static("deflate"));
                }
            }
        }

        for header in &headers_to_unset {
            request.headers_mut().remove(header);
        }

        request
    };

    if args.download {
        request
            .headers_mut()
            .insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    }

    log::trace!("Built reqwest request");
    // Note: Debug impl is incomplete?
    log::trace!("{request:#?}");

    let buffer = Buffer::new(
        args.download,
        args.output.as_deref(),
        io::stdout().is_terminal() || test_pretend_term(),
    )?;
    let is_output_redirected = buffer.is_redirect();
    let print = match args.print {
        Some(print) => print,
        None => Print::new(
            args.verbose,
            args.headers,
            args.body,
            args.meta,
            args.quiet > 0,
            args.offline,
            &buffer,
        ),
    };
    let theme = args.style.unwrap_or_default();
    let pretty = args.pretty.unwrap_or_else(|| buffer.guess_pretty());
    let format_options = args
        .format_options
        .iter()
        .fold(FormatOptions::default(), FormatOptions::merge);
    let mut printer = Printer::new(pretty, theme, args.stream, buffer, format_options);

    let response_charset = args.response_charset;
    let response_mime = args.response_mime.as_deref();

    if print.request_headers {
        printer.print_request_headers(&request, &*cookie_jar)?;
    }
    if print.request_body {
        printer.print_request_body(&mut request)?;
    }

    if !args.offline {
        let mut response = {
            let history_print = args.history_print.unwrap_or(print);
            let mut client = ClientWithMiddleware::new(&client);
            if args.all {
                client = client.with_printer(|prev_response, next_request| {
                    if history_print.response_headers {
                        printer.print_response_headers(prev_response)?;
                    }
                    if history_print.response_body {
                        printer.print_response_body(
                            prev_response,
                            response_charset,
                            response_mime,
                        )?;
                        printer.print_separator()?;
                    }
                    if history_print.response_meta {
                        printer.print_response_meta(prev_response)?;
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
            match status.as_u16() {
                300..=399 if !args.follow => failure_code = Some(ExitCode::from(3)),
                400..=499 => failure_code = Some(ExitCode::from(4)),
                500..=599 => failure_code = Some(ExitCode::from(5)),
                _ => (),
            }

            // Print this if the status code isn't otherwise ending up in the terminal.
            // HTTPie looks at --quiet, since --quiet always suppresses the response
            // headers even if you pass --print=h. But --print takes precedence for us.
            if failure_code.is_some() && (is_output_redirected || !print.response_headers) {
                log::warn!("HTTP {} {}", status.as_u16(), reason_phrase(&response));
            }
        }

        if print.response_headers {
            printer.print_response_headers(&response)?;
        }
        if args.download {
            if failure_code.is_none() {
                download_file(
                    response,
                    args.output,
                    &url,
                    resume,
                    pretty.color(),
                    args.quiet > 0,
                )?;
            }
        } else {
            if print.response_body {
                printer.print_response_body(&mut response, response_charset, response_mime)?;
                if print.response_meta {
                    printer.print_separator()?;
                }
            }
            if print.response_meta {
                printer.print_response_meta(&response)?;
            }
        }
    }

    if let Some(ref mut s) = session {
        let cookie_jar = cookie_jar.lock().unwrap();
        s.save_cookies(cookie_jar.iter_unexpired());
        s.persist()
            .with_context(|| format!("couldn't persist session {}", s.path.display()))?;
    }

    Ok(failure_code.unwrap_or(ExitCode::SUCCESS))
}

/// Configure backtraces for standard panics and anyhow using `$RUST_BACKTRACE`.
///
/// Note: they only check the environment variable once, so this won't take effect if
/// we do it after a panic has already happened or an anyhow error has already been
/// created.
///
/// It's possible for CLI parsing to create anyhow errors before we call this function
/// but it looks like those errors are always fatal.
///
/// https://github.com/rust-lang/rust/issues/93346 will become the preferred way to
/// configure panic backtraces.
fn setup_backtraces() {
    if std::env::var_os("RUST_BACKTRACE").is_some() {
        // User knows best
        return;
    }

    // SAFETY: No other threads are running at this time.
    // (Will become unsafe in the 2024 edition.)
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "1");
    }
}

// JWT token management functions

/// Handle listing all stored JWT tokens
fn handle_jwt_list() -> Result<ExitCode> {
    let store = jwt::JwtTokenStore::load()?;
    let tokens = store.list_tokens();
    
    if tokens.is_empty() {
        println!("No JWT tokens stored.");
    } else {
        println!("Stored JWT tokens:");
        for token_name in tokens {
            let token = store.get_token(token_name).unwrap();
            let status = if token.is_expired() { " (expired)" } else { "" };
            println!("  {} - {} token{}", token_name, token.token_type, status);
        }
    }
    
    Ok(ExitCode::SUCCESS)
}

/// Handle deleting a stored JWT token
fn handle_jwt_delete(token_name: &str) -> Result<ExitCode> {
    let mut store = jwt::JwtTokenStore::load()?;
    
    if store.remove_token(token_name).is_some() {
        store.save()?;
        println!("Deleted JWT token: {}", token_name);
    } else {
        eprintln!("JWT token '{}' not found.", token_name);
        return Ok(ExitCode::from(1));
    }
    
    Ok(ExitCode::SUCCESS)
}

/// Handle showing details of a stored JWT token
fn handle_jwt_show(token_name: &str) -> Result<ExitCode> {
    let store = jwt::JwtTokenStore::load()?;
    
    if let Some(token) = store.get_token(token_name) {
        println!("JWT Token: {}", token_name);
        println!("  Type: {}", token.token_type);
        println!("  Issued: {}", format_timestamp(token.issued_at));
        if let Some(expires_at) = token.expires_at {
            println!("  Expires: {}", format_timestamp(expires_at));
            println!("  Status: {}", if token.is_expired() { "expired" } else { "valid" });
        } else {
            println!("  Expires: never");
            println!("  Status: valid");
        }
        if let Some(scope) = &token.scope {
            println!("  Scope: {}", scope);
        }
        println!("  Has refresh token: {}", token.refresh_token.is_some());
        // Don't print the actual token for security reasons
        println!("  Token: [hidden]");
    } else {
        eprintln!("JWT token '{}' not found.", token_name);
        return Ok(ExitCode::from(1));
    }
    
    Ok(ExitCode::SUCCESS)
}

/// Handle requesting a new JWT token
fn handle_jwt_request(jwt_request: &str, args: &Cli) -> Result<ExitCode> {
    let (name, url_str) = parse_jwt_request_arg(jwt_request)?;
    let url: Url = url_str.parse()
        .with_context(|| format!("Invalid URL: {}", url_str))?;
    
    let mut request = jwt::JwtTokenRequest::new(url)
        .with_grant_type(args.jwt_grant_type.clone());
    
    // Set credentials
    if let (Some(username), Some(password)) = (&args.jwt_username, &args.jwt_password) {
        request = request.with_credentials(username.clone(), password.clone());
    } else if let (Some(client_id), Some(client_secret)) = (&args.jwt_client_id, &args.jwt_client_secret) {
        request = request.with_client_credentials(client_id.clone(), client_secret.clone());
    } else {
        return Err(anyhow!("JWT token request requires either username/password or client_id/client_secret"));
    }
    
    if let Some(scope) = &args.jwt_scope {
        request = request.with_scope(scope.clone());
    }
    
    // Create HTTP client
    let client = Client::builder()
        .timeout(args.timeout.as_ref().and_then(|t| t.as_duration()))
        .build()?;
    
    println!("Requesting JWT token from {}...", request.url);
    
    // Request the token
    let token = request.request_token_sync(&client)
        .context("Failed to request JWT token")?;
    
    // Store the token
    let mut store = jwt::JwtTokenStore::load()?;
    store.store_token(&name, token);
    store.save()?;
    
    println!("Successfully stored JWT token: {}", name);
    
    Ok(ExitCode::SUCCESS)
}

/// Parse JWT request argument in format "name=url"
fn parse_jwt_request_arg(arg: &str) -> Result<(String, String)> {
    if let Some((name, url)) = arg.split_once('=') {
        if name.is_empty() || url.is_empty() {
            return Err(anyhow!("Invalid JWT request format. Use: name=url"));
        }
        Ok((name.to_string(), url.to_string()))
    } else {
        Err(anyhow!("Invalid JWT request format. Use: name=url"))
    }
}

/// Format timestamp for display
fn format_timestamp(timestamp: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let duration = std::time::Duration::from_secs(timestamp);
    let datetime = SystemTime::UNIX_EPOCH + duration;
    
    // Convert to local time string (simplified)
    match datetime.duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{} seconds since epoch", d.as_secs()),
        Err(_) => "invalid timestamp".to_string(),
    }
}

/// Handle refreshing a stored JWT token
fn handle_jwt_refresh(token_name: &str, args: &Cli) -> Result<ExitCode> {
    let mut store = jwt::JwtTokenStore::load()?;
    
    if store.get_token(token_name).is_none() {
        eprintln!("JWT token '{}' not found.", token_name);
        return Ok(ExitCode::from(1));
    }
    
    let token = store.get_token(token_name).unwrap();
    if token.refresh_token.is_none() {
        eprintln!("JWT token '{}' has no refresh token.", token_name);
        return Ok(ExitCode::from(1));
    }
    
    if args.jwt_refresh_url.is_none() {
        eprintln!("No refresh URL provided. Use --jwt-refresh-url to specify the token refresh endpoint.");
        return Ok(ExitCode::from(1));
    }
    
    // Create HTTP client
    let client = Client::builder()
        .timeout(args.timeout.as_ref().and_then(|t| t.as_duration()))
        .build()?;
    
    println!("Refreshing JWT token '{}'...", token_name);
    
    // Attempt to refresh the token
    let refresh_url = args.jwt_refresh_url.as_ref().unwrap();
    match store.refresh_token_if_needed(token_name, &client, Some(refresh_url)) {
        Ok(true) => {
            store.save()?;
            println!("Successfully refreshed JWT token: {}", token_name);
            Ok(ExitCode::SUCCESS)
        }
        Ok(false) => {
            println!("JWT token '{}' did not need refreshing.", token_name);
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("Failed to refresh JWT token '{}': {}", token_name, e);
            Ok(ExitCode::from(1))
        }
    }
}
