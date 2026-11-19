#!/bin/bash
# The key-to-present time, which tier 3's key-echo check (t3_keyecho.py)
# runs; it is no screen of the scenario's. The terminal goes into raw mode
# with no echo, so the program's own output is the only echo. dd reads each
# key the driver types through the keyboard and writes the key back at
# once, until it has read as many keys as the driver types. vtium measures
# each key from the moment it sends the key to dd until a display queues
# the frame that shows the echo, and logs the figures once a second at
# debug level. The pauses keep the frames before and after (the header, the
# done line, the next prompt) out of the frame-times lines stamped inside
# the window. The screen reports its window and the keys dd read on the
# event port, as a benchmark screen does.
# screen-text: none, it shows the keys the driver types.
out=${T3_EVENTS:-/dev/stdout}
keys=54
err=/tmp/t3-key-echo.err
echo "key-echo: type $keys keys"
sleep 1.5
read -r start _ </proc/uptime
saved=$(stty -g)
stty raw -echo
echo "T3-KEYS key-echo" > "$out"
# timeout keeps dd in the terminal's foreground process group, where it may
# read the terminal, and its SIGINT makes dd print its counts before it
# ends, so a key QEMU dropped fails the check instead of hanging it.
timeout --foreground -s INT 60 dd bs=1 count=$keys 2>"$err"
stty "$saved"
read_keys=$(sed -n 's/^\([0-9]*\)+[0-9]* records in$/\1/p' "$err")
sleep 1.5
read -r end _ </proc/uptime
printf '\r\nkey-echo: done\n'
echo "T3-BENCH key-echo start=$start end=$end keys=$keys read=${read_keys:-0}" > "$out"
exit 0
