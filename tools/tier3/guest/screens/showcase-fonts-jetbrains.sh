#!/bin/bash
# The fonts scene's band in the frit showcase-jetbrains: JetBrains Mono 2.304, an outline font, TrueType.
# The scene's caption is showcase-fonts.inc's.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
# shellcheck source=tools/tier3/guest/screens/showcase-fonts.inc
. "$here/showcase-fonts.inc"
band showcase-jetbrains 'JetBrains Mono 2.304' 'an outline font, TrueType'
