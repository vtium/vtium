#!/bin/bash
# look-for: The words "the flood is over" on the top row, then the prompt.
# wrong-looks-like: Anything left of the pictures or their captions.
# screen-text: none, the screen is cleared after the flood it measures.
# A stream of sixel pictures for 4 seconds, measured by the render timer
# (#147). Each picture is the screen's width and 8 rows tall, in bands that
# never repeat a byte, and goes out 4 KB at a time with a pause after each
# part, as a video player's frames arrive: most of the terminal's input
# changes nothing on the screen until a picture's ST. A caption above counts
# the pictures. The tier 3 report judges the window: frames are presented
# while the stream flows, and kmscon does not draw what has not changed (a
# pass that finds nothing to draw counts as skipped; #139). Then the screen
# is cleared and says so.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/sixel-common.inc"
sixel_offered
cell_size
term_size

W=$((cols * cw))
H=$((8 * ch))
pic=$(mktemp)
awk -v w=$W -v h=$H 'BEGIN {
	printf "\033P0;1q\"1;1;%d;%d#1;2;0;60;100#1", w, h
	for (b = 0; b < h; b += 6) {
		if (b)
			printf "-"
		for (x = 0; x < w; x++)
			printf "%s", x % 2 ? "~" : "^"
	}
	printf "\033\\"
}' >"$pic"
parts=$(mktemp -d)
split -b 4096 "$pic" "$parts/p."

# The pauses keep the frames before and after (the clear, the next prompt) out
# of the render timer's one-second intervals that overlap the stream.
sleep 1.5
read -r start _ </proc/uptime
end=$start n=0
while awk -v s="$start" -v e="$end" 'BEGIN { exit !(e - s < 4) }'; do
	n=$((n + 1))
	printf '\033[1;1Hpicture %d\033[2;1H' "$n"
	for part in "$parts"/p.*; do
		cat "$part"
		sleep 0.002
	done
	read -r end _ </proc/uptime
done
sleep 1.5
rm -rf "$pic" "$parts"
printf '\033[H\033[2Jthe flood is over\n'
# The window on the guest's clock, which also stamps kmscon's console lines;
# its "lines" are the pictures sent.
[ -w /dev/ttyS1 ] && echo "T3-BENCH flood lines=$n start=$start end=$end" >/dev/ttyS1
exit 0
