#!/bin/sh

cd /tmp/

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

rm -rf xh*

curl -s https://api.github.com/repos/ducaale/xh/releases/latest \
| grep -wo "https.*$target.tar.gz" \
| wget -qi -

tarball="$(find . -name "xh*$target.tar.gz")"
tar -xzf $tarball --strip-components=1 

chmod +x xh
sudo mv xh /usr/local/bin/

location="$(which xh)"
echo "xh binary location: $location"

version="$(xh --version)"
echo "xh binary version: $version"
