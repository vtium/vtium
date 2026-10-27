#!/bin/sh
# Fetches the pinned Arch Linux cloud image into DIR and verifies it: the
# SHA-256 pinned in the Makefile, and the detached signature against the
# arch-boxes key, identified by the pinned fingerprint. The key is fetched into
# a keyring under DIR, never the user's.
#
#   fetch-image.sh DIR URL IMAGE SHA256 KEY_FINGERPRINT
set -eu
dir=$1 url=$2 image=$3 sha=$4 key=$5
mkdir -p "$dir"
cd "$dir"
if [ ! -f "$image" ]; then
    curl -fL --retry 3 -o "$image.part" "$url/$image"
    mv "$image.part" "$image"
fi
curl -fsSL --retry 3 -o "$image.sig" "$url/$image.sig"
echo "$sha  $image" | sha256sum -c -
export GNUPGHOME="$PWD/gnupg"
[ -d "$GNUPGHOME" ] || mkdir -m 700 "$GNUPGHOME"
gpg --batch --list-keys "$key" >/dev/null 2>&1 ||
    gpg --batch --keyserver hkps://keyserver.ubuntu.com --recv-keys "$key"
gpg --batch --status-fd 1 --verify "$image.sig" "$image" 2>/dev/null |
    grep -q "^\[GNUPG:\] VALIDSIG .* $key\$" ||
    { echo "signature on $image is not by $key" >&2; exit 1; }
echo "$image: signature by $key OK"
