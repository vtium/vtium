# Shared by the kitty graphics screens (kitty*.sh), which source it: a small
# sender written from the kitty graphics protocol's published spec alone
# (https://sw.kovidgoyal.net/kitty/graphics-protocol/, ADR-300's clean room),
# the spec's detection, and the photograph as PNG. It draws nothing itself,
# and sources sixel-common.inc for ask and top_row.
here=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
. "$here/sixel-common.inc"

# The photograph of photos.sh, scaled to 320x240 and saved as PNG
# (photo/LICENSE), so the terminal's image helper decodes it.
photo_png=$here/photo/street-sunset-320.png

# One command, APC G keys ; payload ST, its base64 payload in chunks of 4096
# with m=1 on all but the last, as the spec's chunking asks; the spec reads m
# for data sent inline only, and a name fits one chunk.
kitty_send() {
	local keys=$1 data=$2 i more
	if [ -z "$data" ]; then
		printf '%s_G%s;%s\\' "$esc" "$keys" "$esc"
		return
	fi
	for ((i = 0; i < ${#data}; i += 4096)); do
		more=$((i + 4096 < ${#data}))
		if ((i == 0)); then
			printf '%s_G%s,m=%d;%s%s\\' "$esc" "$keys" "$more" "${data:i:4096}" "$esc"
		else
			printf '%s_Gm=%d;%s%s\\' "$esc" "$more" "${data:i:4096}" "$esc"
		fi
	done
}

# Sends a command that carries i and reads the terminal's reply, which ends
# with ST, into reply, as "i=N;OK" or "i=N;CODE:message"; "no reply" when
# none comes within 3 seconds (a file the image helper reads, or a PNG it
# decodes, answers after the helper). A reply for another i, one that came
# after an earlier command's 3 seconds ran out, is skipped. Echo is off while
# it waits, so the reply is not printed.
kitty_ask() {
	local mode r= want= n
	[[ ,$1, =~ ,i=([0-9]+), ]] && want="_Gi=${BASH_REMATCH[1]};"
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		kitty_send "$1" "$2" >/dev/tty
		for ((n = 0; n < 8; n++)); do
			IFS= read -rsd '\' -t 3 r </dev/tty || break
			[[ $r == *"$want"* ]] && break
		done
		[[ $r == *"$want"* ]] || r=
		stty "$mode" </dev/tty
	else
		kitty_send "$1" "$2"
	fi
	r=${r#*_G}
	reply=${r%"$esc"}
	[ -n "$reply" ] || reply='no reply'
}

# base64 of a string: a file's path or a shared memory object's name
b64() {
	printf '%s' "$1" | base64 -w0
}

# The spec's detection: a query of one pixel, then DA1. A terminal with the
# protocol answers the query first; without it, only DA1 comes back, and the
# page says so and exits. Under tier 3 the refusal is also reported on the
# event port.
kitty_offered() {
	local r
	r=$(ask "${esc}_Gi=31,s=1,v=1,a=q,f=24;AAAA${esc}\\${esc}[c" c)
	if [[ $r != *"_Gi=31;OK"* ]]; then
		# the screen's name first, so each screen's page differs from the last
		printf '%s: this terminal does not answer the kitty graphics query (an\n' \
			"$(basename "$0" .sh)"
		printf 'a=q of one pixel before DA1), so no picture is sent.\n'
		[ -n "$T3_EVENTS" ] && echo T3-NO-KITTY >"$T3_EVENTS"
		exit 0
	fi
}
