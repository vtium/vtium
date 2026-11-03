#!/bin/bash
# look-for: Lines join cell to cell with no gaps and corners meet; the
# look-for: shades and blocks fill whole cells.
# wrong-looks-like: Gaps or dots between line segments, corners that miss,
# wrong-looks-like: or a dark line between rows of vertical lines.
# Box drawing, shades and block elements, which vtium draws as sprites the
# cell's full size, joined cell to cell (the C reference's boxdraw screen).
printf '┌'; for _ in $(seq 30); do printf '─'; done; printf '┐\n'
printf '│ %-29s│\n' 'vtium playtest'
printf '├'; for _ in $(seq 30); do printf '─'; done; printf '┤\n'
printf '│ %-29s│\n' 'box drawing'
printf '└'; for _ in $(seq 30); do printf '─'; done; printf '┘\n'
printf '╔══╦══╗ ╭──╮\n'
printf '║  ║  ║ │  │\n'
printf '╚══╩══╝ ╰──╯\n'
printf '░░▒▒▓▓██ ▀▄▌▐ ▁▂▃▄▅▆▇█\n'
