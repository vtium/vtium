# The DRM cards the guest has, for tier 3's display-unplug and
# display-replug cases (t3_devices.py). The script waits until udev has
# handled the events queued before it, so a card just pulled out or plugged
# in is listed as it is. Then it writes each card's name and the PCI address
# of the card's device on the event port as T3-CARD NAME ADDRESS. It also
# writes each PCI hotplug slot as T3-SLOT NAME ADDRESS POWER ADAPTER, where
# POWER says whether the slot is powered and ADAPTER whether a card is in it,
# and the kernel's last lines about PCIe hotplug as T3-KMSG LINE, when the
# session's user may read the kernel's log. The cases run it before the
# second card is pulled out, while the card is out, and after the card is
# plugged back in; it is no screen of the scenario's.
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
for slot in /sys/bus/pci/slots/*; do
    [ -e "$slot/address" ] || continue
    echo "T3-SLOT ${slot##*/} $(cat "$slot/address") $(cat "$slot/power" 2>/dev/null || echo -) $(cat "$slot/adapter" 2>/dev/null || echo -)" > "$out"
done
dmesg 2>/dev/null | grep -i -e pciehp -e 'pcieport' -e bochs | tail -n 20 | while IFS= read -r line; do
    echo "T3-KMSG $line" > "$out"
done
