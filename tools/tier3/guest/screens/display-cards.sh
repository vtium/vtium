# The DRM cards the guest has, for tier 3's display-unplug and
# display-replug cases (t3_devices.py). The script waits until udev has
# handled the events queued before it, so a card just pulled out or plugged
# in is listed as it is. Then it writes each card's name and the PCI address
# of the card's device on the event port as T3-CARD NAME ADDRESS. The cases
# run it before the second card is pulled out, while the card is out, and
# after the card is plugged back in; it is no screen of the scenario's.
# screen-text: none, it lists the guest's cards.
out=${T3_EVENTS:-/dev/stdout}
udevadm settle --timeout=10
for card in /sys/class/drm/card[0-9]*; do
    name=${card##*/}
    case $name in *-*) continue ;; esac
    [ -e "$card/device" ] || continue
    addr=$(basename "$(readlink -f "$card/device")")
    echo "T3-CARD $name $addr" > "$out"
    echo "$name $addr"
done
