#!/bin/bash
# Prints the judgements demo.sh recorded: one line per section, with the
# last rating given for it in the run, and its comment below it, one indented
# line per line typed. A rating given again after b replaces the earlier one
# here; the file keeps both. Sections whose last rating is skipped are listed
# again at the end, as still to review.
#
#   feedback.sh [--file PATH]             the latest run
#   feedback.sh [--file PATH] --run ID    the run ID (the run column)
#   feedback.sh [--file PATH] --compare   the latest run of each DEMO_LABEL,
#                                         side by side by section
#
# PATH defaults to feedback.tsv in /tmp/vtium-vt (VTIUM_VT_DIR), as in demo.sh.
set -euo pipefail

here=$(cd "$(dirname "$(readlink -f "$0")")" && pwd)
# shellcheck source=tools/vt/common.sh
. "$here/common.sh"

usage() {
	sed -n '7,10s/^# \{0,1\}//p' "$0" >&2
	exit 2
}

die() {
	printf 'feedback: %s\n' "$*" >&2
	exit 1
}

file=$vt_dir/feedback.tsv
mode=latest
run=
while [ $# -gt 0 ]; do
	case $1 in
	--file) [ $# -ge 2 ] || usage; file=$2; shift ;;
	--run) [ $# -ge 2 ] || usage; mode=run; run=$2; shift ;;
	--compare) mode=compare ;;
	*) usage ;;
	esac
	shift
done
[ -r "$file" ] || die "cannot read $file"

# Answer lines only: the header and anything short of the eight columns are
# skipped.
answers() {
	awk -F'\t' 'NF >= 7 && $1 != "time"' "$file"
}

[ -n "$(answers)" ] || die "no answers in $file"

# awk functions both views use: the comment's escapes undone (\\ and \n, as
# demo.sh writes them), each of its lines printed indented, and a sort.
lib='
function unescape(s,   out, c, i) {
	for (i = 1; i <= length(s); i++) {
		c = substr(s, i, 1)
		if (c == "\\" && i < length(s)) {
			c = substr(s, ++i, 1)
			if (c == "n") c = "\n"
		}
		out = out c
	}
	return out
}
function comment(s,   n, l, i) {
	n = split(unescape(s), l, "\n")
	for (i = 1; i <= n; i++) printf "    %s\n", l[i]
}
function sort(a, n,   i, j, t) {
	for (i = 2; i <= n; i++)
		for (j = i; j > 1 && a[j - 1] > a[j]; j--) { t = a[j]; a[j] = a[j - 1]; a[j - 1] = t }
}'

if [ "$mode" = compare ]; then
	answers | awk -F'\t' "$lib"'
	{ line[NR] = $0; if (!($3 in latest)) labels[++nl] = $3; latest[$3] = $2 }
	END {
		for (i = 1; i <= NR; i++) {
			split(line[i], f, "\t")
			if (latest[f[3]] != f[2]) continue
			if (!(f[6] in seen)) { seen[f[6]] = 1; ids[++ns] = f[6] }
			rating[f[3], f[6]] = f[7]
			note[f[3], f[6]] = f[8]
			terminal[f[3]] = "TERM=" f[4] ", " f[5]
		}
		sort(ids, ns)
		for (c = 1; c <= nl; c++) {
			col = substr("ABCDEFGHIJKLMNOPQRSTUVWXYZ", c, 1)
			printf "%s  %s\n   run %s, %s\n", col, labels[c], latest[labels[c]], terminal[labels[c]]
		}
		printf "\n%-24s", "section"
		for (c = 1; c <= nl; c++) printf "  %-7s", substr("ABCDEFGHIJKLMNOPQRSTUVWXYZ", c, 1)
		printf "\n"
		for (s = 1; s <= ns; s++) {
			printf "%-24s", ids[s]
			for (c = 1; c <= nl; c++) {
				r = rating[labels[c], ids[s]]
				printf "  %-7s", r == "" ? "-" : r
			}
			printf "\n"
		}
		for (s = 1; s <= ns; s++)
			for (c = 1; c <= nl; c++)
				if (rating[labels[c], ids[s]] == "skipped") {
					if (!skipped++) printf "\nskipped, still to review\n"
					printf "%s  %s\n", substr("ABCDEFGHIJKLMNOPQRSTUVWXYZ", c, 1), ids[s]
				}
		for (s = 1; s <= ns; s++)
			for (c = 1; c <= nl; c++)
				if (note[labels[c], ids[s]] != "") {
					if (!header++) printf "\ncomments\n"
					printf "%s  %s\n", substr("ABCDEFGHIJKLMNOPQRSTUVWXYZ", c, 1), ids[s]
					comment(note[labels[c], ids[s]])
				}
	}' | sed 's/ *$//'
	exit 0
fi

[ "$mode" = run ] || run=$(answers | awk -F'\t' 'END { print $2 }')
answers | awk -F'\t' -v run="$run" "$lib"'
$2 != run { next }
{
	if (!n++) { started = $1; label = $3; term = $4; size = $5 }
	if (!($6 in rating)) ids[++ns] = $6
	rating[$6] = $7
	note[$6] = $8
}
END {
	if (!n) { print "feedback: no run " run > "/dev/stderr"; exit 1 }
	sort(ids, ns)
	printf "run       %s\nlabel     %s\nterminal  TERM=%s, %s\nstarted   %s\n", run, label, term, size, started
	printf "answers   %d for %d sections\n\n", n, ns
	printf "%-24s  %s\n", "section", "rating"
	for (s = 1; s <= ns; s++) {
		printf "%-24s  %s\n", ids[s], rating[ids[s]]
		if (note[ids[s]] != "") comment(note[ids[s]])
	}
	for (s = 1; s <= ns; s++)
		if (rating[ids[s]] == "skipped") {
			if (!skipped++) printf "\nskipped, still to review\n"
			printf "  %s\n", ids[s]
		}
}' | sed 's/ *$//'
