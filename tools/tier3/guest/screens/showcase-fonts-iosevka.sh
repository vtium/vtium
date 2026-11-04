#!/bin/bash
# The fonts scene's band in the frit showcase-iosevka: Iosevka Term 34.9.1, an outline font, TrueType.
# The scene's caption is showcase-fonts.inc's.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
# shellcheck source=tools/tier3/guest/screens/showcase-fonts.inc
. "$here/showcase-fonts.inc"
band showcase-iosevka 'Iosevka Term 34.9.1' 'an outline font, TrueType'
