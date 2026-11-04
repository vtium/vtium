#!/bin/sh
# Makes vtium's showcase (ADR-1502 D6), the screens the documentation shows:
# fetches the fonts t3_showcase.py pins, runs tier 3's variant
# showcase-1920x1080 in the QEMU/KVM guest, and writes docs/showcase/: each
# scene's capture, the fonts scene's four bands cropped and stacked, the two
# small-machine goldens, and the page. An image whose pixels did not change
# is left as it is. `make showcase` runs it; tools/tier3/README.md (The
# showcase) describes the variant.
#
#   tools/tier3/showcase.sh            fetch the fonts, run the variant, write the showcase
#   tools/tier3/showcase.sh --no-run   write the showcase from the last run
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

run=yes
for arg in "$@"; do
	case $arg in
	--no-run) run=no ;;
	-h | --help)
		sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "showcase.sh: unknown argument $arg; --help lists them" >&2
		exit 2
		;;
	esac
done

if [ "$run" = yes ]; then
	python3 -I -B tools/tier3/t3_showcase.py fetch-fonts
	make tier3 FEATURES=images VARIANTS=showcase-1920x1080
fi
python3 -I -B tools/tier3/t3_showcase.py compose
