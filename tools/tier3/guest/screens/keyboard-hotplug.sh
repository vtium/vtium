# The keyboards the guest has, for tier 3's keyboard-hotplug case
# (t3_devices.py): each by-path name and its event node on the event port as
# T3-KBD NAME NODE, once udev has handled the events queued before, so a
# keyboard just plugged in or pulled out is listed as it is; then the PS/2
# keyboard's interrupts so far, IRQ 1 summed over the CPUs, as T3-I8042 N:
# typing on it raises the count, and typing on a USB keyboard leaves it. The
# case runs it on each keyboard; it is no screen of the scenario's.
# screen-text: none, it lists the guest's keyboards.
out=${T3_EVENTS:-/dev/stdout}
udevadm settle --timeout=10
for link in /dev/input/by-path/*-event-kbd; do
    [ -e "$link" ] || continue
    echo "T3-KBD $(basename "$link") $(readlink -f "$link")" > "$out"
    basename "$link"
done
count=$(awk '$1 == "1:" { n = 0; for (i = 2; i <= NF && $i ~ /^[0-9]+$/; i++) n += $i; print n }' \
    /proc/interrupts)
echo "T3-I8042 $count" > "$out"
echo "PS/2 keyboard interrupts: $count"
