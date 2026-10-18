> **语言**：本文档为简体中文翻译，[点此查看英文原版 README.md](README.md)。

# xh
[![Version info](https://img.shields.io/crates/v/xh.svg)](https://crates.io/crates/xh)
[![Packaging status](https://repology.org/badge/tiny-repos/xh.svg)](https://repology.org/project/xh/versions)

`xh` 是一款友好且快速的 HTTP 请求发送工具。它尽可能重新实现了
[HTTPie](https://httpie.io/) 出色的设计，并专注于性能的提升。

[![asciicast](/assets/xh-demo.gif)](https://asciinema.org/a/475190)

## 安装

### 通过 cURL（Linux 与 macOS）

```
curl -sfL https://raw.githubusercontent.com/ducaale/xh/master/install.sh | sh
```

### 通过 Powershell（Windows）

```
iwr -useb https://raw.githubusercontent.com/ducaale/xh/master/install.ps1 | iex
```


### 通过包管理器

| 操作系统                          | 方式           | 命令                                       |
|-------------------------------|----------------|--------------------------------------------|
| Any                           | Cargo\*        | `cargo install xh --locked`                |
| Any                           | [Huber]        | `huber install xh`                         |
| Any                           | [Mise]         | `mise use xh --global`                     |
| Android ([Termux])            | pkg            | `pkg install xh`                           |
| Android ([Magisk]/[KernelSU]) | MMRL\*\*       | `mmrl install xhhttp`                      |
| Alpine Linux                  | apk\*\*\*      | `apk add xh`                               |
| Arch Linux                    | Pacman         | `pacman -S xh`                             |
| Debian & Ubuntu               | Apt\*\*\*\*    | `sudo apt install xh`                      |
| Fedora                        | Copr\*\*\*\*\* | `sudo dnf install xh`                      |
| FreeBSD                       | FreshPorts     | `pkg install xh`                           |
| NetBSD                        | pkgsrc         | `pkgin install xh`                         |
| Linux & macOS                 | Nixpkgs        | `nix-env -iA nixpkgs.xh`                   |
| Linux & macOS                 | [Flox]         | `flox install xh`                          |
| Linux & macOS                 | Homebrew       | `brew install xh`                          |
| Linux & macOS                 | [Hermit]       | `hermit install xh`                        |
| macOS                         | MacPorts       | `sudo port install xh`                     |
| Windows                       | Scoop          | `scoop install xh`                         |
| Windows                       | Chocolatey     | `choco install xh`                         |
| Windows                       | Winget         | `winget add ducaale.xh`                    |

\* 请确保已安装 Rust 1.85 或更高版本

\*\* 你需要先安装 [MMRL CLI](https://github.com/DerGoogler/MMRL-CLI/releases)

\*\*\* 仅使用 native-tls 构建。

\*\*\*\* 自 Debian 13 与 Ubuntu 25.04 起提供。仅使用 native-tls 构建。

\*\*\*\*\* 通过 `sudo dnf copr enable ducaale/xh` 启用 COPR 仓库

[Huber]: https://github.com/innobead/huber#installing-huber
[Mise]: https://mise.jdx.dev
[Magisk]: https://github.com/topjohnwu/Magisk
[KernelSU]: https://kernelsu.org
[Termux]: https://github.com/termux/termux-app
[Flox]: https://flox.dev/docs/
[Hermit]: https://cashapp.github.io/hermit/

### 通过预构建二进制文件

[release 页面](https://github.com/ducaale/xh/releases)提供了适用于 Linux、macOS 和 Windows 的预构建二进制文件。

## 用法
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
      --debug                            Print full error stack traces and debug log messages
      --all                              Show any intermediary requests/responses while following redirects with --follow
  -P, --history-print <FORMAT>           The same as --print but applies only to intermediary requests/responses
  -q, --quiet...                         Do not print to stdout or stderr
  -S, --stream                           Always stream the response body
  -x, --compress...                      Content compressed (encoded) with Deflate algorithm
  -o, --output <FILE>                    Save output to FILE instead of stdout
  -d, --download                         Download the body to a file instead of printing it
  -c, --continue                         Resume an interrupted download. Requires --download and --output
      --session <FILE>                   Create, or reuse and update a session
      --session-read-only <FILE>         Create or read a session without updating it from the request/response exchange
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
      --http-version <VERSION>           HTTP version to use [possible values: 1.0, 1.1, 2, 2-prior-knowledge, 3-prior-knowledge]
      --resolve <HOST:ADDRESS>           Override DNS resolution for specific domain to a custom IP
      --interface <NAME>                 Bind to a network interface or local IP address
  -4, --ipv4                             Resolve hostname to ipv4 addresses only
  -6, --ipv6                             Resolve hostname to ipv6 addresses only
      --unix-socket <FILE>               Connect using a Unix domain socket
  -I, --ignore-stdin                     Do not attempt to read stdin
      --curl                             Print a translation to a curl command
      --curl-long                        Use the long versions of curl's flags
      --generate <KIND>                  Generate shell completions or man pages
      --help                             Print help
  -V, --version                          Print version

Each option can be reset with a --no-OPTION argument.
```

运行 `xh help` 或参阅 [`man xh`](/doc/xh.1.md) 获取更详细的信息。

### 请求项（Request Items）

`xh` 使用 [HTTPie 的请求项语法](https://httpie.io/docs/cli/request-items)来设置请求头、请求体、查询字符串等。

- `=`/`:=` 用于设置请求体的 JSON 或表单字段（`=` 用于字符串，`:=` 用于其他 JSON 类型）。
- `==` 用于添加查询字符串。
- `@` 用于在 multipart 请求中包含文件，例如 `picture@hello.jpg` 或 `picture@hello.jpg;type=image/jpeg;filename=goodbye.jpg`。
- `:` 用于添加或移除请求头，例如 `connection:keep-alive` 或 `connection:`。
- `;` 用于包含值为空的请求头，例如 `header-without-value;`。

`@` 前缀可用于从文件读取某个值。例如：`x-api-key:@api-key.txt`。

请求体也可以从标准输入读取，或使用 `@filename` 从文件读取。

要构造复杂的 JSON 对象，可以将 JSON 路径作为键使用，例如 `app[container][0][id]=090-5`。
有关此语法的更多信息，请参阅 https://httpie.io/docs/cli/nested-json。

### URL 的简写形式

与 HTTPie 类似，指定请求 URL 的协议部分是可选的，且以冒号开头是 localhost 的简写形式。
`:8000` 等价于 `localhost:8000`，`:/path` 等价于 `localhost/path`。

URL 可以带有 `://` 前缀，从而快速把一个 URL 转换成合法的 xh 或 HTTPie 命令。例如
`http://httpbin.org/json` 会变成 `http ://httpbin.org/json`。


```sh
xh http://localhost:3000/users # resolves to http://localhost:3000/users
xh localhost:3000/users        # resolves to http://localhost:3000/users
xh :3000/users                 # resolves to http://localhost:3000/users
xh :/users                     # resolves to http://localhost:80/users
xh example.com                 # resolves to http://example.com
xh ://example.com              # resolves to http://example.com
```

### 默认发起 HTTPS 请求

如果二进制文件名是 `xhs`、`https` 或 `xhttps` 之一，`xh` 会默认使用 HTTPS 协议。如果你是通过包管理器安装的 `xh`，
`xh` 和 `xhs` 通常都会默认可用。否则，你需要按如下方式自行创建一个：

```sh
cd /path/to/xh && ln -s ./xh ./xhs
xh httpbin.org/get  # resolves to http://httpbin.org/get
xhs httpbin.org/get # resolves to https://httpbin.org/get
```

### 严格兼容模式

如果 `xh` 以 `http` 或 `https` 之名被调用（通过重命名二进制文件），或者设置了 `XH_HTTPIE_COMPAT_MODE` 环境变量，
它就会运行在 HTTPie 兼容模式下。目前唯一的区别是 `--check-status` 不会默认启用。

## 示例

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

## xh 与 HTTPie 的对比

### 优势

- 启动速度更快。
- 以单个静态链接的二进制文件提供，便于安装和随身携带。
- 支持 HTTP/2。
- 内置通过 `--curl` 参数将命令翻译为 curl 命令的功能。
- `--help` 输出简洁、类似速查表。（若要更完整的输出，请传入 `help`。）

### 劣势

- 并未实现 HTTPie 的全部功能。（[#4](https://github.com/ducaale/xh/issues/4)）
- 没有插件系统。
- 整体尚不成熟。HTTPie 历史悠久且经过充分测试。
- 文档较差。

## 相似或相关的项目

- [curlie](https://github.com/rs/curlie) - cURL 的前端封装，增加了 httpie 的易用性
- [httpie-go](https://github.com/nojima/httpie-go) - 用 Go 编写的类 httpie HTTP 客户端
- [curl2httpie](https://github.com/dcb9/curl2httpie) - 在 cURL 与 HTTPie 之间转换命令参数
