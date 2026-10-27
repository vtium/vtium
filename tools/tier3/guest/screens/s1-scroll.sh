#!/bin/bash
# S1 (ADR-103): text scrolling with no image, the baseline the image layer's
# scenarios are measured against. 500 000 lines of plain text (a block of
# 20 000 numbered lines, 25 times) go through the terminal as fast as kmscon
# takes them; the render timer's lines for that window are the variant's S1
# figures in the tier 3 report.
# look-for: Numbered lines of text, each whole on its own row, the last one
# look-for: 020000, then the prompt.
# wrong-looks-like: A line drawn over another, a row left blank or half drawn,
# wrong-looks-like: or a row left from an earlier screen.
# screen-text: none, as the screen shows only the last rows of what it prints.
block=20000 passes=25
text=$(mktemp)
awk -v n=$block 'BEGIN { for (i = 1; i <= n; i++)
	printf "%06d  The quick brown fox jumps over the lazy dog; pack my box with five dozen liquor jugs.\n", i }' \
	>"$text"
# The pauses keep the frames before and after (the clear, the next prompt) out
# of the render timer's one-second intervals that overlap the scroll.
sleep 1.5
read -r start _ </proc/uptime
for ((i = 0; i < passes; i++)); do
	cat "$text"
done
read -r end _ </proc/uptime
sleep 1.5
rm -f "$text"
# The window on the guest's clock, which also stamps kmscon's console lines.
[ -w /dev/ttyS1 ] && echo "T3-BENCH s1-scroll lines=$((block * passes)) start=$start end=$end" >/dev/ttyS1
exit 0
