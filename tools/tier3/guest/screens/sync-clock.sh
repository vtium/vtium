#!/bin/bash
# look-for: A reversed bar on the top row that reads "sync-clock: done",
# look-for: then the prompt.
# wrong-looks-like: A bar drawn in part, or rows left from different updates.
# screen-text: none, it redraws its rows in place with cursor moves.
# A program's own timer redrawing a status bar, a clock and a progress row,
# as a TUI does with no key pressed: every 4 ms for 5 seconds it sends one
# synchronized update (mode 2026) that redraws the three top rows. The
# updates come faster than the display refreshes, so the render timer's
# lines for the window show how many frames the console composes for them
# (ADR-1401 D2: output no key caused reaches the screen at most once a
# refresh). The bar's last update is the same in every run, so the capture
# judges only that the screen finished; the figures are reported, not
# judged.
cols=$(tput cols 2>/dev/null || echo 80)
updates=0

# One synchronized update: the bar, the clock and the progress row, in one
# write.
draw() {
	local bar=$1 clock=$2 progress=$3
	printf '\033[?2026h\033[1;1H\033[7m %-*.*s\033[0m\033[2;1H\033[K%s\033[3;1H\033[K%s\033[?2026l' \
		$((cols - 1)) $((cols - 1)) "$bar" "$clock" "$progress"
}

printf '\033[2J\033[H'
# A pipe nothing writes to, which read's timeout turns into the timer.
exec 3<> <(:)
# The pauses keep the frames before and after (the clear, the next prompt)
# out of the render timer's one-second intervals that overlap the window.
sleep 1.5
read -r start _ </proc/uptime
began=${EPOCHREALTIME/./}
while :; do
	now=${EPOCHREALTIME/./}
	((now - began >= 5000000)) && break
	updates=$((updates + 1))
	ms=$(((now - began) / 1000))
	fill=$((updates % (cols > 1 ? cols : 1)))
	printf -v progress '%*s' "$fill" ''
	draw "sync-clock: update $updates" "clock ${ms} ms" "${progress// /#}"
	read -r -t 0.004 -u 3 _
done
read -r end _ </proc/uptime
draw "sync-clock: done" "" ""
printf '\033[4;1H'
exec 3<&-
sleep 1.5
# The window on the guest's clock, which also stamps the console's lines.
[ -w /dev/ttyS1 ] && echo "T3-BENCH sync-clock start=$start end=$end updates=$updates" >/dev/ttyS1
exit 0
