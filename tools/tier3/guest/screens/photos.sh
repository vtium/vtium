#!/bin/bash
# kit: sixel
# look-for: One photograph twice side by side below the page's first line,
# look-for: chafa left and libsixel (mpv) right, each named under it: a street
# look-for: at sunset, the sky orange to blue. libsixel's copy has a fine grain
# look-for: (mpv sends it in under 100 colors) and can end a few pixels above
# look-for: chafa's: mpv rounds its height to sixel's 6-pixel bands. Then what
# look-for: img2sixel wrote.
# wrong-looks-like: No pictures, a picture over the text or the names, colors
# wrong-looks-like: swapped or washed out, bands in chafa's sky, torn or
# wrong-looks-like: shifted rows, a picture cut off, or one copy in colors
# wrong-looks-like: unlike the other's beyond libsixel's grain.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# ADR-103's acceptance scenario A1: a real photograph through two sixel
# encoders, compared side by side. The photograph is the operator's own, a
# street at sunset, licensed CC BY 4.0 and kept beside these scripts
# (photo/LICENSE). chafa, the operator's everyday tool, draws it on the
# left; libsixel's encoder draws it on the right by way of mpv (ffmpeg
# decodes, libsixel encodes). img2sixel, libsixel's own tool, writes nothing
# with the gdk-pixbuf Arch ships (2.44, through glycin): libsixel 1.10.5 asks
# gdk-pixbuf for the picture before closing the loader, gets none, and
# returns success, so the page prints how many bytes it wrote and a fixed
# libsixel shows here by itself. Each picture is eight rows tall and as wide
# as keeps the photograph's shape, from the cell size the terminal reports
# (XTWINOPS 16), placed at its cell with CUP; the text after them is placed
# with CUP too. Every row is counted from the row the page starts on
# (top_row), so the pictures stay under the page's first line below the VT
# demo's header. The page is 13 rows.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/acceptance.inc"
sixel_offered
needs chafa chafa
needs mpv mpv
cell_size
top_row
r=8
photo_cols "$r"
printf 'A street at sunset, %d by %d cells: chafa left, libsixel right.\n' "$w" "$r"
row=$((top + 2))
chafa_photo "$photo_file" 1 "$row" "$w" "$r"
photo "$photo_file" $((w + 3)) "$row" "$w" "$r"
printf '%s[%d;1H%-*s  %s' "$esc" $((row + r)) "$w" "chafa" "libsixel (mpv)"
printf '%s[%d;1H' "$esc" $((row + r + 1))
if command -v img2sixel >/dev/null; then
	printf 'img2sixel wrote %d bytes for the photograph (libsixel 1.10.5 with\n' \
		"$(img2sixel_bytes $((w * cw)))"
	printf 'gdk-pixbuf 2.44 writes none). '
else
	printf 'img2sixel is not installed (libsixel). '
fi
printf 'Cell %sx%s.%s\n' "$rw" "$rh" "${note:+ $note}"
