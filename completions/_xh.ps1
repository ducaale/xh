
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
            [CompletionResult]::new('-A', 'A', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('--auth-type', 'auth-type', [CompletionResultType]::ParameterName, 'Specify the auth mechanism')
            [CompletionResult]::new('-a', 'a', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS. PASS will be prompted if missing')
            [CompletionResult]::new('--auth', 'auth', [CompletionResultType]::ParameterName, 'Authenticate as USER with PASS. PASS will be prompted if missing')
            [CompletionResult]::new('--bearer', 'bearer', [CompletionResultType]::ParameterName, 'Authenticate with a bearer token')
            [CompletionResult]::new('-o', 'o', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('--output', 'output', [CompletionResultType]::ParameterName, 'Save output to FILE instead of stdout')
            [CompletionResult]::new('--max-redirects', 'max-redirects', [CompletionResultType]::ParameterName, 'Number of redirects to follow, only respected if `follow` is set')
            [CompletionResult]::new('-p', 'p', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('--print', 'print', [CompletionResultType]::ParameterName, 'String specifying what the output should contain')
            [CompletionResult]::new('--pretty', 'pretty', [CompletionResultType]::ParameterName, 'Controls output processing')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('--style', 'style', [CompletionResultType]::ParameterName, 'Output coloring style')
            [CompletionResult]::new('--proxy', 'proxy', [CompletionResultType]::ParameterName, 'Use a proxy for a protocol. For example: `--proxy https:http://proxy.host:8080`')
            [CompletionResult]::new('--default-scheme', 'default-scheme', [CompletionResultType]::ParameterName, 'The default scheme to use if not specified in the URL')
            [CompletionResult]::new('--verify', 'verify', [CompletionResultType]::ParameterName, 'If "no", skip SSL verification. If a file path, use it as a CA bundle')
            [CompletionResult]::new('--cert', 'cert', [CompletionResultType]::ParameterName, 'Use a client side certificate for SSL')
            [CompletionResult]::new('--cert-key', 'cert-key', [CompletionResultType]::ParameterName, 'A private key file to use with --cert')
            [CompletionResult]::new('--offline', 'offline', [CompletionResultType]::ParameterName, 'Construct HTTP requests without sending them anywhere')
            [CompletionResult]::new('-j', 'j', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('--json', 'json', [CompletionResultType]::ParameterName, '(default) Serialize data items from the command line as a JSON object')
            [CompletionResult]::new('-f', 'f', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('--form', 'form', [CompletionResultType]::ParameterName, 'Serialize data items from the command line as form fields')
            [CompletionResult]::new('-m', 'm', [CompletionResultType]::ParameterName, 'Like --form, but force a multipart/form-data request even without files')
            [CompletionResult]::new('--multipart', 'multipart', [CompletionResultType]::ParameterName, 'Like --form, but force a multipart/form-data request even without files')
            [CompletionResult]::new('-I', 'I', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('--ignore-stdin', 'ignore-stdin', [CompletionResultType]::ParameterName, 'Do not attempt to read stdin')
            [CompletionResult]::new('-F', 'F', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('--follow', 'follow', [CompletionResultType]::ParameterName, 'Do follow redirects')
            [CompletionResult]::new('-d', 'd', [CompletionResultType]::ParameterName, 'Download the body to a file instead of printing it')
            [CompletionResult]::new('--download', 'download', [CompletionResultType]::ParameterName, 'Download the body to a file instead of printing it')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print only the response headers, shortcut for --print=h')
            [CompletionResult]::new('--headers', 'headers', [CompletionResultType]::ParameterName, 'Print only the response headers, shortcut for --print=h')
            [CompletionResult]::new('-b', 'b', [CompletionResultType]::ParameterName, 'Print only the response body, Shortcut for --print=b')
            [CompletionResult]::new('--body', 'body', [CompletionResultType]::ParameterName, 'Print only the response body, Shortcut for --print=b')
            [CompletionResult]::new('-c', 'c', [CompletionResultType]::ParameterName, 'Resume an interrupted download. Requires --download and --output')
            [CompletionResult]::new('--continue', 'continue', [CompletionResultType]::ParameterName, 'Resume an interrupted download. Requires --download and --output')
            [CompletionResult]::new('-v', 'v', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
            [CompletionResult]::new('--verbose', 'verbose', [CompletionResultType]::ParameterName, 'Print the whole request as well as the response')
            [CompletionResult]::new('-q', 'q', [CompletionResultType]::ParameterName, 'Do not print to stdout or stderr')
            [CompletionResult]::new('--quiet', 'quiet', [CompletionResultType]::ParameterName, 'Do not print to stdout or stderr')
            [CompletionResult]::new('-S', 'S', [CompletionResultType]::ParameterName, 'Always stream the response body')
            [CompletionResult]::new('--stream', 'stream', [CompletionResultType]::ParameterName, 'Always stream the response body')
            [CompletionResult]::new('--check-status', 'check-status', [CompletionResultType]::ParameterName, 'Exit with an error status code if the server replies with an error')
            [CompletionResult]::new('--curl', 'curl', [CompletionResultType]::ParameterName, 'Print a translation to a `curl` command')
            [CompletionResult]::new('--curl-long', 'curl-long', [CompletionResultType]::ParameterName, 'Use the long versions of curl''s flags')
            [CompletionResult]::new('--https', 'https', [CompletionResultType]::ParameterName, 'Make HTTPS requests if not specified in the URL')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Prints help information')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Prints version information')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Prints version information')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
