use std::convert::TryFrom;
use std::env;
use std::ffi::OsString;
use std::io::Write;
use std::mem;
use std::str::FromStr;

use reqwest::Url;
use structopt::clap::{self, arg_enum, AppSettings, Error, ErrorKind, Result};
use structopt::StructOpt;

use crate::{regex, utils::test_pretend_term, Body, Buffer};

// Some doc comments were copy-pasted from HTTPie

/// xh is a friendly and fast tool for sending HTTP requests.
///
/// It reimplements as much as possible of HTTPie's excellent design.
#[derive(StructOpt, Debug)]
#[structopt(name = "xh", setting = AppSettings::DeriveDisplayOrder)]
pub struct Cli {
    /// Construct HTTP requests without sending them anywhere.
    #[structopt(long)]
    pub offline: bool,

    /// (default) Serialize data items from the command line as a JSON object.
    #[structopt(short = "j", long)]
    pub json: bool,

    /// Serialize data items from the command line as form fields.
    #[structopt(short = "f", long)]
    pub form: bool,

    /// Like --form, but force a multipart/form-data request even without files.
    #[structopt(short, long)]
    pub multipart: bool,

    /// Do not attempt to read stdin.
    #[structopt(short = "I", long = "ignore-stdin")]
    pub ignore_stdin: bool,

    /// Authenticate as USER with PASS. PASS will be prompted if missing.
    ///
    /// Use a trailing colon (i.e. `USER:`) to authenticate with just a username.
    #[structopt(short = "a", long, name = "USER[:PASS]")]
    pub auth: Option<String>,

    /// Authenticate with a bearer token.
    #[structopt(long, name = "TOKEN")]
    pub bearer: Option<String>,

    /// Save output to FILE instead of stdout.
    #[structopt(short, long, name = "FILE")]
    pub output: Option<String>,

    /// Do follow redirects.
    #[structopt(short = "F", long = "follow")]
    pub follow: bool,

    /// Number of redirects to follow, only respected if `follow` is set.
    #[structopt(long = "max-redirects", name = "NUM")]
    pub max_redirects: Option<usize>,

    /// Download the body to a file instead of printing it.
    #[structopt(short = "d", long)]
    pub download: bool,

    /// Print only the response headers, shortcut for --print=h.
    #[structopt(short = "h", long)]
    pub headers: bool,

    /// Print only the response body, Shortcut for --print=b.
    #[structopt(short = "b", long)]
    pub body: bool,

    /// Resume an interrupted download. Requires --download and --output.
    #[structopt(short = "c", long = "continue")]
    pub resume: bool,

    /// String specifying what the output should contain.
    ///
    /// Use `H` and `B` for request header and body respectively,
    /// and `h` and `b` for response hader and body.
    ///
    /// Example: `--print=Hb`
    #[structopt(short = "p", long, name = "FORMAT")]
    pub print: Option<Print>,

    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Do not print to stdout or stderr.
    #[structopt(short = "q", long)]
    pub quiet: bool,

    /// Always stream the response body.
    #[structopt(short = "S", long)]
    pub stream: bool,

    /// Controls output processing.
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true, name = "STYLE")]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long = "style", possible_values = &Theme::variants(), case_insensitive = true, name = "THEME")]
    pub theme: Option<Theme>,

    /// Exit with an error status code if the server replies with an error.
    ///
    /// The exit code will be 4 on 4xx (Client Error), 5 on 5xx (Server Error),
    /// or 3 on 3xx (Redirect) if --follow isn't set.
    ///
    /// If stdout is redirected then a warning is written to stderr.
    #[structopt(long)]
    pub check_status: bool,

    /// Use a proxy for a protocol. For example: `--proxy https:http://proxy.host:8080`.
    ///
    /// PROTOCOL can be `http`, `https` or `all`.
    ///
    /// If your proxy requires credentials, put them in the URL, like so:
    /// `--proxy http:socks5://user:password@proxy.host:8000`.
    ///
    /// You can specify proxies for multiple protocols by repeating this option.
    ///
    /// The environment variables `http_proxy` and `https_proxy` can also be used, but
    /// are completely ignored if --proxy is passed.
    #[structopt(long, value_name = "PROTOCOL:URL", number_of_values = 1)]
    pub proxy: Vec<Proxy>,

    /// The default scheme to use if not specified in the URL.
    #[structopt(long = "default-scheme", name = "SCHEME")]
    pub default_scheme: Option<String>,

    /// The request URL, preceded by an optional HTTP method.
    ///
    /// METHOD can be `get`, `post`, `head`, `put`, `patch`, `delete` or `options`.
    /// If omitted, either a GET or a POST will be done depending on whether the
    /// request sends data.
    #[structopt(name = "[METHOD] URL")]
    raw_method_or_url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(
        name = "REQUEST_ITEM",
        long_help = "Optional key-value pairs to be included in the request.

- key==value to add a parameter to the URL
- key=value to add a JSON field (--json) or form field (--form)
- key:=value to add a complex JSON value (e.g. `numbers:=[1,2,3]`)
- key@filename to upload a file from filename (with --form)
- header:value to add a header
- header: to unset a header
- header; to add a header with an empty value
"
    )]
    raw_rest_args: Vec<String>,

    /// The HTTP method, if supplied.
    #[structopt(skip)]
    pub method: Option<Method>,

    /// The request URL.
    #[structopt(skip)]
    pub url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(skip)]
    pub request_items: Vec<RequestItem>,
}

impl Cli {
    pub fn from_args() -> Self {
        Cli::from_iter(std::env::args())
    }

    pub fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        match Self::from_iter_safe(iter) {
            Ok(cli) => cli,
            Err(err) if err.kind == ErrorKind::HelpDisplayed => {
                // The logic here is a little tricky.
                //
                // Normally with structopt/clap, -h prints short help while --help
                // prints long help.
                //
                // But -h is short for --header, so we want --help to print short help
                // and `help` (pseudo-subcommand) to print long help.
                //
                // --help is baked into clap. So we intercept its special error that
                // would print long help and print short help instead. And if we do
                // want to print long help, then we insert our own error in from_iter_safe
                // with a special tag.
                if env::var_os("XH_HELP2MAN").is_some() {
                    Cli::clap()
                        .template(
                            "\
                                Usage: {usage}\n\
                                \n\
                                {long-about}\n\
                                \n\
                                Options:\n\
                                {flags}\n\
                                {options}\
                            ",
                        )
                        .print_long_help()
                        .unwrap();
                } else if err.message == "XH_PRINT_LONG_HELP" {
                    Cli::clap().print_long_help().unwrap();
                    println!();
                } else {
                    Cli::clap().print_help().unwrap();
                    println!(
                        "\n\nRun `{} help` for more complete documentation.",
                        env!("CARGO_PKG_NAME")
                    );
                }
                safe_exit();
            }
            Err(err) => err.exit(),
        }
    }

    pub fn from_iter_safe<I>(iter: I) -> clap::Result<Self>
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        let mut cli: Self = StructOpt::from_iter_safe(iter)?;
        if cli.raw_method_or_url == "help" {
            return Err(Error {
                message: "XH_PRINT_LONG_HELP".to_string(),
                kind: ErrorKind::HelpDisplayed,
                info: Some(vec!["XH_PRINT_LONG_HELP".to_string()]),
            });
        }
        let mut rest_args = mem::take(&mut cli.raw_rest_args).into_iter();
        match cli.raw_method_or_url.parse::<Method>() {
            Ok(method) => {
                cli.method = Some(method);
                cli.url = rest_args.next().ok_or_else(|| {
                    Error::with_description("Missing URL", ErrorKind::MissingArgumentOrSubcommand)
                })?;
            }
            Err(_) => {
                cli.method = None;
                cli.url = mem::take(&mut cli.raw_method_or_url);
            }
        }
        for request_item in rest_args {
            cli.request_items.push(request_item.parse()?);
        }
        if cli.resume && !cli.download {
            return Err(Error::with_description(
                "--continue only works with --download",
                ErrorKind::MissingArgumentOrSubcommand,
            ));
        }
        if cli.resume && cli.output.is_none() {
            return Err(Error::with_description(
                "--continue requires --output",
                ErrorKind::MissingArgumentOrSubcommand,
            ));
        }
        Ok(cli)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    PATCH,
    DELETE,
    OPTIONS,
}

impl FromStr for Method {
    type Err = Error;
    fn from_str(s: &str) -> Result<Method> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "HEAD" => Ok(Method::HEAD),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "PATCH" => Ok(Method::PATCH),
            "DELETE" => Ok(Method::DELETE),
            "OPTIONS" => Ok(Method::OPTIONS),
            method => Err(Error::with_description(
                &format!("unknown http method {}", method),
                ErrorKind::InvalidValue,
            )),
        }
    }
}

impl From<Method> for reqwest::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::GET => reqwest::Method::GET,
            Method::HEAD => reqwest::Method::HEAD,
            Method::POST => reqwest::Method::POST,
            Method::PUT => reqwest::Method::PUT,
            Method::PATCH => reqwest::Method::PATCH,
            Method::DELETE => reqwest::Method::DELETE,
            Method::OPTIONS => reqwest::Method::OPTIONS,
        }
    }
}

impl From<&Option<Body>> for Method {
    fn from(body: &Option<Body>) -> Self {
        match body {
            Some(_) => Method::POST,
            None => Method::GET,
        }
    }
}

arg_enum! {
    // Uppercase variant names would show up as such in the help text
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Pretty {
        all, colors, format, none
    }
}

impl Pretty {
    pub fn color(self) -> bool {
        matches!(self, Pretty::colors | Pretty::all)
    }

    pub fn format(self) -> bool {
        matches!(self, Pretty::format | Pretty::all)
    }
}

impl From<&Buffer> for Pretty {
    fn from(b: &Buffer) -> Self {
        if test_pretend_term() {
            Pretty::format
        } else if b.is_terminal() {
            Pretty::all
        } else {
            Pretty::none
        }
    }
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Theme {
        auto, solarized
    }
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::auto => "ansi",
            Theme::solarized => "solarized",
        }
    }
}

#[derive(Debug)]
pub struct Print {
    pub request_headers: bool,
    pub request_body: bool,
    pub response_headers: bool,
    pub response_body: bool,
}

impl Print {
    pub fn new(
        verbose: bool,
        headers: bool,
        body: bool,
        quiet: bool,
        offline: bool,
        buffer: &Buffer,
    ) -> Self {
        if verbose {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: true,
                response_body: true,
            }
        } else if quiet {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: false,
            }
        } else if offline {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: false,
                response_body: false,
            }
        } else if headers {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: false,
            }
        } else if body || !buffer.is_terminal() {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: true,
            }
        } else {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: true,
            }
        }
    }
}

impl FromStr for Print {
    type Err = Error;
    fn from_str(s: &str) -> Result<Print> {
        let mut request_headers = false;
        let mut request_body = false;
        let mut response_headers = false;
        let mut response_body = false;

        for char in s.chars() {
            match char {
                'H' => request_headers = true,
                'B' => request_body = true,
                'h' => response_headers = true,
                'b' => response_body = true,
                char => {
                    return Err(Error::with_description(
                        &format!("{:?} is not a valid value", char),
                        ErrorKind::InvalidValue,
                    ))
                }
            }
        }

        let p = Print {
            request_headers,
            request_body,
            response_headers,
            response_body,
        };
        Ok(p)
    }
}

#[derive(Debug, PartialEq)]
pub enum Proxy {
    Http(Url),
    Https(Url),
    All(Url),
}

impl FromStr for Proxy {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let split_arg: Vec<&str> = s.splitn(2, ':').collect();
        match split_arg[..] {
            [protocol, url] => {
                let url = reqwest::Url::try_from(url).map_err(|e| {
                    Error::with_description(
                        &format!(
                            "Invalid proxy URL '{}' for protocol '{}': {}",
                            url, protocol, e
                        ),
                        ErrorKind::InvalidValue,
                    )
                })?;

                match protocol.to_lowercase().as_str() {
                    "http" => Ok(Proxy::Http(url)),
                    "https" => Ok(Proxy::Https(url)),
                    "all" => Ok(Proxy::All(url)),
                    _ => Err(Error::with_description(
                        &format!("Unknown protocol to set a proxy for: {}", protocol),
                        ErrorKind::InvalidValue,
                    )),
                }
            }
            _ => Err(Error::with_description(
                "The value passed to --proxy should be formatted as <PROTOCOL>:<PROXY_URL>",
                ErrorKind::InvalidValue,
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestItem {
    HttpHeader(String, String),
    HttpHeaderToUnset(String),
    UrlParam(String, String),
    DataField(String, String),
    JSONField(String, serde_json::Value),
    FormFile(String, String, Option<String>),
}

impl FromStr for RequestItem {
    type Err = Error;
    fn from_str(request_item: &str) -> Result<RequestItem> {
        regex!(FORM_FILE_TYPED = r"^(.+?)@(.+?);type=(.+?)$");
        regex!(PARAM = r"^(.+?)(==|:=|=|@|:)((?s).+)$");
        regex!(NO_HEADER = r"^(.+?)(:|;)$");

        if let Some(caps) = FORM_FILE_TYPED.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[2].to_string();
            let file_type = caps[3].to_string();
            Ok(RequestItem::FormFile(key, value, Some(file_type)))
        } else if let Some(caps) = PARAM.captures(request_item) {
            let key = caps[1].to_string();
            let value = caps[3].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeader(key, value)),
                "==" => Ok(RequestItem::UrlParam(key, value)),
                "=" => Ok(RequestItem::DataField(key, value)),
                ":=" => Ok(RequestItem::JSONField(
                    key,
                    serde_json::from_str(&value).map_err(|err| {
                        Error::with_description(
                            &format!("{:?}: {}", request_item, err),
                            ErrorKind::InvalidValue,
                        )
                    })?,
                )),
                "@" => Ok(RequestItem::FormFile(key, value, None)),
                _ => unreachable!(),
            }
        } else if let Some(caps) = NO_HEADER.captures(request_item) {
            let key = caps[1].to_string();
            match &caps[2] {
                ":" => Ok(RequestItem::HttpHeaderToUnset(key)),
                ";" => Ok(RequestItem::HttpHeader(key, "".into())),
                _ => unreachable!(),
            }
        } else {
            // TODO: We can also end up here if the method couldn't be parsed
            // and was interpreted as a URL, making the actual URL a request
            // item
            Err(Error::with_description(
                &format!("{:?} is not a valid request item", request_item),
                ErrorKind::InvalidValue,
            ))
        }
    }
}

/// Based on the function used by clap to abort
fn safe_exit() -> ! {
    let _ = std::io::stdout().lock().flush();
    let _ = std::io::stderr().lock().flush();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli> {
        Cli::from_iter_safe(
            Some("xh".to_string())
                .into_iter()
                .chain(args.iter().map(|s| s.to_string())),
        )
    }

    #[test]
    fn implicit_method() {
        let cli = parse(&["example.org"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url, "example.org");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn explicit_method() {
        let cli = parse(&["get", "example.org"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url, "example.org");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn missing_url() {
        parse(&["get"]).unwrap_err();
    }

    #[test]
    fn space_in_url() {
        let cli = parse(&["post", "example.org/foo bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::POST));
        assert_eq!(cli.url, "example.org/foo bar");
        assert!(cli.request_items.is_empty());
    }

    #[test]
    fn request_items() {
        let cli = parse(&["get", "example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url, "example.org");
        assert_eq!(
            cli.request_items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn request_items_implicit_method() {
        let cli = parse(&["example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url, "example.org");
        assert_eq!(
            cli.request_items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn superfluous_arg() {
        parse(&["get", "example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn superfluous_arg_implicit_method() {
        parse(&["example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn multiple_methods() {
        parse(&["get", "post", "example.org"]).unwrap_err();
    }

    #[test]
    fn proxy_invalid_protocol() {
        Cli::from_iter_safe(&[
            "xh",
            "--proxy=invalid:http://127.0.0.1:8000",
            "get",
            "example.org",
        ])
        .unwrap_err();
    }

    #[test]
    fn proxy_invalid_proxy_url() {
        Cli::from_iter_safe(&["xh", "--proxy=http:127.0.0.1:8000", "get", "example.org"])
            .unwrap_err();
    }

    #[test]
    fn proxy_http() {
        let proxy = parse(&["--proxy=http:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::Http(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }

    #[test]
    fn proxy_https() {
        let proxy = parse(&["--proxy=https:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::Https(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }

    #[test]
    fn proxy_all() {
        let proxy = parse(&["--proxy=all:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::All(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }
}
