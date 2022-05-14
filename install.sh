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

_read_bindir() {
    printf "Install location [default: %s]: " "$default_bin"
    read -r XH_BINDIR < /dev/tty
    XH_BINDIR=${XH_BINDIR:-$default_bin}
}

if [ -z "$XH_BINDIR" ]; then
    _read_bindir

    while ! test -d "$XH_BINDIR"; do
      echo "Directory $XH_BINDIR does not exist"
      _read_bindir
    done
fi

tar xzf xh.tar.gz

if test -w "$XH_BINDIR"; then
    mv xh-*/xh "$XH_BINDIR/"
    ln -sf "$XH_BINDIR/xh" "$XH_BINDIR/xhs"
else
    sudo mv xh-*/xh "$XH_BINDIR/"
    sudo ln -sf "$XH_BINDIR/xh" "$XH_BINDIR/xhs"
fi

echo "$("$XH_BINDIR"/xh -V) has been installed to:"
echo " • $XH_BINDIR/xh"
echo " • $XH_BINDIR/xhs"
