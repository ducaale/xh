#!/bin/sh
# Generate man page and completions
set -e
cd "$(dirname "$0")"

XH_HELP2MAN=1 help2man -i doc/man-template.roff -h help -n "Yet another HTTPie clone" -N 'cargo run --' > doc/xh.1

cargo run -- generate_completions completions

# Subcommand check is unnecessary: https://github.com/clap-rs/clap/pull/2359
sed 's/ -n "__fish_use_subcommand"//g' completions/xh.fish > completions/xh.fish.new
mv completions/xh.fish.new completions/xh.fish
