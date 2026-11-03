#!/bin/bash
# Reads the playtest's pass records (ADR-1501), the markdown files demo.sh
# writes: a list of what the pass ran on, then a table of each page's answer
# (yes, no, unsure, skip, or - for none yet) and comment.
#
#   feedback.sh [--dir DIR] [PASS]          one pass: its build, then each page's
#                                           answer and comment (default: the newest)
#   feedback.sh [--dir DIR] --list          every pass, oldest first
#   feedback.sh [--dir DIR] --compare A B   the pages whose answer differs from
#                                           pass A to pass B; exits 1 when a page
#                                           went from yes to no
#   feedback.sh [--dir DIR] --compare       the same for the two newest passes
#   feedback.sh [--dir DIR] --newest LABEL  the path of the newest pass of LABEL
#   feedback.sh --tsv PASS                  the pass as tab-separated lines
#
# PASS is a pass's id in DIR, or the path of a record, such as one committed
# under docs/playtest/. DIR is PLAYTEST_DIR, else
# $XDG_STATE_HOME/vtium/playtest (~/.local/state/vtium/playtest), where
# demo.sh keeps the passes. The newest pass is the one started last.
set -euo pipefail

usage() {
	sed -n '6,15s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'feedback: %s\n' "$*" >&2
	exit 2
}

dir=${PLAYTEST_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/vtium/playtest}
mode=show
args=()
while [ $# -gt 0 ]; do
	case $1 in
	--dir) [ $# -ge 2 ] || usage; dir=$2; shift ;;
	--list) mode=list ;;
	--compare) mode=compare ;;
	--newest) [ $# -ge 2 ] || usage; mode=newest; args+=("$2"); shift ;;
	--tsv) mode=tsv ;;
	-*) usage ;;
	*) args+=("$1") ;;
	esac
	shift
done

# A record as tab-separated lines: "#KEY value" for each line of its list,
# then "PAGE answer comment" for each row of its table, the comment with its
# backslashes doubled and each line break as \n. The cells' escapes, as
# demo.sh writes them, are undone: \\ and \| in the comment, then <br>, &lt;,
# &gt; and &amp;.
tsv() {
	awk '
	function unescape(s,   out, c, i) {
		out = ""
		for (i = 1; i <= length(s); i++) {
			c = substr(s, i, 1)
			if (c == "\\" && i < length(s)) c = substr(s, ++i, 1)
			out = out c
		}
		gsub(/<br>/, "\n", out)
		gsub(/&lt;/, "<", out)
		gsub(/&gt;/, ">", out)
		gsub(/&amp;/, "\\&", out)
		return out
	}
	# the cells of a row "| a | b | c |", split at each | that no backslash
	# escapes
	function cells(line, out,   n, i, c, cur) {
		n = 0
		cur = ""
		for (i = 2; i <= length(line); i++) {
			c = substr(line, i, 1)
			if (c == "\\" && i < length(line)) { cur = cur c substr(line, ++i, 1); continue }
			if (c == "|") { sub(/^ +/, "", cur); sub(/ +$/, "", cur); out[++n] = cur; cur = ""; continue }
			cur = cur c
		}
		return n
	}
	function field(s) {
		gsub(/\\/, "\\\\", s)
		gsub(/\n/, "\\n", s)
		gsub(/\t/, " ", s)
		return s
	}
	/^\| page \| answer \| comment \|$/ { table = 1; next }
	table && /^\|---/ { next }
	table && /^\| / {
		if (cells($0, c) >= 3) printf "%s\t%s\t%s\n", c[1], c[2], field(unescape(c[3]))
		next
	}
	table { table = 0; done = 1 }
	!table && !done && match($0, /^- [a-z0-9-]+: /) {
		key = substr($0, 3, RLENGTH - 4)
		printf "#%s\t%s\n", key, substr($0, RLENGTH + 1)
	}' "$1"
}

# The path of PASS: a record's path, or an id in the passes directory.
path_of() {
	if [ -f "$1" ]; then
		printf '%s' "$1"
	elif [[ $1 =~ ^[0-9T]+-[0-9]+$ ]] && [ -f "$dir/$1.md" ]; then
		printf '%s' "$dir/$1.md"
	else
		die "no pass $1 in $dir, and no such record"
	fi
}

# The records in the passes directory, oldest first: a pass id sorts by the
# time it started.
records() {
	local f
	for f in "$dir"/*.md; do
		if [[ -f $f && ${f##*/} =~ ^[0-9T]+-[0-9]+\.md$ ]]; then printf '%s\n' "$f"; fi
	done | sort
}

# An awk function that prints a comment from a tab-separated line, one
# indented line for each line typed, after PREFIX: \\ is a backslash and \n
# a line break.
lines='
function lines(prefix, s,   i, c, l) {
	l = ""
	for (i = 1; i <= length(s); i++) {
		c = substr(s, i, 1)
		if (c == "\\" && i < length(s)) {
			c = substr(s, ++i, 1)
			if (c == "n") { printf "    %s%s\n", prefix, l; l = ""; continue }
		}
		l = l c
	}
	printf "    %s%s\n", prefix, l
}'

# The value of KEY in a record's tab-separated lines on stdin.
value() {
	awk -F'\t' -v k="#$1" '$1 == k { print $2; exit }'
}

case $mode in
tsv)
	[ ${#args[@]} -eq 1 ] || usage
	tsv "$(path_of "${args[0]}")"
	;;
newest)
	found=
	while IFS= read -r f; do
		[ "$(tsv "$f" | value label)" != "${args[0]}" ] || found=$f
	done < <(records)
	[ -n "$found" ] || exit 1
	printf '%s\n' "$found"
	;;
list)
	[ ${#args[@]} -eq 0 ] || usage
	[ -n "$(records)" ] || die "no passes in $dir"
	printf '%-24s %-10s %s\n' pass answered label
	while IFS= read -r f; do
		t=$(tsv "$f")
		printf '%-24s %-10s %s\n' "$(value pass <<<"$t")" "$(value answered <<<"$t" | sed 's/ of /\//')" \
			"$(value label <<<"$t")"
	done < <(records)
	;;
show)
	[ ${#args[@]} -le 1 ] || usage
	if [ ${#args[@]} -eq 1 ]; then
		f=$(path_of "${args[0]}")
	else
		f=$(records | tail -n 1)
		[ -n "$f" ] || die "no passes in $dir"
	fi
	printf 'pass      %s\n' "$(tsv "$f" | value pass)"
	tsv "$f" | awk -F'\t' "$lines"'
	/^#/ { if ($1 != "#pass") printf "%-10s%s\n", substr($1, 2), $2; next }
	{
		if (!rows++) printf "\n%-24s  %s\n", "page", "answer"
		printf "%-24s  %s\n", $1, $2
		if ($3 != "") lines("", $3)
		if ($2 == "-") left[++nl] = $1
		if ($2 == "skip") skipped[++ns] = $1
	}
	END {
		if (nl) { printf "\nstill to answer\n"; for (i = 1; i <= nl; i++) printf "  %s\n", left[i] }
		if (ns) { printf "\nskipped\n"; for (i = 1; i <= ns; i++) printf "  %s\n", skipped[i] }
	}'
	echo "record    $f"
	;;
compare)
	case ${#args[@]} in
	0)
		mapfile -t two < <(records | tail -n 2)
		[ ${#two[@]} -eq 2 ] || die "--compare needs two passes; $dir has ${#two[@]}"
		a=${two[0]} b=${two[1]}
		;;
	2) a=$(path_of "${args[0]}") b=$(path_of "${args[1]}") ;;
	*) usage ;;
	esac
	{
		tsv "$a" | sed 's/^/A\t/'
		tsv "$b" | sed 's/^/B\t/'
	} | awk -F'\t' "$lines"'
	$2 == "#font" || $2 == "#frit" { fonts[$1, $3] = 1; order_f[++nf] = $1 SUBSEP $3; next }
	$2 ~ /^#/ { meta[$1, substr($2, 2)] = $3; next }
	{
		if (!(($2) in seen)) { seen[$2] = 1; order[++n] = $2 }
		answer[$1, $2] = $3
		note[$1, $2] = $4
		has[$1, $2] = 1
	}
	END {
		for (s = 1; s <= 2; s++) {
			side = s == 1 ? "A" : "B"
			printf "%s  %s  %s, %s answered\n", side, meta[side, "pass"], meta[side, "label"], meta[side, "answered"]
		}
		printf "\n%-24s  %-7s  %-7s\n", "page", "A", "B"
		for (i = 1; i <= n; i++) {
			p = order[i]
			ra = has["A", p] ? answer["A", p] : "(none)"
			rb = has["B", p] ? answer["B", p] : "(none)"
			if (ra == rb) continue
			changed++
			mark = ""
			if (ra == "yes" && rb == "no") { mark = "  <- went from yes to no"; worse[++nw] = p }
			if (!has["A", p]) mark = "  (a page new in B)"
			if (!has["B", p]) mark = "  (a page B does not have)"
			printf "%-24s  %-7s  %-7s%s\n", p, ra, rb, mark
			if (note["A", p] != "") lines("A: ", note["A", p])
			if (note["B", p] != "") lines("B: ", note["B", p])
		}
		if (!changed) printf "(every page has the same answer in both)\n"
		for (i = 1; i <= nf; i++) {
			split(order_f[i], k, SUBSEP)
			other = k[1] == "A" ? "B" : "A"
			if ((other, k[2]) in fonts) continue
			if (!fhead++) printf "\nfont files that differ, so a changed page may come from a font update:\n"
			printf "  %s: %s\n", k[1], k[2]
		}
		printf "\n"
		if (nw) {
			printf "%d page%s went from yes to no:", nw, nw == 1 ? "" : "s"
			for (i = 1; i <= nw; i++) printf " %s", worse[i]
			printf "\n"
			exit 1
		}
		printf "no page went from yes to no\n"
	}' | sed 's/ *$//'
	;;
esac
