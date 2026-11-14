# The display hand-over (ADR-1401 D16), which tier 3's hand-over check
# (t3_handover.py) runs; it is no screen of the scenario's. It turns on
# focus reporting and sends setBackground, so vtium lets go of the card.
# Then it waits until the agent's probe has taken the card as root and let
# it go again, and sends setForeground, so vtium takes the card back. It
# reports on the event port the two reports vtium sent it: a focus-out at
# setBackground and a focus-in at setForeground.
# screen-text: none, it prints what it received.
out=${T3_EVENTS:-/dev/stdout}
done=/tmp/t3-hand-over.done
# Each focus report is three bytes, read with no line discipline. timeout
# keeps head in the terminal's foreground process group, where it may read
# the terminal; in a process group of its own, head would stop at its read.
report() {
    timeout --foreground 30 head -c 3 | od -An -tx1 | tr -d ' \n'
}
saved=$(stty -g)
stty raw -echo
printf '\033[?1004h'
printf '\033]setBackground\007'
echo "T3-HANDOVER sent" > "$out"
first=$(report)
for _ in $(seq 600); do
    [ -e "$done" ] && break
    sleep 0.1
done
printf '\033]setForeground\007'
second=$(report)
printf '\033[?1004l'
stty "$saved"
echo "T3-FOCUS ${first:-none} ${second:-none}" > "$out"
echo "received: ${first:-none} then ${second:-none}"
