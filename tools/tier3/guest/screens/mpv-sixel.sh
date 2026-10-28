#!/bin/bash
# kit: sixel
# look-for: For five seconds mpv plays a test pattern in a 1920x1080 window in
# look-for: the middle of the screen: color bars, a moving rainbow band and
# look-for: squares, a counter rising by 30 a second. Then this page comes back
# look-for: clean, mpv's counts in bold. With --render-timer=osd kmscon's show.
# look-for: mpv's full-screen sixel at 8K is limited by its encoder.
# wrong-looks-like: A frozen, torn or flickering picture, the counter jumping
# wrong-looks-like: in large steps (frames lost), pieces of the video left on
# wrong-looks-like: this page afterwards, or the page not coming back.
# screen-text: none, as mpv draws on the alternate screen
# ADR-103's acceptance scenario A2: mpv --vo=sixel playing an animation. The
# clip is ffmpeg's testsrc2 pattern, 640x360 at 30 frames per second for 5
# seconds, generated as mpv plays it, which mpv scales to a 1920x1080 window
# in the middle of the screen (the largest 16:9 window that fits, on a
# screen smaller than that) and encodes through libsixel without dithering;
# it draws on the alternate screen and leaves it when the clip ends. Full
# screen at 7680x2160, mpv's own encoding holds it to about 4.4 frames a
# second with no terminal attached, so the window keeps the measure on
# kmscon: at 1920x1080 without dithering mpv sends all 150 frames. Tier 3 measures the window: kmscon's render
# timer gives the frames presented, CPU time and damage per frame, and the
# script adds the frames mpv played and dropped (mpv-stats.lua) and kmscon's
# own CPU time over the window, parsing included. kmscon is not the
# bottleneck when it presents the frames mpv sends.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/acceptance.inc"
sixel_offered
needs mpv mpv
cell_size
term_size
# The window in pixels, w by h, and its top left cell.
w=1920 h=1080
if ((w > cols * cw || h > rows * ch)); then
	h=$((rows * ch < cols * cw * 9 / 16 ? rows * ch : cols * cw * 9 / 16))
	w=$((h * 16 / 9))
fi
left=$(((cols - w / cw) / 2 + 1)) top=$(((rows - h / ch) / 2 + 1))
stats=$(mktemp)
play() {
	mpv_plain --vo=sixel --vo-sixel-width="$w" --vo-sixel-height="$h" \
		--vo-sixel-left="$left" --vo-sixel-top="$top" --vo-sixel-dither=none \
		--script="$here/mpv-stats.lua" --script-opts=t3-stats="$stats" "$(clip 640x360 5)"
}
measure play
read -r frames dropped < <(mpv_counts "$stats")
rm -f "$stats"
printf 'mpv --vo=sixel played a 5-second test pattern, 30 frames a second, in a\n'
printf '%dx%d window, and returned to this page.\n' "$w" "$h"
report mpv-sixel "mpv_frames=$frames mpv_dropped=$dropped kmscon_cpu_ms=$kmscon_ms"
