# The keyboards the guest has, for tier 3's keyboard-hotplug case
# (t3_devices.py): each by-path name on the event port as T3-KBD NAME, once
# udev has handled the events queued before, so a keyboard just plugged in
# or pulled out is listed as it is. The case runs it on each keyboard; it is
# no screen of the scenario's.
# screen-text: none, it lists the guest's keyboards.
out=${T3_EVENTS:-/dev/stdout}
udevadm settle --timeout=10
for link in /dev/input/by-path/*-event-kbd; do
    [ -e "$link" ] || continue
    echo "T3-KBD $(basename "$link")" > "$out"
    basename "$link"
done
