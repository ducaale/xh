## [Unreleased]
### Features
- Support setting the content-type for files in multipart requests e.g
  `ht httpbin.org/post --multipart pic@cat.png;type=image/png`
- Add `--follow` and `--max-redirects` for configuring redirect behaviour, see #19 (@Till--H)

### Bug fixes
- Render white text as the default foreground color, see #21 (@blyxxyz)
- Don't insert lines when streaming json.
- Do not explicitly add `Host` header, see #26 (@blyxxyz)

### Other
- Init parsing regex for RequestItem once, see #22 (@jRimbault)
- AUR package for Arch linux, see #15 (@nitsky)

## [0.4.0] - 2021-02-06
### Features
- Support streaming responses. This on by default for unformatted responses and can also
  be enabled via the `--stream` flag

## [0.3.5] - 2021-02-06
### Features
- Support output redirection for downloads e.g `ht -d httpbin.org/json > temp.json`

### Other
- Upgrade to Tokio 1.x.