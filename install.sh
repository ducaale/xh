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

while [[ $# -gt 0 ]]
do
    key="$1"
    case "$key" in
        --install-dir)
            install_dir="$2"
            shift
            shift
            ;;
    esac
done

if [[ -z "$install_dir" ]]; then
    printf "Install location [default: %s]: " "$default_bin"
    read -r bindir < /dev/tty
    bindir=${bindir:-$default_bin}
else
    bindir=${install_dir}
fi

while ! test -d "$bindir"; do
    echo "Directory $bindir does not exist"
    printf "Install location [default: %s]: " "$default_bin"
    read -r bindir < /dev/tty
    bindir=${bindir:-$default_bin}
done

tar xzf xh.tar.gz

if test -w "$bindir"; then
    mv xh-*/xh "$bindir/"
    ln -sf "$bindir/xh" "$bindir/xhs"
else
    sudo mv xh-*/xh "$bindir/"
    sudo ln -sf "$bindir/xh" "$bindir/xhs"
fi

echo "$("$bindir"/xh -V) has been installed to:"
echo " • $bindir/xh"
echo " • $bindir/xhs"
