#!/bin/sh

set -e

if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-apple-darwin"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-unknown-linux-musl"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "aarch64" ]; then
    target="aarch64-unknown-linux-musl"
elif [ "$(uname -s)" = "Linux" ] && ( uname -m | grep -q -e '^arm' ); then
    target="arm-unknown-linux-gnueabihf"
else
    echo "Unsupported OS or architecture"
    exit 1
fi

fetch()
{
    if which curl > /dev/null; then
        if [ "$#" -eq 2 ]; then curl -L -o "$1" "$2"; else curl -sSL "$1"; fi
    elif which wget > /dev/null; then
        if [ "$#" -eq 2 ]; then wget -O "$1" "$2"; else wget -nv -O - "$1"; fi
    else
        echo "Can't find curl or wget, can't download package"
        exit 1
    fi
}

echo "Detected target: $target"

url=$(
    fetch https://api.github.com/repos/ducaale/xh/releases/latest |
    tac | tac | grep -wo -m1 "https://.*$target.tar.gz" || true
)
if ! test "$url"; then
    echo "Could not find release info"
    exit 1
fi

echo "Downloading xh..."

temp_dir=$(mktemp -dt xh.XXXXXX)
trap 'rm -rf "$temp_dir"' EXIT INT TERM
cd "$temp_dir"

if ! fetch xh.tar.gz "$url"; then
    echo "Could not download tarball"
    exit 1
fi

user_bin="$HOME/.local/bin"
case $PATH in
    *:"$user_bin":* | "$user_bin":* | *:"$user_bin")
        default_bin=$user_bin
        ;;
    *)
        default_bin='/usr/local/bin'
        ;;
esac

_read_installdir() {
    printf "Install location [default: %s]: " "$default_bin"
    read -r xh_installdir < /dev/tty
    xh_installdir=${xh_installdir:-$default_bin}
}

if [ -z "$XH_BINDIR" ]; then
    _read_installdir

    while ! test -d "$xh_installdir"; do
        echo "Directory $xh_installdir does not exist"
        _read_installdir
    done
else
    xh_installdir=${XH_BINDIR}
fi

tar xzf xh.tar.gz

if test -w "$xh_installdir" || [ -n "$XH_BINDIR" ]; then
    mv xh-*/xh "$xh_installdir/"
    ln -sf "$xh_installdir/xh" "$xh_installdir/xhs"
else
    sudo mv xh-*/xh "$xh_installdir/"
    sudo ln -sf "$xh_installdir/xh" "$xh_installdir/xhs"
fi

echo "$("$xh_installdir"/xh -V) has been installed to:"
echo " • $xh_installdir/xh"
echo " • $xh_installdir/xhs"
