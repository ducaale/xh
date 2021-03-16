
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
                $element.Value.StartsWith('-')) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'xh' {
            [CompletionResult]::new('--pretty', 'pretty', [CompletionResultType]::ParameterName, 'Controls output processing')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('--style', 'style', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('--print', 'print', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('--output', 'output', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('-A', 'A', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('--auth-type', 'auth-type', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS. PASS will be prompted if missing')
            [CompletionResult]::new('--auth', 'auth', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS. PASS will be prompted if missing')
            [CompletionResult]::new('--bearer', 'bearer', [CompletionResultType]::ParameterName, 'Authenticate with a bearer token')
            [CompletionResult]::new('--max-redirects', 'max-redirects', [CompletionResultType]::ParameterName, 'Number of redirects to follow, only respected if `follow` is set')
            [CompletionResult]::new('--proxy', 'proxy', [CompletionResultType]::ParameterName, 'Use a proxy for a protocol. For example: `--proxy https:http://proxy.host:8080`')
            [CompletionResult]::new('--verify', 'verify', [CompletionResultType]::ParameterName, 'If "no", skip SSL verification. If a file path, use it as a CA bundle')
            [CompletionResult]::new('--cert', 'cert', [CompletionResultType]::ParameterName, 'Use a client side certificate for SSL')
            [CompletionResult]::new('--cert-key', 'cert-key', [CompletionResultType]::ParameterName, 'A private key file to use with --cert')
            [CompletionResult]::new('--default-scheme', 'default-scheme', [CompletionResultType]::ParameterName, 'The default scheme to use if not specified in the URL')
            [CompletionResult]::new('-j', 'j', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('--json', 'json', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('--form', 'form', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('-m', 'm', [CompletionResultType]::ParameterName, 'Like --form, but force a multipart/form-data request even without files')
            [CompletionResult]::new('--multipart', 'multipart', [CompletionResultType]::ParameterName, 'Like --form, but force a multipart/form-data request even without files')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print only the response headers, shortcut for --print=h')
            [CompletionResult]::new('--headers', 'headers', [CompletionResultType]::ParameterName, 'Print only the response headers, shortcut for --print=h')
            [CompletionResult]::new('-b', 'b', [CompletionResultType]::ParameterName, 'Print only the response body, Shortcut for --print=b')
            [CompletionResult]::new('--body', 'body', [CompletionResultType]::ParameterName, 'Print only the response body, Shortcut for --print=b')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
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
            [CompletionResult]::new('--check-status', 'check-status', [CompletionResultType]::ParameterName, 'Exit with an error status code if the server replies with an error')
            [CompletionResult]::new('-F', 'F', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('--follow', 'follow', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('--https', 'https', [CompletionResultType]::ParameterName, 'Make HTTPS requests if not specified in the URL')
            [CompletionResult]::new('-I', 'I', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('--ignore-stdin', 'ignore-stdin', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('--curl', 'curl', [CompletionResultType]::ParameterName, 'Print a translation to a `curl` command')
            [CompletionResult]::new('--curl-long', 'curl-long', [CompletionResultType]::ParameterName, 'Use the long versions of curl''s flags')
            [CompletionResult]::new('--no-auth', 'no-auth', [CompletionResultType]::ParameterName, 'no-auth')
            [CompletionResult]::new('--no-auth-type', 'no-auth-type', [CompletionResultType]::ParameterName, 'no-auth-type')
            [CompletionResult]::new('--no-bearer', 'no-bearer', [CompletionResultType]::ParameterName, 'no-bearer')
            [CompletionResult]::new('--no-body', 'no-body', [CompletionResultType]::ParameterName, 'no-body')
            [CompletionResult]::new('--no-cert', 'no-cert', [CompletionResultType]::ParameterName, 'no-cert')
            [CompletionResult]::new('--no-cert-key', 'no-cert-key', [CompletionResultType]::ParameterName, 'no-cert-key')
            [CompletionResult]::new('--no-check-status', 'no-check-status', [CompletionResultType]::ParameterName, 'no-check-status')
            [CompletionResult]::new('--no-continue', 'no-continue', [CompletionResultType]::ParameterName, 'no-continue')
            [CompletionResult]::new('--no-curl', 'no-curl', [CompletionResultType]::ParameterName, 'no-curl')
            [CompletionResult]::new('--no-curl-long', 'no-curl-long', [CompletionResultType]::ParameterName, 'no-curl-long')
            [CompletionResult]::new('--no-default-scheme', 'no-default-scheme', [CompletionResultType]::ParameterName, 'no-default-scheme')
            [CompletionResult]::new('--no-download', 'no-download', [CompletionResultType]::ParameterName, 'no-download')
            [CompletionResult]::new('--no-follow', 'no-follow', [CompletionResultType]::ParameterName, 'no-follow')
            [CompletionResult]::new('--no-form', 'no-form', [CompletionResultType]::ParameterName, 'no-form')
            [CompletionResult]::new('--no-headers', 'no-headers', [CompletionResultType]::ParameterName, 'no-headers')
            [CompletionResult]::new('--no-https', 'no-https', [CompletionResultType]::ParameterName, 'no-https')
            [CompletionResult]::new('--no-ignore-netrc', 'no-ignore-netrc', [CompletionResultType]::ParameterName, 'no-ignore-netrc')
            [CompletionResult]::new('--no-ignore-stdin', 'no-ignore-stdin', [CompletionResultType]::ParameterName, 'no-ignore-stdin')
            [CompletionResult]::new('--no-json', 'no-json', [CompletionResultType]::ParameterName, 'no-json')
            [CompletionResult]::new('--no-max-redirects', 'no-max-redirects', [CompletionResultType]::ParameterName, 'no-max-redirects')
            [CompletionResult]::new('--no-multipart', 'no-multipart', [CompletionResultType]::ParameterName, 'no-multipart')
            [CompletionResult]::new('--no-offline', 'no-offline', [CompletionResultType]::ParameterName, 'no-offline')
            [CompletionResult]::new('--no-output', 'no-output', [CompletionResultType]::ParameterName, 'no-output')
            [CompletionResult]::new('--no-pretty', 'no-pretty', [CompletionResultType]::ParameterName, 'no-pretty')
            [CompletionResult]::new('--no-print', 'no-print', [CompletionResultType]::ParameterName, 'no-print')
            [CompletionResult]::new('--no-proxy', 'no-proxy', [CompletionResultType]::ParameterName, 'no-proxy')
            [CompletionResult]::new('--no-quiet', 'no-quiet', [CompletionResultType]::ParameterName, 'no-quiet')
            [CompletionResult]::new('--no-stream', 'no-stream', [CompletionResultType]::ParameterName, 'no-stream')
            [CompletionResult]::new('--no-style', 'no-style', [CompletionResultType]::ParameterName, 'no-style')
            [CompletionResult]::new('--no-verbose', 'no-verbose', [CompletionResultType]::ParameterName, 'no-verbose')
            [CompletionResult]::new('--no-verify', 'no-verify', [CompletionResultType]::ParameterName, 'no-verify')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Prints help information')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Prints version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Prints version information')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
