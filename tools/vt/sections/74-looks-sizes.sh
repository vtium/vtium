#!/bin/bash
# look-for: Enter on a size tries it on the console for 10 seconds: the text
# look-for: redraws at that size. The console goes back to its size at the end
# look-for: of the 10 seconds, or on any key but k. k keeps the size until
# look-for: vtium-config quits, and the console then goes back.
# wrong-looks-like: Text that stays at one size after Enter, a console that
# wrong-looks-like: stays at the tried size after 10 seconds or after another
# wrong-looks-like: key, a size that k does not keep, or a console still at
# wrong-looks-like: that size after vtium-config quits.
# The Looks tab's live trial of a size: in a vtium console, Enter on a size
# in a look card's size list tries the size on the console itself, so you
# choose a size by what it looks like. vtium-config sends the console a
# request for the grid that the size gives (CSI 8 ; rows ; columns t), and
# puts the console's grid back when the 10 seconds end, on any key but k,
# and when it quits. vtium honours the request only while the console's
# vtium.conf turns allow-window-ops on, and vtium-config offers the trial
# only while the vtium.conf it is given turns it on too. vtium reads the
# key only when it starts, so the acceptance pass's own vtium.conf
# (tools/vt/config/vtium.conf) turns it on for every page. The page gives
# vtium-config a vtium.conf of its own (common.inc's looks_tui), and only
# this page's has the key. When the console's vtium.conf leaves the key
# off, the page cannot show.
#
# References:
# - ADR-1201 D4, "Sizes, live on a vtium console": Enter tries the
#   highlighted size as a timed trial, and k keeps it.
# - ADR-1201 D4, its note of 2026-11-21: with allow-window-ops off, the
#   console shows its own text at its current size, so there is nothing
#   to judge.
# shellcheck source=tools/vt/sections/common.inc
. "$(dirname "$(readlink -f "$0")")/common.inc"

# Whether the console's vtium.conf turns allow-window-ops on: the last line
# above its first section that sets the switch, allow-window-ops for on and
# no-allow-window-ops for off. A # starts a comment at the start of a line
# or after a blank. A file that is missing or unreadable is left to
# looks_tui, which checks the review run's stage.
console_window_ops() {
	awk '/^[[:space:]]*\[/ { exit }
		{ sub(/(^|[[:space:]])#.*/, ""); gsub(/[[:space:]]/, "") }
		$0 == "allow-window-ops" { on = 1 }
		$0 == "no-allow-window-ops" { on = 0 }
		END { exit !on }' "$1"
}
conf=${ACCEPT_VTIUM_CONF:-}
if [ -n "$conf" ] && [ -r "$conf" ] && ! console_window_ops "$conf"; then
	cannot_show "The console's vtium.conf leaves allow-window-ops off, so vtium ignores vtium-config's requests to try a size."
fi
looks_top=(allow-window-ops)
looks_tui \
	'The Looks tab tries a size from a look card'"'"'s size list on the console.' \
	'  1. Press 3 for the Looks tab. The look big is highlighted.' \
	'  2. Press Enter to open its card, then Down twice to the Size line.' \
	'  3. Press Enter for the size list, move to another size, and press Enter.' \
	'     The text redraws at that size. Wait 10 seconds: the console goes back.' \
	'  4. Press Enter on the Size line again, try a size, then press another key' \
	'     than k: the console goes back at once.' \
	'  5. Try a size once more and press k: the console keeps it. Then quit with' \
	'     q, and n if it asks. The console goes back to its size.'
