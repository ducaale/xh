use std::convert::TryFrom;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::Write;
use std::mem;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;
use encoding_rs::Encoding;
use reqwest::{tls, Method, Url};
use serde::Deserialize;
use structopt::clap::{self, arg_enum, AppSettings, Error, ErrorKind, Result};
use structopt::StructOpt;

use crate::buffer::Buffer;
use crate::regex;
use crate::request_items::RequestItems;
use crate::utils::config_dir;

// Some doc comments were copy-pasted from HTTPie

// structopt guidelines:
// - Only use `short` with an explicit arg (`short = "x"`)
// - Only use `long` with an implicit arg (just `long`)
//   - Unless it needs a different name, but then also use `name = "..."`
// - Add an uppercase value_name to options that take a value
// - Add a line with triple {n} after any long doc comment

/// xh is a friendly and fast tool for sending HTTP requests.
///
/// It reimplements as much as possible of HTTPie's excellent design, with a focus
/// on improved performance.
#[derive(StructOpt, Debug)]
#[structopt(
    name = "xh",
    long_version = long_version(),
    settings = &[
        AppSettings::DeriveDisplayOrder,
        AppSettings::UnifiedHelpMessage,
        AppSettings::ColoredHelp,
        AppSettings::AllArgsOverrideSelf,
    ],
)]
pub struct Cli {
    #[structopt(skip)]
    pub httpie_compat_mode: bool,

    /// (default) Serialize data items from the command line as a JSON object.
    #[structopt(short = "j", long, overrides_with_all = &["form", "multipart"])]
    pub json: bool,

    /// Serialize data items from the command line as form fields.
    #[structopt(short = "f", long, overrides_with_all = &["json", "multipart"])]
    pub form: bool,

    /// Like --form, but force a multipart/form-data request even without files.
    #[structopt(short = "m", long, overrides_with_all = &["json", "form"])]
    pub multipart: bool,

    /// Pass raw request data without extra processing.
    #[structopt(long, value_name = "RAW")]
    pub raw: Option<String>,

    /// Controls output processing.
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true, value_name = "STYLE")]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long, value_name = "THEME", possible_values = &Theme::variants(), case_insensitive = true)]
    pub style: Option<Theme>,

    /// Override the response encoding for terminal display purposes.
    ///
    /// Example: `--response-charset=latin1`
    /// {n}{n}{n}
    #[structopt(long, value_name = "ENCODING", parse(try_from_str = parse_encoding))]
    pub response_charset: Option<&'static Encoding>,

    /// Override the response mime type for coloring and formatting for the terminal
    ///
    /// Example: `--response-mime=application/json`
    /// {n}{n}{n}
    #[structopt(long, value_name = "MIME_TYPE")]
    pub response_mime: Option<String>,

    /// String specifying what the output should contain.
    ///
    /// Use `H` and `B` for request header and body respectively,
    /// and `h` and `b` for response hader and body.
    ///
    /// Example: `--print=Hb`
    /// {n}{n}{n}
    #[structopt(short = "p", long, value_name = "FORMAT")]
    pub print: Option<Print>,

    /// Print only the response headers. Shortcut for --print=h.
    #[structopt(short = "h", long)]
    pub headers: bool,

    /// Print only the response body. Shortcut for --print=b.
    #[structopt(short = "b", long)]
    pub body: bool,

    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Show any intermediary requests/responses while following redirects with --follow.
    #[structopt(long)]
    pub all: bool,

    /// The same as --print but applies only to intermediary requests/responses.
    #[structopt(short = "P", long, value_name = "FORMAT")]
    pub history_print: Option<Print>,

    /// Do not print to stdout or stderr.
    #[structopt(short = "q", long)]
    pub quiet: bool,

    /// Always stream the response body.
    #[structopt(short = "S", long)]
    pub stream: bool,

    /// Save output to FILE instead of stdout.
    #[structopt(short = "o", long, value_name = "FILE", parse(from_os_str))]
    pub output: Option<PathBuf>,

    /// Download the body to a file instead of printing it.
    #[structopt(short = "d", long)]
    pub download: bool,

    /// Resume an interrupted download. Requires --download and --output.
    #[structopt(short = "c", long = "continue", name = "continue")]
    pub resume: bool,

    /// Create, or reuse and update a session.
    ///
    /// Within a session, custom headers, auth credentials, as well as any cookies sent
    /// by the server persist between requests.
    /// {n}{n}{n}
    #[structopt(long, value_name = "FILE", parse(from_os_str))]
    pub session: Option<OsString>,

    /// Create or read a session without updating it form the request/response exchange.
    #[structopt(
        long,
        value_name = "FILE",
        conflicts_with = "session",
        parse(from_os_str)
    )]
    pub session_read_only: Option<OsString>,

    #[structopt(skip)]
    pub is_session_read_only: bool,

    /// Specify the auth mechanism.
    #[structopt(short = "A", long, possible_values = &AuthType::variants(), case_insensitive = true)]
    pub auth_type: Option<AuthType>,

    /// Authenticate as USER with PASS or with TOKEN.
    ///
    /// PASS will be prompted if missing. Use a trailing colon (i.e. `USER:`)
    /// to authenticate with just a username.
    ///
    /// TOKEN is expected if `--auth-type=bearer`.
    /// {n}{n}{n}
    #[structopt(short = "a", long, value_name = "USER[:PASS] | TOKEN")]
    pub auth: Option<String>,

    /// Authenticate with a bearer token.
    #[structopt(long, value_name = "TOKEN", hidden = true)]
    pub bearer: Option<String>,

    /// Do not use credentials from .netrc
    #[structopt(long)]
    pub ignore_netrc: bool,

    /// Construct HTTP requests without sending them anywhere.
    #[structopt(long)]
    pub offline: bool,

    /// (default) Exit with an error status code if the server replies with an error.
    ///
    /// The exit code will be 4 on 4xx (Client Error), 5 on 5xx (Server Error),
    /// or 3 on 3xx (Redirect) if --follow isn't set.
    ///
    /// If stdout is redirected then a warning is written to stderr.
    /// {n}{n}{n}
    #[structopt(long = "check-status", name = "check-status")]
    pub check_status_raw: bool,

    #[structopt(skip)]
    pub check_status: Option<bool>,

    /// Do follow redirects.
    #[structopt(short = "F", long)]
    pub follow: bool,

    /// Number of redirects to follow, only respected if `follow` is set.
    #[structopt(long, value_name = "NUM")]
    pub max_redirects: Option<usize>,

    /// Connection timeout of the request.
    ///
    /// The default value is `0`, i.e., there is no timeout limit.
    /// {n}{n}{n}
    #[structopt(long, value_name = "SEC")]
    pub timeout: Option<Timeout>,

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
    /// {n}{n}{n}
    #[structopt(long, value_name = "PROTOCOL:URL", number_of_values = 1)]
    pub proxy: Vec<Proxy>,

    /// If "no", skip SSL verification. If a file path, use it as a CA bundle.
    ///
    /// Specifying a CA bundle will disable the system's built-in root certificates.
    ///
    /// "false" instead of "no" also works. The default is "yes" ("true").
    /// {n}{n}{n}
    #[structopt(long, value_name = "VERIFY", parse(from_os_str))]
    pub verify: Option<Verify>,

    /// Use a client side certificate for SSL.
    #[structopt(long, value_name = "FILE", parse(from_os_str))]
    pub cert: Option<PathBuf>,

    /// A private key file to use with --cert.
    ///
    /// Only necessary if the private key is not contained in the cert file.
    /// {n}{n}{n}
    #[structopt(long, value_name = "FILE", parse(from_os_str))]
    pub cert_key: Option<PathBuf>,

    /// Force a particular TLS version.
    ///
    /// "auto" or "ssl2.3" gives the default behavior of negotiating a version
    /// with the server.
    #[structopt(long, value_name = "VERSION", parse(from_str = parse_tls_version),
      possible_values = &["auto", "ssl2.3", "tls1", "tls1.1", "tls1.2", "tls1.3"])]
    // The nested option is weird, but parse_tls_version can return None.
    // If the inner option doesn't use a qualified path structopt gets confused.
    pub ssl: Option<std::option::Option<tls::Version>>,

    /// Use the system TLS library instead of rustls (if enabled at compile time).
    #[structopt(long)]
    pub native_tls: bool,

    /// The default scheme to use if not specified in the URL.
    #[structopt(long, value_name = "SCHEME", hidden = true)]
    pub default_scheme: Option<String>,

    /// Make HTTPS requests if not specified in the URL.
    #[structopt(long)]
    pub https: bool,

    /// HTTP version to use
    #[structopt(long, value_name = "VERSION", possible_values = &["1", "1.0", "1.1", "2"])]
    pub http_version: Option<HttpVersion>,

    /// Do not attempt to read stdin.
    #[structopt(short = "I", long)]
    pub ignore_stdin: bool,

    /// Print a translation to a `curl` command.
    ///
    /// For translating the other way, try https://curl2httpie.online/.
    /// {n}{n}{n}
    #[structopt(long)]
    pub curl: bool,

    /// Use the long versions of curl's flags.
    #[structopt(long)]
    pub curl_long: bool,

    /// The request URL, preceded by an optional HTTP method.
    ///
    /// METHOD can be `get`, `post`, `head`, `put`, `patch`, `delete` or `options`.
    /// If omitted, either a GET or a POST will be done depending on whether the
    /// request sends data.
    /// {n}{n}{n}
    #[structopt(value_name = "[METHOD] URL")]
    raw_method_or_url: String,

    /// Optional key-value pairs to be included in the request
    ///
    ///   - key==value to add a parameter to the URL
    ///   - key=value to add a JSON field (--json) or form field (--form)
    ///   - key:=value to add a complex JSON value (e.g. `numbers:=[1,2,3]`)
    ///   - key@filename to upload a file from filename (with --form)
    ///   - @filename to use a file as the request body
    ///   - header:value to add a header
    ///   - header: to unset a header
    ///   - header; to add a header with an empty value
    ///
    /// A backslash can be used to escape special characters (e.g. weird\:key=value).
    #[structopt(value_name = "REQUEST_ITEM", verbatim_doc_comment)]
    raw_rest_args: Vec<String>,

    /// The HTTP method, if supplied.
    #[structopt(skip)]
    pub method: Option<Method>,

    /// The request URL.
    #[structopt(skip = ("http://placeholder".parse::<Url>().unwrap()))]
    pub url: Url,

    /// Optional key-value pairs to be included in the request.
    #[structopt(skip)]
    pub request_items: RequestItems,

    /// The name of the binary.
    #[structopt(skip)]
    pub bin_name: String,
}

/// Names of flags that negate other flags.
///
/// This should in principle contain all options. When adding an option above,
/// add the negating option here. Try to keep it in alphabetical order.
///
/// This list may also be generated by running:
/// cargo expand --all-features cli | rg -o 'with_name\s*\("([^"]*)"\)' -r '    "--no-$1",' | rg -v 'raw-' | sort
/// (https://crates.io/crates/cargo-expand, https://crates.io/crates/ripgrep)
/// But this is fragile, please apply human judgment.
const NEGATION_FLAGS: &[&str] = &[
    "--no-all",
    "--no-auth",
    "--no-auth-type",
    "--no-bearer",
    "--no-body",
    "--no-cert",
    "--no-cert-key",
    "--no-check-status",
    "--no-continue",
    "--no-curl",
    "--no-curl-long",
    "--no-default-scheme",
    "--no-download",
    "--no-follow",
    "--no-form",
    "--no-headers",
    "--no-history-print",
    "--no-http-version",
    "--no-https",
    "--no-ignore-netrc",
    "--no-ignore-stdin",
    "--no-json",
    "--no-max-redirects",
    "--no-multipart",
    "--no-native-tls",
    "--no-offline",
    "--no-output",
    "--no-pretty",
    "--no-print",
    "--no-proxy",
    "--no-quiet",
    "--no-raw",
    "--no-response-charset",
    "--no-response-mime",
    "--no-session",
    "--no-session-read-only",
    "--no-ssl",
    "--no-stream",
    "--no-style",
    "--no-timeout",
    "--no-verbose",
    "--no-verify",
];

impl Cli {
    pub fn parse() -> Self {
        if let Some(default_args) = default_cli_args() {
            let mut args = std::env::args_os();
            Cli::from_iter(
                std::iter::once(args.next().unwrap_or_else(|| "xh".into()))
                    .chain(default_args.into_iter().map(Into::into))
                    .chain(args),
            )
        } else {
            Cli::from_iter(std::env::args_os())
        }
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
        let raw_url = match parse_method(&cli.raw_method_or_url) {
            Some(method) => {
                cli.method = Some(method);
                rest_args.next().ok_or_else(|| {
                    Error::with_description("Missing URL", ErrorKind::MissingArgumentOrSubcommand)
                })?
            }
            None => {
                cli.method = None;
                mem::take(&mut cli.raw_method_or_url)
            }
        };
        for request_item in rest_args {
            cli.request_items.items.push(request_item.parse()?);
        }

        cli.bin_name = app
            .get_bin_name()
            .and_then(|name| name.split('.').next())
            .unwrap_or("xh")
            .to_owned();

        if matches!(cli.bin_name.as_str(), "https" | "xhs" | "xhttps") {
            cli.https = true;
        }
        if matches!(cli.bin_name.as_str(), "http" | "https")
            || env::var_os("XH_HTTPIE_COMPAT_MODE").is_some()
        {
            cli.httpie_compat_mode = true;
        }

        cli.process_relations(&matches)?;

        cli.url = construct_url(
            &raw_url,
            cli.default_scheme.as_deref(),
            cli.request_items.query(),
        )
        .map_err(|err| Error {
            message: format!("Invalid URL: {}", err),
            kind: ErrorKind::ValueValidation,
            info: None,
        })?;

        Ok(cli)
    }

    /// Set flags that are implied by other flags and report conflicting flags.
    fn process_relations(&mut self, matches: &clap::ArgMatches) -> clap::Result<()> {
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
        if self.verbose {
            self.all = true;
        }
        if self.curl_long {
            self.curl = true;
        }
        if self.https {
            self.default_scheme = Some("https".to_string());
        }
        if self.bearer.is_some() {
            self.auth_type = Some(AuthType::bearer);
            self.auth = self.bearer.take();
        }
        self.check_status = match (self.check_status_raw, matches.is_present("no-check-status")) {
            (true, true) => unreachable!(),
            (true, false) => Some(true),
            (false, true) => Some(false),
            (false, false) => None,
        };
        if self.download {
            self.follow = true;
            self.check_status = Some(true);
        }
        // `overrides_with_all` ensures that only one of these is true
        if self.json {
            self.request_items.body_type = BodyType::Json;
        } else if self.form {
            self.request_items.body_type = BodyType::Form;
        } else if self.multipart {
            self.request_items.body_type = BodyType::Multipart;
        }
        if self.session_read_only.is_some() {
            self.is_session_read_only = true;
            self.session = mem::take(&mut self.session_read_only);
        }
        Ok(())
    }

    pub fn clap() -> clap::App<'static, 'static> {
        let mut app = <Self as StructOpt>::clap();
        // Clap 2.33 implements color output via ansi_term crate,
        // which does not handle `NO_COLOR` environment variable.
        // We handle it here.
        if env::var_os("NO_COLOR").is_some() {
            app = app.setting(AppSettings::ColorNever);
        }
        for &flag in NEGATION_FLAGS {
            // `orig` and `flag` both need a static lifetime, so we
            // build `orig` by trimming `flag` instead of building `flag`
            // by extending `orig`
            let orig = flag.strip_prefix("--no-").unwrap();
            app = app.arg(
                // The name is inconsequential, but it has to be unique and it
                // needs a static lifetime, and `flag` satisfies that
                clap::Arg::with_name(&flag[2..])
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

#[derive(Deserialize)]
struct Config {
    default_options: Vec<String>,
}

fn default_cli_args() -> Option<Vec<String>> {
    let content = match fs::read_to_string(config_dir()?.join("config.json")) {
        Ok(file) => Some(file),
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "\n{}: warning: Unable to read config file: {}\n",
                    env!("CARGO_PKG_NAME"),
                    err
                );
            }
            None
        }
    }?;

    match serde_json::from_str::<Config>(&content) {
        Ok(config) => Some(config.default_options),
        Err(err) => {
            eprintln!(
                "\n{}: warning: Unable to parse config file: {}\n",
                env!("CARGO_PKG_NAME"),
                err
            );
            None
        }
    }
}

fn parse_method(method: &str) -> Option<Method> {
    // This unfortunately matches "localhost"
    if !method.is_empty() && method.chars().all(|c| c.is_ascii_alphabetic()) {
        // Method parsing seems to fail if the length is 0 or if there's a null byte
        // Our checks rule those both out, so .unwrap() is safe
        Some(method.to_ascii_uppercase().parse().unwrap())
    } else {
        None
    }
}

fn construct_url(
    url: &str,
    default_scheme: Option<&str>,
    query: Vec<(&str, &str)>,
) -> std::result::Result<Url, url::ParseError> {
    let mut default_scheme = default_scheme.unwrap_or("http://").to_string();
    if !default_scheme.ends_with("://") {
        default_scheme.push_str("://");
    }
    let mut url: Url = if url.starts_with(':') {
        format!("{}{}{}", default_scheme, "localhost", url).parse()?
    } else if !regex!("[a-zA-Z0-9]://.+").is_match(url) {
        format!("{}{}", default_scheme, url).parse()?
    } else {
        url.parse()?
    };
    if !query.is_empty() {
        // If we run this even without adding pairs it adds a `?`, hence
        // the .is_empty() check
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            pairs.append_pair(name, value);
        }
    }
    Ok(url)
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
        basic, bearer, digest
    }
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::basic
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

/// The caller must check in advance if the string is valid. (structopt does this.)
fn parse_tls_version(text: &str) -> Option<tls::Version> {
    match text {
        // ssl2.3 is not a real version but it's how HTTPie spells "auto"
        "auto" | "ssl2.3" => None,
        "tls1" => Some(tls::Version::TLS_1_0),
        "tls1.1" => Some(tls::Version::TLS_1_1),
        "tls1.2" => Some(tls::Version::TLS_1_2),
        "tls1.3" => Some(tls::Version::TLS_1_3),
        _ => unreachable!(),
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

arg_enum! {
    #[allow(non_camel_case_types)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub enum Theme {
        auto, solarized, monokai, fruity
    }
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::auto => "ansi",
            Theme::solarized => "solarized",
            Theme::monokai => "monokai",
            Theme::fruity => "fruity",
        }
    }
}

#[derive(Debug, Clone, Copy)]
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
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Print> {
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
                char => return Err(anyhow!("{:?} is not a valid value", char)),
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

#[derive(Debug)]
pub struct Timeout(Duration);

impl Timeout {
    pub fn as_duration(&self) -> Option<Duration> {
        Some(self.0).filter(|t| t != &Duration::from_nanos(0))
    }
}

impl FromStr for Timeout {
    type Err = anyhow::Error;

    fn from_str(sec: &str) -> anyhow::Result<Timeout> {
        let pos_sec: f64 = match sec.parse::<f64>() {
            Ok(sec) if sec.is_sign_positive() => sec,
            _ => return Err(anyhow!("Invalid seconds as connection timeout")),
        };

        let dur = Duration::from_secs_f64(pos_sec);
        Ok(Timeout(dur))
    }
}

#[derive(Debug, PartialEq)]
pub enum Proxy {
    Http(Url),
    Https(Url),
    All(Url),
}

impl FromStr for Proxy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let split_arg: Vec<&str> = s.splitn(2, ':').collect();
        match split_arg[..] {
            [protocol, url] => {
                let url = reqwest::Url::try_from(url).map_err(|e| {
                    anyhow!(
                        "Invalid proxy URL '{}' for protocol '{}': {}",
                        url,
                        protocol,
                        e
                    )
                })?;

                match protocol.to_lowercase().as_str() {
                    "http" => Ok(Proxy::Http(url)),
                    "https" => Ok(Proxy::Https(url)),
                    "all" => Ok(Proxy::All(url)),
                    _ => Err(anyhow!("Unknown protocol to set a proxy for: {}", protocol)),
                }
            }
            _ => Err(anyhow!(
                "The value passed to --proxy should be formatted as <PROTOCOL>:<PROXY_URL>"
            )),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Verify {
    Yes,
    No,
    CustomCaBundle(PathBuf),
}

impl From<&OsStr> for Verify {
    fn from(verify: &OsStr) -> Verify {
        if let Some(text) = verify.to_str() {
            match text.to_lowercase().as_str() {
                "no" | "false" => return Verify::No,
                "yes" | "true" => return Verify::Yes,
                _ => (),
            }
        }
        Verify::CustomCaBundle(PathBuf::from(verify))
    }
}

impl fmt::Display for Verify {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verify::No => write!(f, "no"),
            Verify::Yes => write!(f, "yes"),
            Verify::CustomCaBundle(path) => write!(f, "custom ca bundle: {}", path.display()),
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum BodyType {
    Json,
    Form,
    Multipart,
}

impl Default for BodyType {
    fn default() -> Self {
        BodyType::Json
    }
}

#[derive(Debug)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http2,
}

impl FromStr for HttpVersion {
    type Err = Error;
    fn from_str(version: &str) -> Result<HttpVersion> {
        match version {
            "1.0" => Ok(HttpVersion::Http10),
            "1" | "1.1" => Ok(HttpVersion::Http11),
            "2" => Ok(HttpVersion::Http2),
            _ => unreachable!(),
        }
    }
}

// HTTPie recognizes some encoding names that encoding_rs doesn't e.g utf16 has to spelled as utf-16.
// There are also some encodings which encoding_rs doesn't support but HTTPie does e.g utf-7.
// See https://github.com/ducaale/xh/pull/184#pullrequestreview-787528027
fn parse_encoding(encoding: &str) -> anyhow::Result<&'static Encoding> {
    let normalized_encoding = encoding.to_lowercase().replace(
        |c: char| (!c.is_alphanumeric() && c != '_' && c != '-' && c != ':'),
        "",
    );

    match normalized_encoding.as_str() {
        "u8" | "utf" => return Ok(encoding_rs::UTF_8),
        "u16" => return Ok(encoding_rs::UTF_16LE),
        _ => (),
    }

    for encoding in &[
        &normalized_encoding,
        &normalized_encoding.replace(&['-', '_'][..], ""),
        &normalized_encoding.replace('_', "-"),
        &normalized_encoding.replace('-', "_"),
    ] {
        if let Some(encoding) = Encoding::for_label(encoding.as_bytes()) {
            return Ok(encoding);
        }
    }

    {
        let mut encoding = normalized_encoding.replace(&['-', '_'][..], "");
        if let Some(first_digit_index) = encoding.find(|c: char| c.is_digit(10)) {
            encoding.insert(first_digit_index, '-');
            if let Some(encoding) = Encoding::for_label(encoding.as_bytes()) {
                return Ok(encoding);
            }
        }
    }

    Err(anyhow::anyhow!(
        "{} is not a supported encoding, please refer to https://encoding.spec.whatwg.org/#names-and-labels \
         for supported encodings",
        encoding
    ))
}

/// Based on the function used by clap to abort
fn safe_exit() -> ! {
    let _ = std::io::stdout().lock().flush();
    let _ = std::io::stderr().lock().flush();
    std::process::exit(0);
}

fn long_version() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), "\n", env!("XH_FEATURES"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::request_items::RequestItem;

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
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn explicit_method() {
        let cli = parse(&["get", "example.org"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn method_edge_cases() {
        // "localhost" is interpreted as method; this is undesirable, but expected
        parse(&["localhost"]).unwrap_err();

        // Non-standard method used by varnish
        let cli = parse(&["purge", ":"]).unwrap();
        assert_eq!(cli.method, Some("PURGE".parse().unwrap()));
        assert_eq!(cli.url.to_string(), "http://localhost/");

        // Zero-length arg should not be interpreted as method, but fail to parse as URL
        parse(&[""]).unwrap_err();
    }

    #[test]
    fn missing_url() {
        parse(&["get"]).unwrap_err();
    }

    #[test]
    fn space_in_url() {
        let cli = parse(&["post", "example.org/foo bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::POST));
        assert_eq!(cli.url.to_string(), "http://example.org/foo%20bar");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn request_items() {
        let cli = parse(&["get", "example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert_eq!(
            cli.request_items.items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn request_items_implicit_method() {
        let cli = parse(&["example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert_eq!(
            cli.request_items.items,
            vec![RequestItem::DataField("foo".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn request_type_overrides() {
        let cli = parse(&["--form", "--json", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, true);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(&["--json", "--form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Form);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, true);
        assert_eq!(cli.multipart, false);

        let cli = parse(&[":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
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

        // In HTTPie, the order doesn't matter, so this would be false
        let cli = parse(&["--no-offline", "--offline", ":"]).unwrap();
        assert_eq!(cli.offline, true);

        // In HTTPie, this resolves to json, but that seems wrong
        let cli = parse(&["--no-form", "--multipart", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(&["--multipart", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(&["--form", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(&["--form", "--json", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
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

        let cli = parse(&[
            "--auth=foo:bar",
            "--auth-type=bearer",
            "--no-auth-type",
            ":",
        ])
        .unwrap();
        assert_eq!(cli.bearer, None);
        assert_eq!(cli.auth_type, None);
    }

    #[test]
    fn negating_check_status() {
        let cli = parse(&[":"]).unwrap();
        assert_eq!(cli.check_status, None);

        let cli = parse(&["--check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(true));

        let cli = parse(&["--no-check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(false));

        let cli = parse(&["--check-status", "--no-check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(false));

        let cli = parse(&["--no-check-status", "--check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(true));
    }

    #[test]
    fn parse_encoding_label() {
        let test_cases = vec![
            ("~~~~UtF////16@@", encoding_rs::UTF_16LE),
            ("utf16", encoding_rs::UTF_16LE),
            ("utf_16_be", encoding_rs::UTF_16BE),
            ("utf16be", encoding_rs::UTF_16BE),
            ("utf-16-be", encoding_rs::UTF_16BE),
            ("utf_8", encoding_rs::UTF_8),
            ("utf8", encoding_rs::UTF_8),
            ("utf-8", encoding_rs::UTF_8),
            ("u8", encoding_rs::UTF_8),
            ("iso8859_6", encoding_rs::ISO_8859_6),
            ("iso_8859-2:1987", encoding_rs::ISO_8859_2),
            ("l1", encoding_rs::WINDOWS_1252),
            ("elot-928", encoding_rs::ISO_8859_7),
        ];

        for (input, output) in test_cases {
            assert_eq!(parse_encoding(input).unwrap(), output)
        }

        assert_eq!(parse_encoding("notreal").is_err(), true);
        assert_eq!(parse_encoding("").is_err(), true);
    }
}
