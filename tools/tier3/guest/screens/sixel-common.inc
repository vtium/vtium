# Shared by the sixel screens (sixel.sh, sixel-ramps.sh, sixel-article.sh),
# which source it: the terminal queries, the cell size, and the pictures
# made from it. It draws nothing itself.
esc=$'\033'
# the sixel data characters, for the six-pixel columns 0 to 63
six='?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`abcdefghijklmnopqrstuvwxyz{|}~'

# The terminal's answer to a query, up to its final byte; empty when it does
# not answer within a second or this is not a terminal. Echo is off before
# the query goes out, so an answer that arrives at once is not printed.
ask() {
	local reply= mode
	if [ -t 0 ] && mode=$(stty -g </dev/tty 2>/dev/null); then
		stty -echo </dev/tty
		printf '%s' "$1" >/dev/tty
		IFS= read -rsd "$2" -t 1 reply </dev/tty
		stty "$mode" </dev/tty
	fi
	printf '%s' "$reply"
}

# Exits with a message unless the terminal offers sixel: DA1 attribute 4.
# The reply is shown with its ESC as ^[ and its final c, which read dropped:
# printed raw it is a control sequence, which swallowed the text after it.
# Under tier 3 the refusal is also reported on the event port.
sixel_offered() {
	local da1 shown
	da1=$(ask "$esc[c" c)
	if [[ ";${da1##*\?};" != *";4;"* ]]; then
		shown=${da1:+${da1//$esc/^[}c}
		# the screen's name first, so each screen's page differs from the last
		printf '%s: this terminal does not offer sixel graphics: its DA1 reply\n' \
			"$(basename "$0" .sh)"
		printf 'has no attribute 4 (%s), so no picture is sent.\n' "${shown:-no reply}"
		[ -n "$T3_EVENTS" ] && echo T3-NO-SIXEL >"$T3_EVENTS"
		exit 0
	fi
}

# The cell size in pixels, cw by ch, from the reply ESC [ 6 ; height ; width
# t, which rw and rh keep as given. Without an answer the pictures are made
# for 12x24 cells, and note says so for the page to print.
cell_size() {
	IFS=';' read -r _ rh rw <<<"$(ask "$esc[16t" t)"
	cw=$rw ch=$rh note=
	if [[ ! $cw =~ ^[1-9][0-9]*$ || ! $ch =~ ^[1-9][0-9]*$ ]]; then
		cw=12 ch=24
		note='XTWINOPS 16 gave no cell size, so the pictures are made for 12x24 cells.'
	fi
}

# The bits of the sixel band starting at row $1 that rows $2 to $3 cover, in m.
bits() {
	local lo=$(($2 - $1)) hi=$(($3 - $1))
	((lo < 0)) && lo=0
	((hi > 5)) && hi=5
	m=0
	((lo <= hi)) && m=$(((1 << (hi + 1)) - (1 << lo)))
}

# A picture $1 by $2 pixels of circles centred in it, the rest transparent.
# The arguments after the size are a color (r,g,b percentages, or none for
# a transparent hole) and a diameter each, the outermost circle first; a
# pixel takes the color of the innermost circle it is in. A pixel is in a
# circle of diameter d when (2x + 1 - w)^2 + (2y + 1 - h)^2 < d^2.
circles() {
	local w=$1 h=$2 s=$esc'P0;1q"1;1;'$1';'$2
	shift 2
	local n=0 top=() end=() rgb=() x j dx r t u y m0
	while (($#)); do
		rgb[n]=$1
		[ "$1" = none ] || s+="#$((n + 1));2;${1//,/;}"
		# each column's first and last row inside circle n
		for ((x = 0; x < w; x++)); do
			dx=$((2 * x + 1 - w))
			r=$(($2 * $2 - dx * dx - 1))
			top[n * w + x]=1 end[n * w + x]=0
			((r < 0)) && continue
			# t = floor(sqrt(r)), by Newton's method
			t=$r
			((r > 1)) && for ((u = (r + 1) / 2; u < t; u = (t + r / t) / 2)); do
				t=$u
			done
			top[n * w + x]=$(((h - t) / 2)) end[n * w + x]=$(((h + t - 1) / 2))
		done
		((n++))
		shift 2
	done
	for ((y = 0; y < h; y += 6)); do
		for ((j = 0; j < n; j++)); do
			[ "${rgb[j]}" = none ] && continue
			s+="#$((j + 1))"
			for ((x = 0; x < w; x++)); do
				bits $y ${top[j * w + x]} ${end[j * w + x]}
				if ((j + 1 < n)); then
					m0=$m
					bits $y ${top[(j + 1) * w + x]} ${end[(j + 1) * w + x]}
					m=$((m0 & ~m))
				fi
				s+=${six:m:1}
			done
			s+='$'
		done
		s+='-'
	done
	printf '%s' "$s$esc\\"
}

# An opaque sixel picture $1 by $2 pixels in one color (r,g,b percentages).
swatch() {
	local w=$1 h=$2 y s=$esc'Pq"1;1;'$1';'$2"#1;2;${3//,/;}"
	for ((y = 0; y < h; y += 6)); do
		bits $y 0 $((h - 1))
		s+="#1!$w${six:m:1}\$-"
	done
	printf '%s' "$s$esc\\"
}

# $2 cells of color $1 (r,g,b, 0 to 255) through the text path: spaces on
# an SGR 48;2 background.
block() {
	printf '%s[48;2;%sm%*s%s[0m' "$esc" "${1//,/;}" "$2" '' "$esc"
}


# The row the cursor is on, counted from 1, in top: DSR 6's reply ESC [ row
# ; col R, else 1. A page that places its pictures with CUP counts its rows
# from top, so it draws the same below the VT demo's header as on the clear
# screen tier 3 gives it.
top_row() {
	local reply
	reply=$(ask "$esc[6n" R)
	top=${reply##*[}
	top=${top%;*}
	[[ $top =~ ^[1-9][0-9]*$ ]] || top=1
}

# The terminal's size in cells, cols by rows: XTWINOPS 18's reply ESC [ 8 ;
# rows ; cols t, else the tty's own, else 80x24.
term_size() {
	IFS=';' read -r _ rows cols <<<"$(ask "$esc[18t" t)"
	if [[ ! $cols =~ ^[1-9][0-9]*$ || ! $rows =~ ^[1-9][0-9]*$ ]]; then
		read -r rows cols < <(stty size </dev/tty 2>/dev/null)
		[[ $cols =~ ^[1-9][0-9]*$ ]] || rows=24 cols=80
	fi
}

# The 8-bit value kmscon makes of a sixel percentage, rounded half up, as
# r,g,b: the color a text twin takes.
eight() {
	local IFS=, c out=
	for c in $1; do
		out+=${out:+,}$(((c * 255 + 50) / 100))
	done
	printf '%s' "$out"
}
