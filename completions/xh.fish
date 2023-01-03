complete -c xh -l raw -d 'Pass raw request data without extra processing' -r
complete -c xh -l pretty -d 'Controls output processing' -r -f -a "{all	(default) Enable both coloring and formatting,colors	Apply syntax highlighting to output,format	Pretty-print json and sort headers,none	Disable both coloring and formatting}"
complete -c xh -s s -l style -d 'Output coloring style' -r -f -a "{auto	,solarized	,monokai	,fruity	}"
complete -c xh -l response-charset -d 'Override the response encoding for terminal display purposes' -r
complete -c xh -l response-mime -d 'Override the response mime type for coloring and formatting for the terminal' -r
complete -c xh -s p -l print -d 'String specifying what the output should contain' -r
complete -c xh -s P -l history-print -d 'The same as --print but applies only to intermediary requests/responses' -r
complete -c xh -s o -l output -d 'Save output to FILE instead of stdout' -r
complete -c xh -l session -d 'Create, or reuse and update a session' -r
complete -c xh -l session-read-only -d 'Create or read a session without updating it form the request/response exchange' -r
complete -c xh -s A -l auth-type -d 'Specify the auth mechanism' -r -f -a "{basic	,bearer	,digest	}"
complete -c xh -s a -l auth -d 'Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer)' -r
complete -c xh -l bearer -d 'Authenticate with a bearer token' -r
complete -c xh -l max-redirects -d 'Number of redirects to follow. Only respected if --follow is used' -r
complete -c xh -l timeout -d 'Connection timeout of the request' -r
complete -c xh -l proxy -d 'Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080' -r
complete -c xh -l verify -d 'If "no", skip SSL verification. If a file path, use it as a CA bundle' -r
complete -c xh -l cert -d 'Use a client side certificate for SSL' -r
complete -c xh -l cert-key -d 'A private key file to use with --cert' -r
complete -c xh -l ssl -d 'Force a particular TLS version' -r -f -a "{auto	,tls1	,tls1.1	,tls1.2	,tls1.3	}"
complete -c xh -l default-scheme -d 'The default scheme to use if not specified in the URL' -r
complete -c xh -l http-version -d 'HTTP version to use' -r -f -a "{1.0	,1.1	,2	}"
complete -c xh -l help -d 'Print help information'
complete -c xh -s V -l version -d 'Print version information'
complete -c xh -s j -l json -d '(default) Serialize data items from the command line as a JSON object'
complete -c xh -s f -l form -d 'Serialize data items from the command line as form fields'
complete -c xh -l multipart -d 'Like --form, but force a multipart/form-data request even without files'
complete -c xh -s h -l headers -d 'Print only the response headers. Shortcut for --print=h'
complete -c xh -s b -l body -d 'Print only the response body. Shortcut for --print=b'
complete -c xh -s v -l verbose -d 'Print the whole request as well as the response'
complete -c xh -l all -d 'Show any intermediary requests/responses while following redirects with --follow'
complete -c xh -s 4 -l ipv4 -d 'Resolve hostname to ipv4 addresses only'
complete -c xh -s 6 -l ipv6 -d 'Resolve hostname to ipv6 addresses only'
complete -c xh -s q -l quiet -d 'Do not print to stdout or stderr'
complete -c xh -s S -l stream -d 'Always stream the response body'
complete -c xh -s d -l download -d 'Download the body to a file instead of printing it'
complete -c xh -s c -l continue -d 'Resume an interrupted download. Requires --download and --output'
complete -c xh -l ignore-netrc -d 'Do not use credentials from .netrc'
complete -c xh -l offline -d 'Construct HTTP requests without sending them anywhere'
complete -c xh -l check-status -d '(default) Exit with an error status code if the server replies with an error'
complete -c xh -s F -l follow -d 'Do follow redirects'
complete -c xh -l native-tls -d 'Use the system TLS library instead of rustls (if enabled at compile time)'
complete -c xh -l https -d 'Make HTTPS requests if not specified in the URL'
complete -c xh -s I -l ignore-stdin -d 'Do not attempt to read stdin'
complete -c xh -l curl -d 'Print a translation to a curl command'
complete -c xh -l curl-long -d 'Use the long versions of curl\'s flags'
complete -c xh -l no-help
complete -c xh -l no-version
complete -c xh -l no-json
complete -c xh -l no-form
complete -c xh -l no-multipart
complete -c xh -l no-raw
complete -c xh -l no-pretty
complete -c xh -l no-style
complete -c xh -l no-response-charset
complete -c xh -l no-response-mime
complete -c xh -l no-print
complete -c xh -l no-headers
complete -c xh -l no-body
complete -c xh -l no-verbose
complete -c xh -l no-all
complete -c xh -l no-history-print
complete -c xh -l no-ipv4
complete -c xh -l no-ipv6
complete -c xh -l no-quiet
complete -c xh -l no-stream
complete -c xh -l no-output
complete -c xh -l no-download
complete -c xh -l no-continue
complete -c xh -l no-session
complete -c xh -l no-session-read-only
complete -c xh -l no-auth-type
complete -c xh -l no-auth
complete -c xh -l no-bearer
complete -c xh -l no-ignore-netrc
complete -c xh -l no-offline
complete -c xh -l no-check-status
complete -c xh -l no-follow
complete -c xh -l no-max-redirects
complete -c xh -l no-timeout
complete -c xh -l no-proxy
complete -c xh -l no-verify
complete -c xh -l no-cert
complete -c xh -l no-cert-key
complete -c xh -l no-ssl
complete -c xh -l no-native-tls
complete -c xh -l no-default-scheme
complete -c xh -l no-https
complete -c xh -l no-http-version
complete -c xh -l no-ignore-stdin
complete -c xh -l no-curl
complete -c xh -l no-curl-long
