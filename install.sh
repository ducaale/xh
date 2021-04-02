#!/bin/sh

temp_dir=$(mktemp -d /tmp/xh.XXXXXXXX)
cd $temp_dir

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

curl -s https://api.github.com/repos/ducaale/xh/releases/latest \
| grep -wo "https.*$target.tar.gz" \
| xargs -n 1 curl -O -sSL

tar xzf xh-*
sudo mv xh-*/xh /usr/local/bin/
sudo ln -s /usr/local/bin/xh /usr/local/bin/xhs

echo "xh binary location: $(which xh)"
echo "xh binary version: $(xh --version)"
