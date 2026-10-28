#!/bin/bash
# kit: sixel
# look-for: A short article: a bold, double-underlined title, a paragraph whose
# look-for: words are styled as they describe themselves, a landscape centred
# look-for: on its own rows, a paragraph about it, and a bar chart at the left
# look-for: with the last paragraph wrapping beside it, then at full width below
# look-for: it. No text under or through a picture; every line ends by the
# look-for: column the first paragraph names, with no word cut.
# wrong-looks-like: No pictures (stock kmscon has no sixel; the page says so),
# wrong-looks-like: text under or through a picture, a ragged or overlong right
# wrong-looks-like: edge, a style unlike the words that name it, or a sentence
# wrong-looks-like: about a picture's size, place or colors that is not true.
# screen-text: none, sixel pictures and cursor moves do not print as rows
# A page of prose with two sixel pictures, as a blog post lays them out: one
# on rows of its own between paragraphs, one floated left with a paragraph
# wrapping beside it. The script wraps the text itself, to the terminal's
# columns (XTWINOPS 18) up to 100, the measure of a page of prose, counting
# each character's cells: two for a wide character or an emoji, none for a
# combining mark or an escape sequence. The prose describes the page as it
# is drawn: each styled phrase names its own style (bold, italic, faint,
# the five underline styles, a colored underline, strike-through, reverse
# video, 256-color and truecolor), and the sentences about each picture
# give its size and place in cells and pixels, computed from the cell size
# the terminal reports (XTWINOPS 16), and its colors. The pictures are made
# here at run time in whole cells: a landscape 40 cells by 6 rows (narrower
# on a narrow screen) and a bar chart 12 cells by 5 rows. The last line is
# the cell size the terminal reports.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/sixel-common.inc"
sixel_offered
cell_size
term_size
# characters are counted as code points, whatever the caller's locale
export LC_ALL=C.UTF-8
shopt -s extglob

# The cells a word shows, in wd: its characters, less escape sequences and
# combining marks (and the zero-width joiner and VS16), plus one more for
# each wide character or emoji.
zero=$'̀-ͯ‍️'
wide=$'ᄀ-ᅟ⺀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦\U0001f300-\U0001faff\U00020000-\U0003fffd'
width() {
	local t=${1//$esc\[*([0-9;:])m/} w
	t=${t//[$zero]/}
	w=${t//[!$wide]/}
	wd=$((${#t} + ${#w}))
}

# Prints a paragraph wrapped to W columns. Its first $1 lines start $2
# columns in and are that much narrower, beside a picture floated left;
# the rest run the full width. nl is the number of lines printed. The
# styles open at a line's end are closed there and opened again on the next
# line, so the indent and the line ends carry none.
flow() {
	local rows=$1 ind=$2 words w line= lw=0 open= start= e rest
	nl=0
	read -ra words <<<"$3"
	for w in "${words[@]}"; do
		width "$w"
		if ((lw && lw + 1 + wd > W - (nl < rows ? ind : 0))); then
			emit
			line= lw=0 start=$open
		fi
		((lw)) && line+=' ' && ((lw++))
		line+=$w
		((lw += wd))
		# the styles open after this word
		rest=$w
		while [[ $rest =~ ($esc\[[0-9\;:]*m)(.*) ]]; do
			e=${BASH_REMATCH[1]} rest=${BASH_REMATCH[2]}
			[ "$e" = "$esc[0m" ] && open= || open+=$e
		done
	done
	((lw)) && emit
	return 0
}
emit() {
	((nl < rows && ind)) && printf '%s[%dC' "$esc" "$ind"
	printf '%s%s%s[0m\n' "$start" "$line" "$esc"
	((nl++))
}

# t = floor(sqrt($1)), by Newton's method.
isqrt() {
	local r=$1 u
	t=$r
	((r > 1)) && for ((u = (r + 1) / 2; u < t; u = (t + r / t) / 2)); do
		t=$u
	done
	return 0
}

# An opaque sixel picture $1 by $2 pixels on color $3, of the layers in
# rgb, top and end: layer k is color rgb[k] over rows top[k*w+x] to
# end[k*w+x] of each column x, painted back to front, the last on top.
paint() {
	local w=$1 h=$2 s=$esc'Pq"1;1;'$1';'$2"#0;2;${3//,/;}" k x y lo hi m c prev n ymin ymax
	for k in "${!rgb[@]}"; do
		s+="#$((k + 1));2;${rgb[k]//,/;}"
	done
	for ((y = 0; y < h; y += 6)); do
		for k in "${!rgb[@]}"; do
			# a layer clear of this band adds nothing
			((ylo[k] > y + 5 || yhi[k] < y)) && continue
			s+="#$((k + 1))"
			prev= n=0
			for ((x = 0; x < w; x++)); do
				lo=$((top[k * w + x] - y)) hi=$((end[k * w + x] - y))
				((lo < 0)) && lo=0
				((hi > 5)) && hi=5
				((lo <= hi)) && m=$(((1 << (hi + 1)) - (1 << lo))) || m=0
				c=${six:m:1}
				if [ "$c" = "$prev" ]; then
					((n++))
				else
					((n > 3)) && s+="!$n$prev" || for ((; n; n--)); do s+=$prev; done
					prev=$c n=1
				fi
			done
			((n > 3)) && s+="!$n$prev" || for ((; n; n--)); do s+=$prev; done
			s+='$'
		done
		s+='-'
	done
	printf '%s' "$s$esc\\"
}

# Layer k as rows $2 to $3 of every column from $4 to $5 (or all of them).
rect() {
	local k=$1 x
	for ((x = 0; x < w; x++)); do
		if ((x >= ${4:-0} && x <= ${5:-w})); then
			top[k * w + x]=$2 end[k * w + x]=$3
		else
			top[k * w + x]=1 end[k * w + x]=0
		fi
	done
	ylo[k]=$2 yhi[k]=$3
}

# The landscape: a sky in three bands paling downward, a sun at the upper
# right, a mountain at the left and a hill across the bottom.
landscape() {
	local w=$1 h=$2 x dx r cx cy d px py bw hx y
	rgb=(10,20,50 20,40,70 50,70,95 100,85,15 45,45,50 15,50,20) top=() end=() ylo=() yhi=()
	rect 0 0 $((h / 3 - 1))
	rect 1 $((h / 3)) $((2 * h / 3 - 1))
	rect 2 $((2 * h / 3)) $((h - 1))
	# the sun, of diameter d about (cx, cy), in doubled coordinates
	cx=$((2 * w * 78 / 100)) cy=$((2 * h * 30 / 100)) d=$((h * 36 / 100))
	ylo[3]=$h yhi[3]=0
	for ((x = 0; x < w; x++)); do
		dx=$((2 * x + 1 - cx))
		r=$((d * d - dx * dx - 1))
		top[3 * w + x]=1 end[3 * w + x]=0
		((r < 0)) && continue
		isqrt $r
		top[3 * w + x]=$(((cy - t) / 2)) end[3 * w + x]=$(((cy + t - 1) / 2))
		((top[3 * w + x] < ylo[3])) && ylo[3]=${top[3 * w + x]}
		((end[3 * w + x] > yhi[3])) && yhi[3]=${end[3 * w + x]}
	done
	# the mountain: a peak at (px, py), its sides reaching the bottom bw
	# either side of it
	px=$((w * 33 / 100)) py=$((h * 22 / 100)) bw=$((w * 30 / 100))
	# the hill: a parabola from 62% down at hx to the bottom
	hx=$((w * 72 / 100))
	for ((x = 0; x < w; x++)); do
		dx=$((x > px ? x - px : px - x))
		top[4 * w + x]=$((py + dx * (h - py) / bw)) end[4 * w + x]=$((h - 1))
		top[5 * w + x]=$((h * 62 / 100 + (x - hx) * (x - hx) * (h * 30 / 100) / (w * w * 9 / 16)))
		end[5 * w + x]=$((h - 1))
	done
	ylo[4]=$py yhi[4]=$((h - 1)) ylo[5]=$((h * 62 / 100)) yhi[5]=$((h - 1))
	paint $w $h 0,0,0
}

# The chart: five bars rising left to right on white, above a black line.
chart() {
	local w=$1 h=$2 k base mt f
	rgb=(90,15,15 100,50,0 95,85,10 15,65,25 15,35,85 0,0,0) top=() end=() ylo=() yhi=()
	base=$((h - ch / 4)) mt=$((ch / 4))
	f=(30 48 64 80 94)
	for k in 0 1 2 3 4; do
		rect $k $((base - f[k] * (base - mt) / 100)) $((base - 1)) \
			$(((1 + 2 * k) * cw + cw / 4)) $(((3 + 2 * k) * cw - cw / 4 - 1))
	done
	rect 5 $base $((base + (ch > 20 ? ch / 10 : 2) - 1)) $((cw / 2)) $((w - cw / 2 - 1))
	paint $w $h 100,100,100
}

W=$((cols < 100 ? cols : 100))
lw=$((W - 4 < 40 ? W - 4 : 40)) lh=6 lx=$(((W - lw) / 2))
cwid=12 crow=5

X=$esc'[0m'
B=$esc'[1m' F=$esc'[2m' I=$esc'[3m' V=$esc'[7m' S=$esc'[9m'
U1=$esc'[4:1m' U2=$esc'[4:2m' U3=$esc'[4:3m' U4=$esc'[4:4m' U5=$esc'[4:5m'
UR=$esc'[4:3;58:2::255:0:0m'
O8=$esc'[38;5;208m' OT=$esc'[38;2;255;128;0;48;2;0;0;128m'
cafe=cafe$'́'

printf '%s%sHow this page is drawn%s\n\n' "$B" "$U2" "$X"
flow 0 0 "This is a short article drawn by a shell script, which wraps every line \
itself to $W columns of this $cols-column screen, counting the cells each character \
shows rather than its bytes or escape codes. This word is ${B}bold$X, this one is \
${I}italic$X, and this one is ${F}faint$X. These words are ${U1}underlined once$X, \
${U2}underlined twice$X, ${U3}underlined with a curly line$X, ${U4}underlined with \
dots$X and ${U5}underlined with dashes$X, and ${UR}this curly underline is red$X. \
${S}These words are struck through.$X ${V}This phrase is in reverse video.$X \
${O8}This phrase is orange from the 256-colour palette$X, and ${OT}this one is \
orange in truecolour on dark blue$X. A wide word, 漢字, takes two cells for each \
of its two characters; the e in $cafe carries a combining accent that takes no \
cell of its own; and this emoji, 😀, takes two cells."
printf '\n'
# the landscape on rows of its own: drawn from the saved cursor, then the
# rows it covers left blank
printf '%s7' "$esc"
((lx)) && printf '%s[%dC' "$esc" $lx
landscape $((lw * cw)) $((lh * ch))
printf '%s8' "$esc"
for ((i = 0; i < lh; i++)); do printf '\n'; done
printf '\n'
flow 0 0 "The picture above is a sixel landscape $lw cells wide and $lh rows tall, \
$((lw * cw)) by $((lh * ch)) pixels at this terminal's $cw by $ch pixel cells, \
centred on rows of its own between the first paragraph and this one, from column \
$((lx + 1)) to column $((lx + lw)): a deep blue sky that pales toward the horizon, \
a yellow sun at the upper right, a grey mountain on the left and a green hill \
across the bottom. No text runs beside it or under it."
printf '\n%s7' "$esc"
chart $((cwid * cw)) $((crow * ch))
printf '%s8' "$esc"
flow $crow $((cwid + 2)) "The chart to the left is a second sixel, $cwid cells \
wide and $crow rows tall, $((cwid * cw)) by $((crow * ch)) pixels: five bars \
rising from left to right in red, orange, yellow, green and blue on white, above a \
black line. This paragraph starts beside it, at column $((cwid + 3)), and wraps in \
the $((W - cwid - 2)) columns to its right for as long as the chart lasts, with two \
blank columns between the chart and the text. Once the text passes the chart's \
last row it returns to the left edge and runs the full $W columns. Every line, \
beside the chart or below it, ends at or before column $W, and no word is cut in \
two."
for ((i = nl; i < crow; i++)); do printf '\n'; done
printf '\n'
[ "$note" ] && printf '%s\n' "$note"
printf 'Cells of %sx%s pixels, as XTWINOPS 16 reports them.\n' "${rw:-?}" "${rh:-?}"
