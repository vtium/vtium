# Box drawing and block elements, which the font renderer must join cell to
# cell without gaps. Correct: unbroken frames, and blocks that fill their cells.
printf '┌'; for i in $(seq 30); do printf '─'; done; printf '┐\n'
printf '│ %-29s│\n' 'kmscon tier 3'
printf '├'; for i in $(seq 30); do printf '─'; done; printf '┤\n'
printf '│ %-29s│\n' 'box drawing'
printf '└'; for i in $(seq 30); do printf '─'; done; printf '┘\n'
printf '╔══╦══╗ ╭──╮\n'
printf '║  ║  ║ │  │\n'
printf '╚══╩══╝ ╰──╯\n'
printf '░░▒▒▓▓██ ▀▄▌▐ ▁▂▃▄▅▆▇█\n'
