# ht
Yet another [HTTPie](https://httpie.io/) clone in Rust.

[![asciicast](/assets/ht-demo.gif)](https://asciinema.org/a/382056)

## Building from source
You will need rust 1.46 or later. To compile run `cargo build --release`.

## Usage
```
ht 0.2.0

USAGE:
    ht [FLAGS] [OPTIONS] <METHOD> <URL> [REQUEST_ITEM]...

FLAGS:
        --offline         Construct HTTP requests without sending them anywhere
    -j, --json            (default) Data items from the command line are serialized as a JSON object
    -f, --form            Data items from the command line are serialized as form fields
    -m, --multipart       Similar to --form, but always sends a multipart/form-data request (i.e., even without files)
    -I, --ignore-stdin    Do not attempt to read stdin
    -d, --download
    -v, --verbose         Print the whole request as well as the response
    -q, --quiet           Do not print to stdout or stderr
    -h, --help            Prints help information
    -V, --version         Prints version information

OPTIONS:
    -A, --auth-type <auth-type>              Specify the auth mechanism [possible values: Basic, Bearer]
    -a, --auth <auth>
    -o, --output <output>                    Save output to FILE instead of stdout
    -p, --print <print>                      String specifying what the output should contain
        --pretty <pretty>                    Controls output processing [possible values: All, Colors, Format, None]
    -s, --style <theme>                      Output coloring style [possible values: Auto, Solarized]
        --default-scheme <default-scheme>    The default scheme to use if not specified in the URL

ARGS:
    <METHOD>             The HTTP method to be used for the request [possible values: GET, POST, PUT, PATCH, DELETE]
    <URL>
    <REQUEST_ITEM>...    Optional key-value pairs to be included in the request
```

## Syntaxes and themes used
- [Sublime-HTTP](https://github.com/samsalisbury/Sublime-HTTP)
- [json-kv](https://github.com/aurule/json-kv)
- [Sublime Packages](https://github.com/sublimehq/Packages/tree/fa6b8629c95041bf262d4c1dab95c456a0530122)
- [ansi-dark theme](https://github.com/sharkdp/bat/blob/master/assets/themes/ansi-dark.tmTheme)
