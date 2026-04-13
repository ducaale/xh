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

{{options}}

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
