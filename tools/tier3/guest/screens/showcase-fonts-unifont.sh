#!/bin/bash
# The fonts scene's band in the frit showcase-unifont: GNU Unifont 17.0.03, a bitmap font built into vtium, drawn at twice its 8x16.
# The scene's caption is showcase-fonts.inc's.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
# shellcheck source=tools/tier3/guest/screens/showcase-fonts.inc
. "$here/showcase-fonts.inc"
band showcase-unifont 'GNU Unifont 17.0.03' 'a bitmap font built into vtium, drawn at twice its 8x16'
