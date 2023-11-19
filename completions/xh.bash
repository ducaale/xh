_xh() {
    local i cur prev opts cmd
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    cmd=""
    opts=""

    for i in ${COMP_WORDS[@]}
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="xh"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        xh)
            opts="-j -f -s -p -h -b -m -v -P -q -S -o -d -c -A -a -F -4 -6 -I -V --json --form --multipart --raw --pretty --format-options --style --response-charset --response-mime --print --headers --body --meta --verbose --all --history-print --quiet --stream --output --download --continue --session --session-read-only --auth-type --auth --bearer --ignore-netrc --offline --check-status --follow --max-redirects --timeout --proxy --verify --cert --cert-key --ssl --native-tls --default-scheme --https --http-version --resolve --interface --ipv4 --ipv6 --ignore-stdin --curl --curl-long --help --no-json --no-form --no-multipart --no-raw --no-pretty --no-format-options --no-style --no-response-charset --no-response-mime --no-print --no-headers --no-body --no-meta --no-verbose --no-all --no-history-print --no-quiet --no-stream --no-output --no-download --no-continue --no-session --no-session-read-only --no-auth-type --no-auth --no-bearer --no-ignore-netrc --no-offline --no-check-status --no-follow --no-max-redirects --no-timeout --no-proxy --no-verify --no-cert --no-cert-key --no-ssl --no-native-tls --no-default-scheme --no-https --no-http-version --no-resolve --no-interface --no-ipv4 --no-ipv6 --no-ignore-stdin --no-curl --no-curl-long --no-help --version <[METHOD] URL> [REQUEST_ITEM]..."
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --raw)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --pretty)
                    COMPREPLY=($(compgen -W "all colors format none" -- "${cur}"))
                    return 0
                    ;;
                --format-options)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --style)
                    COMPREPLY=($(compgen -W "auto solarized monokai fruity" -- "${cur}"))
                    return 0
                    ;;
                -s)
                    COMPREPLY=($(compgen -W "auto solarized monokai fruity" -- "${cur}"))
                    return 0
                    ;;
                --response-charset)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --response-mime)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --print)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -p)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --history-print)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -P)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -o)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --session)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --session-read-only)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --auth-type)
                    COMPREPLY=($(compgen -W "basic bearer digest" -- "${cur}"))
                    return 0
                    ;;
                -A)
                    COMPREPLY=($(compgen -W "basic bearer digest" -- "${cur}"))
                    return 0
                    ;;
                --auth)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -a)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bearer)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-redirects)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --proxy)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --verify)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --cert)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --cert-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --ssl)
                    COMPREPLY=($(compgen -W "auto tls1 tls1.1 tls1.2 tls1.3" -- "${cur}"))
                    return 0
                    ;;
                --default-scheme)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --http-version)
                    COMPREPLY=($(compgen -W "1.0 1.1 2" -- "${cur}"))
                    return 0
                    ;;
                --resolve)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --interface)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

complete -F _xh -o bashdefault -o default xh
