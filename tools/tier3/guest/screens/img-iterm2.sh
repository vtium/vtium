#!/bin/bash
# kit: sixel
# look-for: Four iTerm2 pictures from row 3. At column 3, red, green, blue and
# look-for: yellow quarters in a white frame, 8 cells by 3 rows, from a PNG at
# look-for: its own size. At column 15, the same frame and size showing an
# look-for: animated GIF's last frame: yellow top left, blue top right, green
# look-for: bottom left, red bottom right. At column 27, the PNG at twice its
# look-for: size, 16 cells by 6 rows, sent in parts. At column 47, the PNG at
# look-for: half its size, 4 cells wide and a row and a half tall. Under them,
# look-for: the row and column the cursor came to rest at after each.
# wrong-looks-like: No pictures (the screen says sixel is not offered, which
# wrong-looks-like: iTerm2 pictures need), a missing picture with the cursor
# wrong-looks-like: still at row 3 after it, the GIF on its red top left first
# wrong-looks-like: frame or still changing, or a picture a cell off.
# screen-text: none, iTerm2 pictures and cursor moves do not print as rows
# iTerm2's inline pictures (ADR-1103): OSC 1337 File= with inline=1 and the
# file in base64, as imgcat sends it, decoded by vtium's image helper. vtium
# takes them while it offers sixel and has the helper, and places them under
# sixel's rules, so the screen checks sixel's DA1 attribute first. The
# pictures are tools/tier3/t3_pictures.py's: the quarters as a PNG, and
# quad.gif, an animated GIF of the quarters turned a quarter clockwise in
# each of its three frames, 100 ms each, with no loop extension, so it plays
# once and stops on its last frame by itself. The width and height keys
# size a picture in cells (16 by 6) or in pixels (32px wide, its height
# following its shape). The third picture comes in parts (MultipartFile=,
# FilePart= of 30 base64 characters, FileEnd), as iTerm2 sends a file too
# large for one string; 30 is no multiple of 4, so the console carries the
# characters a part leaves over into the next.
#
# iTerm2's protocol sends no reply, so after each picture the screen asks
# where the cursor is (DSR 6): vtium leaves it on the row of the picture's
# last pixel row, at the picture's left column, and a picture that was
# dropped leaves it where it was. The places are reported on the event port
# and printed under the pictures.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
sixel_offered
pictures_there
top_row

printf 'img-iterm2: iTerm2 pictures (OSC 1337 File=), a PNG and an animated GIF\n'
places=()
at 3 3
iterm2_send "name=$(b64 quad.png);size=$(stat -c %s "$pics/quad.png");inline=1" quad.png
cursor_event "png"
places+=("$where")
at 3 15
iterm2_send "inline=1" quad.gif
cursor_event "gif"
places+=("$where")
at 3 27
iterm2_parts "inline=1;width=16;height=6" quad.png 30
cursor_event "parts"
places+=("$where")
at 3 47
iterm2_send "inline=1;width=32px" quad.png
cursor_event "pixels"
places+=("$where")
at 11 1
printf 'After each picture the cursor was at:\n'
printf '  the PNG at its own size: %s\n' "${places[0]}"
printf '  the GIF, which played once: %s\n' "${places[1]}"
printf '  the PNG sent in parts, width=16;height=6: %s\n' "${places[2]}"
printf '  the PNG at width=32px: %s\n' "${places[3]}"
