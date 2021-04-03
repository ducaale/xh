#!/bin/sh

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
    echo "unsupported OS or architecture"
    exit 1
fi

echo "Detected target: $target"

url=$(curl -sSL https://api.github.com/repos/ducaale/xh/releases/latest | grep -wo -m1 "https://.*$target.tar.gz")
if ! test "$url"; then
    echo "Could not find release info"
    exit 1
fi

echo "Downloading xh..."

if ! curl -SLO "$url"; then
    echo "Could not download tarball"
    exit 1
fi

tar xzf xh-*
sudo mv xh-*/xh /usr/local/bin/
sudo ln -sf /usr/local/bin/xh /usr/local/bin/xhs

echo "xh v$(xh --version | cut -c4-8) has been installed to /usr/local/bin"
