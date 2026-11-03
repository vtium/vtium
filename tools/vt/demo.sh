#!/bin/bash
# The playtest (ADR-1501): the operator walks every page in sections/, in file
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
# recorded as skip. Every answer rewrites the pass's record, a markdown file
# named after the pass in the passes directory: PLAYTEST_DIR, else
# $XDG_STATE_HOME/vtium/playtest (~/.local/state/vtium/playtest). The record
# holds the build, the terminal, and each page's answer and comment; a pass
# quit part way keeps its answers, and the next run with the same label
# offers to resume it at its first unanswered page. feedback.sh prints a
# pass, lists them and compares two.
#
# It runs in any terminal as any user and needs nothing from vtium, so the same
# pages can be compared in vtium and in another terminal.
#
#   demo.sh [PAGE...]   PAGE is a file name without the number and .sh, such
#                       as sgr; with none, every page runs
#
# DEMO_LABEL names the terminal on the title page and in the record; run-vt.sh
# sets it to "vtium dev build <git describe>". DEMO_VT, DEMO_BINARY and
# DEMO_FEATURES, when set, name the VT, the binary and its features in the
# record. DEMO_NOPROMPT=1 shows the guidance, asks nothing and records
# nothing: any key goes to the next page and q quits. DEMO_NOPAGE=1 also waits
# for no key and leaves the last page on the screen, for checking the pages
# outside a VT. PLAYTEST_PAGES names another directory of pages, for the
# tests.
set -u
shopt -u patsub_replacement 2>/dev/null || true

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
nopage=${DEMO_NOPAGE:-}
noprompt=${DEMO_NOPROMPT:-$nopage}
passes=${PLAYTEST_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/vtium/playtest}
label=${DEMO_LABEL:-unlabelled}
pass=
record=
rows=25
cols=80
key=
note=
go=

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

# Reads one key into $key. An escape sequence (an arrow key, a late reply from
# the terminal) is read whole and returned as ESC, so its bytes are never taken
# for keys.
read_key() {
	IFS= read -rsn1 key </dev/tty || exit 0
	[ "$key" != $'\033' ] || drain
}

# Discards keys already typed, so a key pressed before a prompt appeared does
# not answer it.
drain() {
	while IFS= read -rsn1 -t 0.05 _ </dev/tty; do :; done
}

# Shows PROMPT in reverse video on the current line and waits for a key; q ends
# the walk.
page_end() {
	[ -z "$nopage" ] || return 0
	printf '\033[7m %s \033[0m' "$1"
	read_key
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
draw_page() {
	reset_terminal
	clear_screen
	printf '\033[1m[%d/%d] %s\033[0m\n\n' $(($1 + 1)) ${#pages[@]} "$(name_of "${pages[$1]}")"
	status=0
	DEMO_TOP=3 bash "${pages[$1]}" || status=$?
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
		printf '# vtium playtest pass %s\n\n' "$pass"
		printf -- '- pass: %s\n' "$pass"
		printf -- '- label: %s\n' "$(one_line "$label")"
		printf -- '- vt: %s\n' "$(one_line "${DEMO_VT:-not given}")"
		printf -- '- binary: %s\n' "$(one_line "${DEMO_BINARY:-not given}")"
		printf -- '- features: %s\n' "$(one_line "${DEMO_FEATURES:-not given}")"
		printf -- '- terminal: TERM=%s, %s rows by %s columns\n' "$(one_line "${TERM:-unset}")" "$rows" "$cols"
		printf -- '- da1: %s\n' "${da1:-no answer}"
		printf -- '- started: %s\n' "$started"
		printf -- '- updated: %s\n' "$(date -Iseconds)"
		printf -- '- answered: %d of %d\n\n' "$n" ${#pages[@]}
		printf '| page | answer | comment |\n|---|---|---|\n'
		for i in "${!pages[@]}"; do
			printf '| %s | %s | %s |\n' "$(id_of "${pages[$i]}")" "${answers[$i]:--}" "$(cell "${notes[$i]:-}")"
		done
		for f in "${extra[@]}"; do printf '%s\n' "$f"; done
		printf '\n## What each page shows when it is right\n\n'
		for i in "${!pages[@]}"; do
			printf -- '- %s: %s\n' "$(id_of "${pages[$i]}")" "$(meta look-for "${pages[$i]}")"
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
		'#'*) ;;
		*)
			found=
			for i in "${!pages[@]}"; do
				[ "$(id_of "${pages[$i]}")" = "$k" ] || continue
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
# on, so echo is on for the comment.
read_comment() {
	local line lines=()
	prompt 'comment: end it with Ctrl-D or a line holding only .'
	printf '\n'
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
	note=$(printf '%s\n' "${lines[@]}")
}

ask_keys='y yes  n no  u unsure  s skip  c comment  r redraw  b back  q quit'

# Shows page I with its footer and reads keys until one answers the page, goes
# back or quits; go says which. When the footer does not fit below the page,
# it gets a screen of its own.
ask() {
	local i=$1 row
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
				"$(name_of "${pages[$i]}")"
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
				read_comment
				# An empty comment changes nothing: the page again.
				[ -n "$note" ] || continue 2
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
	note='this build cannot show this page'
	rate "$1" skip
	printf '\n'
	drain
	prompt 'recorded as skip  any key: next page  b back  q quit'
	read_key
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
	printf '\033[1mvtium playtest: pass %s\033[0m\n\n' "$pass"
	for i in "${!pages[@]}"; do
		id=$(id_of "${pages[$i]}")
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

pages=()
for f in "${PLAYTEST_PAGES:-$here/sections}"/*.sh; do
	if [ $# -eq 0 ]; then
		pages+=("$f")
	else
		for want in "$@"; do
			[ "$(name_of "$f")" != "$want" ] || pages+=("$f")
		done
	fi
done
[ ${#pages[@]} -gt 0 ] || { echo "playtest: no such page: $*" >&2; exit 2; }

[ -n "$noprompt" ] || init_passes
read_size

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
	trap 'reset_terminal; stty "$tty_mode" </dev/tty' EXIT
	stty -echo -icanon min 1 time 0 </dev/tty
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
			page_end 'end of the playtest: press a key to finish'
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

start=0
resume=
if [ -n "$recording" ] && resumable; then
	resume=1
	for i in "${!pages[@]}"; do
		[ -n "${answers[$i]:-}" ] || { start=$i; break; }
	done
fi

clear_screen
printf '\033[1mvtium playtest\033[0m\n\n'
printf '  running   %s\n' "$label"
printf '  terminal  TERM=%s, %s rows by %s columns\n' "${TERM:-unset}" "$rows" "$cols"
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
n=${#pages[@]} per=$(((${#pages[@]} + 2) / 3))
for ((r = 0; r < per; r++)); do
	line=
	for ((c = 0; c < 3; c++)); do
		i=$((c * per + r))
		[ "$i" -lt "$n" ] || continue
		printf -v cell '%2d %-22s' $((i + 1)) "$(name_of "${pages[$i]}")"
		line+="  $cell"
	done
	printf '%s\n' "${line% }"
done
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
	read_key
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
