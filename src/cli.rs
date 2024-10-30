use std::convert::TryFrom;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::mem;
use std::net::{IpAddr, Ipv6Addr};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{anyhow, Context};
use clap::{self, ArgAction, FromArgMatches, ValueEnum};
use encoding_rs::Encoding;
use regex_lite::Regex;
use reqwest::{tls, Method, Url};
use serde::Deserialize;

use crate::buffer::Buffer;
use crate::redacted::SecretString;
use crate::request_items::RequestItems;
use crate::utils::config_dir;

// Some doc comments were copy-pasted from HTTPie

// clap guidelines:
// - Only use `short` with an explicit arg (`short = "x"`)
// - Only use `long` with an implicit arg (just `long`)
//   - Unless it needs a different name, but then also use `name = "..."`
// - Add an uppercase value_name to options that take a value

/// xh is a friendly and fast tool for sending HTTP requests.
///
/// It reimplements as much as possible of HTTPie's excellent design, with a focus
/// on improved performance.
#[derive(clap::Parser, Debug)]
#[clap(
    version,
    long_version = long_version(),
    disable_help_flag = true,
    args_override_self = true
)]
pub struct Cli {
    #[clap(skip)]
    pub httpie_compat_mode: bool,

    /// (default) Serialize data items from the command line as a JSON object.
    ///
    /// Overrides both --form and --multipart.
    #[clap(short = 'j', long, overrides_with_all = &["form", "multipart"])]
    pub json: bool,

    /// Serialize data items from the command line as form fields.
    ///
    /// Overrides both --json and --multipart.
    #[clap(short = 'f', long, overrides_with_all = &["json", "multipart"])]
    pub form: bool,

    /// Like --form, but force a multipart/form-data request even without files.
    ///
    /// Overrides both --json and --form.
    #[clap(long, conflicts_with = "raw", overrides_with_all = &["json", "form"])]
    pub multipart: bool,

    /// Pass raw request data without extra processing.
    #[clap(long, value_name = "RAW")]
    pub raw: Option<String>,

    /// Controls output processing.
    #[clap(
        long,
        value_enum,
        value_name = "STYLE",
        long_help = "\
Controls output processing. Possible values are:

    all      (default) Enable both coloring and formatting
    colors   Apply syntax highlighting to output
    format   Pretty-print json and sort headers
    none     Disable both coloring and formatting

Defaults to \"format\" if the NO_COLOR env is set and to \"none\" if stdout is not tty."
    )]
    pub pretty: Option<Pretty>,

    /// Set output formatting options.
    #[clap(
        long,
        value_name = "FORMAT_OPTIONS",
        long_help = "\
Set output formatting options. Supported option are:

    json.indent:<NUM>
    json.format:<true|false>
    headers.sort:<true|false>

Example: --format-options=json.indent:2,headers.sort:false"
    )]
    pub format_options: Vec<FormatOptions>,

    /// Output coloring style.
    #[clap(short = 's', long, value_enum, value_name = "THEME")]
    pub style: Option<Theme>,

    /// Override the response encoding for terminal display purposes.
    ///
    /// Example: --response-charset=latin1
    #[clap(long, value_name = "ENCODING", value_parser = parse_encoding)]
    pub response_charset: Option<&'static Encoding>,

    /// Override the response mime type for coloring and formatting for the terminal.
    ///
    /// Example: --response-mime=application/json
    #[clap(long, value_name = "MIME_TYPE")]
    pub response_mime: Option<String>,

    /// String specifying what the output should contain
    #[clap(
        short = 'p',
        long,
        value_name = "FORMAT",
        long_help = "\
String specifying what the output should contain

    'H' request headers
    'B' request body
    'h' response headers
    'b' response body
    'm' response metadata

Example: --print=Hb"
    )]
    pub print: Option<Print>,

    /// Print only the response headers. Shortcut for --print=h.
    #[clap(short = 'h', long)]
    pub headers: bool,

    /// Print only the response body. Shortcut for --print=b.
    #[clap(short = 'b', long)]
    pub body: bool,

    /// Print only the response metadata. Shortcut for --print=m.
    #[clap(short = 'm', long)]
    pub meta: bool,

    /// Print the whole request as well as the response.
    ///
    /// Additionally, this enables --all for printing intermediary
    /// requests/responses while following redirects.
    ///
    /// Using verbose twice i.e. -vv will print the response metadata as well.
    ///
    /// Equivalent to --print=HhBb --all.
    #[clap(short = 'v', long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Print full error stack traces and debug log messages.
    ///
    /// Logging can be configured in more detail using the `$RUST_LOG` environment
    /// variable. Set `RUST_LOG=trace` to show even more messages.
    /// See https://docs.rs/env_logger/0.11.3/env_logger/#enabling-logging.
    #[clap(long)]
    pub debug: bool,

    /// Show any intermediary requests/responses while following redirects with --follow.
    #[clap(long)]
    pub all: bool,

    /// The same as --print but applies only to intermediary requests/responses.
    #[clap(short = 'P', long, value_name = "FORMAT")]
    pub history_print: Option<Print>,

    /// Do not print to stdout or stderr.
    ///
    ///  Using quiet twice i.e. -qq will suppress warnings as well.
    #[clap(short = 'q', long, action = ArgAction::Count)]
    pub quiet: u8,

    /// Always stream the response body.
    #[clap(short = 'S', long = "stream", name = "stream")]
    pub stream_raw: bool,

    #[clap(skip)]
    pub stream: Option<bool>,

    /// Save output to FILE instead of stdout.
    #[clap(short = 'o', long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Download the body to a file instead of printing it.
    ///
    /// The Accept-Encoding header is set to identify and any redirects will be followed.
    #[clap(short = 'd', long)]
    pub download: bool,

    /// Resume an interrupted download. Requires --download and --output.
    #[clap(
        short = 'c',
        long = "continue",
        name = "continue",
        requires = "download",
        requires = "output"
    )]
    pub resume: bool,

    /// Create, or reuse and update a session.
    ///
    /// Within a session, custom headers, auth credentials, as well as any cookies sent
    /// by the server persist between requests.
    #[clap(long, value_name = "FILE")]
    pub session: Option<OsString>,

    /// Create or read a session without updating it form the request/response exchange.
    #[clap(long, value_name = "FILE", conflicts_with = "session")]
    pub session_read_only: Option<OsString>,

    #[clap(skip)]
    pub is_session_read_only: bool,

    /// Specify the auth mechanism.
    #[clap(short = 'A', long, value_enum)]
    pub auth_type: Option<AuthType>,

    /// Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer).
    ///
    /// PASS will be prompted if missing. Use a trailing colon (i.e. "USER:")
    /// to authenticate with just a username.
    ///
    /// TOKEN is expected if --auth-type=bearer.
    #[clap(short = 'a', long, value_name = "USER[:PASS] | TOKEN")]
    pub auth: Option<SecretString>,

    /// Authenticate with a bearer token.
    #[clap(long, value_name = "TOKEN", hide = true)]
    pub bearer: Option<SecretString>,

    /// Do not use credentials from .netrc
    #[clap(long)]
    pub ignore_netrc: bool,

    /// Construct HTTP requests without sending them anywhere.
    #[clap(long)]
    pub offline: bool,

    /// (default) Exit with an error status code if the server replies with an error.
    ///
    /// The exit code will be 4 on 4xx (Client Error), 5 on 5xx (Server Error),
    /// or 3 on 3xx (Redirect) if --follow isn't set.
    ///
    /// If stdout is redirected then a warning is written to stderr.
    #[clap(long = "check-status", name = "check-status")]
    pub check_status_raw: bool,

    #[clap(skip)]
    pub check_status: Option<bool>,

    /// Do follow redirects.
    #[clap(short = 'F', long)]
    pub follow: bool,

    /// Number of redirects to follow. Only respected if --follow is used.
    #[clap(long, value_name = "NUM")]
    pub max_redirects: Option<usize>,

    /// Connection timeout of the request.
    ///
    /// The default value is "0", i.e., there is no timeout limit.
    #[clap(long, value_name = "SEC")]
    pub timeout: Option<Timeout>,

    /// Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080.
    ///
    /// PROTOCOL can be "http", "https" or "all".
    ///
    /// If your proxy requires credentials, put them in the URL, like so:
    /// --proxy http:socks5://user:password@proxy.host:8000.
    ///
    /// You can specify proxies for multiple protocols by repeating this option.
    ///
    /// The environment variables "http_proxy" and "https_proxy" can also be used, but
    /// are completely ignored if --proxy is passed.
    #[clap(long, value_name = "PROTOCOL:URL", number_of_values = 1)]
    pub proxy: Vec<Proxy>,

    /// If "no", skip SSL verification. If a file path, use it as a CA bundle.
    ///
    /// Specifying a CA bundle will disable the system's built-in root certificates.
    ///
    /// "false" instead of "no" also works. The default is "yes" ("true").
    #[clap(long, value_name = "VERIFY", value_parser = VerifyParser)]
    pub verify: Option<Verify>,

    /// Use a client side certificate for SSL.
    #[clap(long, value_name = "FILE")]
    pub cert: Option<PathBuf>,

    /// A private key file to use with --cert.
    ///
    /// Only necessary if the private key is not contained in the cert file.
    #[clap(long, value_name = "FILE")]
    pub cert_key: Option<PathBuf>,

    /// Force a particular TLS version.
    ///
    /// "auto" gives the default behavior of negotiating a version
    /// with the server.
    #[clap(long, value_name = "VERSION", value_parser)]
    pub ssl: Option<TlsVersion>,

    /// Use the system TLS library instead of rustls (if enabled at compile time).
    #[clap(long, hide = cfg!(not(all(feature = "native-tls", feature = "rustls"))))]
    pub native_tls: bool,

    /// The default scheme to use if not specified in the URL.
    #[clap(long, value_name = "SCHEME", hide = true)]
    pub default_scheme: Option<String>,

    /// Bypass dot segment (/../ or /./) URL squashing.
    #[clap(long)]
    pub path_as_is: bool,

    /// Make HTTPS requests if not specified in the URL.
    #[clap(long)]
    pub https: bool,

    /// HTTP version to use
    #[clap(long, value_name = "VERSION", value_parser)]
    pub http_version: Option<HttpVersion>,

    /// Override DNS resolution for specific domain to a custom IP.
    ///
    /// You can override multiple domains by repeating this option.
    ///
    /// Example: --resolve=example.com:127.0.0.1
    #[clap(long, value_name = "HOST:ADDRESS")]
    pub resolve: Vec<Resolve>,

    /// Bind to a network interface or local IP address.
    ///
    /// Example: --interface=eth0 --interface=192.168.0.2
    #[clap(long, value_name = "NAME")]
    pub interface: Option<String>,

    /// Resolve hostname to ipv4 addresses only.
    #[clap(short = '4', long)]
    pub ipv4: bool,

    /// Resolve hostname to ipv6 addresses only.
    #[clap(short = '6', long)]
    pub ipv6: bool,

    /// Do not attempt to read stdin.
    ///
    /// This disables the default behaviour of reading the request body from stdin
    /// when a redirected input is detected.
    ///
    /// It is recommended to pass this flag when using xh for scripting purposes.
    /// For more information, refer to https://httpie.io/docs/cli/best-practices.
    #[clap(short = 'I', long)]
    pub ignore_stdin: bool,

    /// Print a translation to a curl command.
    ///
    /// For translating the other way, try https://curl2httpie.online/.
    #[clap(long)]
    pub curl: bool,

    /// Use the long versions of curl's flags.
    #[clap(long)]
    pub curl_long: bool,

    /// Print help.
    #[clap(long, action = ArgAction::HelpShort)]
    pub help: Option<bool>,

    /// The request URL, preceded by an optional HTTP method.
    ///
    /// If the method is omitted, it will default to GET, or to POST
    /// if the request contains a body.
    ///
    /// The URL scheme defaults to "http://" normally, or "https://" if
    /// the program is invoked as "xhs".
    ///
    /// A leading colon works as shorthand for localhost. ":8000" is equivalent
    /// to "localhost:8000", and ":/path" is equivalent to "localhost/path".
    #[clap(value_name = "[METHOD] URL")]
    raw_method_or_url: String,

    /// Optional key-value pairs to be included in the request.
    ///
    /// The separator is used to determine the type:
    ///
    ///     key==value
    ///         Add a query string to the URL.
    ///
    ///     key=value
    ///         Add a JSON property (--json) or form field (--form) to
    ///         the request body.
    ///
    ///     key:=value
    ///         Add a field with a literal JSON value to the request body.
    ///
    ///         Example: "numbers:=[1,2,3] enabled:=true"
    ///
    ///     key@filename
    ///         Upload a file (requires --form or --multipart).
    ///
    ///         To set the filename and mimetype, ";type=" and
    ///         ";filename=" can be used respectively.
    ///
    ///         Example: "pfp@ra.jpg;type=image/jpeg;filename=profile.jpg"
    ///
    ///     @filename
    ///         Use a file as the request body.
    ///
    ///     header:value
    ///         Add a header, e.g. "user-agent:foobar"
    ///
    ///     header:
    ///         Unset a header, e.g. "connection:"
    ///
    ///     header;
    ///         Add a header with an empty value.
    ///
    /// An "@" prefix can be used to read a value from a file. For example: "x-api-key:@api-key.txt".
    ///
    /// A backslash can be used to escape special characters, e.g. "weird\:key=value".
    ///
    /// To construct a complex JSON object, the REQUEST_ITEM's key can be set to a JSON path instead of a field name.
    /// For more information on this syntax, refer to https://httpie.io/docs/cli/nested-json.
    #[clap(value_name = "REQUEST_ITEM", verbatim_doc_comment)]
    raw_rest_args: Vec<String>,

    /// The HTTP method, if supplied.
    #[clap(skip)]
    pub method: Option<Method>,

    /// The request URL.
    #[clap(skip = ("http://placeholder".parse::<Url>().unwrap()))]
    pub url: Url,

    /// Optional key-value pairs to be included in the request.
    #[clap(skip)]
    pub request_items: RequestItems,

    /// The name of the binary.
    #[clap(skip)]
    pub bin_name: String,
}

impl Cli {
    pub fn parse() -> Self {
        if let Some(default_args) = default_cli_args() {
            let mut args = std::env::args_os();
            Self::parse_from(
                std::iter::once(args.next().unwrap_or_else(|| "xh".into()))
                    .chain(default_args.into_iter().map(Into::into))
                    .chain(args),
            )
        } else {
            Self::parse_from(std::env::args_os())
        }
    }

    pub fn parse_from<I>(iter: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        match Self::try_parse_from(iter) {
            Ok(cli) => cli,
            Err(err) => err.exit(),
        }
    }

    pub fn try_parse_from<I>(iter: I) -> clap::error::Result<Self>
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        let mut app = Self::into_app();
        let matches = app.try_get_matches_from_mut(iter)?;
        let mut cli = Self::from_arg_matches(&matches)?;

        match cli.raw_method_or_url.as_str() {
            "help" => {
                // opt-out of clap's auto-generated possible values help for --pretty
                // as we already list them in the long_help
                app = app.mut_arg("pretty", |a| a.hide_possible_values(true));

                app.print_long_help().unwrap();
                safe_exit();
            }
            "generate-completions" => return Err(generate_completions(app, cli.raw_rest_args)),
            "generate-manpages" => return Err(generate_manpages(app, cli.raw_rest_args)),
            _ => {}
        }
        let mut rest_args = mem::take(&mut cli.raw_rest_args).into_iter();
        let raw_url = match parse_method(&cli.raw_method_or_url) {
            Some(method) => {
                cli.method = Some(method);
                rest_args.next().ok_or_else(|| {
                    app.error(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        "Missing <URL>",
                    )
                })?
            }
            None => {
                cli.method = None;
                mem::take(&mut cli.raw_method_or_url)
            }
        };
        for request_item in rest_args {
            cli.request_items.items.push(
                request_item
                    .parse()
                    .map_err(|err: clap::error::Error| err.format(&mut app))?,
            );
        }

        app.get_bin_name()
            .and_then(|name| name.split('.').next())
            .unwrap_or("xh")
            .clone_into(&mut cli.bin_name);

        if matches!(cli.bin_name.as_str(), "https" | "xhs" | "xhttps") {
            cli.https = true;
        }
        if matches!(cli.bin_name.as_str(), "http" | "https")
            || env::var_os("XH_HTTPIE_COMPAT_MODE").is_some()
        {
            cli.httpie_compat_mode = true;
        }

        cli.process_relations(&matches)?;

        cli.url = construct_url(&raw_url, cli.default_scheme.as_deref(), cli.path_as_is).map_err(
            |err| {
                app.error(
                    clap::error::ErrorKind::ValueValidation,
                    format!("Invalid <URL>: {}", err),
                )
            },
        )?;

        if cfg!(not(feature = "rustls")) {
            cli.native_tls = true;
        }

        Ok(cli)
    }

    /// Set flags that are implied by other flags and report conflicting flags.
    fn process_relations(&mut self, matches: &clap::ArgMatches) -> clap::error::Result<()> {
        if self.verbose > 0 {
            self.all = true;
        }
        if self.curl_long {
            self.curl = true;
        }
        if self.https {
            self.default_scheme = Some("https".to_string());
        }
        if self.bearer.is_some() {
            self.auth_type = Some(AuthType::Bearer);
            self.auth = self.bearer.take();
        }
        self.check_status = match (self.check_status_raw, matches.get_flag("no-check-status")) {
            (true, true) => unreachable!(),
            (true, false) => Some(true),
            (false, true) => Some(false),
            (false, false) => None,
        };
        self.stream = match (self.stream_raw, matches.get_flag("no-stream")) {
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
        if self.raw.is_some() && !self.request_items.is_body_empty() {
            return Err(Self::into_app().error(
                clap::error::ErrorKind::ValueValidation,
                "Request body (from --raw) and request data (key=value) cannot be mixed.",
            ));
        }
        if self.session_read_only.is_some() {
            self.is_session_read_only = true;
            self.session = mem::take(&mut self.session_read_only);
        }
        Ok(())
    }

    pub fn into_app() -> clap::Command {
        let app = <Self as clap::CommandFactory>::command();

        // Every option should have a --no- variant that makes it as if it was
        // never passed.
        // https://github.com/clap-rs/clap/issues/815
        // https://github.com/httpie/httpie/blob/225dccb2186f14f871695b6c4e0bfbcdb2e3aa28/httpie/cli/argparser.py#L312
        // Unlike HTTPie we apply the options in order, so the --no- variant
        // has to follow the original to apply. You could have a chain of
        // --x=y --no-x --x=z where the last one takes precedence.
        let negations: Vec<_> = app
            .get_arguments()
            .filter(|a| !a.is_positional())
            .map(|opt| {
                let long = opt.get_long().expect("long option");
                clap::Arg::new(format!("no-{}", long))
                    .long(format!("no-{}", long))
                    .hide(true)
                    .action(ArgAction::SetTrue)
                    // overrides_with is enough to make the flags take effect
                    // We never have to check their values, they'll simply
                    // unset previous occurrences of the original flag
                    .overrides_with(opt.get_id())
            })
            .collect();

        app.args(negations)
            .after_help(format!("Each option can be reset with a --no-OPTION argument.\n\nRun \"{} help\" for more complete documentation.", env!("CARGO_PKG_NAME")))
            .after_long_help("Each option can be reset with a --no-OPTION argument.")
    }

    pub fn logger_config(&self) -> env_logger::Builder {
        if self.debug || std::env::var_os("RUST_LOG").is_some() {
            let env = env_logger::Env::default().default_filter_or("debug");
            let mut builder = env_logger::Builder::from_env(env);

            let start = std::time::Instant::now();
            builder.format(move |buf, record| {
                let time = start.elapsed().as_secs_f64();
                let level = record.level();
                let style = buf.default_level_style(level);
                let module = record.module_path().unwrap_or("");
                let args = record.args();
                writeln!(
                    buf,
                    "[{time:.6}s {style}{level: <5}{style:#} {module}] {args}"
                )
            });

            builder
        } else {
            let env = env_logger::Env::default();
            let mut builder = env_logger::Builder::from_env(env);
            if self.quiet >= 2 {
                builder.filter_level(log::LevelFilter::Error);
            } else {
                builder.filter_level(log::LevelFilter::Warn);
            }

            let bin_name = self.bin_name.clone();
            builder.format(move |buf, record| {
                let level = match record.level() {
                    log::Level::Error => "error",
                    log::Level::Warn => "warning",
                    log::Level::Info => "info",
                    log::Level::Debug => "debug",
                    log::Level::Trace => "trace",
                };
                let args = record.args();
                writeln!(buf, "{bin_name}: {level}: {args}")
            });

            builder
        }
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
                // Can't use log::warn!() because logging isn't initialized yet
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
    path_as_is: bool,
) -> std::result::Result<Url, url::ParseError> {
    let mut default_scheme = default_scheme.unwrap_or("http://").to_string();
    if !default_scheme.ends_with("://") {
        default_scheme.push_str("://");
    }
    let url_string = if let Some(url) = url.strip_prefix("://") {
        // Allow users to quickly convert a URL copied from a clipboard to xh/HTTPie command
        // by simply adding a space before `://`.
        // Example: https://example.org -> https ://example.org
        format!("{}{}", default_scheme, url)
    } else if url.starts_with(':') {
        format!("{}{}{}", default_scheme, "localhost", url)
    } else if !Regex::new("[a-zA-Z0-9]://.+").unwrap().is_match(url) {
        format!("{}{}", default_scheme, url)
    } else {
        url.to_string()
    };
    if path_as_is {
        build_raw_url(&url_string)
    } else {
        Ok(url_string.parse()?)
    }
}

fn build_raw_url(url_string: &str) -> std::result::Result<Url, url::ParseError> {
    let url_parsed = Url::parse(url_string)?;
    let after_scheme = url_string
        .split_once("://")
        .map(|it| it.1)
        .unwrap_or(url_string);
    let path = after_scheme.split_once('/').unwrap().1;

    let root = if cfg!(windows) { "C:/" } else { "/" };
    let mut url = Url::from_file_path(format!("{}{}", root, path)).unwrap();

    url.set_host(url_parsed.host_str())?;
    url.set_scheme(url_parsed.scheme()).unwrap();
    url.set_port(url_parsed.port()).unwrap();
    url.set_query(url_parsed.query());
    url.set_username(url_parsed.username()).unwrap();
    url.set_password(url_parsed.password()).unwrap();
    url.set_fragment(url_parsed.fragment());
    Ok(url)
}

#[cfg(feature = "man-completion-gen")]
// This signature is a little weird: we either return an error or don't return at all
fn generate_completions(mut app: clap::Command, rest_args: Vec<String>) -> clap::error::Error {
    let bin_name = app.get_bin_name().unwrap().to_string();
    if rest_args.len() != 1 {
        return app.error(
            clap::error::ErrorKind::WrongNumberOfValues,
            "Usage: xh generate-completions <DIRECTORY>",
        );
    }

    for &shell in clap_complete::Shell::value_variants() {
        // Elvish complains about multiple deprecations and these don't seem to work
        if shell != clap_complete::Shell::Elvish {
            clap_complete::generate_to(shell, &mut app, &bin_name, &rest_args[0]).unwrap();
        }
    }
    safe_exit();
}

#[cfg(feature = "man-completion-gen")]
fn generate_manpages(mut app: clap::Command, rest_args: Vec<String>) -> clap::error::Error {
    use roff::{bold, italic, roman, Roff};
    use time::OffsetDateTime as DateTime;

    if rest_args.len() != 1 {
        return app.error(
            clap::error::ErrorKind::WrongNumberOfValues,
            "Usage: xh generate-manpages <DIRECTORY>",
        );
    }

    let items: Vec<_> = app.get_arguments().filter(|i| !i.is_hide_set()).collect();

    let mut request_items_roff = Roff::new();
    let request_items = items
        .iter()
        .find(|opt| opt.get_id() == "raw_rest_args")
        .unwrap();
    let request_items_help = request_items
        .get_long_help()
        .or_else(|| request_items.get_help())
        .expect("request_items is missing help")
        .to_string();

    // replace the indents in request_item help with proper roff controls
    // For example:
    //
    // ```
    // normal help normal help
    // normal help normal help
    //
    //   request-item-1
    //     help help
    //
    //   request-item-2
    //     help help
    //
    // normal help normal help
    // ```
    //
    // Should look like this with roff controls
    //
    // ```
    // normal help normal help
    // normal help normal help
    // .RS 12
    // .TP
    // request-item-1
    // help help
    // .TP
    // request-item-2
    // help help
    // .RE
    //
    // .RS
    // normal help normal help
    // .RE
    // ```
    let lines: Vec<&str> = request_items_help.lines().collect();
    let mut rs = false;
    for i in 0..lines.len() {
        if lines[i].is_empty() {
            let prev = lines[i - 1].chars().take_while(|&x| x == ' ').count();
            let next = lines[i + 1].chars().take_while(|&x| x == ' ').count();
            if prev != next && next > 0 {
                if !rs {
                    request_items_roff.control("RS", ["8"]);
                    rs = true;
                }
                request_items_roff.control("TP", ["4"]);
            } else if prev != next && next == 0 {
                request_items_roff.control("RE", []);
                request_items_roff.text(vec![roman("")]);
                request_items_roff.control("RS", []);
            } else {
                request_items_roff.text(vec![roman(lines[i])]);
            }
        } else {
            request_items_roff.text(vec![roman(lines[i].trim())]);
        }
    }
    request_items_roff.control("RE", []);

    let mut options_roff = Roff::new();
    let non_pos_items = items
        .iter()
        .filter(|a| !a.is_positional())
        .collect::<Vec<_>>();

    for opt in non_pos_items {
        let mut header = vec![];
        if let Some(short) = opt.get_short() {
            header.push(bold(format!("-{}", short)));
        }
        if let Some(long) = opt.get_long() {
            if !header.is_empty() {
                header.push(roman(", "));
            }
            header.push(bold(format!("--{}", long)));
        }
        if opt.get_action().takes_values() {
            let value_name = &opt.get_value_names().unwrap();
            if opt.get_long().is_some() {
                header.push(roman("="));
            } else {
                header.push(roman(" "));
            }

            if opt.get_id() == "auth" {
                header.push(italic("USER"));
                header.push(roman("["));
                header.push(italic(":PASS"));
                header.push(roman("] | "));
                header.push(italic("TOKEN"));
            } else {
                header.push(italic(value_name.join(" ")));
            }
        }
        let mut body = vec![];

        let mut help = opt
            .get_long_help()
            .or_else(|| opt.get_help())
            .expect("option is missing help")
            .to_string();
        if !help.ends_with('.') {
            help.push('.')
        }
        body.push(roman(help));

        let possible_values = opt.get_possible_values();
        if !possible_values.is_empty()
            && !opt.is_hide_possible_values_set()
            && opt.get_id() != "pretty"
        {
            let possible_values_text = format!(
                "\n\n[possible values: {}]",
                possible_values
                    .iter()
                    .map(|v| v.get_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            body.push(roman(possible_values_text));
        }
        options_roff.control("TP", ["4"]);
        options_roff.text(header);
        options_roff.text(body);
    }

    let mut manpage = fs::read_to_string(format!("{}/man-template.roff", rest_args[0])).unwrap();

    let current_date = {
        let (year, month, day) = DateTime::now_utc().date().to_calendar_date();
        format!("{}-{:02}-{:02}", year, u8::from(month), day)
    };

    manpage = manpage.replace("{{date}}", &current_date);
    manpage = manpage.replace("{{version}}", app.get_version().unwrap());
    manpage = manpage.replace("{{request_items}}", request_items_roff.to_roff().trim());
    manpage = manpage.replace("{{options}}", options_roff.to_roff().trim());

    fs::write(format!("{}/xh.1", rest_args[0]), manpage).unwrap();
    safe_exit();
}

#[cfg(not(feature = "man-completion-gen"))]
fn generate_completions(mut _app: clap::Command, _rest_args: Vec<String>) -> clap::error::Error {
    clap::Error::raw(
        clap::error::ErrorKind::InvalidSubcommand,
        "generate-completions requires enabling man-completion-gen feature\n",
    )
}

#[cfg(not(feature = "man-completion-gen"))]
fn generate_manpages(mut _app: clap::Command, _rest_args: Vec<String>) -> clap::error::Error {
    clap::Error::raw(
        clap::error::ErrorKind::InvalidSubcommand,
        "generate-manpages requires enabling man-completion-gen feature\n",
    )
}

#[derive(Default, ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AuthType {
    #[default]
    Basic,
    Bearer,
    Digest,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum TlsVersion {
    // ssl2.3 is not a real version but it's how HTTPie spells "auto"
    #[clap(name = "auto", alias = "ssl2.3")]
    Auto,
    #[clap(name = "tls1")]
    Tls1_0,
    #[clap(name = "tls1.1")]
    Tls1_1,
    #[clap(name = "tls1.2")]
    Tls1_2,
    #[clap(name = "tls1.3")]
    Tls1_3,
}

impl From<TlsVersion> for Option<tls::Version> {
    fn from(version: TlsVersion) -> Self {
        match version {
            TlsVersion::Auto => None,
            TlsVersion::Tls1_0 => Some(tls::Version::TLS_1_0),
            TlsVersion::Tls1_1 => Some(tls::Version::TLS_1_1),
            TlsVersion::Tls1_2 => Some(tls::Version::TLS_1_2),
            TlsVersion::Tls1_3 => Some(tls::Version::TLS_1_3),
        }
    }
}

#[derive(ValueEnum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Pretty {
    /// (default) Enable both coloring and formatting
    All,
    /// Apply syntax highlighting to output
    Colors,
    /// Pretty-print json and sort headers
    Format,
    /// Disable both coloring and formatting
    None,
}

impl Pretty {
    pub fn color(self) -> bool {
        matches!(self, Pretty::Colors | Pretty::All)
    }

    pub fn format(self) -> bool {
        matches!(self, Pretty::Format | Pretty::All)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormatOptions {
    pub json_indent: Option<usize>,
    pub json_format: Option<bool>,
    pub headers_sort: Option<bool>,
}

impl FormatOptions {
    pub fn merge(mut self, other: &Self) -> Self {
        self.json_indent = other.json_indent.or(self.json_indent);
        self.json_format = other.json_format.or(self.json_format);
        self.headers_sort = other.headers_sort.or(self.headers_sort);
        self
    }
}

impl FromStr for FormatOptions {
    type Err = anyhow::Error;
    fn from_str(options: &str) -> anyhow::Result<FormatOptions> {
        let mut format_options = FormatOptions::default();

        for argument in options.to_lowercase().split(',') {
            let (key, value) = argument
                .split_once(':')
                .context("Format options consist of a key and a value, separated by a \":\".")?;

            let value_error = || format!("Invalid value '{value}' in '{argument}'");

            match key {
                "json.indent" => {
                    format_options.json_indent = Some(value.parse().with_context(value_error)?);
                }
                "json.format" => {
                    format_options.json_format = Some(value.parse().with_context(value_error)?);
                }
                "headers.sort" => {
                    format_options.headers_sort = Some(value.parse().with_context(value_error)?);
                }
                "json.sort_keys" | "xml.format" | "xml.indent" => {
                    return Err(anyhow!("Unsupported option '{key}'"));
                }
                _ => {
                    return Err(anyhow!("Unknown option '{key}'"));
                }
            }
        }
        Ok(format_options)
    }
}

#[derive(Default, ValueEnum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Theme {
    #[default]
    Auto,
    Solarized,
    Monokai,
    Fruity,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Auto => "ansi",
            Theme::Solarized => "solarized",
            Theme::Monokai => "monokai",
            Theme::Fruity => "fruity",
        }
    }

    pub(crate) fn as_syntect_theme(&self) -> &'static syntect::highlighting::Theme {
        &crate::formatting::THEMES.themes[self.as_str()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Print {
    pub request_headers: bool,
    pub request_body: bool,
    pub response_headers: bool,
    pub response_body: bool,
    pub response_meta: bool,
}

impl Print {
    pub fn new(
        verbose: u8,
        headers: bool,
        body: bool,
        meta: bool,
        quiet: bool,
        offline: bool,
        buffer: &Buffer,
    ) -> Self {
        if verbose > 0 {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: true,
                response_body: true,
                response_meta: verbose > 1,
            }
        } else if quiet {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: false,
                response_meta: false,
            }
        } else if offline {
            Print {
                request_headers: true,
                request_body: true,
                response_headers: false,
                response_body: false,
                response_meta: false,
            }
        } else if headers {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: false,
                response_meta: false,
            }
        } else if body || !buffer.is_terminal() {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: true,
                response_meta: false,
            }
        } else if meta {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: false,
                response_body: false,
                response_meta: true,
            }
        } else {
            Print {
                request_headers: false,
                request_body: false,
                response_headers: true,
                response_body: true,
                response_meta: false,
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
        let mut response_meta = false;

        for char in s.chars() {
            match char {
                'H' => request_headers = true,
                'B' => request_body = true,
                'h' => response_headers = true,
                'b' => response_body = true,
                'm' => response_meta = true,
                char => return Err(anyhow!("{:?} is not a valid value", char)),
            }
        }

        let p = Print {
            request_headers,
            request_body,
            response_headers,
            response_body,
            response_meta,
        };
        Ok(p)
    }
}

#[derive(Debug, Clone)]
pub struct Timeout(Duration);

impl Timeout {
    pub fn as_duration(&self) -> Option<Duration> {
        Some(self.0).filter(|t| !t.is_zero())
    }
}

impl FromStr for Timeout {
    type Err = anyhow::Error;

    fn from_str(sec: &str) -> anyhow::Result<Timeout> {
        match f64::from_str(sec) {
            Ok(s) if !s.is_nan() => {
                if s.is_sign_negative() {
                    Err(anyhow!("Connection timeout is negative"))
                } else if s >= Duration::MAX.as_secs_f64() || s.is_infinite() {
                    Err(anyhow!("Connection timeout is too big"))
                } else {
                    Ok(Timeout(Duration::from_secs_f64(s)))
                }
            }
            _ => Err(anyhow!("Connection timeout is not a valid number")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct Resolve {
    pub domain: String,
    pub addr: IpAddr,
}

impl FromStr for Resolve {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        if s.chars().filter(|&c| c == ':').count() == 2 {
            // More than two colons could mean an IPv6 address.
            // Exactly two colons probably means the user added a port, curl-style.
            return Err(anyhow!(
                "Value should be formatted as <HOST>:<ADDRESS> (not <HOST>:<PORT>:<ADDRESS>)"
            ));
        }

        let (domain, raw_addr) = s
            .split_once(':')
            .context("Value should be formatted as <HOST>:<ADDRESS>")?;

        let addr = if raw_addr.starts_with('[') && raw_addr.ends_with(']') {
            // Support IPv6 addresses enclosed in square brackets e.g. [::1]
            Ipv6Addr::from_str(&raw_addr[1..raw_addr.len() - 1]).map(IpAddr::V6)
        } else {
            raw_addr.parse()
        }
        .with_context(|| format!("Invalid address '{raw_addr}'"))?;

        Ok(Resolve {
            domain: domain.to_string(),
            addr,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verify {
    Yes,
    No,
    CustomCaBundle(PathBuf),
}

impl clap::builder::ValueParserFactory for Verify {
    type Parser = VerifyParser;
    fn value_parser() -> Self::Parser {
        VerifyParser
    }
}

#[derive(Clone, Debug)]
pub struct VerifyParser;
impl clap::builder::TypedValueParser for VerifyParser {
    type Value = Verify;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> clap::error::Result<Self::Value, clap::Error> {
        Ok(match value.to_ascii_lowercase().to_str() {
            Some("no") | Some("false") => Verify::No,
            Some("yes") | Some("true") => Verify::Yes,
            _ => Verify::CustomCaBundle(PathBuf::from(value)),
        })
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

#[derive(Default, Debug, PartialEq, Eq, Copy, Clone)]
pub enum BodyType {
    #[default]
    Json,
    Form,
    Multipart,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum HttpVersion {
    #[clap(name = "1.0", alias = "1")]
    Http10,
    #[clap(name = "1.1")]
    Http11,
    #[clap(name = "2")]
    Http2,
    #[clap(name = "2-prior-knowledge")]
    Http2PriorKnowledge,
}

/// HTTPie uses Python's str.decode(). That one's very accepting of different spellings.
/// encoding_rs is not.
///
/// Python accepts `utf16` and `u16` (and even `~~~~UtF////16@@`), encoding_rs makes you
/// spell it `utf-16`.
///
/// There are also some encodings which encoding_rs doesn't support but HTTPie does, e.g utf-7.
///
/// See https://github.com/ducaale/xh/pull/184#pullrequestreview-787528027
///
/// We interpret `utf-16` as LE (little-endian) UTF-16, but that's not quite right.
/// In Python it turns on BOM sniffing: it defaults to LE (at least on LE machines)
/// but if there's a byte order mark at the start of the document it may switch to
/// BE instead.
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

    for encoding in [
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
        if let Some(first_digit_index) = encoding.find(|c: char| c.is_ascii_digit()) {
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

    fn parse<I>(args: I) -> clap::error::Result<Cli>
    where
        I: IntoIterator,
        I::Item: Into<OsString> + Clone,
    {
        Cli::try_parse_from(
            Some("xh".into())
                .into_iter()
                .chain(args.into_iter().map(Into::into)),
        )
    }

    #[test]
    fn implicit_method() {
        let cli = parse(["example.org"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn explicit_method() {
        let cli = parse(["get", "example.org"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn method_edge_cases() {
        // "localhost" is interpreted as method; this is undesirable, but expected
        parse(["localhost"]).unwrap_err();

        // Non-standard method used by varnish
        let cli = parse(["purge", ":"]).unwrap();
        assert_eq!(cli.method, Some("PURGE".parse().unwrap()));
        assert_eq!(cli.url.to_string(), "http://localhost/");

        // Zero-length arg should not be interpreted as method, but fail to parse as URL
        parse([""]).unwrap_err();
    }

    #[test]
    fn missing_url() {
        parse(["get"]).unwrap_err();
    }

    #[test]
    fn space_in_url() {
        let cli = parse(["post", "example.org/foo bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::POST));
        assert_eq!(cli.url.to_string(), "http://example.org/foo%20bar");
        assert!(cli.request_items.items.is_empty());
    }

    #[test]
    fn url_with_leading_double_slash_colon() {
        let cli = parse(["://example.org"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://example.org/");
    }

    #[test]
    fn url_with_leading_colon() {
        let cli = parse([":3000"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://localhost:3000/");

        let cli = parse([":3000/users"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://localhost:3000/users");

        let cli = parse([":"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://localhost/");

        let cli = parse([":/users"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://localhost/users");
    }

    #[test]
    fn url_with_scheme() {
        let cli = parse(["https://example.org"]).unwrap();
        assert_eq!(cli.url.to_string(), "https://example.org/");
    }

    #[test]
    fn url_without_scheme() {
        let cli = parse(["example.org"]).unwrap();
        assert_eq!(cli.url.to_string(), "http://example.org/");
    }

    #[test]
    fn request_items() {
        let cli = parse(["get", "example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, Some(Method::GET));
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert_eq!(
            cli.request_items.items,
            vec![RequestItem::DataField {
                key: "foo".to_string(),
                raw_key: "foo".to_string(),
                value: "bar".to_string()
            }]
        );
    }

    #[test]
    fn request_items_implicit_method() {
        let cli = parse(["example.org", "foo=bar"]).unwrap();
        assert_eq!(cli.method, None);
        assert_eq!(cli.url.to_string(), "http://example.org/");
        assert_eq!(
            cli.request_items.items,
            vec![RequestItem::DataField {
                key: "foo".to_string(),
                raw_key: "foo".to_string(),
                value: "bar".to_string()
            }]
        );
    }

    #[test]
    fn request_type_overrides() {
        let cli = parse(["--form", "--json", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, true);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(["--json", "--form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Form);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, true);
        assert_eq!(cli.multipart, false);

        let cli = parse([":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);
    }

    #[test]
    fn superfluous_arg() {
        parse(["get", "example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn superfluous_arg_implicit_method() {
        parse(["example.org", "foobar"]).unwrap_err();
    }

    #[test]
    fn multiple_methods() {
        parse(["get", "post", "example.org"]).unwrap_err();
    }

    #[test]
    fn proxy_invalid_protocol() {
        Cli::try_parse_from([
            "xh",
            "--proxy=invalid:http://127.0.0.1:8000",
            "get",
            "example.org",
        ])
        .unwrap_err();
    }

    #[test]
    fn proxy_invalid_proxy_url() {
        Cli::try_parse_from(["xh", "--proxy=http:127.0.0.1:8000", "get", "example.org"])
            .unwrap_err();
    }

    #[test]
    fn proxy_http() {
        let proxy = parse(["--proxy=http:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::Http(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }

    #[test]
    fn proxy_https() {
        let proxy = parse(["--proxy=https:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::Https(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }

    #[test]
    fn proxy_all() {
        let proxy = parse(["--proxy=all:http://127.0.0.1:8000", "get", "example.org"])
            .unwrap()
            .proxy;

        assert_eq!(
            proxy,
            vec!(Proxy::All(Url::parse("http://127.0.0.1:8000").unwrap()))
        );
    }

    #[test]
    fn executable_name() {
        let args = Cli::try_parse_from(["xhs", "example.org"]).unwrap();
        assert_eq!(args.https, true);
    }

    #[test]
    fn executable_name_extension() {
        let args = Cli::try_parse_from(["xhs.exe", "example.org"]).unwrap();
        assert_eq!(args.https, true);
    }

    #[test]
    fn negated_flags() {
        let cli = parse(["--no-offline", ":"]).unwrap();
        assert_eq!(cli.offline, false);

        // In HTTPie, the order doesn't matter, so this would be false
        let cli = parse(["--no-offline", "--offline", ":"]).unwrap();
        assert_eq!(cli.offline, true);

        // In HTTPie, this resolves to json, but that seems wrong
        let cli = parse(["--no-form", "--multipart", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(["--multipart", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Multipart);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, true);

        let cli = parse(["--form", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, false);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(["--form", "--json", "--no-form", ":"]).unwrap();
        assert_eq!(cli.request_items.body_type, BodyType::Json);
        assert_eq!(cli.json, true);
        assert_eq!(cli.form, false);
        assert_eq!(cli.multipart, false);

        let cli = parse(["--curl-long", "--no-curl-long", ":"]).unwrap();
        assert_eq!(cli.curl_long, false);
        let cli = parse(["--no-curl-long", "--curl-long", ":"]).unwrap();
        assert_eq!(cli.curl_long, true);

        let cli = parse(["-do=fname", "--continue", "--no-continue", ":"]).unwrap();
        assert_eq!(cli.resume, false);
        let cli = parse(["-do=fname", "--no-continue", "--continue", ":"]).unwrap();
        assert_eq!(cli.resume, true);

        let cli = parse(["-I", "--no-ignore-stdin", ":"]).unwrap();
        assert_eq!(cli.ignore_stdin, false);
        let cli = parse(["--no-ignore-stdin", "-I", ":"]).unwrap();
        assert_eq!(cli.ignore_stdin, true);

        let cli = parse([
            "--proxy=http:http://foo",
            "--proxy=http:http://bar",
            "--no-proxy",
            ":",
        ])
        .unwrap();
        assert!(cli.proxy.is_empty());

        let cli = parse([
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

        let cli = parse([
            "--proxy=http:http://foo",
            "--no-proxy",
            "--proxy=https:http://bar",
            ":",
        ])
        .unwrap();
        assert_eq!(cli.proxy, vec![Proxy::Https("http://bar".parse().unwrap())]);

        let cli = parse(["--bearer=baz", "--no-bearer", ":"]).unwrap();
        assert_eq!(cli.bearer, None);

        let cli = parse(["--style=solarized", "--no-style", ":"]).unwrap();
        assert_eq!(cli.style, None);

        let cli = parse([
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
        let cli = parse([":"]).unwrap();
        assert_eq!(cli.check_status, None);

        let cli = parse(["--check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(true));

        let cli = parse(["--no-check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(false));

        let cli = parse(["--check-status", "--no-check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(false));

        let cli = parse(["--no-check-status", "--check-status", ":"]).unwrap();
        assert_eq!(cli.check_status, Some(true));
    }

    #[test]
    fn negating_stream() {
        let cli = parse([":"]).unwrap();
        assert_eq!(cli.stream, None);

        let cli = parse(["--stream", ":"]).unwrap();
        assert_eq!(cli.stream, Some(true));

        let cli = parse(["--no-stream", ":"]).unwrap();
        assert_eq!(cli.stream, Some(false));

        let cli = parse(["--stream", "--no-stream", ":"]).unwrap();
        assert_eq!(cli.stream, Some(false));

        let cli = parse(["--no-stream", "--stream", ":"]).unwrap();
        assert_eq!(cli.stream, Some(true));
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
            assert_eq!(parse_encoding(input).unwrap(), output);
        }

        assert_eq!(parse_encoding("notreal").is_err(), true);
        assert_eq!(parse_encoding("").is_err(), true);
    }

    #[test]
    fn parse_format_options() {
        let invalid_format_options = vec![
            // malformed strings
            ":8",
            "json.indent:",
            ":",
            "",
            "json.format:true, json.indent:4",
            // invalid values
            "json.indent:-8",
            "json.format:ffalse",
            // unsupported options
            "json.sort_keys:true",
            "xml.format:false",
            "xml.indent:false",
            // invalid options
            "toml.format:true",
        ];

        for format_option in invalid_format_options {
            assert!(FormatOptions::from_str(format_option).is_err());
        }

        assert!(FormatOptions::from_str(
            "json.indent:8,json.format:true,headers.sort:false,JSON.FORMAT:TRUE"
        )
        .is_ok());
    }

    #[test]
    fn merge_format_options() {
        let format_option_one = FormatOptions::from_str("json.indent:2").unwrap();
        let format_option_two =
            FormatOptions::from_str("headers.sort:true,headers.sort:false").unwrap();
        assert_eq!(
            format_option_one.merge(&format_option_two),
            FormatOptions {
                json_indent: Some(2),
                headers_sort: Some(false),
                json_format: None
            }
        )
    }

    #[test]
    fn parse_resolve() {
        let invalid_test_cases = [
            "example.com:[127.0.0.1]",
            "example.com:80:[::1]",
            "example.com::::1",
            "example.com:1",
            "example.com:example.com",
            "http://example.com:127.0.0.1",
            "http://example.com:[::1]",
            "http://example.com:80:[::1]",
        ];

        for input in invalid_test_cases {
            assert!(Resolve::from_str(input).is_err())
        }

        assert!(Resolve::from_str("example.com:127.0.0.1").is_ok());
        assert!(Resolve::from_str("example.com:::1").is_ok());
        assert!(Resolve::from_str("example.com:[::1]").is_ok());
    }
}
