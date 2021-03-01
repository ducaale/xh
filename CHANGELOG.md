## [0.8.1] - 2021-03-01
### Features
- Highlight Javascript and CSS, see #82 (@blyxxyz)
- Check if text is actually JSON before formatting it, see #82 (@blyxxyz)
- Default to a content-type of application/json when reading a body from stdin, see #82 (@blyxxyz)

## [0.8.0] - 2021-02-28
### Features
- More robust detection of the method and URL arguments, see #55 (@blyxxyz)
- Improvements to the generation downloaded filenames, see #56 (@blyxxyz)
- _--continue_ now works for resuming downloads. It was incomplete before, see #59 (@blyxxyz)
- _--check-status_ is supported, and is automatically active for downloads
  (so you don't download error pages), see #59 (@blyxxyz)
- Add the _--proxy_ option, see #62 (@otaconix)
- Add _--bearer_ flag for Bearer Authentication and remove _--auth-type_, see #64 (@blyxxyz)
- Add support for manpages, see #64 (@blyxxyz)
- Add _help_ subcommand for printing long help and update _--help_ to print short help, see #64 (@blyxxyz)
- Support escaping characters in request items with backslash, see #66 (@blyxxyz)
- Add support for _--verify_ to skip the host’s SSL certificate verification, see #44 (@jihchi, @otaconix)
- Add support for _--cert/cert-key_ for using client side certificate for the SSL communication, see #44 (@jihchi, @otaconix)
- Add _--curl_ flag to print equivalent curl command, see #69 (@blyxxyz)
- Replace _--default-scheme_ by _--https_. _--default-scheme_ is still kept as an undocumented flag, see #73 (@blyxxyz)
- If `xh` is invoked as `xhs`, `https`, or `xhttps`, run as if _--https_ was used, see #73 (@blyxxyz)
- Support `NO_COLOR` environment variable to turn colors off by default, see #73 (@blyxxyz)
- Make _--json_/_--form_/_--multipart_ override each other and force content-type. If you use multiple of those flags,
  all but the last will be ignored. And if you use them without request items the appropriate headers will still be set,
  see #73 (@blyxxyz)
- Try to detect undeclared JSON response bodies: If the response is javascript or plain text,
  check if it's JSON, see #73 (@blyxxyz)
- Add shell autocompletion generation, see #76 (@blyxxyz)

### Other
- Make structopt usage more consistent, see #67 (@blyxxyz)
- Remove use of async, make --stream work consistently, see #41 (@blyxxyz)
- Introduce clippy and fmt in CI, see #75 (@ducaale)

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