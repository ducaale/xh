# xh
[![Version info](https://img.shields.io/crates/v/xh.svg)](https://crates.io/crates/xh)
[![Packaging status](https://repology.org/badge/tiny-repos/xh.svg)](https://repology.org/project/xh/versions)

`xh` is a friendly and fast tool for sending HTTP requests. It reimplements as much
as possible of [HTTPie's](https://httpie.io/) excellent design, with a focus
on improved performance.

[![asciicast](/assets/xh-demo.gif)](https://asciinema.org/a/475190)

## Installation

### via cURL (Linux & macOS)

```
curl -sfL https://raw.githubusercontent.com/ducaale/xh/master/install.sh | sh
```

### via Powershell (Windows)

```
iwr -useb https://raw.githubusercontent.com/ducaale/xh/master/install.ps1 | iex
```


### via a package manager

| OS                            | Method     | Command                                    |
|-------------------------------|------------|--------------------------------------------|
| Any                           | Cargo\*    | `cargo install xh --locked`                |
| Any                           | [Huber]    | `huber install xh`                         |
| Android ([Termux])            | pkg        | `pkg install xh`                           |
| Android ([Magisk]/[KernelSU]) | MMRL\*\*   | `mmrl install xhhttp`                      |
| Alpine Linux                  | apk\*\*\*  | `apk add xh`                               |
| Arch Linux                    | Pacman     | `pacman -S xh`                             |
| Debian & Ubuntu               | Apt\*\*\*\*| `sudo apt install xh`                      |
| FreeBSD                       | FreshPorts | `pkg install xh`                           |
| NetBSD                        | pkgsrc     | `pkgin install xh`                         |
| Linux & macOS                 | Nixpkgs    | `nix-env -iA nixpkgs.xh`                   |
| Linux & macOS                 | Homebrew   | `brew install xh`                          |
| macOS                         | MacPorts   | `sudo port install xh`                     |
| Windows                       | Scoop      | `scoop install xh`                         |
| Windows                       | Chocolatey | `choco install xh`                         |

\* Make sure that you have Rust 1.64 or later installed

\*\* You will need to install the [MMRL CLI](https://github.com/DerGoogler/MMRL-CLI/releases)

\*\*\* The xh package is available in Edge and will be in v3.17+. It is built with native-tls only.

\*\*\*\* You will need to add the apt repository from https://apt.cli.rs/

[Huber]: https://github.com/innobead/huber#installing-huber
[Magisk]: https://github.com/topjohnwu/Magisk
[KernelSU]: https://kernelsu.org
[Termux]: https://github.com/termux/termux-app

### via pre-built binaries
The [release page](https://github.com/ducaale/xh/releases) contains prebuilt binaries for Linux, macOS and Windows.

## Usage
```
Usage: xh [OPTIONS] <[METHOD] URL> [REQUEST_ITEM]...

Arguments:
  <[METHOD] URL>     The request URL, preceded by an optional HTTP method
  [REQUEST_ITEM]...  Optional key-value pairs to be included in the request.

Options:
  -j, --json                             (default) Serialize data items from the command line as a JSON object
  -f, --form                             Serialize data items from the command line as form fields
      --multipart                        Like --form, but force a multipart/form-data request even without files
      --raw <RAW>                        Pass raw request data without extra processing
      --pretty <STYLE>                   Controls output processing [possible values: all, colors, format, none]
      --format-options <FORMAT_OPTIONS>  Set output formatting options
  -s, --style <THEME>                    Output coloring style [possible values: auto, solarized, monokai, fruity]
      --response-charset <ENCODING>      Override the response encoding for terminal display purposes
      --response-mime <MIME_TYPE>        Override the response mime type for coloring and formatting for the terminal
  -p, --print <FORMAT>                   String specifying what the output should contain
  -h, --headers                          Print only the response headers. Shortcut for --print=h
  -b, --body                             Print only the response body. Shortcut for --print=b
  -m, --meta                             Print only the response metadata. Shortcut for --print=m
  -v, --verbose...                       Print the whole request as well as the response
      --all                              Show any intermediary requests/responses while following redirects with --follow
  -P, --history-print <FORMAT>           The same as --print but applies only to intermediary requests/responses
  -q, --quiet                            Do not print to stdout or stderr
  -S, --stream                           Always stream the response body
  -o, --output <FILE>                    Save output to FILE instead of stdout
  -d, --download                         Download the body to a file instead of printing it
  -c, --continue                         Resume an interrupted download. Requires --download and --output
      --session <FILE>                   Create, or reuse and update a session
      --session-read-only <FILE>         Create or read a session without updating it form the request/response exchange
  -A, --auth-type <AUTH_TYPE>            Specify the auth mechanism [possible values: basic, bearer, digest]
  -a, --auth <USER[:PASS] | TOKEN>       Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer)
      --ignore-netrc                     Do not use credentials from .netrc
      --offline                          Construct HTTP requests without sending them anywhere
      --check-status                     (default) Exit with an error status code if the server replies with an error
  -F, --follow                           Do follow redirects
      --max-redirects <NUM>              Number of redirects to follow. Only respected if --follow is used
      --timeout <SEC>                    Connection timeout of the request
      --proxy <PROTOCOL:URL>             Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080
      --verify <VERIFY>                  If "no", skip SSL verification. If a file path, use it as a CA bundle
      --cert <FILE>                      Use a client side certificate for SSL
      --cert-key <FILE>                  A private key file to use with --cert
      --ssl <VERSION>                    Force a particular TLS version [possible values: auto, tls1, tls1.1, tls1.2, tls1.3]
      --https                            Make HTTPS requests if not specified in the URL
      --http-version <VERSION>           HTTP version to use [possible values: 1.0, 1.1, 2]
      --interface <NAME>                 Bind to a network interface or local IP address
  -4, --ipv4                             Resolve hostname to ipv4 addresses only
  -6, --ipv6                             Resolve hostname to ipv6 addresses only
  -I, --ignore-stdin                     Do not attempt to read stdin
      --curl                             Print a translation to a curl command
      --curl-long                        Use the long versions of curl's flags
      --help                             Print help
  -V, --version                          Print version

Each option can be reset with a --no-OPTION argument.
```

Run `xh help` for more detailed information.

### Request Items

`xh` uses [HTTPie's request-item syntax](https://httpie.io/docs#request-items) to set headers, request body, query string, etc.

- `=`/`:=` for setting the request body's JSON or form fields (`=` for strings and `:=` for other JSON types).
- `==` for adding query strings.
- `@` for including files in multipart requests e.g `picture@hello.jpg` or `picture@hello.jpg;type=image/jpeg;filename=goodbye.jpg`.
- `:` for adding or removing headers e.g `connection:keep-alive` or `connection:`.
- `;` for including headers with empty values e.g `header-without-value;`.

An `@` prefix can be used to read a value from a file. For example: `x-api-key:@api-key.txt`.

The request body can also be read from standard input, or from a file using `@filename`.

To construct a complex JSON object, a JSON path can be used as a key e.g `app[container][0][id]=090-5`.
For more information on this syntax, refer to https://httpie.io/docs/cli/nested-json.

### Shorthand form for URLs

Similar to HTTPie, specifying the scheme portion of the request URL is optional, and a leading colon works as shorthand
for localhost. `:8000` is equivalent to `localhost:8000`, and `:/path` is equivalent to `localhost/path`.

URLs can have a leading `://` which allows quickly converting a URL into a valid xh or HTTPie command. For example
`http://httpbin.org/json` becomes `http ://httpbin.org/json`.


```sh
xh http://localhost:3000/users # resolves to http://localhost:3000/users
xh localhost:3000/users        # resolves to http://localhost:3000/users
xh :3000/users                 # resolves to http://localhost:3000/users
xh :/users                     # resolves to http://localhost:80/users
xh example.com                 # resolves to http://example.com
xh ://example.com              # resolves to http://example.com
```

### Making HTTPS requests by default

`xh` will default to HTTPS scheme if the binary name is one of `xhs`, `https`, or `xhttps`. If you have installed `xh`
via a package manager, both `xh` and `xhs` should be available by default. Otherwise, you need to create one like this:

```sh
cd /path/to/xh && ln -s ./xh ./xhs
xh httpbin.org/get  # resolves to http://httpbin.org/get
xhs httpbin.org/get # resolves to https://httpbin.org/get
```

### Strict compatibility mode

If `xh` is invoked as `http` or `https` (by renaming the binary), or if the `XH_HTTPIE_COMPAT_MODE` environment variable is set,
it will run in HTTPie compatibility mode. The only current difference is that `--check-status` is not enabled by default.

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

# Send a POST request with body read from stdin.
echo "[1, 2, 3]" | xh post httpbin.org/post

# Send a PUT request and pipe the result to less
xh put httpbin.org/put id:=49 age:=25 | less

# Download and save to res.json
xh -d httpbin.org/json -o res.json

# Make a request with a custom user agent
xh httpbin.org/get user-agent:foobar
```

## How xh compares to HTTPie

### Advantages

- Improved startup speed.
- Available as a single statically linked binary that's easy to install and carry around.
- HTTP/2 support.
- Builtin translation to curl commands with the `--curl` flag.
- Short, cheatsheet-style output from `--help`. (For longer output, pass `help`.)

### Disadvantages

- Not all of HTTPie's features are implemented. ([#4](https://github.com/ducaale/xh/issues/4))
- No plugin system.
- General immaturity. HTTPie is old and well-tested.
- Worse documentation.

## Similar or related Projects

- [curlie](https://github.com/rs/curlie) - frontend to cURL that adds the ease of use of httpie
- [httpie-go](https://github.com/nojima/httpie-go) - httpie-like HTTP client written in Go
- [curl2httpie](https://github.com/dcb9/curl2httpie) - convert command arguments between cURL and HTTPie
