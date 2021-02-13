## [0.7.0] - 2021-02-12
### Features
- Follow redirects if downloading a file, see #51 (@blyxxyz)
- Allow form value regex to match newlines, see #46 (@blyxxyz)
- Adds --headers option, see #42 (@sanpii)

### Other
- Rename ht binary to xh

## [0.6.0] - 2021-02-08
### Features
- Add support for OPTIONS HTTP method, see #17 (@plombard)
- Add _--body_ flag for printing only response body, see #38 (@idanski)
- Add content length to file upload stream, see #32 (@blyxxyz)
- Include User-Agent header in outgoing requests, see #33 (@blyxxyz)

### Other
- Ensure filename from `content-disposition` doesn't overwrite existing files,
  isn't a hidden file, or doesn't end up outside the current directory,
  see #37 (@blyxxyz)
- Bubble errors up to main() instead of panicking see #37 (@blyxxyz)

## [0.5.0] - 2021-02-07
### Features
- Add support for HEAD requests, see #16 (@Till--H)
- Support setting the content-type for files in multipart requests e.g
  `ht httpbin.org/post --multipart pic@cat.png;type=image/png`
- Add `--follow` and `--max-redirects` for configuring redirect behaviour, see #19 (@Till--H)

### Bug fixes
- Render white text as the default foreground color, see #21 (@blyxxyz)
- Don't insert lines when streaming json.
- Do not explicitly add `Host` header, see #26 (@blyxxyz)

### Other
- Init parsing regex for RequestItem once, see #22 (@jRimbault)

## [0.4.0] - 2021-02-06
### Features
- Support streaming responses. This on by default for unformatted responses and can also
  be enabled via the `--stream` flag

## [0.3.5] - 2021-01-31
### Features
- Support output redirection for downloads e.g `ht -d httpbin.org/json > temp.json`

### Other
- Upgrade to Tokio 1.x.