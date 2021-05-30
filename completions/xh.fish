complete -c xh -l pretty -d 'Controls output processing' -r -f -a "all colors format none"
complete -c xh -s s -l style -d 'Output coloring style' -r -f -a "auto solarized"
complete -c xh -s p -l print -d 'String specifying what the output should contain'
complete -c xh -s o -l output -d 'Save output to FILE instead of stdout'
complete -c xh -l session -d 'Create, or reuse and update a session'
complete -c xh -l session-read-only -d 'Create or read a session without updating it form the request/response exchange'
complete -c xh -s A -l auth-type -d 'Specify the auth mechanism' -r -f -a "basic bearer"
complete -c xh -s a -l auth -d 'Authenticate as USER with PASS. PASS will be prompted if missing'
complete -c xh -l bearer -d 'Authenticate with a bearer token'
complete -c xh -l max-redirects -d 'Number of redirects to follow, only respected if `follow` is set'
complete -c xh -l timeout -d 'Connection timeout of the request'
complete -c xh -l proxy -d 'Use a proxy for a protocol. For example: `--proxy https:http://proxy.host:8080`'
complete -c xh -l verify -d 'If "no", skip SSL verification. If a file path, use it as a CA bundle'
complete -c xh -l cert -d 'Use a client side certificate for SSL'
complete -c xh -l cert-key -d 'A private key file to use with --cert'
complete -c xh -l default-scheme -d 'The default scheme to use if not specified in the URL'
complete -c xh -s j -l json -d '(default) Serialize data items from the command line as a JSON object'
complete -c xh -s f -l form -d 'Serialize data items from the command line as form fields'
complete -c xh -s m -l multipart -d 'Like --form, but force a multipart/form-data request even without files'
complete -c xh -s h -l headers -d 'Print only the response headers, shortcut for --print=h'
complete -c xh -s b -l body -d 'Print only the response body, Shortcut for --print=b'
complete -c xh -s v -l verbose -d 'Print the whole request as well as the response'
complete -c xh -s q -l quiet -d 'Do not print to stdout or stderr'
complete -c xh -s S -l stream -d 'Always stream the response body'
complete -c xh -s d -l download -d 'Download the body to a file instead of printing it'
complete -c xh -s c -l continue -d 'Resume an interrupted download. Requires --download and --output'
complete -c xh -l ignore-netrc -d 'Do not use credentials from .netrc'
complete -c xh -l offline -d 'Construct HTTP requests without sending them anywhere'
complete -c xh -l check-status -d 'Exit with an error status code if the server replies with an error'
complete -c xh -s F -l follow -d 'Do follow redirects'
complete -c xh -l https -d 'Make HTTPS requests if not specified in the URL'
complete -c xh -s I -l ignore-stdin -d 'Do not attempt to read stdin'
complete -c xh -l curl -d 'Print a translation to a `curl` command'
complete -c xh -l curl-long -d 'Use the long versions of curl\'s flags'
complete -c xh -l no-auth
complete -c xh -l no-auth-type
complete -c xh -l no-bearer
complete -c xh -l no-body
complete -c xh -l no-cert
complete -c xh -l no-cert-key
complete -c xh -l no-check-status
complete -c xh -l no-continue
complete -c xh -l no-curl
complete -c xh -l no-curl-long
complete -c xh -l no-default-scheme
complete -c xh -l no-download
complete -c xh -l no-follow
complete -c xh -l no-form
complete -c xh -l no-headers
complete -c xh -l no-https
complete -c xh -l no-ignore-netrc
complete -c xh -l no-ignore-stdin
complete -c xh -l no-json
complete -c xh -l no-max-redirects
complete -c xh -l no-multipart
complete -c xh -l no-offline
complete -c xh -l no-output
complete -c xh -l no-pretty
complete -c xh -l no-print
complete -c xh -l no-proxy
complete -c xh -l no-quiet
complete -c xh -l no-session
complete -c xh -l no-session-read-only
complete -c xh -l no-stream
complete -c xh -l no-style
complete -c xh -l no-timeout
complete -c xh -l no-verbose
complete -c xh -l no-verify
complete -c xh -l help -d 'Prints help information'
complete -c xh -s V -l version -d 'Prints version information'
