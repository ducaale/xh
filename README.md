# xh
[![Version info](https://img.shields.io/crates/v/xh.svg)](https://crates.io/crates/xh)

Yet another [HTTPie](https://httpie.io/) clone in Rust.

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
Make sure that you have Rust 1.46 or later installed.

```
cargo install xh
```

## Usage
```
xh 0.7.0
USAGE:
    xh [FLAGS] [OPTIONS] <[METHOD] URL> [REQUEST_ITEM]...

FLAGS:
        --offline         Construct HTTP requests without sending them anywhere
    -j, --json            (default) Data items from the command line are serialized as a JSON object
    -f, --form            Data items from the command line are serialized as form fields
    -m, --multipart       Similar to --form, but always sends a multipart/form-data request (i.e., even without files)
    -I, --ignore-stdin    Do not attempt to read stdin
    -F, --follow          Do follow redirects
    -d, --download
    -h, --headers         Print only the response headers, shortcut for --print=h
    -b, --body            Print only the response body, Shortcut for --print=b
    -c, --continue        Resume an interrupted download
    -v, --verbose         Print the whole request as well as the response
    -q, --quiet           Do not print to stdout or stderr
    -S, --stream          Always stream the response body
        --help            Prints help information
    -V, --version         Prints version information

OPTIONS:
    -A, --auth-type <auth-type>              Specify the auth mechanism [possible values: Basic, Bearer]
    -a, --auth <auth>
    -o, --output <output>                    Save output to FILE instead of stdout
        --max-redirects <max-redirects>      Number of redirects to follow, only respected if `follow` is set
    -p, --print <print>                      String specifying what the output should contain
        --pretty <pretty>                    Controls output processing [possible values: All, Colors, Format, None]
    -s, --style <theme>                      Output coloring style [possible values: Auto, Solarized]
        --default-scheme <default-scheme>    The default scheme to use if not specified in the URL

ARGS:
    <[METHOD] URL>       The request URL, preceded by an optional HTTP method
    <REQUEST_ITEM>...    Optional key-value pairs to be included in the request
```

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
