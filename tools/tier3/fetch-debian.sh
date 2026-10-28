#!/bin/sh
# Fetches the pinned Debian cloud image for the guest without systemd into
# DIR and verifies it against the SHA-256 pinned in the Makefile. Debian
# signs no cloud image; the pin was taken from the image whose SHA-512
# matched the build's published SHA512SUMS, and is the trust anchor.
#
#   fetch-debian.sh DIR URL IMAGE SHA256
set -eu
dir=$1 url=$2 image=$3 sha=$4
mkdir -p "$dir"
cd "$dir"
if [ ! -f "$image" ]; then
    curl -fL --retry 3 -o "$image.part" "$url/$image"
    mv "$image.part" "$image"
fi
echo "$sha  $image" | sha256sum -c -
