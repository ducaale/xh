use std::io::{stderr, stdout, Write};

use anyhow::{anyhow, Result};
use reqwest::Method;

use crate::{
    cli::{Cli, Verify},
    request_items::{Body, RequestItem, RequestItems},
    url::construct_url,
};

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
    pub args: Vec<String>,
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

    fn flag(&mut self, short: &'static str, long: &'static str) {
        if self.long {
            self.args.push(long.to_string());
        } else {
            self.args.push(short.to_string());
        }
    }

    fn push(&mut self, arg: impl Into<String>) {
        self.args.push(arg.into());
    }

    fn env(&mut self, var: &'static str, value: String) {
        self.env.push((var, value));
    }

    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, value) in &self.env {
            write!(f, "{}={} ", key, shell_escape::escape(value.into()))?;
        }
        write!(f, "curl")?;
        for arg in &self.args {
            write!(f, " {}", shell_escape::escape(arg.into()))?;
        }
        Ok(())
    }
}

pub fn translate(args: Cli) -> Result<Command> {
    let request_items = RequestItems::new(args.request_items);
    let query = request_items.query();
    let (headers, headers_to_unset) = request_items.headers()?;
    let url = construct_url(&args.url, args.default_scheme.as_deref(), query)?;

    let mut cmd = Command::new(args.curl_long);

    let ignored = &[
        (args.offline, "--offline"),          // No equivalent
        (args.json, "-j/--json"),             // Doesn't do anything in the first place
        (args.body, "-b/--body"),             // Already the default
        (args.print.is_some(), "-p/--print"), // No straightforward equivalent
        (args.quiet, "-q/--quiet"),           // No equivalent, -s/--silent suppresses other stuff
        (args.pretty.is_some(), "--pretty"),  // No equivalent
        (args.theme.is_some(), "-s/--style"), // No equivalent
    ];

    for (present, flag) in ignored {
        if *present {
            cmd.warn(format!("Ignored {}", flag));
        }
    }

    // Silently ignored:
    // - .ignore_stdin: assumed by default
    //   (to send stdin, --data-binary @- -H 'Content-Type: application/octet-stream')
    // - .curl and .curl_long: you are here

    // Output options
    if args.verbose {
        // Far from an exact match, but it does print the request headers
        cmd.flag("-v", "--verbose");
    }
    if args.stream {
        // curl sorta streams by default, but its buffer stops it from
        // showing up right away
        cmd.flag("-N", "--no-buffer");
    }
    if args.check_status {
        // Suppresses output on failure, unlike us
        cmd.flag("-f", "--fail");
    }

    // HTTP options
    if args.follow {
        cmd.flag("-L", "--location");
    }
    if let Some(num) = args.max_redirects {
        cmd.push("--max-redirects");
        cmd.push(num.to_string());
    }
    if let Some(filename) = args.output {
        cmd.flag("-o", "--output");
        cmd.push(filename);
    } else if args.download {
        cmd.flag("-O", "--remote-name");
    }
    if args.resume {
        cmd.flag("-C", "--continue-at");
        cmd.push("-"); // Tell curl to guess, like we do
    }
    match args.verify {
        Verify::CustomCABundle(filename) => {
            cmd.push("--cacert");
            // TODO: maybe filename should be as bytes?
            // (does the way we have structopt set up even accept non-unicode?)
            cmd.push(filename.to_string_lossy());
        }
        Verify::No => {
            cmd.flag("-k", "--insecure");
        }
        Verify::Yes => {}
    }
    if let Some(cert) = args.cert {
        cmd.flag("-E", "--cert");
        // TODO: as bytes?
        cmd.push(cert.to_string_lossy());
    }
    if let Some(keyfile) = args.cert_key {
        cmd.push("--key");
        cmd.push(keyfile.to_string_lossy());
    }
    for proxy in args.proxy {
        match proxy {
            crate::cli::Proxy::All(proxy) => {
                cmd.flag("-x", "--proxy");
                cmd.push(proxy.into_string());
            }
            crate::cli::Proxy::Http(proxy) => {
                // These don't seem to have corresponding flags
                cmd.env("http_proxy", proxy.into_string());
            }
            crate::cli::Proxy::Https(proxy) => {
                cmd.env("https_proxy", proxy.into_string());
            }
        }
    }

    if args.method == Some(Method::HEAD) {
        cmd.flag("-I", "--head");
    } else if args.method == Some(Method::OPTIONS) {
        // If you're sending an OPTIONS you almost certainly want to see the headers
        cmd.flag("-i", "--include");
        cmd.flag("-X", "--request");
        cmd.push("OPTIONS");
    } else if args.headers {
        // The best option for printing just headers seems to be to use -I
        // but with an explicit method as an override.
        // But this is a hack that actually fails if data is sent.
        // See discussion on https://lornajane.net/posts/2014/view-only-headers-with-curl
        let method = args.method.unwrap_or_else(|| request_items.pick_method());
        cmd.flag("-I", "--head");
        cmd.flag("-X", "--request");
        cmd.push(method.to_string());
        if method != Method::GET {
            cmd.warn(
                "-I/--head is incompatible with sending data. Consider omitting -h/--headers."
                    .to_string(),
            );
        }
    } else if let Some(method) = args.method {
        cmd.flag("-X", "--request");
        cmd.push(method.to_string());
    }
    // We assume that curl's automatic detection of when to do a POST matches
    // ours so we can ignore the None case

    cmd.push(url.to_string());

    // Payload
    for (header, value) in headers.iter() {
        cmd.flag("-H", "--header");
        if value.is_empty() {
            cmd.push(format!("{};", header));
        } else {
            cmd.push(format!("{}: {}", header, value.to_str()?));
        }
    }
    for header in headers_to_unset {
        cmd.flag("-H", "--header");
        cmd.push(format!("{}:", header));
    }
    if let Some(auth) = args.auth {
        // curl implements this flag the same way, including password prompt
        cmd.flag("-u", "--user");
        cmd.push(auth);
    }
    if let Some(token) = args.bearer {
        cmd.push("--oauth2-bearer");
        cmd.push(token);
    }

    if args.multipart || request_items.has_form_files() {
        // We can't use .body() here because we can't look inside the multipart
        // form after construction and we don't want to actually read the files
        for item in request_items.0 {
            match item {
                RequestItem::JSONField(..) => {
                    return Err(anyhow!("JSON values are not supported in multipart fields"));
                }
                RequestItem::DataField(key, value) => {
                    cmd.flag("-F", "--form");
                    cmd.push(format!("{}={}", key, value));
                }
                RequestItem::FormFile(key, value, file_type) => {
                    cmd.flag("-F", "--form");
                    if let Some(file_type) = file_type {
                        cmd.push(format!("{}=@{};type={}", key, value, file_type));
                    } else {
                        cmd.push(format!("{}=@{}", key, value));
                    }
                }
                _ => {}
            }
        }
    } else {
        match request_items.body(args.form, false)? {
            Some(Body::Form(items)) => {
                for (key, value) in items {
                    // More faithful than -F, but doesn't have a short version
                    // New in curl 7.18.0 (January 28 2008), *probably* old enough
                    // Otherwise passing --multipart helps
                    cmd.push("--data-urlencode");
                    // Encoding this is tricky: --data-urlencode expects name
                    // to be encoded but not value and doesn't take strings
                    let mut encoded = serde_urlencoded::to_string(&[(key, "")])?;
                    encoded.push_str(&value);
                    cmd.push(encoded);
                }
            }
            Some(Body::Json(map)) => {
                cmd.flag("-H", "--header");
                cmd.push("content-type: application/json");

                let json_string = serde_json::Value::from(map).to_string();
                cmd.flag("-d", "--data");
                cmd.push(json_string);
            }
            Some(Body::Multipart(..)) => unreachable!(),
            Some(Body::Raw(..)) => unreachable!(),
            None => {}
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
            ("xh httpbin.org/get", "curl 'http://httpbin.org/get'"),
            (
                "xh httpbin.org/post x=3",
                r#"curl 'http://httpbin.org/post' -H 'content-type: application/json' -d '{"x":"3"}'"#,
            ),
            (
                "xh --form httpbin.org/post x\\=y=z=w",
                "curl 'http://httpbin.org/post' --data-urlencode 'x%3Dy=z=w'",
            ),
            (
                "xh put httpbin.org/put",
                "curl -X PUT 'http://httpbin.org/put'",
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
                "curl -I -X GET 'http://httpbin.org/get'",
            ),
            (
                "xh options httpbin.org/get",
                "curl -i -X OPTIONS 'http://httpbin.org/get'",
            ),
            (
                "xh --proxy http:localhost:1080 httpbin.org/get",
                "http_proxy='localhost:1080' curl 'http://httpbin.org/get'",
            ),
            (
                "xh --proxy all:localhost:1080 httpbin.org/get",
                "curl -x 'localhost:1080' 'http://httpbin.org/get'",
            ),
            (
                "xh httpbin.org/post x:=[3]",
                r#"curl 'http://httpbin.org/post' -H 'content-type: application/json' -d '{"x":[3]}'"#,
            ),
            (
                "xh --form httpbin.org/post x@/dev/null",
                "curl 'http://httpbin.org/post' -F 'x=@/dev/null'",
            ),
            (
                "xh --bearer foobar post httpbin.org/post",
                "curl -X POST 'http://httpbin.org/post' --oauth2-bearer foobar",
            ),
            (
                "xh httpbin.org/get foo:Bar baz; user-agent:",
                "curl 'http://httpbin.org/get' -H 'foo: Bar' -H 'baz;' -H 'user-agent:'",
            ),
            (
                "xh -d httpbin.org/get",
                "curl -L -O 'http://httpbin.org/get'",
            ),
            (
                "xh -d -o foobar --continue httpbin.org/get",
                "curl -L -o foobar -C - 'http://httpbin.org/get'",
            ),
            (
                "xh --curl-long -d -o foobar --continue httpbin.org/get",
                "curl --location --output foobar --continue-at - 'http://httpbin.org/get'",
            ),
        ];
        for (input, output) in expected {
            let cli = Cli::from_iter_safe(input.split_whitespace()).unwrap();
            let cmd = translate(cli).unwrap();
            assert_eq!(cmd.to_string(), output, "Wrong output for {:?}", input);
        }
    }
}
