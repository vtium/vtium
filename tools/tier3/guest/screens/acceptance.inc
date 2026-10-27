# Shared by ADR-103's sixel acceptance screens (photos.sh, photo-scroll.sh,
# mpv-sixel.sh, tmux-grid.inc), which source it: the
# programs they need, the photograph, the clip, and the figures a measured
# screen reports. It draws nothing itself, and sources sixel-common.inc.
here=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
. "$here/sixel-common.inc"

# The photograph the pages draw, beside these scripts: a street at sunset,
# 2040x1536, the operator's own, licensed CC BY 4.0 (photo/LICENSE). It
# travels with the scripts, onto tier 3's disc and into the VT demo's
# sections, so nothing is installed for it.
photo_dir=$here/photo
photo_file=street-sunset.jpg photo_w=2040 photo_h=1536

# Exits with a message unless program $1 is installed; $2 is the Arch package
# that has it.
needs() {
	command -v "$1" >/dev/null && return
	printf '%s is not installed, so this page has nothing to show: install %s\n' "$1" "$2"
	printf 'and run the page again.\n'
	exit 0
}

# The width in cells, in w, of the photograph $1 rows tall with its shape
# kept, to the nearest cell, for the cell size cell_size gave.
photo_cols() {
	w=$((($1 * ch * photo_w + photo_h * cw / 2) / (photo_h * cw)))
}

# mpv with nothing of the user's: no config, no input from the terminal, no
# audio, no status line.
mpv_plain() {
	mpv --no-config --really-quiet --no-input-terminal --ao=null --no-audio "$@"
}

# Photograph $1 drawn by chafa, the operator's everyday sixel tool, at
# column $2 and row $3, counted from 1, as large as fits in $4 by $5 cells
# with its shape kept. chafa reads the cell size from the tty (TIOCGWINSZ);
# it asks the terminal nothing (--probe=off) and works on one thread, so its
# output is the same on every run.
chafa_photo() {
	printf '%s[%d;%dH' "$esc" "$3" "$2"
	chafa -f sixels --probe=off --polite=on --threads=1 --size="$4x$5" "$photo_dir/$1"
}

# Photograph $1 drawn through libsixel's encoder (mpv's sixel output) at
# column $2 and row $3, counted from 1, stretched to fill $4 by $5 cells
# (photo_cols keeps the photograph's shape to within half a cell); mpv rounds
# its height down to sixel's 6-pixel bands, so it can end a few pixels short
# of the last row. The
# picture stays on the screen as ordinary sixel content: mpv draws it on the
# main screen, clears nothing and exits after the one frame.
photo() {
	mpv_plain --vo=sixel --vo-sixel-alt-screen=no --vo-sixel-config-clear=no \
		--vo-sixel-left="$2" --vo-sixel-top="$3" \
		--vo-sixel-width=$(($4 * cw)) --vo-sixel-height=$(($5 * ch)) \
		--keepaspect=no --image-display-duration=0 "$photo_dir/$1"
}

# How many bytes img2sixel writes for a photograph: libsixel 1.10.5 reads
# images through gdk-pixbuf, and with gdk-pixbuf 2.44 it gets no picture and
# writes nothing, so the page says so rather than showing an empty space.
img2sixel_bytes() {
	img2sixel -w "$1" "$photo_dir/$photo_file" 2>/dev/null | wc -c
}

# The clip mpv plays: ffmpeg's testsrc2 pattern, generated as it plays, so
# nothing is downloaded or committed. $1 is its size, $2 its length in
# seconds; 30 frames per second.
clip() {
	printf 'av://lavfi:testsrc2=size=%s:rate=30:duration=%s' "$1" "$2"
}

# The kmscon process this terminal belongs to: the nearest ancestor named
# kmscon, so another console on the machine is never measured instead.
kmscon_pid() {
	local p=$$ comm
	while [ "$p" -gt 1 ]; do
		read -r comm <"/proc/$p/comm" || break
		[ "$comm" = kmscon ] && { echo "$p"; return; }
		p=$(awk '{print $4}' "/proc/$p/stat") || break
	done
	return 1
}

# CPU time of process $1 so far, all its threads, in clock ticks.
cpu_ticks() {
	awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo 0
}

# Seconds since boot, the clock that stamps kmscon's console lines.
now() {
	local t _
	read -r t _ </proc/uptime
	echo "$t"
}

# Runs "$@" as one measured window. Pauses keep the frames before and after
# out of the render timer's one-second intervals that overlap it. Sets
# start, end and kmscon_ms (kmscon's CPU time over the window, parsing
# included, in milliseconds).
measure() {
	local pid t0=0 t1=0
	pid=$(kmscon_pid) || pid=
	sleep 1.5
	[ -z "$pid" ] || t0=$(cpu_ticks "$pid")
	start=$(now)
	"$@"
	end=$(now)
	[ -z "$pid" ] || t1=$(cpu_ticks "$pid")
	kmscon_ms=$(((t1 - t0) * 1000 / $(getconf CLK_TCK)))
	[ -n "$pid" ] || kmscon_ms=
	sleep 1.5
}

# mpv's frame counts from the stats files mpv-stats.lua wrote ($@): frames
# shown, summed, and frames dropped, summed.
mpv_counts() {
	awk -F= '$1 == "frames" { f += $2 } $1 == "dropped" { d += $2 }
		END { printf "%d %d\n", f, d }' "$@" 2>/dev/null
}

# Reports the window to the tier 3 harness on its event port, which t3 names
# in T3_EVENTS, as "T3-BENCH NAME start=S end=E KEY=VALUE...". Anywhere
# else, as on the VT demo's pages, it prints the figures under the page, mpv's
# own counts and the frame rate they make first and in bold when the page
# played video (per pane, with panes=N), so the reviewer can set what mpv sent
# beside kmscon's rate in its corner (--render-timer=osd) and see which side
# was slow.
report() {
	local name=$1 kv frames= dropped= panes=1 kvs
	shift
	if [ -n "${T3_EVENTS:-}" ]; then
		echo "T3-BENCH $name start=$start end=$end $*" >"$T3_EVENTS"
		return
	fi
	read -ra kvs <<<"$*"
	for kv in "${kvs[@]}"; do
		case $kv in
		mpv_frames=*) frames=${kv#*=} ;;
		mpv_dropped=*) dropped=${kv#*=} ;;
		panes=*) panes=${kv#*=} ;;
		esac
	done
	if [ -n "$frames" ] && [ -n "$dropped" ]; then
		printf '\n%s[1mmpv sent %d frames to the terminal and dropped %d (%d%% sent): %s frames a second%s.%s[0m' \
			"$esc" "$frames" "$dropped" $((100 * frames / (frames + dropped > 0 ? frames + dropped : 1))) \
			"$(awk -v f="$frames" -v n="$panes" -v a="$start" -v b="$end" \
				'BEGIN { printf "%.1f", (b > a ? f / n / (b - a) : 0) }')" \
			"$( ((panes > 1)) && printf ' in each pane')" "$esc"
	fi
	printf '\nmeasured over %s seconds: %s\n' \
		"$(awk -v a="$start" -v b="$end" 'BEGIN { printf "%.1f", b - a }')" "$*"
}
