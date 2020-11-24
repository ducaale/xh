# Yahc
Yet another [HTTPie](https://httpie.io/) clone.

[![asciicast](https://asciinema.org/a/375052.svg)](https://asciinema.org/a/375052)

## Building from source
You will need rust 1.46 or later. To compile run cargo build --release.

## Usage
```
yahc.exe [FLAGS] [OPTIONS] <METHOD> <URL> [REQUEST_ITEM]...
```

## Syntaxes and themes used
- [Sublime-HTTP](https://github.com/samsalisbury/Sublime-HTTP)
- [json-kv](https://github.com/aurule/json-kv)
- [Sublime Packages](https://github.com/sublimehq/Packages/tree/fa6b8629c95041bf262d4c1dab95c456a0530122)
- [ansi-dark theme](https://github.com/sharkdp/bat/blob/master/assets/themes/ansi-dark.tmTheme)

## TODO
- [ ] Decode responses compressed in deflate format
- [ ] Support streaming requests and responses
- [ ] Add Monokai theme
- [ ] Port remaining flags from HTTPie
- [ ] Come up with a better name than Yahc
