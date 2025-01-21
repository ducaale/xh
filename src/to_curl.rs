use std::io::{stderr, stdout, Write};

use anyhow::{anyhow, Context, Result};
use os_display::Quotable;
use reqwest::{tls, Method};
use std::ffi::OsString;

use crate::cli::{AuthType, Cli, HttpVersion, Verify};
use crate::request_items::{Body, RequestItem, FORM_CONTENT_TYPE, JSON_ACCEPT, JSON_CONTENT_TYPE};
use crate::utils::{url_with_query, HeaderValueExt};

pub fn print_curl_translation(args: Cli) -> Result<()> {
    let cmd = translate(args)?;
    let mut stderr = stderr();
    for warning in &cmd.warnings {
        writeln!(stderr, "Warning: {}", warning)?;
    }
    if !cmd.warnings.is_empty() {
        writeln!(stderr)?;
    }
    writeln!(stdout(), "{}", cmd)?;
    Ok(())
}

pub struct Command {
    pub long: bool,
    pub args: Vec<OsString>,
    pub env: Vec<(&'static str, String)>,
    pub warnings: Vec<String>,
}

impl Command {
    fn new(long: bool) -> Command {
        Command {
            long,
            args: Vec::new(),
            env: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn opt(&mut self, short: &'static str, long: &'static str) {
        if self.long {
            self.args.push(long.into());
        } else {
            self.args.push(short.into());
        }
    }

    fn arg(&mut self, arg: impl Into<OsString>) {
        self.args.push(arg.into());
    }

    fn header(&mut self, name: &str, value: &str) {
        self.opt("-H", "--header");
        self.arg(format!("{}: {}", name, value));
    }

    fn env(&mut self, var: &'static str, value: impl Into<String>) {
        self.env.push((var, value.into()));
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, value) in &self.env {
            // This is wrong for Windows, but there doesn't seem to be a
            // right way
            write!(f, "{}={} ", key, value.maybe_quote())?;
        }
        write!(f, "curl")?;
        for arg in &self.args {
            write!(f, " {}", arg.maybe_quote().external(true))?;
        }
        Ok(())
    }
}

pub fn translate(args: Cli) -> Result<Command> {
    let (headers, headers_to_unset) = args.request_items.headers()?;

    let mut cmd = Command::new(args.curl_long);

    let ignored = [
        // No equivalent
        (args.offline, "--offline"),
        // Already the default
        (args.body, "-b/--body"),
        // No straightforward equivalent
        (args.print.is_some(), "-p/--print"),
        // No equivalent
        (args.pretty.is_some(), "--pretty"),
        // No equivalent
        (args.style.is_some(), "-s/--style"),
        // No equivalent
        (args.compress > 0, "-x/--compress"),
        // No equivalent
        (args.response_charset.is_some(), "--response-charset"),
        // No equivalent
        (args.response_mime.is_some(), "--response-mime"),
        // Already the default
        (args.all, "--all"),
        // No (straightforward?) equivalent
        (args.history_print.is_some(), "-P/--history-print"),
        // Might be possible to emulate with --cookie-jar but tricky
        (args.session.is_some(), "--session"),
        // Already the default (usually, depends on compile time options)
        // Unclear if you can even change this at runtime
        (args.native_tls, "--native-tls"),
    ];

    for (present, flag) in ignored {
        if present {
            cmd.warn(format!("Ignored {}", flag));
        }
    }

    if args.follow && !matches!(args.method, Some(Method::GET) | None) {
        cmd.warn("Using a combination of -X/--request and -L/--location which may cause unintended side effects.");
    }

    // Silently ignored:
    // - .ignore_stdin: assumed by default
    //   (to send stdin, --data-binary @- -H 'Content-Type: application/octet-stream')
    // - .curl and .curl_long: you are here

    // Output options
    if args.verbose > 0 {
        // Far from an exact match, but it does print the request headers
        cmd.opt("-v", "--verbose");
    }
    if args.quiet > 0 {
        // Also not an exact match but it suppresses error messages which
        // is sorta like suppressing warnings
        cmd.opt("-s", "--silent");
    }
    if args.debug {
        // Again not an exact match but it's something
        // This actually overrides --verbose
        cmd.arg("--trace");
        cmd.arg("-");
    }
    if args.stream == Some(true) {
        // curl sorta streams by default, but its buffer stops it from
        // showing up right away
        cmd.opt("-N", "--no-buffer");
    }
    // Since --fail is more disruptive than HTTPie's --check-status flag, we will not enable
    // it unless the user explicitly sets the latter flag
    if args.check_status == Some(true) {
        // Suppresses output on failure, unlike us
        cmd.opt("-f", "--fail");
    }

    // HTTP options
    if args.follow {
        cmd.opt("-L", "--location");
    }
    if let Some(num) = args.max_redirects {
        cmd.arg("--max-redirs");
        cmd.arg(num.to_string());
    }
    if let Some(filename) = args.output {
        let filename = filename.to_str().ok_or_else(|| anyhow!("Invalid UTF-8"))?;
        cmd.opt("-o", "--output");
        cmd.arg(filename);
    } else if args.download {
        cmd.opt("-O", "--remote-name");
    }
    if args.resume {
        cmd.opt("-C", "--continue-at");
        cmd.arg("-"); // Tell curl to guess, like we do
    }
    match args.verify.unwrap_or(Verify::Yes) {
        Verify::CustomCaBundle(filename) => {
            cmd.arg("--cacert");
            cmd.arg(filename);
        }
        Verify::No => {
            cmd.opt("-k", "--insecure");
        }
        Verify::Yes => {}
    }
    if let Some(cert) = args.cert {
        cmd.opt("-E", "--cert");
        cmd.arg(cert);
    }
    if let Some(keyfile) = args.cert_key {
        cmd.arg("--key");
        cmd.arg(keyfile);
    }
    if let Some(tls_version) = args.ssl.and_then(Into::into) {
        match tls_version {
            tls::Version::TLS_1_0 => {
                cmd.arg("--tlsv1.0");
                cmd.arg("--tls-max");
                cmd.arg("1.0");
            }
            tls::Version::TLS_1_1 => {
                cmd.arg("--tlsv1.1");
                cmd.arg("--tls-max");
                cmd.arg("1.1");
            }
            tls::Version::TLS_1_2 => {
                cmd.arg("--tlsv1.2");
                cmd.arg("--tls-max");
                cmd.arg("1.2");
            }
            tls::Version::TLS_1_3 => {
                cmd.arg("--tlsv1.3");
                cmd.arg("--tls-max");
                cmd.arg("1.3");
            }
            _ => unreachable!(),
        }
    }
    for proxy in args.proxy {
        match proxy {
            crate::cli::Proxy::All(proxy) => {
                cmd.opt("-x", "--proxy");
                cmd.arg(String::from(proxy));
            }
            crate::cli::Proxy::Http(proxy) => {
                // These don't seem to have corresponding flags
                cmd.env("http_proxy", proxy);
            }
            crate::cli::Proxy::Https(proxy) => {
                cmd.env("https_proxy", proxy);
            }
        }
    }
    if let Some(timeout) = args.timeout.and_then(|t| t.as_duration()) {
        cmd.arg("--max-time");
        cmd.arg(timeout.as_secs_f64().to_string());
    }
    if let Some(http_version) = args.http_version {
        match http_version {
            HttpVersion::Http10 => cmd.arg("--http1.0"),
            HttpVersion::Http11 => cmd.arg("--http1.1"),
            HttpVersion::Http2 => cmd.arg("--http2"),
            HttpVersion::Http2PriorKnowledge => cmd.arg("--http2-prior-knowledge"),
        }
    }

    if args.method == Some(Method::HEAD) {
        cmd.opt("-I", "--head");
    } else if args.method == Some(Method::OPTIONS) {
        // If you're sending an OPTIONS you almost certainly want to see the headers
        cmd.opt("-i", "--include");
        cmd.opt("-X", "--request");
        cmd.arg("OPTIONS");
    } else if args.headers {
        // The best option for printing just headers seems to be to use -I
        // but with an explicit method as an override.
        // But this is a hack that actually fails if data is sent.
        // See discussion on https://lornajane.net/posts/2014/view-only-headers-with-curl

        let method = match args.method {
            Some(method) => method,
            // unwrap_or_else causes borrowing issues
            None => args.request_items.pick_method(),
        };
        cmd.opt("-I", "--head");
        cmd.opt("-X", "--request");
        cmd.arg(method.to_string());
        if method != Method::GET {
            cmd.warn(
                "-I/--head is incompatible with sending data. Consider omitting -h/--headers."
                    .to_string(),
            );
        }
    } else if let Some(method) = args.method {
        cmd.opt("-X", "--request");
        cmd.arg(method.to_string());
    } else {
        // We assume that curl's automatic detection of when to do a POST matches
        // ours so we can ignore the None case
    }

    let url = url_with_query(args.url, &args.request_items.query()?);

    if url.as_str().contains(['[', ']', '{', '}']) {
        cmd.opt("-g", "--globoff")
    }

    cmd.arg(url.to_string());

    // Force ipv4/ipv6 options
    match (args.ipv4, args.ipv6) {
        (true, false) => cmd.opt("-4", "--ipv4"),
        (false, true) => cmd.opt("-6", "--ipv6"),
        _ => (),
    };

    if let Some(interface) = args.interface {
        cmd.arg("--interface");
        cmd.arg(interface);
    };

    if !args.resolve.is_empty() {
        let port = url
            .port_or_known_default()
            .with_context(|| format!("Unsupported URL scheme: '{}'", url.scheme()))?;

        cmd.warn("Inferred port number in --resolve from request URL.");
        for resolve in args.resolve {
            cmd.arg("--resolve");
            cmd.arg(format!("{}:{}:{}", resolve.domain, port, resolve.addr));
        }
    }

    // Payload
    for (header, value) in headers.iter() {
        cmd.opt("-H", "--header");
        if value.is_empty() {
            cmd.arg(format!("{};", header));
        } else {
            cmd.arg(format!("{}: {}", header, value.to_utf8_str()?));
        }
    }
    for header in headers_to_unset {
        cmd.opt("-H", "--header");
        cmd.arg(format!("{}:", header));
    }
    if args.ignore_netrc {
        // Already the default, so a bit questionable
        cmd.arg("--no-netrc");
    }
    if let Some(auth) = args.auth {
        match args.auth_type.unwrap_or_default() {
            AuthType::Basic => {
                cmd.arg("--basic");
                // curl implements this flag the same way, including password prompt
                cmd.opt("-u", "--user");
                cmd.arg(auth);
            }
            AuthType::Digest => {
                cmd.arg("--digest");
                // curl implements this flag the same way, including password prompt
                cmd.opt("-u", "--user");
                cmd.arg(auth);
            }
            AuthType::Bearer => {
                cmd.arg("--oauth2-bearer");
                cmd.arg(auth);
            }
        }
    }

    if let Some(raw) = args.raw {
        if args.form {
            cmd.header("content-type", FORM_CONTENT_TYPE);
        } else {
            cmd.header("content-type", JSON_CONTENT_TYPE);
            cmd.header("accept", JSON_ACCEPT);
        }

        cmd.opt("-d", "--data");
        cmd.arg(raw);
    } else if args.request_items.is_multipart() {
        // We can't use .body() here because we can't look inside the multipart
        // form after construction and we don't want to actually read the files
        for item in args.request_items.items {
            match item {
                RequestItem::JsonField(..) | RequestItem::JsonFieldFromFile(..) => {
                    return Err(anyhow!("JSON values are not supported in multipart fields"));
                }
                RequestItem::DataField { key, value, .. } => {
                    cmd.opt("-F", "--form");
                    cmd.arg(format!("{}={}", key, value));
                }
                RequestItem::DataFieldFromFile { key, value, .. } => {
                    cmd.opt("-F", "--form");
                    cmd.arg(format!("{}=<{}", key, value));
                }
                RequestItem::FormFile {
                    key,
                    file_name,
                    file_type,
                    file_name_header,
                } => {
                    cmd.opt("-F", "--form");
                    let mut val = format!("{}=@{}", key, file_name);
                    if let Some(file_type) = file_type {
                        val.push_str(";type=");
                        val.push_str(&file_type);
                    }
                    if let Some(file_name_header) = file_name_header {
                        val.push_str(";filename=");
                        val.push_str(&file_name_header);
                    }
                    cmd.arg(val);
                }
                RequestItem::HttpHeader(..) => {}
                RequestItem::HttpHeaderFromFile(..) => {}
                RequestItem::HttpHeaderToUnset(..) => {}
                RequestItem::UrlParam(..) => {}
                RequestItem::UrlParamFromFile(..) => {}
            }
        }
    } else {
        match args.request_items.body()? {
            Body::Form(items) => {
                if items.is_empty() {
                    // Force the header
                    cmd.header("content-type", FORM_CONTENT_TYPE);
                }
                for (key, value) in items {
                    // More faithful than -F, but doesn't have a short version
                    // New in curl 7.18.0 (January 28 2008), *probably* old enough
                    // Otherwise passing --multipart helps
                    cmd.arg("--data-urlencode");
                    // Encoding this is tricky: --data-urlencode expects name
                    // to be encoded but not value and doesn't take strings
                    let mut encoded = serde_urlencoded::to_string([(key, "")])?;
                    encoded.push_str(&value);
                    cmd.arg(encoded);
                }
            }
            Body::Json(value) if !value.is_null() => {
                cmd.header("content-type", JSON_CONTENT_TYPE);
                cmd.header("accept", JSON_ACCEPT);

                let json_string = value.to_string();
                cmd.opt("-d", "--data");
                cmd.arg(json_string);
            }
            Body::Json(..) if args.json => {
                cmd.header("content-type", JSON_CONTENT_TYPE);
                cmd.header("accept", JSON_ACCEPT);
            }
            Body::Json(..) => {}
            Body::Multipart { .. } => unreachable!(),
            Body::Raw(..) => unreachable!(),
            Body::File {
                file_name,
                file_type,
                file_name_header: _,
            } => {
                if let Some(file_type) = file_type {
                    cmd.header("content-type", file_type.to_str()?);
                } else {
                    cmd.header("content-type", JSON_CONTENT_TYPE);
                }
                cmd.arg("--data-binary");
                let mut arg = OsString::from("@");
                arg.push(file_name);
                cmd.arg(arg);
            }
        }
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples() {
        let expected = vec![
            ("xh httpbin.org/get", "curl http://httpbin.org/get"),
            ("xh httpbin.org/get -4", "curl http://httpbin.org/get -4"),
            ("xh httpbin.org/get -6", "curl http://httpbin.org/get -6"),
            (
                "xh httpbin.org/post x=3",
                #[cfg(not(windows))]
                r#"curl http://httpbin.org/post -H 'content-type: application/json' -H 'accept: application/json, */*;q=0.5' -d '{"x":"3"}'"#,
                #[cfg(windows)]
                r#"curl http://httpbin.org/post -H 'content-type: application/json' -H 'accept: application/json, */*;q=0.5' -d '{\"x\":\"3\"}'"#,
            ),
            (
                "xh --form httpbin.org/post x\\=y=z=w",
                "curl http://httpbin.org/post --data-urlencode 'x%3Dy=z=w'",
            ),
            (
                "xh put httpbin.org/put",
                "curl -X PUT http://httpbin.org/put",
            ),
            (
                "xh --https httpbin.org/get x==3",
                "curl 'https://httpbin.org/get?x=3'",
            ),
            (
                "xhs httpbin.org/get x==3",
                "curl 'https://httpbin.org/get?x=3'",
            ),
            (
                "xh -h httpbin.org/get",
                "curl -I -X GET http://httpbin.org/get",
            ),
            (
                "xh options httpbin.org/get",
                "curl -i -X OPTIONS http://httpbin.org/get",
            ),
            (
                "xh --proxy http:localhost:1080 httpbin.org/get",
                "http_proxy=localhost:1080 curl http://httpbin.org/get",
            ),
            (
                "xh --proxy all:localhost:1080 httpbin.org/get",
                "curl -x localhost:1080 http://httpbin.org/get",
            ),
            (
                "xh httpbin.org/post x:=[3]",
                #[cfg(not(windows))]
                r#"curl http://httpbin.org/post -H 'content-type: application/json' -H 'accept: application/json, */*;q=0.5' -d '{"x":[3]}'"#,
                #[cfg(windows)]
                r#"curl http://httpbin.org/post -H 'content-type: application/json' -H 'accept: application/json, */*;q=0.5' -d '{\"x\":[3]}'"#,
            ),
            (
                "xh --json httpbin.org/post",
                "curl http://httpbin.org/post -H 'content-type: application/json' -H 'accept: application/json, */*;q=0.5'",
            ),
            (
                "xh --form httpbin.org/post x@/dev/null",
                "curl http://httpbin.org/post -F 'x=@/dev/null'",
            ),
            (
                "xh --form httpbin.org/post",
                "curl http://httpbin.org/post -H 'content-type: application/x-www-form-urlencoded'",
            ),
            (
                "xh --bearer foobar post httpbin.org/post",
                "curl -X POST http://httpbin.org/post --oauth2-bearer foobar",
            ),
            (
                "xh httpbin.org/get foo:Bar baz; user-agent:",
                "curl http://httpbin.org/get -H 'foo: Bar' -H 'baz;' -H user-agent:",
            ),
            (
                "xh -d httpbin.org/get",
                "curl -f -L -O http://httpbin.org/get",
            ),
            (
                "xh -d -o foobar --continue httpbin.org/get",
                "curl -f -L -o foobar -C - http://httpbin.org/get",
            ),
            (
                "xh --curl-long -d -o foobar --continue httpbin.org/get",
                "curl --fail --location --output foobar --continue-at - http://httpbin.org/get",
            ),
            (
                "xh httpbin.org/post @foo.txt",
                #[cfg(not(windows))]
                "curl http://httpbin.org/post -H 'content-type: text/plain' --data-binary @foo.txt",
                #[cfg(windows)]
                "curl http://httpbin.org/post -H 'content-type: text/plain' --data-binary '@foo.txt'",
            ),
            (
                "xh http://example.com/[1-100].png?q={80,90}",
                "curl -g 'http://example.com/[1-100].png?q={80,90}'",
            ),
            (
                "xh https://exmaple.com/ hello:你好",
                "curl https://exmaple.com/ -H 'hello: 你好'"
            )
        ];
        for (input, output) in expected {
            let cli = Cli::try_parse_from(input.split_whitespace()).unwrap();
            let cmd = translate(cli).unwrap();
            assert_eq!(cmd.to_string(), output, "Wrong output for {:?}", input);
        }
    }
}
