#!/bin/bash
# kit: kitty
# look-for: Two chains of pictures, each an orange and violet parent 6 cells
# look-for: by 2 rows, a cyan and magenta child 4 by 2 that starts 8 columns
# look-for: right of the parent and 1 row down, and a grandchild 4 by 2 that
# look-for: starts 2 columns left of the child and 2 rows down. The left
# look-for: chain's parent is at row 4, column 3. The right chain's parent was
# look-for: placed at row 4, column 41, then moved to row 12, column 49: its
# look-for: child and grandchild moved with it, and nothing is left at row 4
# look-for: on the right.
# wrong-looks-like: No pictures (the screen says the kitty graphics protocol
# wrong-looks-like: is not offered), a child or grandchild a cell off its
# wrong-looks-like: offset, a child left behind at row 4 when its parent moved,
# wrong-looks-like: or a parent drawn twice.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# Kitty relative placements (ADR-1100 D13): a placement made with P, the
# parent's image id, and Q, the parent's placement id, takes its place from
# its parent, H columns right and V rows down from the parent's top left
# cell. Two images are transmitted with a=t: the parent (i=20) and the child
# (i=21), raw RGBA pixels made by tools/tier3/t3_pictures.py. Each chain is a
# parent placement, a child of it (H=8, V=1) and a child of the child (H=-2,
# V=2). The right chain's parent is then placed again with the same image
# and placement id at another cell, which replaces it and keeps its
# children, so they move with it. Every command carries q=2.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/img-common.inc"
kitty_offered
pictures_there
top_row

printf 'img-relative: kitty relative placements (P, Q, H, V)\n'
kitty_send "a=t,i=20,q=2,f=32,s=48,v=32" "$(pic64 parent.rgba)"
kitty_send "a=t,i=21,q=2,f=32,s=32,v=32" "$(pic64 child.rgba)"
# chain: the parent placement's id, then its column
chain() {
	at 4 "$2"
	kitty_send "a=p,i=20,p=$1,q=2,C=1"
	kitty_send "a=p,i=21,p=$1,q=2,P=20,Q=$1,H=8,V=1"
	kitty_send "a=p,i=21,p=$(($1 + 10)),q=2,P=21,Q=$1,H=-2,V=2"
}
chain 1 3
chain 2 41
at 12 49
kitty_send "a=p,i=20,p=2,q=2,C=1"
at 3 1
printf 'parent at row 4, column 3'
at 3 41
printf 'parent placed here, then moved'
at 11 41
printf 'moved to row 12, column 49'
at 20 1
printf 'Each child starts 8 columns right of its parent and 1 row down; each\n'
printf 'grandchild 2 columns left of its parent and 2 rows down.\n'
