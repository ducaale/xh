
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'xh' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'xh'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'xh' {
            [CompletionResult]::new('--raw', 'raw', [CompletionResultType]::ParameterName, 'Pass raw request data without extra processing')
            [CompletionResult]::new('--pretty', 'pretty', [CompletionResultType]::ParameterName, 'Controls output processing')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('--style', 'style', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('--response-charset', 'response-charset', [CompletionResultType]::ParameterName, 'Override the response encoding for terminal display purposes')
            [CompletionResult]::new('--response-mime', 'response-mime', [CompletionResultType]::ParameterName, 'Override the response mime type for coloring and formatting for the terminal')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('--print', 'print', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('-P', 'P', [CompletionResultType]::ParameterName, 'The same as --print but applies only to intermediary requests/responses')
            [CompletionResult]::new('--history-print', 'history-print', [CompletionResultType]::ParameterName, 'The same as --print but applies only to intermediary requests/responses')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('--output', 'output', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('--session', 'session', [CompletionResultType]::ParameterName, 'Create, or reuse and update a session')
            [CompletionResult]::new('--session-read-only', 'session-read-only', [CompletionResultType]::ParameterName, 'Create or read a session without updating it form the request/response exchange')
            [CompletionResult]::new('-A', 'A', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('--auth-type', 'auth-type', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer)')
            [CompletionResult]::new('--auth', 'auth', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS (-A basic|digest) or with TOKEN (-A bearer)')
            [CompletionResult]::new('--bearer', 'bearer', [CompletionResultType]::ParameterName, 'Authenticate with a bearer token')
            [CompletionResult]::new('--max-redirects', 'max-redirects', [CompletionResultType]::ParameterName, 'Number of redirects to follow. Only respected if --follow is used')
            [CompletionResult]::new('--timeout', 'timeout', [CompletionResultType]::ParameterName, 'Connection timeout of the request')
            [CompletionResult]::new('--proxy', 'proxy', [CompletionResultType]::ParameterName, 'Use a proxy for a protocol. For example: --proxy https:http://proxy.host:8080')
            [CompletionResult]::new('--verify', 'verify', [CompletionResultType]::ParameterName, 'If "no", skip SSL verification. If a file path, use it as a CA bundle')
            [CompletionResult]::new('--cert', 'cert', [CompletionResultType]::ParameterName, 'Use a client side certificate for SSL')
            [CompletionResult]::new('--cert-key', 'cert-key', [CompletionResultType]::ParameterName, 'A private key file to use with --cert')
            [CompletionResult]::new('--ssl', 'ssl', [CompletionResultType]::ParameterName, 'Force a particular TLS version')
            [CompletionResult]::new('--default-scheme', 'default-scheme', [CompletionResultType]::ParameterName, 'The default scheme to use if not specified in the URL')
            [CompletionResult]::new('--http-version', 'http-version', [CompletionResultType]::ParameterName, 'HTTP version to use')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help information')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version information')
            [CompletionResult]::new('-j', 'j', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('--json', 'json', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('--form', 'form', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('--multipart', 'multipart', [CompletionResultType]::ParameterName, 'Like --form, but force a multipart/form-data request even without files')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print only the response headers. Shortcut for --print=h')
            [CompletionResult]::new('--headers', 'headers', [CompletionResultType]::ParameterName, 'Print only the response headers. Shortcut for --print=h')
            [CompletionResult]::new('-b', 'b', [CompletionResultType]::ParameterName, 'Print only the response body. Shortcut for --print=b')
            [CompletionResult]::new('--body', 'body', [CompletionResultType]::ParameterName, 'Print only the response body. Shortcut for --print=b')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
            [CompletionResult]::new('--all', 'all', [CompletionResultType]::ParameterName, 'Show any intermediary requests/responses while following redirects with --follow')
            [CompletionResult]::new('-4', '4', [CompletionResultType]::ParameterName, 'Resolve hostname to ipv4 addresses only')
            [CompletionResult]::new('--ipv4', 'ipv4', [CompletionResultType]::ParameterName, 'Resolve hostname to ipv4 addresses only')
            [CompletionResult]::new('-6', '6', [CompletionResultType]::ParameterName, 'Resolve hostname to ipv6 addresses only')
            [CompletionResult]::new('--ipv6', 'ipv6', [CompletionResultType]::ParameterName, 'Resolve hostname to ipv6 addresses only')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Do not print to stdout or stderr')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Do not print to stdout or stderr')
            [CompletionResult]::new('-S', 'S', [CompletionResultType]::ParameterName, 'Always stream the response body')
            [CompletionResult]::new('--stream', 'stream', [CompletionResultType]::ParameterName, 'Always stream the response body')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Download the body to a file instead of printing it')
            [CompletionResult]::new('--download', 'download', [CompletionResultType]::ParameterName, 'Download the body to a file instead of printing it')
            [CompletionResult]::new('-c', 'c', [CompletionResultType]::ParameterName, 'Resume an interrupted download. Requires --download and --output')
            [CompletionResult]::new('--continue', 'continue', [CompletionResultType]::ParameterName, 'Resume an interrupted download. Requires --download and --output')
            [CompletionResult]::new('--ignore-netrc', 'ignore-netrc', [CompletionResultType]::ParameterName, 'Do not use credentials from .netrc')
            [CompletionResult]::new('--offline', 'offline', [CompletionResultType]::ParameterName, 'Construct HTTP requests without sending them anywhere')
            [CompletionResult]::new('--check-status', 'check-status', [CompletionResultType]::ParameterName, '(default) Exit with an error status code if the server replies with an error')
            [CompletionResult]::new('-F', 'F', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('--follow', 'follow', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('--native-tls', 'native-tls', [CompletionResultType]::ParameterName, 'Use the system TLS library instead of rustls (if enabled at compile time)')
            [CompletionResult]::new('--https', 'https', [CompletionResultType]::ParameterName, 'Make HTTPS requests if not specified in the URL')
            [CompletionResult]::new('-I', 'I', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('--ignore-stdin', 'ignore-stdin', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('--curl', 'curl', [CompletionResultType]::ParameterName, 'Print a translation to a curl command')
            [CompletionResult]::new('--curl-long', 'curl-long', [CompletionResultType]::ParameterName, 'Use the long versions of curl''s flags')
            [CompletionResult]::new('--no-help', 'no-help', [CompletionResultType]::ParameterName, 'no-help')
            [CompletionResult]::new('--no-version', 'no-version', [CompletionResultType]::ParameterName, 'no-version')
            [CompletionResult]::new('--no-json', 'no-json', [CompletionResultType]::ParameterName, 'no-json')
            [CompletionResult]::new('--no-form', 'no-form', [CompletionResultType]::ParameterName, 'no-form')
            [CompletionResult]::new('--no-multipart', 'no-multipart', [CompletionResultType]::ParameterName, 'no-multipart')
            [CompletionResult]::new('--no-raw', 'no-raw', [CompletionResultType]::ParameterName, 'no-raw')
            [CompletionResult]::new('--no-pretty', 'no-pretty', [CompletionResultType]::ParameterName, 'no-pretty')
            [CompletionResult]::new('--no-style', 'no-style', [CompletionResultType]::ParameterName, 'no-style')
            [CompletionResult]::new('--no-response-charset', 'no-response-charset', [CompletionResultType]::ParameterName, 'no-response-charset')
            [CompletionResult]::new('--no-response-mime', 'no-response-mime', [CompletionResultType]::ParameterName, 'no-response-mime')
            [CompletionResult]::new('--no-print', 'no-print', [CompletionResultType]::ParameterName, 'no-print')
            [CompletionResult]::new('--no-headers', 'no-headers', [CompletionResultType]::ParameterName, 'no-headers')
            [CompletionResult]::new('--no-body', 'no-body', [CompletionResultType]::ParameterName, 'no-body')
            [CompletionResult]::new('--no-verbose', 'no-verbose', [CompletionResultType]::ParameterName, 'no-verbose')
            [CompletionResult]::new('--no-all', 'no-all', [CompletionResultType]::ParameterName, 'no-all')
            [CompletionResult]::new('--no-history-print', 'no-history-print', [CompletionResultType]::ParameterName, 'no-history-print')
            [CompletionResult]::new('--no-ipv4', 'no-ipv4', [CompletionResultType]::ParameterName, 'no-ipv4')
            [CompletionResult]::new('--no-ipv6', 'no-ipv6', [CompletionResultType]::ParameterName, 'no-ipv6')
            [CompletionResult]::new('--no-quiet', 'no-quiet', [CompletionResultType]::ParameterName, 'no-quiet')
            [CompletionResult]::new('--no-stream', 'no-stream', [CompletionResultType]::ParameterName, 'no-stream')
            [CompletionResult]::new('--no-output', 'no-output', [CompletionResultType]::ParameterName, 'no-output')
            [CompletionResult]::new('--no-download', 'no-download', [CompletionResultType]::ParameterName, 'no-download')
            [CompletionResult]::new('--no-continue', 'no-continue', [CompletionResultType]::ParameterName, 'no-continue')
            [CompletionResult]::new('--no-session', 'no-session', [CompletionResultType]::ParameterName, 'no-session')
            [CompletionResult]::new('--no-session-read-only', 'no-session-read-only', [CompletionResultType]::ParameterName, 'no-session-read-only')
            [CompletionResult]::new('--no-auth-type', 'no-auth-type', [CompletionResultType]::ParameterName, 'no-auth-type')
            [CompletionResult]::new('--no-auth', 'no-auth', [CompletionResultType]::ParameterName, 'no-auth')
            [CompletionResult]::new('--no-bearer', 'no-bearer', [CompletionResultType]::ParameterName, 'no-bearer')
            [CompletionResult]::new('--no-ignore-netrc', 'no-ignore-netrc', [CompletionResultType]::ParameterName, 'no-ignore-netrc')
            [CompletionResult]::new('--no-offline', 'no-offline', [CompletionResultType]::ParameterName, 'no-offline')
            [CompletionResult]::new('--no-check-status', 'no-check-status', [CompletionResultType]::ParameterName, 'no-check-status')
            [CompletionResult]::new('--no-follow', 'no-follow', [CompletionResultType]::ParameterName, 'no-follow')
            [CompletionResult]::new('--no-max-redirects', 'no-max-redirects', [CompletionResultType]::ParameterName, 'no-max-redirects')
            [CompletionResult]::new('--no-timeout', 'no-timeout', [CompletionResultType]::ParameterName, 'no-timeout')
            [CompletionResult]::new('--no-proxy', 'no-proxy', [CompletionResultType]::ParameterName, 'no-proxy')
            [CompletionResult]::new('--no-verify', 'no-verify', [CompletionResultType]::ParameterName, 'no-verify')
            [CompletionResult]::new('--no-cert', 'no-cert', [CompletionResultType]::ParameterName, 'no-cert')
            [CompletionResult]::new('--no-cert-key', 'no-cert-key', [CompletionResultType]::ParameterName, 'no-cert-key')
            [CompletionResult]::new('--no-ssl', 'no-ssl', [CompletionResultType]::ParameterName, 'no-ssl')
            [CompletionResult]::new('--no-native-tls', 'no-native-tls', [CompletionResultType]::ParameterName, 'no-native-tls')
            [CompletionResult]::new('--no-default-scheme', 'no-default-scheme', [CompletionResultType]::ParameterName, 'no-default-scheme')
            [CompletionResult]::new('--no-https', 'no-https', [CompletionResultType]::ParameterName, 'no-https')
            [CompletionResult]::new('--no-http-version', 'no-http-version', [CompletionResultType]::ParameterName, 'no-http-version')
            [CompletionResult]::new('--no-ignore-stdin', 'no-ignore-stdin', [CompletionResultType]::ParameterName, 'no-ignore-stdin')
            [CompletionResult]::new('--no-curl', 'no-curl', [CompletionResultType]::ParameterName, 'no-curl')
            [CompletionResult]::new('--no-curl-long', 'no-curl-long', [CompletionResultType]::ParameterName, 'no-curl-long')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
