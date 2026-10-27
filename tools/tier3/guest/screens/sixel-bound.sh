#!/bin/bash
# look-for: Under the heading, eight stripes 6 pixels tall across the whole
# look-for: width, red, green, blue, yellow and again, then the line below them.
# wrong-looks-like: No stripes and no line below the heading: kmscon stopped
# wrong-looks-like: parsing where its bound on one call's work cut the picture.
# screen-text: none, a sixel picture does not print as rows
# One sixel picture with more work in it than kmscon parses in one call:
# libtsm stops after about 16 million pixels of image work, and kmscon
# parses the rest of what it read on later turns of its event loop. The
# picture is 10000x48 pixels, each 6-pixel band painted 40 times over in
# turning colours, about 19 million pixels from 3.5 KB of output, all in
# one read; the picture appears only when its last byte is parsed. The
# line after it is parsed after the picture.
esc=$'\033'
s=$esc'Pq"1;1;10000;48#1;2;100;0;0#2;2;0;100;0#3;2;0;0;100#4;2;100;100;0'
for ((b = 0; b < 8; b++)); do
	for ((p = 1; p <= 40; p++)); do
		s+="#$(((p + b) % 4 + 1))!10000~\$"
	done
	s+='-'
done
printf 'One sixel with more work than kmscon parses at once:\n'
printf '%s%s\\\r\n' "$s" "$esc"
printf 'Parsed in order: this line comes after the picture.\n'
