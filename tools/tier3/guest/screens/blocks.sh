#!/bin/bash
# look-for: Full blocks stack into solid shapes, halves into solid columns;
# look-for: light, double, heavy and rounded box lines run unbroken down rows.
# wrong-looks-like: A dark line between rows of blocks or across vertical box
# wrong-looks-like: lines where the cell is taller than the font's glyph (#46).
# Block elements and box drawing stacked across rows and columns, as tier 2's
# blocks screen: each cell is drawn to the cell's full size, so nothing shows
# between rows whatever the font's line spacing.
printf '██████ ▄▄▄▄ ┌──┬──┐ ╔══╦══╗ ┏━━┳━━┓ ╭──╮\n'
printf '██████ ████ │  │  │ ║  ║  ║ ┃  ┃  ┃ │  │\n'
printf '██████ ▀▀▀▀ ├──┼──┤ ╠══╬══╣ ┣━━╋━━┫ ╰──╯\n'
printf '▐▌▐▌▐▌ ▖▗▘▝ │  │  │ ║  ║  ║ ┃  ┃  ┃ ╱╲╳\n'
printf '▐▌▐▌▐▌ ▙▛▜▟ └──┴──┘ ╚══╩══╝ ┗━━┻━━┛ ▚▞\n'
printf '▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉█ ▔▕ ░▒▓█ ┄┈╌ ┅┉╍\n'
printf '┆┊╎ ┇┋╏ ╞╡╟╢ ╒╕╓╖ ┍┑┎┒ ┝┥┠┨ ╼╽╾╿ ╴╵╶╷\n'
printf '┆┊╎ ┇┋╏ ╘╛╙╜ ╪╫┿╂ ┕┙┖┚ ╀╁╃╄ ╅╆╇╈ ╉╊\n'
printf '\033[33;44m██▀▄▌▐░▒▓ ┼╋╬ ╭╮╰╯\033[0m\n'
