#!/bin/bash
# kit: sixel
# look-for: For five seconds tmux shows 9 panes, 3 by 3, every pane playing the
# look-for: test pattern inside its borders, its counter rising: every pane
# look-for: animates. This page judges tmux compatibility; the frame rate is
# look-for: reported, not judged, as tmux and mpv set it (under half the frames
# look-for: at 8K). Then this page cleared, mpv's counts and rate in bold.
# wrong-looks-like: "SIXEL IMAGE" placeholders in place of video, a picture
# wrong-looks-like: across a pane border or in the wrong pane, frozen panes,
# wrong-looks-like: or pieces of video left on this page afterwards.
# screen-text: none, as tmux draws on the alternate screen
# ADR-103's acceptance scenario A4 in a 3 by 3 grid (tmux-grid.inc).
n=3
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/tmux-grid.inc"
