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
# (ACCEPT_FRIT) with what that font set checks (frit_line).
language_page() {
	local dir code name sentence words english supsub
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
	row italic 3 'b italic' '1;3'
	row dim 2 reverse 7
	row underline 4 double 4:2
	row curly 4:3 dotted 4:4
	row dashed 4:5 'red line' '4;58:5:196'
	row struck 9 overline 53
	row raised 73 lowered 74
	printf '%-10s%s\n' Unicode: "$supsub"
	frit_line
}

# The font set the walk switched to for the page (ACCEPT_FRIT) and what that
# font set checks, from the frit's "# checks:" lines.
frit_line() {
	local dir frit checks
	dir=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
	frit=${ACCEPT_FRIT:-}
	if [ -n "$frit" ] && [ -f "$dir/frits/$frit.vtfrit" ]; then
		checks=$(sed -n 's/^# checks: //p' "$dir/frits/$frit.vtfrit" | paste -sd' ' -)
		printf 'Font %s: %s\n' "$frit" "$checks"
	else
		printf 'Font: the console'"'"'s own; the walk named no font set for this page.\n'
	fi
}

# Ends a page this build cannot show: says why, on the page and in the file
# ACCEPT_WHY names, which accept.sh puts in the record, and exits 77, which
# accept.sh records as skip, never as no.
cannot_show() {
	[ -z "${ACCEPT_WHY:-}" ] || printf '%s' "$1" >"$ACCEPT_WHY"
	printf '\n  This build cannot show this page.\n  %s\n' "$1"
	printf '  The page is recorded as skip, not as no.\n'
	exit 77
}

# shellcheck source=tools/vt/blink.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/../blink.inc"

# For a page that blinks text: the blink style vtium draws it in, what that
# looks like when it is right, and where the style comes from (blink.inc),
# each folded to 80 columns.
blink_line() {
	blink_style
	printf 'Blink: %s. Right: %s.\n' "$blink_style" "$(blink_right)" | fold -s -w 80
	# vtium-render's mono.rs: at 1 bit a cell is lit or unlit, so partial
	# blinks as full
	[ "$blink_style" != partial ] ||
		printf 'On a 1-bit display vtium draws partial as full: blinking text hides.\n'
	printf 'The blink style is %s.\n' "$blink_source" | fold -s -w 80
}

# For a page that walks vtium-config's TUI: runs the review run's own
# vtium-config on a vtium.conf of the page's own, then removes it. A review
# run stages its vtium-config and vtium beside the copy of vtium.conf that
# ACCEPT_VTIUM_CONF names, and its frits in the folder frits there. The page
# copies those frits into a folder of its own and gives vtium-config a home
# of its own, so the TUI reads and writes none of your drafts, settings or
# installed files, and nothing it does reaches the console's own vtium.conf.
# The page makes that folder in $XDG_RUNTIME_DIR, which run-vt.sh gives the
# acceptance pass when the user's runtime directory is the user's alone. Only
# when the session has none does the folder go in your home folder. The
# folder never goes in /tmp: vtium refuses a font set in a folder that other
# users can write to, so the TUI would count both looks' font sets as
# problems.
# The page removes the folder however it exits. When vtium stops while the
# TUI is open, the page's terminal hangs up and its bash gets SIGHUP or
# SIGTERM. bash waits for vtium-config to end, then exits and removes the
# folder on the way out.
# The vtium.conf has two looks: big, in the outline font set at 20 points,
# which the head desk uses, and small, in the bitmap font set, with vtium's
# own pointer pictures. The page prints its steps and waits for a key before
# the TUI starts, so the steps can be read first. The TUI draws on the
# alternate screen, so the steps show again when it quits. Away from a
# review run, or off a terminal, the page cannot show.
looks_tui() {
	local stage=${ACCEPT_VTIUM_CONF:-} dir status
	stage=${stage%/config/vtium.conf}
	[ -n "$stage" ] && [ -x "$stage/vtium-config" ] && [ -x "$stage/vtium" ] && [ -d "$stage/frits" ] ||
		cannot_show "No review run staged vtium-config, which make vt-accept does."
	[ -t 0 ] || cannot_show "vtium-config's TUI needs a terminal."
	printf '%s\n' "$@"
	printf '\n  Press a key to open vtium-config. Quit it with q, then n if it asks.\n'
	read -rsn1 </dev/tty
	dir=$(mktemp -d "${XDG_RUNTIME_DIR:-$HOME}/vtium-looks.XXXXXX") || cannot_show "The page could not make a folder for its vtium.conf."
	# shellcheck disable=SC2064 # the folder's name is fixed now, quoted once
	trap "rm -rf -- $(printf %q "$dir")" EXIT
	trap 'exit 129' HUP
	trap 'exit 143' TERM
	mkdir -p "$dir/etc" "$dir/home"
	cp -R "$stage/frits" "$dir/frits"
	printf '%s\n' 'version = 1' "font-frits = $dir/frits" '[look big]' 'font-name = acceptance-outline' \
		'font-size = 20' '[look small]' 'font-name = acceptance-bitmap' \
		'pointer-theme = /usr/share/vtium/pointers.svg' '[head desk]' 'look = big' '[rules]' \
		'display * -> desk' >"$dir/etc/vtium.conf"
	HOME="$dir/home" XDG_CONFIG_HOME="$dir/home/.config" XDG_DATA_HOME="$dir/home/.local/share" \
		XDG_STATE_HOME="$dir/home/.local/state" XDG_CACHE_HOME="$dir/home/.cache" \
		"$stage/vtium-config" --configdir "$dir/etc" --frits "$dir/frits" --vtium "$stage/vtium" tui
	status=$?
	trap - EXIT HUP TERM
	rm -rf -- "$dir"
	printf '  vtium-config ended with status %s. Answer for what it showed.\n' "$status"
}
