# Yahc
Yet another [HTTPie](https://httpie.io/) clone.

[![asciicast](https://asciinema.org/a/377579.svg)](https://asciinema.org/a/377579)

## Building from source
You will need rust 1.46 or later. To compile run `cargo build --release`.

## Usage
```
yahc 0.1.0
Yet another HTTPie clone

USAGE:
    yahc.exe [FLAGS] [OPTIONS] <METHOD> <URL> [REQUEST_ITEM]...

FLAGS:
    -f, --form            Data items from the command line are serialized as form fields
    -h, --help            Prints help information
    -i, --ignore-stdin    Do not attempt to read stdin
    -j, --json            (default) Data items from the command line are serialized as a JSON object
    -m, --multipart       Similar to --form, but always sends a multipart/form-data request (i.e., even without files)
        --offline         Construct HTTP requests without sending them anywhere
    -V, --version         Prints version information
    -v, --verbose         Print the whole request as well as the response

OPTIONS:
    -a, --auth <auth>
    -A, --auth-type <auth-type>              Specify the auth mechanism [possible values: Basic, Bearer]
        --default-scheme <default-scheme>    The default scheme to use if not specified in the URL
        --pretty <pretty>                    Controls output processing [possible values: All, Colors, Format, None]
    -p, --print <print>                      String specifying what the output should contain
    -s, --style <theme>                      Output coloring style [possible values: Auto, Solarized]

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
