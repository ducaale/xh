module completions {

  def "nu-complete xh pretty" [] {
    [ "all" "colors" "format" "none" ]
  }

  def "nu-complete xh style" [] {
    [ "auto" "solarized" "monokai" "fruity" ]
  }

  def "nu-complete xh auth_type" [] {
    [ "basic" "bearer" "digest" ]
  }

  def "nu-complete xh ssl" [] {
    [ "auto" "tls1" "tls1.1" "tls1.2" "tls1.3" ]
  }

  def "nu-complete xh http_version" [] {
    [ "1.0" "1.1" "2" "2-prior-knowledge" ]
  }

  def "nu-complete xh generate" [] {
    [ "complete-bash" "complete-elvish" "complete-fish" "complete-nushell" "complete-powershell" "complete-zsh" "man" ]
  }

  # xh is a friendly and fast tool for sending HTTP requests
  export extern xh [
    --json(-j)                # (default) Serialize data items from the command line as a JSON object
    --form(-f)                # Serialize data items from the command line as form fields
    --multipart               # Like --form, but force a multipart/form-data request even without files
    --raw: string             # Pass raw request data without extra processing
    --pretty: string@"nu-complete xh pretty" # Controls output processing
    --format-options: string  # Set output formatting options
    --style(-s): string@"nu-complete xh style" # Output coloring style
    --response-charset: string # Override the response encoding for terminal display purposes
    --response-mime: string   # Override the response mime type for coloring and formatting for the terminal
    --print(-p): string       # String specifying what the output should contain
    --headers(-h)             # Print only the response headers. Shortcut for --print=h
    --body(-b)                # Print only the response body. Shortcut for --print=b
    --meta(-m)                # Print only the response metadata. Shortcut for --print=m
    --verbose(-v)             # Print the whole request as well as the response
    --debug                   # Print full error stack traces and debug log messages
    --all                     # Show any intermediary requests/responses while following redirects with --follow
    --history-print(-P): string # The same as --print but applies only to intermediary requests/responses
    --quiet(-q)               # Do not print to stdout or stderr
    --stream(-S)              # Always stream the response body
    --compress(-x)            # Content compressed (encoded) with Deflate algorithm. The Content-Encoding header is set to deflate
    --output(-o): string      # Save output to FILE instead of stdout
    --download(-d)            # Download the body to a file instead of printing it
    --continue(-c)            # Resume an interrupted download. Requires --download and --output
    --session: string         # Create, or reuse and update a session
    --session-read-only: string # Create or read a session without updating it form the request/response exchange
    --auth-type(-A): string@"nu-complete xh auth_type" # Specify the auth mechanism
    --auth(-a): string        # Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer)
    --bearer: string          # Authenticate with a bearer token
    --ignore-netrc            # Do not use credentials from .netrc
    --offline                 # Construct HTTP requests without sending them anywhere
    --check-status            # (default) Exit with an error status code if the server replies with an error
    --follow(-F)              # Do follow redirects
    --max-redirects: string   # Number of redirects to follow. Only respected if --follow is used
    --timeout: string         # Connection timeout of the request
    --proxy: string           # Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080
    --verify: string          # If "no", skip SSL verification. If a file path, use it as a CA bundle
    --cert: string            # Use a client side certificate for SSL
    --cert-key: string        # A private key file to use with --cert
    --ssl: string@"nu-complete xh ssl" # Force a particular TLS version
    --native-tls              # Use the system TLS library instead of rustls (if enabled at compile time)
    --default-scheme: string  # The default scheme to use if not specified in the URL
    --https                   # Make HTTPS requests if not specified in the URL
    --http-version: string@"nu-complete xh http_version" # HTTP version to use
    --resolve: string         # Override DNS resolution for specific domain to a custom IP
    --interface: string       # Bind to a network interface or local IP address
    --ipv4(-4)                # Resolve hostname to ipv4 addresses only
    --ipv6(-6)                # Resolve hostname to ipv6 addresses only
    --ignore-stdin(-I)        # Do not attempt to read stdin
    --curl                    # Print a translation to a curl command
    --curl-long               # Use the long versions of curl's flags
    --generate: string@"nu-complete xh generate" # Generate shell completions or man pages
    --help                    # Print help
    raw_method_or_url: string # The request URL, preceded by an optional HTTP method
    ...raw_rest_args: string  # Optional key-value pairs to be included in the request.
    --no-json
    --no-form
    --no-multipart
    --no-raw
    --no-pretty
    --no-format-options
    --no-style
    --no-response-charset
    --no-response-mime
    --no-print
    --no-headers
    --no-body
    --no-meta
    --no-verbose
    --no-debug
    --no-all
    --no-history-print
    --no-quiet
    --no-stream
    --no-compress
    --no-output
    --no-download
    --no-continue
    --no-session
    --no-session-read-only
    --no-auth-type
    --no-auth
    --no-bearer
    --no-ignore-netrc
    --no-offline
    --no-check-status
    --no-follow
    --no-max-redirects
    --no-timeout
    --no-proxy
    --no-verify
    --no-cert
    --no-cert-key
    --no-ssl
    --no-native-tls
    --no-default-scheme
    --no-https
    --no-http-version
    --no-resolve
    --no-interface
    --no-ipv4
    --no-ipv6
    --no-ignore-stdin
    --no-curl
    --no-curl-long
    --no-generate
    --no-help
    --version(-V)             # Print version
  ]

}

export use completions *
