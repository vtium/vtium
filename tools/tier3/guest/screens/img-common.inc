# Shared by the screens of vtium's images variant (img-*.sh), which source
# it: the kitty sender, the kitty query and the sixel query of
# kitty-common.inc and sixel-common.inc, the test pictures that
# tools/tier3/t3_pictures.py writes into pictures/ beside the scripts when a
# run builds the variant's disc, the reports on the event port, and the
# senders of iTerm2 pictures and kitty Unicode placeholders. It draws
# nothing itself.
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

# Sends kitty command $1, which carries i, with the payload $2, and reports
# its reply on the event port, "no reply" when none comes within 3 seconds.
# It reads the reply as kitty_ask does, and also takes one that names a
# placement after the image (i=N,p=P;...), which kitty_ask skips while it
# waits for i=N; alone. kitty_ask stays as it is, since kitty-common.inc is
# a byte-for-byte copy of the C reference's harness.
ask_event() {
	local mode r= id= n
	[[ ,$1, =~ ,i=([0-9]+), ]] && id=${BASH_REMATCH[1]}
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		kitty_send "$1" "$2" >/dev/tty
		for ((n = 0; n < 8; n++)); do
			IFS= read -rsd '\' -t 3 r </dev/tty || break
			[[ $r == *"_Gi=$id;"* || $r == *"_Gi=$id,"* ]] && break
		done
		[[ $r == *"_Gi=$id;"* || $r == *"_Gi=$id,"* ]] || r=
		stty "$mode" </dev/tty
	else
		kitty_send "$1" "$2"
	fi
	r=${r#*_G}
	reply=${r%"$esc"}
	[ -n "$reply" ] || reply='no reply'
	reply_event "$reply"
}

# Reports where the cursor is on the event port under tier 3, as "$1 at row
# R, column C", the row counted as `at` counts it, and keeps it in where.
# The read waits up to 3 seconds, as kitty_ask does, since a picture the
# image helper decodes holds the terminal's answer until the helper is done
# (ADR-1100 D7).
cursor_event() {
	local mode r=
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '%s[6n' "$esc" >/dev/tty
		IFS= read -rsd R -t 3 r </dev/tty
		stty "$mode" </dev/tty
	fi
	if [[ ${r##*[} =~ ^([0-9]+)\;([0-9]+)$ ]]; then
		where="row $((BASH_REMATCH[1] - top + 1)), column ${BASH_REMATCH[2]}"
	else
		where="nowhere"
	fi
	reply_event "$1 at $where"
}

# An iTerm2 picture (OSC 1337 File=, ADR-1103): keys $1, then the base64 of
# test picture $2, ended with BEL as imgcat ends it.
iterm2_send() {
	printf '%s]1337;File=%s:%s\a' "$esc" "$1" "$(pic64 "$2")"
}

# The same picture sent in parts, as iTerm2 sends a large one: MultipartFile=
# with keys $1, then the base64 of test picture $2 in FilePart= strings of $3
# characters, then FileEnd. A part need not be a multiple of 4 characters.
iterm2_parts() {
	local data i
	data=$(pic64 "$2")
	printf '%s]1337;MultipartFile=%s\a' "$esc" "$1"
	for ((i = 0; i < ${#data}; i += $3)); do
		printf '%s]1337;FilePart=%s\a' "$esc" "${data:i:$3}"
	done
	printf '%s]1337;FileEnd\a' "$esc"
}

# The first eight of the kitty spec's row and column diacritics as UTF-8,
# U+0305, U+030D, U+030E, U+0310, U+0312, U+033D, U+033E and U+033F: a
# diacritic's value is its place in the list, counted from 0 (ADR-1100 D13).
marks=($'\xcc\x85' $'\xcc\x8d' $'\xcc\x8e' $'\xcc\x90' $'\xcc\x92' $'\xcc\xbd' $'\xcc\xbe'
	$'\xcc\xbf')

# One kitty Unicode placeholder cell: U+10EEEE, then a diacritic for each of
# its arguments, the picture's row, its column and the top byte of the
# image id, of which a cell may give the first, the first two, all three or
# none.
placeholder() {
	local n
	printf '\xf4\x8e\xbb\xae'
	for n in "$@"; do
		printf '%s' "${marks[n]}"
	done
}
