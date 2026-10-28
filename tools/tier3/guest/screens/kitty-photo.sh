#!/bin/bash
# kit: kitty
# look-for: The street at sunset, 20 cells by 8 rows under the first line, sky
# look-for: orange to blue, the photograph's shape kept, then the reply
# look-for: i=1;OK under it.
# wrong-looks-like: No picture with i=1;OK (the helper decoded nothing), a
# wrong-looks-like: reply other than OK, colours swapped or washed out, the
# wrong-looks-like: picture stretched or over the text, or no reply at all.
# screen-text: none, a kitty picture and cursor moves do not print as rows
# The kitty graphics protocol's inline PNG (t=d, f=100, ADR-103 step 4c):
# the photograph of photos.sh, scaled to 320x240 and saved as PNG
# (photo/LICENSE), sent through the pty in 4096-byte chunks and placed at the
# cursor in 20x8 cells without moving it (C=1). kmscon's image helper decodes
# it, as the session's user or, before login, the pre-login user, and the
# reply the terminal sends back is printed under the picture.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/kitty-common.inc"
kitty_offered
top_row
printf 'The photograph as PNG, sent inline (t=d, f=100), in 20x8 cells:\n'
kitty_ask "a=T,i=1,f=100,C=1,c=20,r=8" "$(base64 -w0 "$photo_png")"
printf '%s[%d;1Hreply: %s\n' "$esc" $((top + 9)) "$reply"
