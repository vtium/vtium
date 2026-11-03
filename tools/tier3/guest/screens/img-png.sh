#!/bin/bash
# kit: kitty
# look-for: Four pictures at row 3, each with its reply under it: at column 3,
# look-for: red, green, blue and yellow quarters in a white frame, from a PNG
# look-for: sent inline, i=41;OK; at column 23, an orange ring around a blue
# look-for: disc from a PNG file, the dots behind it showing at its corners,
# look-for: i=42;OK; at column 43, the quarters from a PNG temporary file,
# look-for: i=43;OK, with "removed" under it; at column 63, the quarters from
# look-for: raw pixels compressed with zlib, i=44;OK. Row 10 gives the reply to
# look-for: a file the session's user cannot read, i=45;EBADF and its message.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), a missing picture with a reply other than
# wrong-looks-like: OK (the image helper did not start, read or decode), a
# wrong-looks-like: temporary file "kept", or any reply but EBADF on row 10.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# Pictures that vtium's image helper, vtium-image, reads and decodes for the
# console (ADR-1101): a PNG sent inline (f=100), a PNG read from a file
# (t=f), a PNG read from a temporary file (t=t) that the helper removes once
# read, and raw pixels compressed with zlib (o=z), which the helper inflates.
# The files are tools/tier3/t3_pictures.py's, readable by the session's
# user, whom the helper runs as; /etc/shadow, which that user cannot read,
# is refused with EBADF. Each command asks for its reply (no q), which the
# screen prints under its picture and reports on the event port.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-png: pictures the image helper decodes, with their replies\n'
for r in 3 4 5; do
	at $r 23
	printf '. . . .'
done
at 3 3
kitty_ask "a=T,i=41,C=1,f=100" "$(pic64 quad.png)"
report "$reply"
at 7 3
printf '%s' "$reply"
at 3 23
kitty_ask "a=T,i=42,C=1,f=100,t=f" "$(b64 "$pics/disc.png")"
report "$reply"
at 7 23
printf '%s' "$reply"
tmp=$(mktemp --suffix=.png /tmp/tty-graphics-protocol-XXXXXX)
cp "$pics/quad.png" "$tmp"
at 3 43
kitty_ask "a=T,i=43,C=1,f=100,t=t" "$(b64 "$tmp")"
report "$reply"
at 7 43
printf '%s' "$reply"
at 8 43
if [ -e "$tmp" ]; then kept=kept; rm -f "$tmp"; else kept=removed; fi
report "$kept"
printf '%s' "$kept"
at 3 63
kitty_ask "a=T,i=44,C=1,f=32,s=64,v=48,o=z" "$(pic64 quad.rgba.z)"
report "$reply"
at 7 63
printf '%s' "$reply"
at 10 1
kitty_ask "a=T,i=45,C=1,f=100,t=f" "$(b64 /etc/shadow)"
report "$reply"
printf 'a file the user cannot read: %s\n' "$reply"
