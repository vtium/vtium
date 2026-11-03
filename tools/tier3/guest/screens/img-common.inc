# Shared by the screens of vtium's images variant (img-*.sh), which source
# it: the kitty sender, the kitty query and the sixel query of
# kitty-common.inc and sixel-common.inc, and the test pictures that
# tools/tier3/t3_pictures.py writes into pictures/ beside the scripts when a
# run builds the variant's disc. It draws nothing itself.
here=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
. "$here/kitty-common.inc"
pics=$here/pictures

# Exits with a message unless the test pictures are there. Under tier 3 the
# missing pictures are also reported on the event port, and the screen's
# capture then fails against its golden.
pictures_there() {
	if [ ! -f "$pics/quad.six" ]; then
		printf '%s: no test pictures in %s;\n' "$(basename "$0" .sh)" "$pics"
		printf 'tools/tier3/t3_pictures.py writes them.\n'
		[ -n "$T3_EVENTS" ] && echo T3-NO-PICTURES >"$T3_EVENTS"
		exit 0
	fi
}

# Moves the cursor to row $1 and column $2, the rows counted from the row
# the screen started on (top_row).
at() {
	printf '%s[%d;%dH' "$esc" $((top + $1 - 1)) "$2"
}

# The base64 of test picture $1, for a kitty command's payload.
pic64() {
	base64 -w0 "$pics/$1"
}

# Reports $1, a kitty reply or what the screen found, on the event port under
# tier 3, where the driver holds it to the replies the screen must get
# (t3_evaluate.REPLIES), whether or not the screen's golden is reviewed yet.
reply_event() {
	[ -n "$T3_EVENTS" ] && echo "T3-REPLY $1" >"$T3_EVENTS"
	return 0
}
