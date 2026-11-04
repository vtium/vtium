#!/bin/bash
# The fonts scene's band in the frit showcase-terminus: Terminus 4.49.1, ter-v32n, a bitmap console font, PSF, 16x32.
# The scene's caption is showcase-fonts.inc's.

# shellcheck source=tools/tier3/guest/screens/showcase.inc
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/showcase.inc"
# shellcheck source=tools/tier3/guest/screens/showcase-fonts.inc
. "$here/showcase-fonts.inc"
band showcase-terminus 'Terminus 4.49.1, ter-v32n' 'a bitmap console font, PSF, 16x32'
