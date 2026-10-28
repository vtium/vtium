# A console on a small display, every line at most 40 columns: a ruler to
# column 40, a listing in ls's colors, the 16 background colors, styled
# words, a box of sprite lines, Latin-1, the underline styles and a bar of
# block sprites.
# look-for: The ruler's 40 ends at the right edge of a 40-column screen.
# look-for: Every line on its own row, nothing wrapped; the box lines join.
# wrong-looks-like: A line wrapped or cut at the edge, a glyph spilling out
# wrong-looks-like: of its cell, or a gap in the box lines or the bar.
printf '%s\n' '----:----1----:----2----:----3----:---40'
printf '\033[1;34mbin\033[0m  \033[1;34metc\033[0m  \033[1;32mrun.sh\033[0m  notes.txt  '
printf '\033[1;36mlib.so\033[0m\n'
for c in 40 41 42 43 44 45 46 47; do printf '\033[%sm    ' "$c"; done
printf '\033[0m\n'
for c in 100 101 102 103 104 105 106 107; do printf '\033[%sm    ' "$c"; done
printf '\033[0m\n'
printf '\033[31mred\033[0m \033[32mgreen\033[0m \033[33myellow\033[0m \033[34mblue\033[0m '
printf '\033[35mmagenta\033[0m \033[36mcyan\033[0m\n'
printf '\033[1mbold\033[0m \033[2mfaint\033[0m \033[4munder\033[0m \033[7mreverse\033[0m '
printf '\033[9mstruck\033[0m\n'
printf '%s\n' '┌──────┬──────┬───────┐' '│ cpu  │ mem  │ load  │' \
    '├──────┼──────┼───────┤' '│ 12%  │ 48M  │ 0.08  │' '└──────┴──────┴───────┘'
printf '%s\n' 'café naïve £5 ½ ¿¡ ©® «» ×÷ ÆØÅ'
printf '\033[4:2mdouble\033[0m \033[4:3mcurly\033[0m \033[4:4mdotted\033[0m '
printf '\033[4:5mdashed\033[0m\n'
printf '[\033[32m%s\033[0m%s] 50%%\n' '████████████████' '░░░░░░░░░░░░░░░░'
