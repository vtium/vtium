# SGR attributes and colors: the 16 palette colors as foreground and
# background, bold, underline, reverse, the 256-color cube and a 24-bit ramp.
# Correct: each color in its own cell, the cube and ramp without gaps.
for c in 30 31 32 33 34 35 36 37 90 91 92 93 94 95 96 97; do
    printf '\033[%sm fg%-3s\033[0m' "$c" "$c"
done
printf '\n'
for c in 40 41 42 43 44 45 46 47 100 101 102 103 104 105 106 107; do
    printf '\033[30;%sm bg%-3s\033[0m' "$c" "$c"
done
printf '\n\033[1mbold\033[0m \033[4munderline\033[0m \033[7mreverse\033[0m '
printf '\033[1;4;31mbold-underline-red\033[0m \033[2mfaint\033[0m \033[3mitalic\033[0m\n'
for row in 0 1 2 3 4 5; do
    for c in $(seq $((16 + row * 36)) $((51 + row * 36))); do
        printf '\033[48;5;%sm  ' "$c"
    done
    printf '\033[0m\n'
done
for i in $(seq 0 63); do
    printf '\033[48;2;%d;%d;%dm ' $((i * 4)) $((255 - i * 4)) 128
done
printf '\033[0m\n'
