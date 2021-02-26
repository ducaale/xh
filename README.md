# xh
[![Version info](https://img.shields.io/crates/v/xh.svg)](https://crates.io/crates/xh)

xh is a friendly and fast tool for sending HTTP requests. It reimplements as much
as possible of [HTTPie's](https://httpie.io/) excellent design.

[![asciicast](/assets/xh-demo.gif)](https://asciinema.org/a/390748)

## Installation

### On macOS via Homebrew
```
brew install xh
```

### On windows via Scoop
```
scoop install xh
```

### On Arch linux via Pacman
```
pacman -S xh
```

### From binaries
The [release page](https://github.com/ducaale/xh/releases) contains prebuilt binaries for Linux, macOS and Windows.

### From source
Make sure that you have Rust 1.45 or later installed.

```
cargo install xh
```

## Usage
```
USAGE:
    xh [FLAGS] [OPTIONS] <[METHOD] URL> [--] [REQUEST_ITEM]...

FLAGS:
        --offline         Construct HTTP requests without sending them anywhere
    -j, --json            (default) Serialize data items from the command line as a JSON object
    -f, --form            Serialize data items from the command line as form fields
    -m, --multipart       Like --form, but force a multipart/form-data request even without files
    -I, --ignore-stdin    Do not attempt to read stdin
    -F, --follow          Do follow redirects
    -d, --download        Download the body to a file instead of printing it
    -h, --headers         Print only the response headers, shortcut for --print=h
    -b, --body            Print only the response body, Shortcut for --print=b
    -c, --continue        Resume an interrupted download. Requires --download and --output
    -v, --verbose         Print the whole request as well as the response
    -q, --quiet           Do not print to stdout or stderr
    -S, --stream          Always stream the response body
        --check-status    Exit with an error status code if the server replies with an error
        --curl            Print a translation to a `curl` command
        --curl-long       Use the long versions of curl's flags
        --https           Make HTTPS requests if not specified in the URL
        --help            Prints help information
    -V, --version         Prints version information

OPTIONS:
    -a, --auth <USER[:PASS]>         Authenticate as USER with PASS. PASS will be prompted if missing
        --bearer <TOKEN>             Authenticate with a bearer token
    -o, --output <FILE>              Save output to FILE instead of stdout
        --max-redirects <NUM>        Number of redirects to follow, only respected if `follow` is set
    -p, --print <FORMAT>             String specifying what the output should contain
        --pretty <STYLE>             Controls output processing [possible values: all, colors, format, none]
    -s, --style <THEME>              Output coloring style [possible values: auto, solarized]
        --proxy <PROTOCOL:URL>...    Use a proxy for a protocol. For example: `--proxy https:http://proxy.host:8080`
        --verify <VERIFY>            If "no", skip SSL verification. If a file path, use it as a CA bundle
        --cert <FILE>                Use a client side certificate for SSL
        --cert-key <FILE>            A private key file to use with --cert

ARGS:
    <[METHOD] URL>       The request URL, preceded by an optional HTTP method
    <REQUEST_ITEM>...    Optional key-value pairs to be included in the request
```

Run `xh help` for more detailed information.

## Request Items

`xh` uses [HTTPie's request-item syntax](https://httpie.io/docs#request-items) to set headers, request body, query string, etc.

* `=`/`:=` for setting the request body's JSON fields (`=` for strings and `:=` for other JSON types).
* `==` for adding query strings.
* `@` for including files in multipart requests e.g `picture@hello.jpg` or `picture@hello.jpg;type=image/jpeg`.
* `:` for adding or removing headers e.g `connection:keep-alive` or `connection:`.
* `;` for including headers with empty values e.g `header-without-value;`.

## Examples

```sh
# Send a GET request
xh httpbin.org/json

# Send a POST request with body {"name": "ahmed", "age": 24}
xh httpbin.org/post name=ahmed age:=24

# Send a GET request with querystring id=5&sort=true
xh get httpbin.org/json id==5 sort==true

# Send a GET request and include a header named x-api-key with value 12345
xh get httpbin.org/json x-api-key:12345

# Send a PUT request and pipe the result to less
xh put httpbin.org/put id:=49 age:=25 | less

# Download and save to res.json
xh -d httpbin.org/json -o res.json
```

## Syntaxes and themes used
- [Sublime-HTTP](https://github.com/samsalisbury/Sublime-HTTP)
- [json-kv](https://github.com/aurule/json-kv)
- [Sublime Packages](https://github.com/sublimehq/Packages/tree/fa6b8629c95041bf262d4c1dab95c456a0530122)
- [ansi-dark theme](https://github.com/sharkdp/bat/blob/master/assets/themes/ansi-dark.tmTheme)
