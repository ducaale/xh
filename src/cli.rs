use std::convert::TryFrom;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::io::Write;
use std::mem;
use std::path::PathBuf;
use std::str::FromStr;

use reqwest::{Method, Url};
use structopt::clap::{self, arg_enum, AppSettings, Error, ErrorKind, Result};
use structopt::StructOpt;

use crate::{buffer::Buffer, request_items::RequestItem, utils::test_pretend_term};

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
    #[structopt(short = "j", long, overrides_with_all = &["form", "multipart"])]
    pub json: bool,

    /// Serialize data items from the command line as form fields.
    #[structopt(short = "f", long, overrides_with_all = &["json", "multipart"])]
    pub form: bool,

    /// Like --form, but force a multipart/form-data request even without files.
    #[structopt(short = "m", long, overrides_with_all = &["json", "form"])]
    pub multipart: bool,

    #[structopt(skip)]
    pub request_type: RequestType,

    /// Do not attempt to read stdin.
    #[structopt(short = "I", long)]
    pub ignore_stdin: bool,

    // Currently deprecated in favor of --bearer, un-hide if new auth types are introduced
    /// Specify the auth mechanism.
    #[structopt(short = "A", long, possible_values = &AuthType::variants(),
                default_value = "basic", case_insensitive = true, hidden = true)]
    pub auth_type: AuthType,

    /// Authenticate as USER with PASS. PASS will be prompted if missing.
    ///
    /// Use a trailing colon (i.e. `USER:`) to authenticate with just a username.
    #[structopt(short = "a", long, value_name = "USER[:PASS]")]
    pub auth: Option<String>,

    /// Authenticate with a bearer token.
    #[structopt(long, value_name = "TOKEN")]
    pub bearer: Option<String>,

    /// Save output to FILE instead of stdout.
    #[structopt(short = "o", long, value_name = "FILE")]
    pub output: Option<String>,

    /// Do follow redirects.
    #[structopt(short = "F", long)]
    pub follow: bool,

    /// Number of redirects to follow, only respected if `follow` is set.
    #[structopt(long, value_name = "NUM")]
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
    #[structopt(short = "c", long = "continue", name = "continue")]
    pub resume: bool,

    /// String specifying what the output should contain.
    ///
    /// Use `H` and `B` for request header and body respectively,
    /// and `h` and `b` for response hader and body.
    ///
    /// Example: `--print=Hb`
    #[structopt(short = "p", long, value_name = "FORMAT")]
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
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true, value_name = "STYLE")]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long, value_name = "THEME", possible_values = &Theme::variants(), case_insensitive = true)]
    pub style: Option<Theme>,

    /// Exit with an error status code if the server replies with an error.
    ///
    /// The exit code will be 4 on 4xx (Client Error), 5 on 5xx (Server Error),
    /// or 3 on 3xx (Redirect) if --follow isn't set.
    ///
    /// If stdout is redirected then a warning is written to stderr.
    #[structopt(long)]
    pub check_status: bool,

    /// Print a translation to a `curl` command.
    ///
    /// For translating the other way, try https://curl2httpie.online/.
    #[structopt(long)]
    pub curl: bool,

    /// Use the long versions of curl's flags.
    #[structopt(long)]
    pub curl_long: bool,

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
    #[structopt(long, value_name = "SCHEME", hidden = true)]
    pub default_scheme: Option<String>,

    /// Make HTTPS requests if not specified in the URL.
    #[structopt(long)]
    pub https: bool,

    /// The request URL, preceded by an optional HTTP method.
    ///
    /// METHOD can be `get`, `post`, `head`, `put`, `patch`, `delete` or `options`.
    /// If omitted, either a GET or a POST will be done depending on whether the
    /// request sends data.
    #[structopt(value_name = "[METHOD] URL")]
    raw_method_or_url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(
        value_name = "REQUEST_ITEM",
        long_help = r"Optional key-value pairs to be included in the request.

- key==value to add a parameter to the URL
- key=value to add a JSON field (--json) or form field (--form)
- key:=value to add a complex JSON value (e.g. `numbers:=[1,2,3]`)
- key@filename to upload a file from filename (with --form)
- header:value to add a header
- header: to unset a header
- header; to add a header with an empty value

A backslash can be used to escape special characters (e.g. weird\:key=value).
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

    /// If "no", skip SSL verification. If a file path, use it as a CA bundle.
    ///
    /// Specifying a CA bundle will disable the system's built-in root certificates.
    ///
    /// "false" instead of "no" also works. The default is "yes" ("true").
    #[structopt(long, default_value, hide_default_value = true, value_name = "VERIFY")]
    pub verify: Verify,

    /// Use a client side certificate for SSL.
    #[structopt(long, value_name = "FILE")]
    pub cert: Option<PathBuf>,

    /// A private key file to use with --cert.
    ///
    /// Only necessary if the private key is not contained in the cert file.
    #[structopt(long, value_name = "FILE")]
    pub cert_key: Option<PathBuf>,
}

// Names of flags that negate other flags
// This should in principle contain all options. It's easiest to line it
// up with the output of --help.
// Warning: Nothing in clap or in our code checks that these match up with
// other arguments. If in doubt, add a test.
const NEGATION_FLAGS: &[&str] = &[
    // FLAGS
    "--no-offline",
    "--no-json",
    "--no-form",
    "--no-multipart",
    "--no-ignore-stdin",
    "--no-follow",
    "--no-download",
    "--no-headers",
    "--no-body",
    "--no-continue",
    "--no-verbose",
    "--no-quiet",
    "--no-stream",
    "--no-check-status",
    "--no-curl",
    "--no-curl-long",
    "--no-https",
    // OPTIONS
    "--no-auth",
    "--no-bearer",
    "--no-output",
    "--no-max-redirects",
    "--no-print",
    "--no-pretty",
    "--no-style",
    "--no-proxy",
    "--no-verify", // A little misleading, but HTTPie has it like this
    "--no-cert",
    "--no-cert-key",
];

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
                                {options}\n\
                                {after-help}\
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
        let mut app = Self::clap();
        let matches = app.get_matches_from_safe_borrow(iter)?;
        let mut cli = Self::from_clap(&matches);

        match cli.raw_method_or_url.as_str() {
            "help" => {
                return Err(Error {
                    message: "XH_PRINT_LONG_HELP".to_string(),
                    kind: ErrorKind::HelpDisplayed,
                    info: Some(vec!["XH_PRINT_LONG_HELP".to_string()]),
                })
            }
            "print_completions" => return Err(print_completions(app, cli.raw_rest_args)),
            "generate_completions" => return Err(generate_completions(app, cli.raw_rest_args)),
            _ => {}
        }
        let mut rest_args = mem::take(&mut cli.raw_rest_args).into_iter();
        match parse_method(&cli.raw_method_or_url) {
            Some(method) => {
                cli.method = Some(method);
                cli.url = rest_args.next().ok_or_else(|| {
                    Error::with_description("Missing URL", ErrorKind::MissingArgumentOrSubcommand)
                })?;
            }
            None => {
                cli.method = None;
                cli.url = mem::take(&mut cli.raw_method_or_url);
            }
        }
        for request_item in rest_args {
            cli.request_items.push(request_item.parse()?);
        }

        if matches!(
            app.get_bin_name().and_then(|name| name.split('.').next()),
            Some("https") | Some("xhs") | Some("xhttps")
        ) {
            cli.https = true;
        }

        cli.process_relations()?;
        Ok(cli)
    }

    /// Set flags that are implied by other flags and report conflicting flags.
    fn process_relations(&mut self) -> clap::Result<()> {
        if self.resume && !self.download {
            return Err(Error::with_description(
                "--continue only works with --download",
                ErrorKind::MissingArgumentOrSubcommand,
            ));
        }
        if self.resume && self.output.is_none() {
            return Err(Error::with_description(
                "--continue requires --output",
                ErrorKind::MissingArgumentOrSubcommand,
            ));
        }
        if self.download {
            self.follow = true;
        }
        if self.curl_long {
            self.curl = true;
        }
        if self.https {
            self.default_scheme = Some("https".to_string());
        }
        if self.auth_type == AuthType::bearer && self.auth.is_some() {
            self.bearer = self.auth.take();
        }
        // `overrides_with_all` ensures that only one of these is true
        if self.json {
            // Also the default, so this shouldn't do anything
            self.request_type = RequestType::Json;
        } else if self.form {
            self.request_type = RequestType::Form;
        } else if self.multipart {
            self.request_type = RequestType::Multipart;
        }
        Ok(())
    }

    pub fn clap() -> clap::App<'static, 'static> {
        let mut app = <Self as StructOpt>::clap();
        for &flag in NEGATION_FLAGS {
            // `orig` and `flag` both need a static lifetime, so we
            // build `orig` by trimming `flag` instead of building `flag`
            // by extending `orig`
            let orig = flag.strip_prefix("--no-").unwrap();
            app = app.arg(
                // The name is inconsequential, but it has to be unique and it
                // needs a static lifetime, and `flag` satisfies that
                clap::Arg::with_name(flag)
                    .long(flag)
                    .hidden(true)
                    // overrides_with is enough to make the flags take effect
                    // We never have to check their values, they'll simply
                    // unset previous occurrences of the original flag
                    .overrides_with(orig),
            );
        }
        app.after_help("Each option can be reset with a --no-OPTION argument.")
    }
}

fn parse_method(method: &str) -> Option<Method> {
    // These are defined as constants on Method
    const METHODS: &[&str] = &[
        "GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS", "CONNECT", "PATCH", "TRACE",
    ];
    let method = method.to_ascii_uppercase();
    if METHODS.contains(&method.as_str()) {
        Some(method.parse().unwrap())
    } else {
        None
    }
}

// This signature is a little weird: we either return an error or don't
// return at all
fn print_completions(mut app: clap::App, rest_args: Vec<String>) -> Error {
    let bin_name = match app.get_bin_name() {
        // This name is borrowed from `app`, and `gen_completions_to()` mutably
        // borrows `app`, so we need to do a clone
        Some(name) => name.to_owned(),
        None => return Error::with_description("Missing binary name", ErrorKind::EmptyValue),
    };
    if rest_args.len() != 1 {
        return Error::with_description(
            "Usage: xh print_completions <SHELL>",
            ErrorKind::WrongNumberOfValues,
        );
    }
    let shell = match rest_args[0].parse() {
        Ok(shell) => shell,
        Err(_) => return Error::with_description("Unknown shell name", ErrorKind::InvalidValue),
    };
    let mut buf = Vec::new();
    app.gen_completions_to(bin_name, shell, &mut buf);
    let mut completions = String::from_utf8(buf).unwrap();
    if matches!(shell, clap::Shell::Fish) {
        // We don't have (proper) subcommands, so this check is unnecessary and
        // slightly harmful
        // See https://github.com/clap-rs/clap/pull/2359, currently unreleased
        completions = completions.replace(r#" -n "__fish_use_subcommand""#, "");
    }
    print!("{}", completions);
    safe_exit();
}

fn generate_completions(mut app: clap::App, rest_args: Vec<String>) -> Error {
    let bin_name = match app.get_bin_name() {
        Some(name) => name.to_owned(),
        None => return Error::with_description("Missing binary name", ErrorKind::EmptyValue),
    };
    if rest_args.len() != 1 {
        return Error::with_description(
            "Usage: xh generate_completions <DIRECTORY>",
            ErrorKind::WrongNumberOfValues,
        );
    }
    for &shell in &clap::Shell::variants() {
        // Elvish complains about multiple deprecations and these don't seem to work
        // If you must use them, generate them manually with xh print_completions elvish
        if shell != "elvish" {
            app.gen_completions(&bin_name, shell.parse().unwrap(), &rest_args[0]);
        }
    }
    safe_exit();
}

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq)]
    pub enum AuthType {
        basic, bearer
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
            if env::var_os("NO_COLOR").is_some() {
                // https://no-color.org/
                Pretty::format
            } else {
                Pretty::all
            }
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

#[derive(Debug, PartialEq)]
pub enum Verify {
    Yes,
    No,
    CustomCABundle(PathBuf),
}

impl FromStr for Verify {
    type Err = Error;
    fn from_str(verify: &str) -> Result<Verify> {
        match verify.to_lowercase().as_str() {
            "no" | "false" => Ok(Verify::No),
            "yes" | "true" => Ok(Verify::Yes),
            path => Ok(Verify::CustomCABundle(PathBuf::from(path))),
        }
    }
}

impl Default for Verify {
    fn default() -> Self {
        Verify::Yes
    }
}

impl fmt::Display for Verify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verify::No => write!(f, "no"),
            Verify::Yes => write!(f, "yes"),
            Verify::CustomCABundle(path) => write!(f, "custom ca bundle: {}", path.display()),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum RequestType {
    Json,
    Form,
    Multipart,
}

impl Default for RequestType {
    fn default() -> Self {
        RequestType::Json
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
    fn auth() {
        let cli = parse(&["--auth=user:pass", ":"]).unwrap();
        assert_eq!(cli.auth.as_deref(), Some("user:pass"));
        assert_eq!(cli.bearer, None);

        let cli = parse(&["--auth=user:pass", "--auth-type=basic", ":"]).unwrap();
        assert_eq!(cli.auth.as_deref(), Some("user:pass"));
        assert_eq!(cli.bearer, None);

        let cli = parse(&["--auth=token", "--auth-type=bearer", ":"]).unwrap();
        assert_eq!(cli.auth, None);
        assert_eq!(cli.bearer.as_deref(), Some("token"));

        let cli = parse(&["--bearer=token", "--auth-type=bearer", ":"]).unwrap();
        assert_eq!(cli.auth, None);
        assert_eq!(cli.bearer.as_deref(), Some("token"));

        let cli = parse(&["--auth-type=bearer", ":"]).unwrap();
        assert_eq!(cli.auth, None);
        assert_eq!(cli.bearer, None);
    }

    #[test]
    fn request_type_overrides() {
        let cli = parse(&["--form", "--json", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Json);
        assert_eq!(cli.json, true);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(&["--json", "--form", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Form);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, true);
        assert_eq!(cli.multipart, false);

        let cli = parse(&[":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Json);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);
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

    #[test]
    fn executable_name() {
        let args = Cli::from_iter_safe(&["xhs", "example.org"]).unwrap();
        assert_eq!(args.https, true);
    }

    #[test]
    fn executable_name_extension() {
        let args = Cli::from_iter_safe(&["xhs.exe", "example.org"]).unwrap();
        assert_eq!(args.https, true);
    }

    #[test]
    fn negated_flags() {
        let cli = parse(&["--no-offline", ":"]).unwrap();
        assert_eq!(cli.offline, false);

        let cli = parse(&["--check-status", "--no-check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, false);

        // In HTTPie, the order doesn't matter, so this would be false
        let cli = parse(&["--no-offline", "--offline", ":"]).unwrap();
        assert_eq!(cli.offline, true);

        // In HTTPie, this resolves to json, but that seems wrong
        let cli = parse(&["--no-form", "--multipart", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(&["--multipart", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(&["--form", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Json);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(&["--form", "--json", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_type, RequestType::Json);
        assert_eq!(cli.json, true);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(&["--curl-long", "--no-curl-long", ":"]).unwrap();
        assert_eq!(cli.curl_long, false);
        let cli = parse(&["--no-curl-long", "--curl-long", ":"]).unwrap();
        assert_eq!(cli.curl_long, true);

        let cli = parse(&["-do=fname", "--continue", "--no-continue", ":"]).unwrap();
        assert_eq!(cli.resume, false);
        let cli = parse(&["-do=fname", "--no-continue", "--continue", ":"]).unwrap();
        assert_eq!(cli.resume, true);

        let cli = parse(&["-I", "--no-ignore-stdin", ":"]).unwrap();
        assert_eq!(cli.ignore_stdin, false);
        let cli = parse(&["--no-ignore-stdin", "-I", ":"]).unwrap();
        assert_eq!(cli.ignore_stdin, true);

        let cli = parse(&[
            "--proxy=http:http://foo",
            "--proxy=http:http://bar",
            "--no-proxy",
            ":",
        ])
        .unwrap();
        assert!(cli.proxy.is_empty());

        let cli = parse(&[
            "--no-proxy",
            "--proxy=http:http://foo",
            "--proxy=https:http://bar",
            ":",
        ])
        .unwrap();
        assert_eq!(
            cli.proxy,
            vec![
                Proxy::Http("http://foo".parse().unwrap()),
                Proxy::Https("http://bar".parse().unwrap())
            ]
        );

        let cli = parse(&[
            "--proxy=http:http://foo",
            "--no-proxy",
            "--proxy=https:http://bar",
            ":",
        ])
        .unwrap();
        assert_eq!(cli.proxy, vec![Proxy::Https("http://bar".parse().unwrap())]);

        let cli = parse(&["--bearer=baz", "--no-bearer", ":"]).unwrap();
        assert_eq!(cli.bearer, None);

        let cli = parse(&["--style=solarized", "--no-style", ":"]).unwrap();
        assert_eq!(cli.style, None);
    }
}
