## NAME

`xh` - Friendly and fast tool for sending HTTP requests

## SYNOPSIS

```
xh [OPTIONS] [METHOD] URL [REQUEST_ITEM ...]
```

## DESCRIPTION

**xh** is an HTTP client with a friendly command line interface. It strives to
have readable output and easy-to-use options.

xh is mostly compatible with HTTPie: see http(1).

The `--curl` option can be used to print a curl(1) translation of the
command instead of sending a request.

## POSITIONAL ARGUMENTS

- `[METHOD]`:
  The HTTP method to use for the request.
  
  This defaults to GET, or to POST if the request contains a body.

- `URL`:
  The URL to request.

  The URL scheme defaults to `http://` normally, or `https://` if
  the program is invoked as `xhs`.

  A leading colon works as shorthand for localhost. `:8000` is equivalent
  to `localhost:8000`, and `:/path` is equivalent to `localhost/path`.

- `[REQUEST_ITEM ...]`:

  Optional key-value pairs to be included in the request.

  The separator is used to determine the type:

  - `key==value`:
    Add a query string to the URL.

  - `key=value`:
    Add a JSON property (`--json`) or form field (`--form`) to the request body.

  - `key:=value`:
    Add a field with a literal JSON value to the request body.
  
    Example: `numbers:=[1,2,3] enabled:=true`

  - `key@filename`:
    Upload a file (requires `--form` or `--multipart`).
  
    To set the filename and mimetype, `;type=` and `;filename=` can be used respectively.
  
    Example: `pfp@ra.jpg;type=image/jpeg;filename=profile.jpg`

  - `@filename`:
    Use a file as the request body.

  - `header:value`:
    Add a header, e.g. `user-agent:foobar`

  - `header:`:
    Unset a header, e.g. `connection:`

  - `header;`:
    Add a header with an empty value.

  An `@` prefix can be used to read a value from a file. For example: `x-api-key:@api-key.txt`.
  
  A backslash can be used to escape special characters, e.g. `weird\:key=value`.
  
  To construct a complex JSON object, the `REQUEST_ITEM`'s key can be set to a JSON path instead of a field name. For more information on this syntax, refer to https://httpie.io/docs/cli/nested-json.

## OPTIONS

Each `--OPTION` can be reset with a `--no-OPTION` argument.

- `-j`, `--json`: (default) Serialize data items from the command line as a JSON object.
  
  Overrides both --form and --multipart.

- `-f`, `--form`: Serialize data items from the command line as form fields.
  
  Overrides both --json and --multipart.

- `--multipart`: Like --form, but force a multipart/form-data request even without files.
  
  Overrides both --json and --form.

- `--raw`=`RAW`: Pass raw request data without extra processing.

- `--pretty`=`STYLE`: Controls output processing. Possible values are:
  
  - `all`: (default) Enable both coloring and formatting
  - `colors`: Apply syntax highlighting to output
  - `format`: Pretty-print json and sort headers
  - `none`: Disable both coloring and formatting
  
  Defaults to "format" if the NO_COLOR env is set and to "none" if stdout is not tty.

- `--format-options`=`FORMAT_OPTIONS`: Set output formatting options. Supported option are:
  
  - `json.indent:<NUM>`
  - `json.format:<true|false>`
  - `xml.indent:<NUM>`
  - `xml.format:<true|false>`
  - `headers.sort:<true|false>`
  
  Example: --format-options=json.indent:2,headers.sort:false.

- `-s`, `--style`=`THEME`: Output coloring style.

  [possible values: `auto`, `solarized`, `monokai`, `fruity`]

- `--response-charset`=`ENCODING`: Override the response encoding for terminal display purposes.
  
  Example: --response-charset=latin1.

- `--response-mime`=`MIME_TYPE`: Override the response mime type for coloring and formatting for the terminal.
  
  Example: --response-mime=application/json.

- `-p`, `--print`=`FORMAT`: String specifying what the output should contain
  
  - `H`: request headers
  - `B`: request body
  - `h`: response headers
  - `b`: response body
  - `m`: response metadata
  
  Example: --print=Hb.

- `-h`, `--headers`: Print only the response headers. Shortcut for --print=h.

- `-b`, `--body`: Print only the response body. Shortcut for --print=b.

- `-m`, `--meta`: Print only the response metadata. Shortcut for --print=m.

- `-v`, `--verbose`: Print the whole request as well as the response.
  
  Additionally, this enables --all for printing intermediary requests/responses while following redirects.
  
  Using verbose twice i.e. -vv will print the response metadata as well.
  
  Equivalent to --print=HhBb --all.

- `--debug`: Print full error stack traces and debug log messages.
  
  Logging can be configured in more detail using the `$RUST_LOG` environment variable. Set `RUST_LOG=trace` to show even more messages. See https://docs.rs/env_logger/0.11.3/env_logger/#enabling-logging.

- `--all`: Show any intermediary requests/responses while following redirects with --follow.

- `-P`, `--history-print`=`FORMAT`: The same as --print but applies only to intermediary requests/responses.

- `-q`, `--quiet`: Do not print to stdout or stderr.
  
  Using quiet twice i.e. -qq will suppress warnings as well.

- `-S`, `--stream`: Always stream the response body.

- `-x`, `--compress`: Content compressed (encoded) with Deflate algorithm.
  
  The Content-Encoding header is set to deflate.
  
  Compression is skipped if it appears that compression ratio is negative. Compression can be forced by repeating this option.
  
  Note: Compression cannot be used if the Content-Encoding request header is present.

- `-o`, `--output`=`FILE`: Save output to FILE instead of stdout.

- `-d`, `--download`: Download the body to a file instead of printing it.
  
  The Accept-Encoding header is set to identity and any redirects will be followed.

- `-c`, `--continue`: Resume an interrupted download. Requires --download and --output.

- `--session`=`FILE`: Create, or reuse and update a session.
  
  Within a session, custom headers, auth credentials, as well as any cookies sent by the server persist between requests.

- `--session-read-only`=`FILE`: Create or read a session without updating it from the request/response exchange.

- `-A`, `--auth-type`=`AUTH_TYPE`: Specify the auth mechanism.

  [possible values: `basic`, `bearer`, `digest`]

- `-a`, `--auth`=`USER[:PASS] | TOKEN`: Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer).
  
  PASS will be prompted if missing. Use a trailing colon (i.e. "USER:") to authenticate with just a username.
  
  TOKEN is expected if --auth-type=bearer.

- `--ignore-netrc`: Do not use credentials from .netrc.

- `--offline`: Construct HTTP requests without sending them anywhere.

- `--check-status`: (default) Exit with an error status code if the server replies with an error.
  
  The exit code will be 4 on 4xx (Client Error), 5 on 5xx (Server Error), or 3 on 3xx (Redirect) if --follow isn't set.
  
  If stdout is redirected then a warning is written to stderr.

- `-F`, `--follow`: Do follow redirects.

- `--max-redirects`=`NUM`: Number of redirects to follow. Only respected if --follow is used.

- `--timeout`=`SEC`: Connection timeout of the request.
  
  The default value is "0", i.e., there is no timeout limit.

- `--proxy`=`PROTOCOL:URL`: Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080.
  
  PROTOCOL can be "all", "http" or "https".
  
  If your proxy requires credentials, put them in the URL, like so: --proxy http:socks5://user:password@proxy.host:8000.
  
  You can specify proxies for multiple protocols by repeating this option.
  
  The environment variables "ALL_PROXY", "HTTP_PROXY" and "HTTPS_PROXY" can also be used, but are completely ignored if --proxy is passed.

- `--verify`=`VERIFY`: If "no", skip SSL verification. If a file path, use it as a CA bundle.
  
  Specifying a CA bundle will disable the system's built-in root certificates.
  
  "false" instead of "no" also works. The default is "yes" ("true").

- `--cert`=`FILE`: Use a client side certificate for SSL.

- `--cert-key`=`FILE`: A private key file to use with --cert.
  
  Only necessary if the private key is not contained in the cert file.

- `--ssl`=`VERSION`: Force a particular TLS version.
  
  "auto" gives the default behavior of negotiating a version with the server.

  [possible values: `auto`, `tls1`, `tls1.1`, `tls1.2`, `tls1.3`]

- `--native-tls`: Use the system TLS library instead of rustls (if enabled at compile time).

- `--https`: Make HTTPS requests if not specified in the URL.

- `--http-version`=`VERSION`: HTTP version to use.

  [possible values: `1.0`, `1.1`, `2`, `2-prior-knowledge`, `3-prior-knowledge`]

- `--resolve`=`HOST:ADDRESS`: Override DNS resolution for specific domain to a custom IP.
  
  You can override multiple domains by repeating this option.
  
  Example: --resolve=example.com:127.0.0.1.

- `--interface`=`NAME`: Bind to a network interface or local IP address.
  
  Example: --interface=eth0 --interface=192.168.0.2.

- `-4`, `--ipv4`: Resolve hostname to ipv4 addresses only.

- `-6`, `--ipv6`: Resolve hostname to ipv6 addresses only.

- `--unix-socket`=`FILE`: Connect using a Unix domain socket.
  
  Example: xh :/index.html --unix-socket=/var/run/temp.sock.

- `-I`, `--ignore-stdin`: Do not attempt to read stdin.
  
  This disables the default behaviour of reading the request body from stdin when a redirected input is detected.
  
  It is recommended to pass this flag when using xh for scripting purposes. For more information, refer to https://httpie.io/docs/cli/best-practices.

- `--curl`: Print a translation to a curl command.
  
  For translating the other way, try https://curl2httpie.online/.

- `--curl-long`: Use the long versions of curl's flags.

- `--generate`=`KIND`: Generate shell completions or man pages. Possible values are:
  
  - `complete-bash`: Generate completions for bash
  - `complete-elvish`: Generate completions for elvish
  - `complete-fish`: Generage completions for fish
  - `complete-nushell`: Generate completions for nushell
  - `complete-powershell`: Generate completions for powershell
  - `complete-zsh`: Generate completions for zsh
  - `man`: Generate manual page in roff format
  
  Example: xh --generate=complete-bash > xh.bash.

- `--help`: Print help.

- `-V`, `--version`: Print version.

## EXIT STATUS

- `0`: Successful program execution.
- `1`: Usage, syntax or network error.
- `2`: Request timeout.
- `3`: Unexpected HTTP 3xx Redirection.
- `4`: HTTP 4xx Client Error.
- `5`: HTTP 5xx Server Error.
- `6`: Too many redirects.

## ENVIRONMENT

- `XH_CONFIG_DIR`:
  Specifies where to look for config.json and named session data.
  The default is `~/.config/xh` for Linux/macOS and `%APPDATA%\xh` for Windows.

- `XH_HTTPIE_COMPAT_MODE`:
  Enables the HTTPie Compatibility Mode. The only current difference is that
  `--check-status` is not enabled by default. An alternative to setting this
  environment variable is to rename the binary to either http or https.

- `REQUESTS_CA_BUNDLE`, `CURL_CA_BUNDLE`:
  Sets a custom CA bundle path.

- `ALL_PROXY=[protocol://]<host>[:port]`:
  Sets the proxy server for all requests (unless overridden for a specific protocol).

- `HTTP_PROXY=[protocol://]<host>[:port]`:
  Sets the proxy server to use for HTTP.

- `HTTPS_PROXY=[protocol://]<host>[:port]`:
  Sets the proxy server to use for HTTPS.

- `NO_PROXY`:
  List of comma-separated hosts for which to ignore the other proxy environment variables. `*` matches all host names.

- `NETRC`:
  Location of the `.netrc` file.

- `NO_COLOR`:
  Disables output coloring. See <https://no-color.org>

- `RUST_LOG`:
  Configure low-level debug messages. See https://docs.rs/env_logger/0.11.3/env_logger/#enabling-logging

## FILES

- `~/.config/xh/config.json`:
  xh configuration file. The only configurable option is `default_options`
  which is a list of default shell arguments that gets passed to `xh`.
  Example:

  ```json
  { "default_options": ["--native-tls", "--style=solarized"] }
  ```

- `~/.netrc`, `~/_netrc`:
Auto-login information file.

- `~/.config/xh/sessions`:
Session data directory grouped by domain and port number.

## EXAMPLES

Send a GET request:
```
xh httpbin.org/json
```

Send a POST request with body `{"name": "ahmed", "age": 24}`:
```
xh httpbin.org/post name=ahmed age:=24
```

Send a GET request to http://httpbin.org/json?id=5&sort=true:
```
xh get httpbin.org/json id==5 sort==true
```

Send a GET request and include a header named `X-Api-Key` with value `12345`:
```
xh get Ihttpbin.org/json x-api-key:12345
```

Send a POST request with body read from stdin:
```
echo "[1, 2, 3]" | xh post httpbin.org/post
```

Send a PUT request and pipe the result to `less`:
```
xh put httpbin.org/put id:=49 age:=25 | less
```

Download and save to `res.json`:
```
xh -d httpbin.org/json -o res.jso\fR
```

Make a request with a custom user agent:
```
xh httpbin.org/get user-agent:foobar
```

Make an HTTPS request to https://example.com:
```
xhs example.com
```

## REPORTING BUGS
xh's Github issues https://github.com/ducaale/xh/issues

## SEE ALSO
curl(1), http(1)

HTTPie's online documentation https://httpie.io/docs/cli
