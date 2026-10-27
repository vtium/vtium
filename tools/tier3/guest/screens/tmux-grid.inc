# ADR-103's acceptance scenario A4, shared by tmux-2x2.sh and tmux-3x3.sh,
# which source it after setting n: tmux in an n by n grid of panes, each
# playing the test pattern through mpv --vo=sixel without dithering, which
# spares mpv's encoder (as in mpv-sixel). tmux decodes each pane's
# sixel pictures and draws them again on kmscon, so n * n streams and
# placements change at once. tmux runs on a server of its own (-L), with a
# config that names kmscon's sixel support: tmux turns sixel on for a
# terminal only through terminal-features, and draws a "SIXEL IMAGE"
# placeholder without it; the screen counts the pictures tmux sends to
# kmscon, and fails when there are none. When every pane's clip ends its
# pane closes and tmux exits; the page then leaves the alternate screen
# itself and must come back with nothing of the videos left.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/acceptance.inc"
sixel_offered
needs mpv mpv
needs tmux tmux
dir=$(mktemp -d)
sock=t3-$$
printf 'set -g status off\nset -as terminal-features ",*:sixel"\n' >"$dir/tmux.conf"
grid() {
	local i play
	for ((i = 0; i < n * n; i++)); do
		# Every pane waits for the same signal, so the clips play together.
		# Paths go into tmux's shell command quoted with %q.
		local wait=(tmux -L "$sock" wait-for go)
		local mpv=(mpv --no-config --really-quiet --no-input-terminal --ao=null --no-audio
			--vo=sixel --vo-sixel-dither=none
			--script="$here/mpv-stats.lua" --script-opts=t3-stats="$dir/$i"
			"$(clip 320x180 5)")
		printf -v play '%q ' "${wait[@]}"
		play+="; $(printf '%q ' "${mpv[@]}")"
		if ((i == 0)); then
			tmux -L "$sock" -f "$dir/tmux.conf" new-session -d -s grid "$play"
		else
			tmux -L "$sock" split-window -t grid "$play"
			tmux -L "$sock" select-layout -t grid tiled >/dev/null
		fi
	done
	(sleep 1; tmux -L "$sock" wait-for -S go) &
	# The client runs under script, which passes its output on to this
	# terminal unchanged and copies it to a pipe where the sixel pictures
	# tmux sends (DCS introducers, ESC P ... q) are counted: a tmux that
	# draws "SIXEL IMAGE" placeholders sends none, though kmscon still
	# presents as many frames.
	mkfifo "$dir/out"
	grep -ao $'\eP[0-9;]*q' <"$dir/out" | wc -l >"$dir/sixels" &
	script -q -E never -O "$dir/out" -c "$(printf '%q ' tmux -L "$sock" attach -t grid)"
	wait
}
measure grid
# tmux, exiting, drops the output it still holds, which can cut a sixel
# picture off inside its DCS, and gives up its restore sequences (cursor
# shown, alternate screen left) when the pty is full (#154). So the page
# closes any DCS left open, leaves the alternate screen, shows the cursor
# and clears the screen and the scrollback itself, and what follows does not
# depend on what tmux managed to write.
printf '\033\\\033[?1049l\033[?25h'
clear
printf '\033[3J'
read -r frames dropped < <(mpv_counts "$dir"/[0-9]*)
read -r sixels <"$dir/sixels" || sixels=0
rm -rf "$dir"
printf 'tmux played the test pattern in %d panes, %d by %d, each through\n' $((n * n)) "$n" "$n"
printf 'mpv --vo=sixel, and returned to this page when they closed.\n'
report "tmux-${n}x$n" "mpv_frames=$frames mpv_dropped=$dropped kmscon_cpu_ms=$kmscon_ms panes=$((n * n)) tmux_sixels=$sixels"
