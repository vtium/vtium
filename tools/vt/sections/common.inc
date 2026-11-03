# Sourced by the acceptance pass's pages (tools/vt/sections/*.sh, ADR-1501) that ask
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

# The row the page starts on, in top: ACCEPT_TOP, which accept.sh sets to the
# row below its two header rows, else the cursor's row as the terminal
# reports it, else 1.
top_row() {
	local r=${ACCEPT_TOP:-}
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

# A language page (ADR-1501 D7): the sentence of the language CODE in
# text/udhr-article1.tsv, its meaning in English, then a few of its words in
# each style SGR gives, two styles to a row, then Unicode's superscript and
# subscript characters, and the font set the walk switched to for the page
# (ACCEPT_FRIT) with what that font set checks, from the frit's
# "# checks:" lines. A style marked * is one vtium does not draw yet: it
# looks plain, or upright for italic, and the page says so.
language_page() {
	local dir code name sentence words english supsub frit checks
	dir=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
	while IFS=$'\t' read -r code name sentence words english supsub; do
		[ "$code" = "$1" ] && break
	done <"$dir/text/udhr-article1.tsv"
	[ "$code" = "$1" ] || { printf 'no language %s in text/udhr-article1.tsv\n' "$1"; return 1; }
	# label, SGR, and the column the cell starts at
	cell() { printf '%s[%dG%-10s%s[%sm%s%s[0m' "$esc" "$3" "$1" "$esc" "$2" "$words" "$esc"; }
	row() { cell "$1" "$2" 1; cell "$3" "$4" 41; printf '\n'; }
	printf '%s, from the Universal Declaration of Human Rights:\n' "$name"
	printf '%s\n' "$sentence"
	printf 'In English: All human beings are born free and equal in dignity and rights.\n'
	printf 'Below, the words for "%s" in each style:\n' "$english"
	row plain 0 bold 1
	row 'italic*' 3 'b italic*' '1;3'
	row dim 2 reverse 7
	row underline 4 double 4:2
	row curly 4:3 dotted 4:4
	row dashed 4:5 'red line' '4;58:5:196'
	row struck 9 'overline*' 53
	row 'SGR 73*' 73 'SGR 74*' 74
	printf '%-10s%s\n' Unicode: "$supsub"
	printf '* not drawn yet: italic stays upright, the rest look plain. Bold is widened.\n'
	frit=${ACCEPT_FRIT:-}
	if [ -n "$frit" ] && [ -f "$dir/frits/$frit.vtfrit" ]; then
		checks=$(sed -n 's/^# checks: //p' "$dir/frits/$frit.vtfrit" | paste -sd' ' -)
		printf 'Font %s: %s\n' "$frit" "$checks"
	else
		printf 'Font: the console'"'"'s own; the walk named no font set for this page.\n'
	fi
}

# Ends a page this build cannot show: says why, on the page, and exits 77,
# which accept.sh records as skip, never as no.
cannot_show() {
	printf '\n  This build cannot show this page.\n  %s\n' "$1"
	printf '  The page is recorded as skip, not as no.\n'
	exit 77
}
