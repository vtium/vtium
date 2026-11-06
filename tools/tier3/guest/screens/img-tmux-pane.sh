#!/bin/bash
# kit: sixel
# look-for: For a few seconds tmux shows 4 panes, 2 by 2: the top left one
# look-for: plays the test pattern, the other three hold a line of text that
# look-for: never changes. Then this page, cleared, says what tmux showed.
# wrong-looks-like: "SIXEL IMAGE" placeholders in place of video, video in a
# wrong-looks-like: pane other than the top left, or pieces of video left on
# wrong-looks-like: this page afterwards.
# screen-text: none, as tmux draws on the alternate screen
# Per-pane damage (vtium/vtium#30): tmux in a 2 by 2 grid of panes, the top
# left one playing ffmpeg's test pattern through mpv --vo=sixel, which tmux
# decodes and draws again on vtium, and the other three still. Only the
# playing pane changes, so each frame vtium draws must damage that pane's
# pixels and no more. The screen reports its window on the event port as a
# benchmark, with the pane's size in pixels (pane_px), and the harness fails
# the case when the render timer's mean damage per frame, over the window or
# in any one second wholly inside it, is larger than the pane
# (t3_bench.pane_verdict). The window opens 2.5 s after
# tmux drew its panes and closes 2 s before tmux exits, so neither full
# redraw falls in a second of the render timer that overlaps it. The render
# timer's damage covers what a frame drew into its buffer, which is older by
# as many frames as the display keeps buffers, so the variant keeps two
# (guest/vtium-images.conf): each frame then counts the pane's changes of
# two frames, still inside the pane.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/acceptance.inc"
sixel_offered
needs mpv mpv
needs tmux tmux
cell_size
dir=$(mktemp -d)
sock=t3-$$
printf 'set -g status off\nset -as terminal-features ",*:sixel"\n' >"$dir/tmux.conf"
# Pane 0 waits for the signal, plays the clip, notes when it ended, and ends
# the session 2 s later; the other panes print a line and wait.
printf -v play '%q ' mpv --no-config --really-quiet --no-input-terminal --ao=null --no-audio \
	--vo=sixel --vo-sixel-dither=none --script="$here/mpv-stats.lua" \
	--script-opts=t3-stats="$dir/stats" "$(clip 320x180 4)"
printf -v ended '%q ' cp /proc/uptime "$dir/end"
printf -v quit '%q ' tmux -L "$sock" kill-server
tmux -L "$sock" -f "$dir/tmux.conf" new-session -d -s pane -x 80 -y 30 \
	"tmux -L $sock wait-for go; $play; $ended; sleep 2; $quit"
for i in 1 2 3; do
	tmux -L "$sock" split-window -t pane "printf 'pane %d: still\n' $i; exec sleep 60"
	tmux -L "$sock" select-layout -t pane tiled >/dev/null
done
tmux -L "$sock" select-pane -t pane:0.0
read -r pw ph < <(tmux -L "$sock" display -p -t pane:0.0 '#{pane_width} #{pane_height}')
(
	sleep 2.5
	cp /proc/uptime "$dir/start"
	tmux -L "$sock" wait-for -S go
) &
mkfifo "$dir/out"
grep -ao $'\eP[0-9;]*q' <"$dir/out" | wc -l >"$dir/sixels" &
script -q -E never -O "$dir/out" -c "$(printf '%q ' tmux -L "$sock" attach -t pane)"
wait
printf '\033\\\033[?1049l\033[?25h'
clear
printf '\033[3J'
read -r start _ <"$dir/start" || start=0
read -r end _ <"$dir/end" || end=0
read -r frames dropped < <(mpv_counts "$dir/stats")
read -r sixels <"$dir/sixels" || sixels=0
rm -rf "$dir"
printf 'tmux played the test pattern in the top left of 4 panes, 2 by 2,\n'
printf 'through mpv --vo=sixel, and returned to this page when it ended.\n'
report img-tmux-pane "mpv_frames=$frames mpv_dropped=$dropped tmux_sixels=$sixels pane_px=$((pw * cw * ph * ch))"
