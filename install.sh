#!/bin/sh

set -e

if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-apple-darwin"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-unknown-linux-musl"
elif [ "$(uname -s)" = "Linux" ] && ( uname -m | grep -q -e '^arm' -e '^aarch' ); then
    target="arm-unknown-linux-gnueabihf"
else
    echo "Unsupported OS or architecture"
    exit 1
fi

if which curl > /dev/null; then
    fetch='curl -sSL -o'
elif which wget > /dev/null; then
    fetch='wget -nv -O'
else
    echo "Can't find curl or wget, can't download package"
    exit 1
fi

echo "Detected target: $target"

url=$(
    $fetch - https://api.github.com/repos/ducaale/xh/releases/latest |
    tac | tac | grep -wo -m1 "https://.*$target.tar.gz" || true
)
if ! test "$url"; then
    echo "Could not find release info"
    exit 1
fi

echo "Downloading xh..."

temp_dir=$(mktemp -d /tmp/xh.XXXXXXXX)
trap 'rm -rf "$temp_dir"' EXIT INT TERM
cd "$temp_dir"

if ! $fetch xh.tar.gz "$url"; then
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

printf "Install location [default: %s]: " "$default_bin"
read -r bindir < /dev/tty
bindir=${bindir:-$default_bin}

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

echo "$("$bindir"/xh --version) has been installed to $bindir/xh"
