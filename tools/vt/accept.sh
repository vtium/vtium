#!/bin/bash
# The acceptance pass (ADR-1501): the operator walks every page in sections/, in file
# name order, in one pass, and answers each with one key. Below each page a
# footer gives the page's guidance, held in its file: what a correct screen
# looks like (its "# look-for:" lines) and what a defect looks like
# ("# wrong-looks-like:"). Then one key answers it:
#
#   y looks right   n looks wrong   u unsure   s skip
#   c types a comment of any length, then asks for the answer
#   r draws the page again   b goes back one page   q quits; the pass resumes later
#
# A page that exits 77 says on screen why this build cannot show it, and is
# recorded as skip. A page whose "# frits:" line names font sets (frits, in
# sections/frits) is walked once in each: the walk switches the console to
# the frit with OSC 50 before the page, checks with OSC 50's query that it
# did, and switches back to the console's own font after. A frit whose font
# files are missing, or that vtium does not switch to, makes the page a skip
# that says why. Every answer rewrites the pass's record, a markdown file
# named after the pass in the passes directory: ACCEPT_DIR, else
# $XDG_STATE_HOME/vtium/acceptance (~/.local/state/vtium/acceptance). The record
# holds the build, the terminal, and each page's answer and comment; a pass
# quit part way keeps its answers, and the next run with the same label
# offers to resume it at its first unanswered page. feedback.sh prints a
# pass, lists them and compares two.
#
# It runs in any terminal as any user and needs nothing from vtium, so the same
# pages can be compared in vtium and in another terminal.
#
#   accept.sh [PAGE...]   PAGE is a file name without the number and .sh, such
#                       as sgr; with none, every page runs
#
# ACCEPT_LABEL names the terminal on the title page and in the record; run-vt.sh
# sets it to "vtium dev build <git describe>". ACCEPT_VT, ACCEPT_BINARY and
# ACCEPT_FEATURES, when set, name the VT, the binary and its features in the
# record. ACCEPT_NOPROMPT=1 shows the guidance, asks nothing and records
# nothing: any key goes to the next page and q quits. ACCEPT_NOPAGE=1 also waits
# for no key and leaves the last page on the screen, for checking the pages
# outside a VT. ACCEPT_PAGES names another directory of pages, for the
# tests.
set -u
shopt -u patsub_replacement 2>/dev/null || true

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=tools/vt/blink.inc
. "$here/blink.inc"
nopage=${ACCEPT_NOPAGE:-}
noprompt=${ACCEPT_NOPROMPT:-$nopage}
passes=${ACCEPT_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/vtium/acceptance}
label=${ACCEPT_LABEL:-unlabelled}
pass=
record=
rows=25
cols=80
key=
note=
go=
bracketed=

clear_screen() {
	printf '\033[0m\033[H\033[2J\033[3J'
}

# What a page may have changed, put back before the next one: the pen, the
# whole-screen reverse video, the cursor's shape and visibility, the palette,
# the scroll region and kitty pictures.
reset_terminal() {
	printf '\033[0m\033[?5l\033[0 q\033]104\033\\\033[?25h\033[r\033_Ga=d,d=A,q=2\033\\'
}

read_size() {
	local s
	s=$(stty size 2>/dev/null </dev/tty) || s=
	rows=${s% *}
	cols=${s#* }
	[[ $rows =~ ^[1-9][0-9]*$ && $cols =~ ^[1-9][0-9]*$ ]] || { rows=25; cols=80; }
}

# A mark for a page's header when the terminal is smaller than the 80x25 the
# pages are made for: a font switch changes the grid, so the size is read
# before each page.
small_mark() {
	[ "$rows" -lt 25 ] || [ "$cols" -lt 80 ] || return 0
	printf '  \033[31m(%sx%s, smaller than 80x25)\033[0m' "$cols" "$rows"
}

# Reads one key into $key. An escape sequence (an arrow key, a late reply from
# the terminal) is read whole and returned as ESC, so its bytes are never taken
# for keys. So is a paste: the walk turns on bracketed paste (DECSET 2004), so
# the terminal sends pasted text between ESC [ 200 ~ and ESC [ 201 ~, and the
# walk drops it whole, so a pasted y, n, c or q never answers a page.
read_key() {
	local c seq=
	# the rest of a paste whose start drain dropped
	[ -z "$in_paste" ] || skip_paste
	IFS= read -rsn1 key </dev/tty || exit 0
	[ "$key" = $'\033' ] || return 0
	if IFS= read -rsn1 -t 0.05 c </dev/tty && [ "$c" = '[' ]; then
		# a CSI sequence: parameters, then a final byte from @ to ~
		while IFS= read -rsn1 -t 0.05 c </dev/tty; do
			seq+=$c
			case $c in [@-~]) break ;; esac
		done
	fi
	if [ "$seq" = '200~' ]; then
		skip_paste
	else
		drain
	fi
}

# Reads a paste after its start, up to and with its end, ESC [ 201 ~, and drops
# it. A paste whose end does not come within a second of its last byte ends
# there, so a terminal that never sends the end cannot hang the walk; a paste
# that pauses longer than that between two of its bytes has its rest read as
# keys.
skip_paste() {
	local c tail=
	in_paste=
	while IFS= read -rsn1 -d '' -t 1 c </dev/tty; do
		tail=$tail$c
		[ ${#tail} -le 6 ] || tail=${tail:1}
		[ "$tail" != $'\033[201~' ] || return 0
	done
}

# Discards keys already typed, so a key pressed before a prompt appeared does
# not answer it, and any paste among them. A paste whose start drain reads and
# whose end has not come when drain stops leaves in_paste set, and read_key
# drops the rest of it, however long after the prompt it arrives. drain stops
# once 50 ms pass with no byte, so a paste's start marker split across writes
# more than 50 ms apart is not seen as one, and the paste's text after it is
# read as keys; terminals write the marker in one write, with the paste.
in_paste=
drain() {
	local c tail=
	while IFS= read -rsn1 -d '' -t 0.05 c </dev/tty; do
		tail=$tail$c
		[ ${#tail} -le 6 ] || tail=${tail:1}
		case $tail in
		*$'\033[200~') in_paste=1 tail='' ;;
		*$'\033[201~') in_paste='' tail='' ;;
		esac
	done
}

# Reads one key, as read_key does, for a prompt that any key answers: an
# escape sequence or a paste, which read_key returns as ESC, is no key there,
# so a paste never moves the walk on.
read_any_key() {
	read_key
	while [ "$key" = $'\033' ]; do read_key; done
}

# Shows PROMPT in reverse video on the current line and waits for a key; q ends
# the walk.
page_end() {
	[ -z "$nopage" ] || return 0
	printf '\033[7m %s \033[0m' "$1"
	read_any_key
	case $key in q | Q) clear_screen; exit 0 ;; esac
}

name_of() {
	local n=${1##*/}
	n=${n%.sh}
	printf '%s' "${n#[0-9][0-9]-}"
}

id_of() {
	local n=${1##*/}
	printf '%s' "${n%.sh}"
}

# Entry I's id in the record: the page's file name without .sh, and for a
# page walked in a frit, @ and the frit's name.
entry_id() {
	printf '%s%s' "$(id_of "${pages[$1]}")" "${frits[$1]:+@${frits[$1]}}"
}

# Entry I's name on the screen.
entry_name() {
	printf '%s%s' "$(name_of "${pages[$1]}")" "${frits[$1]:+ @ ${frits[$1]}}"
}

# A page's metadata lines "# KEY: text", joined into one line.
meta() {
	sed -n "s/^# $1: //p" "$2" | paste -sd' ' -
}

# TEXT folded to the screen's width, after a ten-column LABEL on the first
# line and under it on the others.
wrapped() {
	local label=$1 line
	[ -n "$2" ] || return 0
	while IFS= read -r line; do
		printf '%-10s%s\n' "$label" "${line% }"
		label=
	done < <(printf '%s\n' "$2" | fold -s -w $((cols - 11)))
}

# The footer of page I, without its prompt line, into foot[]: a blank line
# and the guidance.
footer_lines() {
	local f=${pages[$1]}
	mapfile -t foot < <(
		echo
		wrapped 'right:' "$(meta look-for "$f")"
		wrapped 'wrong:' "$(meta wrong-looks-like "$f")"
	)
}

# Draws page I; status is the page's exit status, 77 when this build cannot
# show it.
status=0
unshown_why=
draw_page() {
	local frit=${frits[$1]}
	reset_terminal
	unshown_why=
	# ACCEPT_NOPROMPT draws a frit's page in the font the terminal has.
	if [ -n "$frit" ] && [ -z "$noprompt" ]; then
		switch_font "$frit" || unshown_why=$why
	else
		restore_font
	fi
	read_size
	clear_screen
	printf '\033[1m[%d/%d] %s\033[0m%s\n\n' $(($1 + 1)) ${#pages[@]} "$(entry_name "$1")" "$(small_mark)"
	status=0
	if [ -n "$unshown_why" ]; then
		printf '  This build cannot show this page.\n  %s\n' "$unshown_why"
		printf '  The page is recorded as skip, not as no.\n'
		status=77
		return
	fi
	ACCEPT_TOP=3 ACCEPT_FRIT=$frit bash "${pages[$1]}" || status=$?
	[ "$status" -eq 0 ] || [ "$status" -eq 77 ] ||
		printf '\n\033[31m(the page exited with status %d)\033[0m\n' "$status"
}

# The terminal's answer to QUERY, up to the byte FINAL, which is dropped;
# empty when none comes within a second.
ask_terminal() {
	local reply=
	printf '%s' "$1" >/dev/tty
	IFS= read -rsd "$2" -t 1 reply </dev/tty || reply=
	printf '%s' "$reply"
}

# The cursor's row, asked of the terminal (DSR 6); fails when the terminal
# does not answer within a second.
cursor_row() {
	local reply
	reply=$(ask_terminal $'\033[6n' R)
	reply=${reply##*[}
	reply=${reply%;*}
	[[ $reply =~ ^[0-9]+$ ]] || return 1
	printf '%s' "$reply"
}

# The font OSC 50's query names, or nothing when the terminal does not
# answer it: font operations are off, or the terminal is not vtium.
query_font() {
	local r
	r=$(ask_terminal $'\033]50;?\a' $'\a')
	[[ $r == *']50;'* ]] || return 0
	printf '%s' "${r##*]50;}"
}

# The font files frit NAME names, a path a line: outline:, file: and bdf:
# sources as they are, a kbd: source as the first file of its name in the
# kbd console font directories, or kbd:NAME when there is none. font: and
# builtin: sources name no file the walk can check.
frit_files() {
	local line value d s found
	while IFS= read -r line || [ -n "$line" ]; do
		[[ $line =~ ^source=(outline|file|bdf|kbd):(.+)$ ]] || continue
		value=${BASH_REMATCH[2]}
		if [ "${BASH_REMATCH[1]}" != kbd ]; then
			printf '%s\n' "$value"
			continue
		fi
		found=
		for d in /usr/share/kbd/consolefonts /usr/lib/kbd/consolefonts; do
			for s in '' .psf .psfu .psf.gz .psfu.gz; do
				[ -n "$found" ] || [ ! -f "$d/$value$s" ] || found=$d/$value$s
			done
		done
		printf '%s\n' "${found:-kbd:$value}"
	done <"$frit_dir/$1.vtfrit"
}

# The lines the record gives the frits this walk uses: each frit file's
# SHA-256, and each font file's SHA-256 and the package that owns it, so two
# passes tell a change in vtium from a font update. missing[NAME] lists a
# frit's files this machine lacks.
font_lines=()
declare -A missing=()
font_sums() {
	local name path sum pkg
	for name in $(printf '%s\n' "${frits[@]}" | sed '/^$/d' | sort -u); do
		if [ ! -f "$frit_dir/$name.vtfrit" ]; then
			font_lines+=("- frit: $name missing from $frit_dir")
			missing[$name]="$frit_dir/$name.vtfrit"
			continue
		fi
		sum=$(sha256sum <"$frit_dir/$name.vtfrit") sum=${sum%% *}
		font_lines+=("- frit: $name sha256 $sum")
		while IFS= read -r path; do
			if [ ! -f "$path" ]; then
				font_lines+=("- font: $name $path missing")
				missing[$name]+="${missing[$name]:+, }$path"
				continue
			fi
			sum=$(sha256sum <"$path") sum=${sum%% *}
			pkg=
			if command -v pacman >/dev/null; then
				pkg=$(pacman -Qo "$path" 2>/dev/null | sed -n 's/.* is owned by //p')
			fi
			font_lines+=("- font: $name $path sha256 $sum${pkg:+ package $pkg}")
		done < <(frit_files "$name")
	done
	fonts=$(printf '%s\n' "${font_lines[@]}" | sha256sum) fonts=${fonts%% *}
}

# Switches the console to frit NAME and checks that it did; fails with why
# set when its files are missing or vtium does not take it.
why=
current=
switch_font() {
	if [ -n "${missing[$1]:-}" ]; then
		why="the font set $1 needs files this machine lacks: ${missing[$1]}"
		return 1
	fi
	[ "$current" = "$1" ] && return 0
	printf '\033]50;%s\a' "$1" >/dev/tty
	# the switch is taken when the terminal reads it: a round trip first
	cursor_row >/dev/null
	local now
	now=$(query_font)
	if [ "$now" != "$1" ]; then
		why="vtium did not switch to the font set $1: OSC 50 named '${now:-nothing}'"
		return 1
	fi
	current=$1
}

# Switches the console back to the font it had when the walk started.
restore_font() {
	[ -n "$current" ] || return 0
	[ -z "$font_start" ] || printf '\033]50;%s\a' "$font_start" >/dev/tty
	current=
}

# The machine the pass runs on, as lines for the record: its model, its
# kernel, each GPU or display controller with its kernel driver, and each
# connected display with its connector, its preferred mode and the make and
# model its EDID gives, then the text area vtium reports. A pass is evidence
# about this hardware only (ADR-1501 D3). ACCEPT_SYSFS and ACCEPT_PROC name
# other roots for /sys and /proc, for the tests.
hardware_lines=()
hardware=
hardware_scan() {
	local sys=${ACCEPT_SYSFS:-/sys} proc=${ACCEPT_PROC:-/proc} d card c v name area slot
	hardware_lines=()
	v=
	for name in sys_vendor product_name product_version; do
		[ ! -r "$sys/class/dmi/id/$name" ] || v+=" $(one_line "$(cat "$sys/class/dmi/id/$name")")"
	done
	hardware_lines+=("- machine: ${v# }")
	[ -n "${v# }" ] || hardware_lines[-1]="- machine: not given by /sys/class/dmi"
	hardware_lines+=("- kernel: $(cat "$proc/sys/kernel/osrelease" 2>/dev/null || echo unknown)")
	for card in "$sys"/class/drm/card[0-9]*; do
		[[ ${card##*/} =~ ^card[0-9]+$ ]] || continue
		d=$card/device
		slot=$(readlink "$d" 2>/dev/null) slot=${slot##*/}
		name=
		if [ -z "${ACCEPT_SYSFS:-}" ] && [ -n "$slot" ] && command -v lspci >/dev/null; then
			name=$(lspci -s "$slot" 2>/dev/null | cut -d' ' -f2- | head -n 1)
		fi
		v=$(readlink "$d/driver" 2>/dev/null) v=${v##*/}
		hardware_lines+=("- gpu: ${card##*/} driver ${v:-none}, $(cat "$d/vendor" 2>/dev/null || echo ?):$(cat "$d/device" 2>/dev/null || echo ?) at ${slot:-?}${name:+, $(one_line "$name")}")
		for c in "$card"-*; do
			[ "$(cat "$c/status" 2>/dev/null)" = connected ] || continue
			hardware_lines+=("- display: ${c##*/card[0-9]-}, preferred mode $(head -n 1 "$c/modes" 2>/dev/null || echo unknown), $(edid_name "$c/edid")")
		done
	done
	area=$(ask_terminal $'\033[14t' t)
	[[ $area =~ \[4\;([0-9]+)\;([0-9]+)$ ]] && area="${BASH_REMATCH[2]}x${BASH_REMATCH[1]} pixels" || area="no answer"
	hardware_lines+=("- text-area: $area, as vtium reports it (XTWINOPS 14)")
	hardware=$(printf '%s\n' "${hardware_lines[@]}" | grep -v '^- text-area' | sha256sum) hardware=${hardware%% *}
}

# The make and model an EDID file holds: the manufacturer's three letters,
# the product code and the monitor's name descriptor, or why there are none.
edid_name() {
	local b o i n v
	[ -s "$1" ] || { printf 'no EDID'; return; }
	read -ra b < <(od -An -tu1 -v "$1" | tr -s ' \n' '  ')
	[ ${#b[@]} -ge 128 ] || { printf 'EDID too short'; return; }
	v=$((b[8] << 8 | b[9]))
	printf 'EDID make %b%b%b' "\\x$(printf %x $(((v >> 10 & 31) + 64)))" \
		"\\x$(printf %x $(((v >> 5 & 31) + 64)))" "\\x$(printf %x $(((v & 31) + 64)))"
	printf ' product 0x%04X' $((b[10] | b[11] << 8))
	for o in 54 72 90 108; do
		[ "${b[o]}" -eq 0 ] && [ "${b[o + 1]}" -eq 0 ] && [ "${b[o + 3]}" -eq 252 ] || continue
		n=
		for ((i = o + 5; i < o + 18; i++)); do
			[ "${b[i]}" -ne 10 ] || break
			[ "${b[i]}" -lt 32 ] || [ "${b[i]}" -gt 126 ] || n+=$(printf "\\x$(printf %x "${b[i]}")")
		done
		printf ' model %s' "$(printf '%b' "${n% }")"
	done
}

# A value for one line of the record's list: tabs and line breaks become
# spaces.
one_line() {
	local s=${1//$'\t'/ }
	s=${s//$'\n'/ }
	printf '%s' "${s//$'\r'/ }"
}

# A comment as one cell of the record's table: &, < and > as entities, a
# backslash as \\, | as \|, a tab as a space and a line break as <br>.
# feedback.sh reads it back.
cell() {
	local s=${1//&/&amp;}
	s=${s//</&lt;}
	s=${s//>/&gt;}
	s=${s//\\/\\\\}
	s=${s//|/\\|}
	s=${s//$'\t'/ }
	s=${s//$'\r'/}
	printf '%s' "${s//$'\n'/<br>}"
}

# Creates the passes directory, mode 0700 under a parent of the user's, and
# sets recording, or why_not when it cannot be written. Like the guide's log, a
# record goes only into a directory of the user's own.
recording=
why_not=
init_passes() {
	mkdir -p -- "$(dirname -- "$passes")" 2>/dev/null
	[ -e "$passes" ] || [ -L "$passes" ] || mkdir -m 0700 -- "$passes" 2>/dev/null
	if [ -L "$passes" ] || [ ! -d "$passes" ] || [ ! -O "$passes" ] || [ "$(stat -c %a -- "$passes")" != 700 ]; then
		why_not="$passes is a symlink, or not a directory of yours alone"
	elif [ ! -w "$passes" ]; then
		why_not="$passes cannot be written"
	else
		recording=1
	fi
}

# The pass's answers, by page index, and what the record says of the build.
answers=()
notes=()
extra=()
started=
da1=

# A new pass id: the time and the PID.
new_pass() {
	pass=$(date +%Y%m%dT%H%M%S)-$$
	record=$passes/$pass.md
	started=$(date -Iseconds)
	answers=()
	notes=()
	extra=()
}

# Writes the whole record of the pass, through a file beside it renamed into
# place, so a record is never seen half written.
write_record() {
	local i n=0 tmp f
	[ -n "$recording" ] || return 0
	for i in "${!pages[@]}"; do [ -z "${answers[$i]:-}" ] || n=$((n + 1)); done
	tmp=$passes/.$pass.md.new
	{
		printf '# vtium acceptance pass %s\n\n' "$pass"
		printf -- '- pass: %s\n' "$pass"
		printf -- '- label: %s\n' "$(one_line "$label")"
		printf -- '- vt: %s\n' "$(one_line "${ACCEPT_VT:-not given}")"
		printf -- '- binary: %s\n' "$(one_line "${ACCEPT_BINARY:-not given}")"
		printf -- '- features: %s\n' "$(one_line "${ACCEPT_FEATURES:-not given}")"
		printf -- '- terminal: TERM=%s, %s rows by %s columns at the start\n' "$(one_line "${TERM:-unset}")" "$start_rows" "$start_cols"
		printf -- '- da1: %s\n' "${da1:-no answer}"
		printf '%s\n' "${hardware_lines[@]}"
		printf -- '- hardware: %s; this pass is evidence about the hardware above only\n' "$hardware"
		printf -- '- font-start: %s\n' "${font_start:-no answer to the OSC 50 query}"
		printf -- '- blink-text: %s (%s); the pages that blink text are judged in it\n' "$blink_style" "$(one_line "$blink_source")"
		[ ${#font_lines[@]} -eq 0 ] || printf '%s\n' "${font_lines[@]}"
		printf -- '- fonts: %s\n' "$fonts"
		printf -- '- started: %s\n' "$started"
		printf -- '- updated: %s\n' "$(date -Iseconds)"
		printf -- '- answered: %d of %d\n\n' "$n" ${#pages[@]}
		printf '| page | answer | comment |\n|---|---|---|\n'
		for i in "${!pages[@]}"; do
			printf '| %s | %s | %s |\n' "$(entry_id "$i")" "${answers[$i]:--}" "$(cell "${notes[$i]:-}")"
		done
		for f in "${extra[@]}"; do printf '%s\n' "$f"; done
		printf '\n## What each page shows when it is right\n\n'
		for f in "${files[@]}"; do
			printf -- '- %s: %s\n' "$(id_of "$f")" "$(meta look-for "$f")"
		done
	} >"$tmp" 2>/dev/null && mv -f -- "$tmp" "$record" || {
		rm -f -- "$tmp"
		recording='' why_not="writing $record failed"
	}
}

# Records ANSWER for page I with the comment typed for it.
rate() {
	answers[$1]=$2
	notes[$1]=$note
	write_record
}

# Reads back the newest pass in the passes directory whose label is this
# run's, when it still has a page of this run's without an answer: sets pass,
# record, started, answers, notes and extra, and returns 0. feedback.sh
# finds the pass and reads its record; a comment comes back with its
# backslashes doubled and its line breaks as \n, which printf %b undoes.
resumable() {
	local newest k v comment i found
	newest=$("$here/feedback.sh" --dir "$passes" --newest "$label" 2>/dev/null) || return 1
	[ -n "$newest" ] || return 1
	answers=() notes=() extra=()
	while IFS=$'\t' read -r k v comment; do
		case $k in
		'#pass') pass=$v ;;
		'#started') started=$v ;;
		'#fonts') [ "$v" = "$fonts" ] || return 1 ;;
		'#hardware') [ "${v%%;*}" = "$hardware" ] || return 1 ;;
		'#'*) ;;
		*)
			found=
			for i in "${!pages[@]}"; do
				[ "$(entry_id "$i")" = "$k" ] || continue
				found=1
				[ "$v" = - ] || answers[$i]=$v
				notes[$i]=$(printf '%b' "$comment")
			done
			[ -n "$found" ] || [ "$v" = - ] || extra+=("| $k | $v | $(cell "$(printf '%b' "$comment")") |")
			;;
		esac
	done < <("$here/feedback.sh" --tsv "$newest")
	[[ $pass =~ ^[0-9T]+-[0-9]+$ ]] || return 1
	record=$passes/$pass.md
	for i in "${!pages[@]}"; do
		[ -n "${answers[$i]:-}" ] || return 0
	done
	return 1
}

prompt() {
	printf '\r\033[K\033[7m %s \033[0m' "$1"
}

# Reads a comment of any number of lines into note, below the page so the page
# stays in view. Ctrl-D on an empty line or a line holding only "." ends it.
# Ctrl-D reaches the line editor as a key, never as the end of the walk's
# input. The line editor shows what is typed only while the terminal's echo is
# on, so echo is on for the comment. A paste is comment text. The walk turns
# the terminal's bracketed paste off while the line editor reads, and the line
# editor turns it on again for itself when its enable-bracketed-paste setting
# allows, as it does by default; it then takes a paste into the line whole,
# keeping its first line when it has several, so a pasted "." line never ends
# the comment. With that setting off in the user's inputrc, the terminal sends
# a paste as plain text, never markers the line editor would take as junk: its
# lines are typed lines, and what follows a "." line among them reaches the
# answer prompt in the same write, where drain drops it. The line editor turns
# bracketed paste off when it returns a line, so the walk turns it on again
# after the comment.
read_comment() {
	local line lines=()
	prompt 'comment: end it with Ctrl-D or a line holding only .'
	printf '\n'
	[ -z "$bracketed" ] || printf '\033[?2004l' >/dev/tty
	stty echo </dev/tty 2>/dev/null
	while :; do
		if ! IFS= read -r -e -p '> ' line </dev/tty; then
			printf '\n'
			break
		fi
		[ "$line" != . ] || break
		lines+=("$line")
	done
	stty -echo </dev/tty 2>/dev/null
	[ -z "$bracketed" ] || printf '\033[?2004h' >/dev/tty
	note=$(printf '%s\n' "${lines[@]}")
}

ask_keys='y yes  n no  u unsure  s skip  c comment  r redraw  b back  q quit'

# Shows page I with its footer and reads keys until one answers the page, goes
# back or quits; go says which. When the footer does not fit below the page,
# it gets a screen of its own.
ask() {
	local i=$1 row kept
	note=${notes[$i]:-}
	while :; do
		read_size
		draw_page "$i"
		if [ "$status" -eq 77 ]; then
			unshown "$i"
			return
		fi
		footer_lines "$i"
		if ! row=$(cursor_row) || [ $((row + ${#foot[@]})) -gt "$rows" ]; then
			drain
			printf '\n\033[7m %s \033[0m' 'press a key for the questions on this page'
			read_key
			clear_screen
			printf '\033[1m[%d/%d] %s: questions\033[0m\n' $((i + 1)) ${#pages[@]} \
				"$(entry_name "$i")"
		fi
		printf '%s\n' "${foot[@]}"
		drain
		prompt "$ask_keys"
		while :; do
			read_key
			case $key in
			y | Y) rate "$i" yes; go=next; return ;;
			n | N) rate "$i" no; go=next; return ;;
			u | U) rate "$i" unsure; go=next; return ;;
			s | S) rate "$i" skip; go=next; return ;;
			c | C)
				kept=$note
				read_comment
				# An empty comment keeps the comment the page had, and
				# the page is drawn again.
				[ -n "$note" ] || { note=$kept; continue 2; }
				ask_answer "$i"
				go=next
				return
				;;
			# The page drawn again from the start, for a page whose keys
			# changed the terminal's state.
			r | R) continue 2 ;;
			b | B) [ "$i" -eq 0 ] || { go=back; return; } ;;
			q | Q) go=quit; return ;;
			esac
			prompt "$ask_keys"
		done
	done
}

# Page I said this build cannot show it: it is recorded as skip at once, with
# that note, and any key goes on.
unshown() {
	note=${unshown_why:-this build cannot show this page}
	rate "$1" skip
	printf '\n'
	drain
	prompt 'recorded as skip  any key: next page  b back  q quit'
	read_any_key
	case $key in
	b | B) [ "$1" -eq 0 ] && go=next || go=back ;;
	q | Q) go=quit ;;
	*) go=next ;;
	esac
}

# Records the comment of page I with the one-key answer that follows it.
ask_answer() {
	drain
	prompt 'y yes  n no  u unsure  s skip'
	while :; do
		read_key
		case $key in
		y | Y) rate "$1" yes; return ;;
		n | N) rate "$1" no; return ;;
		u | U) rate "$1" unsure; return ;;
		s | S) rate "$1" skip; return ;;
		esac
		prompt 'y yes  n no  u unsure  s skip'
	done
}

# The answers of this pass, then q.
summary() {
	local i id nt left=0
	reset_terminal
	clear_screen
	printf '\033[1mvtium acceptance pass %s\033[0m\n\n' "$pass"
	for i in "${!pages[@]}"; do
		id=$(entry_id "$i")
		nt=${notes[$i]:-}
		nt=${nt//$'\n'/ / }
		printf '  %-22s %-7s %s\n' "$id" "${answers[$i]:--}" "${nt:0:$((cols - 35))}"
		[ -n "${answers[$i]:-}" ] || left=$((left + 1))
	done
	echo
	if [ -n "$recording" ]; then
		[ "$left" -eq 0 ] || printf '  %d pages still to answer: the next run offers to resume this pass.\n' "$left"
		printf '  recorded in %s\n' "$record"
		printf '  tools/vt/feedback.sh prints it, and --compare sets two passes side by side\n\n'
	else
		printf '  \033[31mnot recorded: %s\033[0m\n\n' "$why_not"
	fi
	drain
	prompt 'q exit'
	while :; do
		read_key
		case $key in q | Q) return ;; esac
		prompt 'q exit'
	done
}

# The entries: each page, once in each frit its "# frits:" line names.
pages=()
frits=()
frit_dir=${ACCEPT_PAGES:-$here/sections}/frits
for f in "${ACCEPT_PAGES:-$here/sections}"/*.sh; do
	take=
	if [ $# -eq 0 ]; then
		take=1
	else
		for want in "$@"; do
			[ "$(name_of "$f")" != "$want" ] || take=1
		done
	fi
	[ -n "$take" ] || continue
	set_frits=$(meta frits "$f")
	if [ -z "$set_frits" ]; then
		pages+=("$f") frits+=("")
	else
		for fr in $set_frits; do
			pages+=("$f") frits+=("$fr")
		done
	fi
done
[ ${#pages[@]} -gt 0 ] || { echo "accept: no such page: $*" >&2; exit 2; }
# The page files, each once.
files=()
for f in "${pages[@]}"; do
	[ "${files[*]: -1}" = "$f" ] || files+=("$f")
done

[ -n "$noprompt" ] || init_passes
read_size
# The size the pass starts at, for the record; each page reads it again.
start_rows=$rows start_cols=$cols

# Echo stays off for the whole run, so neither a key typed between prompts nor
# the terminal's answer to cursor_row lands on a page. read_comment turns it on
# while a comment is typed.
#
# Line editing stays off too, as the C reference's demo learned. read -n1 turns
# it off only while it reads, and a Ctrl-D that arrives between two reads,
# while the page or a prompt is drawn, meets a line-editing terminal: it
# becomes an end of file, which the next read receives as a NUL byte and
# drops, and that read then waits for another key. Off for the whole run,
# Ctrl-D is a key whenever it arrives.
if [ -z "$nopage" ] && tty_mode=$(stty -g </dev/tty 2>/dev/null); then
	trap 'restore_font; reset_terminal; printf "\033[?2004l" >/dev/tty; stty "$tty_mode" </dev/tty' EXIT
	stty -echo -icanon min 1 time 0 </dev/tty
	# Pasted text comes bracketed, so the walk can tell it from keys (read_key).
	printf '\033[?2004h' >/dev/tty
	bracketed=1
fi

if [ -n "$noprompt" ]; then
	for i in "${!pages[@]}"; do
		draw_page "$i"
		[ "$status" -eq 77 ] || {
			footer_lines "$i"
			printf '%s\n' "${foot[@]}"
		}
		if [ "$i" -lt $((${#pages[@]} - 1)) ]; then
			page_end 'press a key for next, q to quit'
		else
			page_end 'end of the acceptance pass: press a key to finish'
		fi
	done
	[ -n "$nopage" ] || clear_screen
	exit 0
fi

# The terminal's DA1 answer, for the record: attribute 4 is sixel, which
# vtium offers only in a build with pictures.
da1=$(ask_terminal $'\033[c' c)
da1=${da1##*\?}
[[ $da1 =~ ^[0-9\;]+$ ]] || da1=
# The font to switch back to, and the frits' files, for the record.
font_start=$(query_font)
[[ $font_start =~ ^[[:alnum:]._+-]{1,64}$ ]] || font_start=
font_sums
hardware_scan
# The blink style the pages that blink text name, for the record.
blink_style

start=0
resume=
if [ -n "$recording" ] && resumable; then
	resume=1
	for i in "${!pages[@]}"; do
		[ -n "${answers[$i]:-}" ] || { start=$i; break; }
	done
fi

clear_screen
printf '\033[1mvtium acceptance pass\033[0m\n\n'
printf '  running   %s\n' "$label"
printf '  terminal  TERM=%s, %s rows by %s columns\n' "${TERM:-unset}" "$rows" "$cols"
if [ -n "$(small_mark)" ]; then
	printf '  \033[31mThe terminal is smaller than 80x25: a page may be cut, and its\n'
	printf '  questions moved to a screen of their own.\033[0m\n'
fi
[ $# -eq 0 ] || printf '  pages     named on the command line\n'
echo
printf '  Each page says below it what right and wrong look like. Answer each with\n'
printf '  one key: y looks right, n looks wrong, u unsure, s skip; c types a comment\n'
printf '  first, b goes back a page, q quits, and the pass resumes later.\n'
if [ -n "$recording" ]; then
	printf '  Passes are kept in %s.\n' "$passes"
else
	printf '  \033[31mThe pass is not recorded: %s.\033[0m\n' "$why_not"
fi
echo
names=()
for f in "${files[@]}"; do names+=("$(name_of "$f")"); done
n=${#pages[@]} per=$(((${#names[@]} + 3) / 4))
for ((r = 0; r < per; r++)); do
	line=
	for ((c = 0; c < 4; c++)); do
		i=$((c * per + r))
		[ "$i" -lt ${#names[@]} ] || continue
		printf -v cell '%-18s' "${names[$i]:0:18}"
		line+="  $cell"
	done
	printf '%s\n' "${line% }"
done
[ "$n" -eq ${#names[@]} ] || printf '  %d pages; the language pages are walked once in each font set.\n' "$n"
echo
if [ -n "$resume" ]; then
	k=0
	for i in "${!pages[@]}"; do [ -z "${answers[$i]:-}" ] || k=$((k + 1)); done
	printf '  Pass %s of this build stopped part way.\n\n' "$pass"
	prompt "r resume it at page $((start + 1)) ($k of $n answered)  n new pass  q quit"
	while :; do
		read_key
		case $key in
		r | R) break ;;
		n | N) resume='' start=0; break ;;
		q | Q) clear_screen; exit 0 ;;
		esac
	done
else
	prompt 'press a key to start the pass, q to quit'
	read_any_key
	case $key in q | Q) clear_screen; exit 0 ;; esac
fi
[ -n "$resume" ] || new_pass
write_record

i=$start
while [ "$i" -lt ${#pages[@]} ]; do
	ask "$i"
	case $go in
	next) i=$((i + 1)) ;;
	back) i=$((i - 1)) ;;
	quit) break ;;
	esac
done
summary
clear_screen
