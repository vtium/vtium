# Shared by the showcase scenes (showcase-*.sh), which source it: the
# terminal's size, the font switch the fonts scene makes, the colors the
# scenes write, and the sender of kitty graphics commands. It draws nothing itself. The scenes are tier 3's variant
# showcase-1920x1080 (tools/tier3/t3_showcase.py, ADR-1502 D6); each writes
# fixed bytes, so every run draws the same screen.

# shellcheck shell=bash
# The scenes read here, showcase, rows and cols.
# shellcheck disable=SC2034
here=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
esc=$'\033'
showcase=$here/showcase

# The terminal's rows and columns, in rows and cols.
read -r rows cols < <(stty size </dev/tty 2>/dev/null || echo '27 120')

# 24-bit foreground and background colors, and the reset.
fg() { printf '%s[38;2;%d;%d;%dm' "$esc" "$1" "$2" "$3"; }
bg() { printf '%s[48;2;%d;%d;%dm' "$esc" "$1" "$2" "$3"; }
off() { printf '%s[0m' "$esc"; }

# The kitty command $1 with the base64 payload $2, in chunks of 4096.
kitty() {
	local data=$2 i more
	for ((i = 0; i < ${#data}; i += 4096)); do
		more=$((i + 4096 < ${#data}))
		if ((i == 0)); then
			printf '%s_G%s,m=%d;%s%s\\' "$esc" "$1" "$more" "${data:i:4096}" "$esc"
		else
			printf '%s_Gm=%d;%s%s\\' "$esc" "$more" "${data:i:4096}" "$esc"
		fi
	done
}

# The terminal's answer to QUERY, up to the byte FINAL, which is dropped;
# empty when none comes within 3 seconds. Echo is off while it waits.
ask() {
	local reply='' mode
	if mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '%s' "$1" >/dev/tty
		IFS= read -rsd "$2" -t 3 reply </dev/tty
		stty "$mode" </dev/tty
	fi
	printf '%s' "$reply"
}

# Switches the console to frit $1 with OSC 50, then makes a round trip to the
# terminal so the switch has been read, and asks OSC 50's query which font
# is in use, as the acceptance pass does (ADR-1501 D6). Under tier 3 a
# switch that did not take is reported on the event port, and the case
# fails; the screen says so either way. The terminal's size changes with its
# cell, so rows and cols are read again.
switch_font() {
	local now
	printf '%s]50;%s\a' "$esc" "$1" >/dev/tty
	ask "${esc}[6n" R >/dev/null
	now=$(ask "$esc]50;?"$'\a' $'\a')
	now=${now##*]50;}
	if [ "$now" != "$1" ]; then
		printf 'vtium did not switch to the font %s: OSC 50 named %s\n' "$1" "${now:-nothing}"
		[ -n "${T3_EVENTS:-}" ] && echo "T3-FONT-REFUSED $1 ${now:-none}" >"$T3_EVENTS"
	fi
	# shellcheck disable=SC2034
	read -r rows cols < <(stty size </dev/tty)
}
