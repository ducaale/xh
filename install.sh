#!/bin/sh

set -e

temp_dir=$(mktemp -d /tmp/xh.XXXXXXXX)
cd "$temp_dir"

if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-apple-darwin"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
    target="x86_64-unknown-linux-musl"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "amd64" ]; then
    target="x86_64-unknown-linux-musl"
elif [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "arm" ]; then
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
    grep -wo -m1 "https://.*$target.tar.gz" || true
)
if ! test "$url"; then
    echo "Could not find release info"
    exit 1
fi

echo "Downloading xh..."

if ! $fetch xh.tar.gz "$url"; then
    echo "Could not download tarball"
    exit 1
fi

tar xzf xh.tar.gz
sudo mv xh-*/xh /usr/local/bin/
sudo ln -sf /usr/local/bin/xh /usr/local/bin/xhs

echo "$(/usr/local/bin/xh --version) has been installed to /usr/local/bin"
