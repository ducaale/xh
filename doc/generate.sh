#!/usr/bin/env bash
set -e
cd "$(dirname "${BASH_SOURCE[0]}")"
XH_HELP2MAN=1 help2man -i ./man-template.roff -h help -n "Yet another HTTPie clone" -N 'cargo run --' > xh.1
