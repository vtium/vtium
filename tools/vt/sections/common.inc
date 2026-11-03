# Sourced by the playtest's pages (tools/vt/sections/*.sh, ADR-1501) that ask
# the terminal something or place things by row; it draws nothing itself.
# shellcheck shell=bash
# shellcheck disable=SC2034 # esc and top are read by the pages that source this
esc=$'\033'

# The terminal's answer to QUERY, up to and including the byte FINAL, with
# FINAL dropped; empty when the terminal does not answer within a second or
# the page is not on a terminal. Echo is off while it waits, so the answer is
# not printed onto the page.
ask() {
	local reply= mode
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '%s' "$1" >/dev/tty
		IFS= read -rsd "$2" -t 1 reply </dev/tty
		stty "$mode" </dev/tty
	fi
	printf '%s' "$reply"
}

# The row the page starts on, in top: DEMO_TOP, which demo.sh sets to the
# row below its two header rows, else the cursor's row as the terminal
# reports it, else 1.
top_row() {
	local r=${DEMO_TOP:-}
	[[ $r =~ ^[1-9][0-9]*$ ]] || {
		r=$(ask "$esc[6n" R)
		r=${r##*[}
		r=${r%;*}
	}
	[[ $r =~ ^[1-9][0-9]*$ ]] && top=$r || top=1
}

# Whether the terminal shows pictures: its DA1 answer holds attribute 4,
# which vtium gives only in a build with the images feature.
pictures_shown() {
	local da1
	da1=$(ask "$esc[c" c)
	[[ ";${da1##*\?};" == *";4;"* ]]
}

# Whether the terminal takes kitty graphics, as the protocol says to ask: a
# query of one raw pixel, which vtium answers itself without the image
# helper, then DA1. A terminal without the protocol answers only DA1.
kitty_shown() {
	[[ $(ask "${esc}_Gi=31,s=1,v=1,a=q,f=24;AAAA${esc}\\${esc}[c" c) == *"_Gi=31;OK"* ]]
}

# Whether the image helper decodes what this terminal is sent: a PNG of one
# pixel sent inline with the kitty protocol's query (a=q), which the terminal
# answers OK only once the helper has decoded it. The helper may take a
# moment to start, so this waits up to 3 seconds.
helper_decodes() {
	local mode r=
	local png='iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII='
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '%s_Gi=32,a=q,f=100;%s%s\\' "$esc" "$png" "$esc" >/dev/tty
		IFS= read -rsd '\' -t 3 r </dev/tty
		stty "$mode" </dev/tty
	fi
	[[ $r == *"_Gi=32;OK"* ]]
}

# The cell size in pixels, cw by ch, from XTWINOPS 16's answer, ESC [ 6 ;
# height ; width t; 0 by 0 when the terminal does not say.
cell_size() {
	local h w
	IFS=';' read -r _ h w <<<"$(ask "$esc[16t" t)"
	[[ $w =~ ^[1-9][0-9]*$ && $h =~ ^[1-9][0-9]*$ ]] || { w=0 h=0; }
	cw=$w ch=$h
}

# One kitty graphics command, APC G KEYS ; PAYLOAD ST, its base64 payload in
# chunks of 4096 bytes with m=1 on every chunk but the last, as the
# protocol's chunking asks.
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

# Ends a page this build cannot show: says why, on the page, and exits 77,
# which demo.sh records as skip, never as no.
cannot_show() {
	printf '\n  This build cannot show this page.\n  %s\n' "$1"
	printf '  The page is recorded as skip, not as no.\n'
	exit 77
}
