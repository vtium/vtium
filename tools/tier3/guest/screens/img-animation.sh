#!/bin/bash
# kit: kitty
# look-for: Two kitty animations, stopped. At row 3, column 5, a picture 8
# look-for: cells by 3 rows in a white frame, showing its third frame: cyan
# look-for: top left, green top right, blue bottom left, magenta bottom right.
# look-for: At row 3, column 20, a picture 4 cells by 2 rows in a white frame
# look-for: that has played its frames once and stopped on its last: yellow
# look-for: top left and bottom right, magenta top right and bottom left.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), red or violet in the top left of the
# wrong-looks-like: first picture or yellow in its bottom right (another
# wrong-looks-like: frame), cyan in the second picture (it never played), or
# wrong-looks-like: a picture that keeps changing.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# Kitty animation (ADR-1102), as the kitty spec describes it: a picture's
# own pixels are its first frame, the root, and a=f adds a frame laid on a
# canvas, here an earlier frame (c) with the data at x, y over it, by alpha
# blending, or by replacement with X=1. The frames' data are the patches
# tools/tier3/t3_pictures.py writes, each the inside of one quarter, so
# every frame keeps the white frame.
#
# The first image (i=50) is the quarters with three frames past the root:
# frame 2 makes the top left cyan, frame 3 the bottom right of frame 2
# magenta, and frame 4 the top left of frame 3 violet. It runs looping
# forever (s=3, v=1), and then one command makes frame 3 current and stops
# it (c=3, s=1), so it shows frame 3 however long it ran: the frame on show
# depends on the bytes alone, never on when the capture is taken.
#
# The second image (i=51) is the child, with two frames that make its top
# left and then its bottom right yellow, each shown for 100 ms. It runs with
# v=2, which plays the frames once (ADR-1102 D4), so it stops on its last
# frame by itself, which shows that vtium's animation clock plays it.
#
# Each a=f and each a=a asks for its reply, which the screen reports on the
# event port; the transmissions carry q=2.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-animation: kitty animations, stopped on a chosen frame\n'
at 3 5
kitty_send "a=T,i=50,q=2,C=1,f=32,s=64,v=48" "$(pic64 quad.rgba)"
ask_event "a=f,i=50,c=1,x=1,y=1,s=31,v=23" "$(pic64 patch-cyan.rgba)"
ask_event "a=f,i=50,c=2,x=32,y=24,s=31,v=23,X=1" "$(pic64 patch-magenta.rgba)"
ask_event "a=f,i=50,c=3,x=1,y=1,s=31,v=23" "$(pic64 patch-violet.rgba)"
ask_event "a=a,i=50,r=1,z=200"
ask_event "a=a,i=50,s=3,v=1"
ask_event "a=a,i=50,c=3,s=1"

at 3 20
kitty_send "a=T,i=51,q=2,C=1,f=32,s=32,v=32" "$(pic64 child.rgba)"
ask_event "a=f,i=51,c=1,x=1,y=1,s=15,v=15,z=100" "$(pic64 patch-yellow.rgba)"
ask_event "a=f,i=51,c=2,x=16,y=16,s=15,v=15,z=100" "$(pic64 patch-yellow.rgba)"
ask_event "a=a,i=51,r=1,z=100"
ask_event "a=a,i=51,s=3,v=2"

at 8 1
printf 'Row 3, column 5: run looping, then stopped on frame 3 of 4 (c=3, s=1).\n'
printf 'Row 3, column 20: played once through its 3 frames (v=2), stopped on the last.\n'
