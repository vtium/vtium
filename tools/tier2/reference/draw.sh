#!/bin/sh
# Draws vtium's unifont screens with the C reference into OUT, as goldens:
# the kmscon commit ../pin.tsv names and the libtsm commit its
# subprojects/libtsm.wrap pins, read through git archive and built in a
# temporary directory, then pixels.c over what `tier2 screens` prints.
#
#   tools/tier2/reference/draw.sh OUT [KMSCON] [LIBTSM]
#
# KMSCON and LIBTSM default to the checkouts beside this repository. Needs
# gcc, meson, ninja, pkg-config, and kmscon's build dependencies.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out=$1
kmscon=${2:-$root/../kmscon}
libtsm=${3:-$root/../libtsm}
commit=$(awk '!/^#/ && $1 == "kmscon" { print $2 }' "$here/../pin.tsv")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/kmscon"
git -C "$kmscon" archive "$commit" | tar -x -C "$work/kmscon"
rev=$(sed -n 's/^revision = //p' "$work/kmscon/subprojects/libtsm.wrap")
mkdir "$work/kmscon/subprojects/libtsm"
git -C "$libtsm" archive "$rev" | tar -x -C "$work/kmscon/subprojects/libtsm"

# the C reference's tests need libcheck; the drawing program needs none
meson setup "$work/build" "$work/kmscon" --force-fallback-for=libtsm \
	-Dlibtsm:tests=false -Dtests=false -Ddocs=disabled -Dvideo_drm3d=disabled \
	-Drenderer_gltex=disabled -Dfont_pango=disabled -Dsd_login=disabled \
	-Ddbus=disabled >"$work/meson.log"
ninja -C "$work/build" >"$work/ninja.log"

# font_unifont.c reads the font from these, which pixels.c fills from the
# blob genunifont wrote
mkdir "$work/inc"
printf '#include <stddef.h>\nextern const char *_binary_font_unifont_data_start;\nextern size_t _binary_font_unifont_data_size;\n' \
	>"$work/inc/font_unifont_data.bin.h"
k=$work/kmscon
b=$work/build
# shellcheck disable=SC2046 # pkg-config's flags are words
gcc -O2 -std=gnu11 -w -D_GNU_SOURCE -include "$b/config.h" -I"$work/inc" -I"$k" \
	-I"$k/src" -I"$k/src/shl/htable" -I"$k/src/misc" -I"$b/src" \
	-I"$k/subprojects/libtsm/src/tsm" -I"$b/subprojects/libtsm/src/tsm" \
	$(pkg-config --cflags libdrm) -o "$work/pixels" "$here/pixels.c" \
	"$b/src/shl/libshl.a" "$b/src/shl/htable/libext-htable.a" "$b/src/misc/libmisc.a" \
	-L"$b/subprojects/libtsm/src/tsm" -ltsm -Wl,-rpath,"$b/subprojects/libtsm/src/tsm" \
	-lz -lm -lpthread

cargo run -q --release --manifest-path "$root/Cargo.toml" -p vtium-tier2 --bin tier2 -- \
	screens >"$work/screens.tsv"
mkdir -p "$out"
"$work/pixels" "$b/src/font/font_unifont_data" "$work/screens.tsv" "$out"
echo "drawn by kmscon $commit and libtsm $rev"
