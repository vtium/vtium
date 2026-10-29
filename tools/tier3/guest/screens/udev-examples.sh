# vtium's example udev rules and hwdb fragment, as the tree installs them,
# read by the guest's own udev (ADR-1401 D4), for tier 3's udev-examples
# case (t3_devices.py). udevadm verify checks every rule of the example
# rules, as T3-RULES-VERIFY; each keyboard's VTIUM_ properties under them,
# from udevadm test, which changes nothing, go on the event port as
# T3-UDEV NAME PROPERTY..., by its by-path name, with the hwdb fragment the
# guest's setup installed, its token's match changed to the PS/2 keyboard;
# the fragment as shipped is built into a scratch root and queried for three
# devices, as T3-HWDB-BUILD and T3-HWDB MODALIAS PROPERTY... The case runs
# it; it is no screen of the scenario's.
# screen-text: none, it prints udev's reading of the examples.
out=${T3_EVENTS:-/dev/stdout}
examples=/usr/share/doc/vtium/examples
msg=$(udevadm verify --no-style --no-summary "$examples/71-vtium-local.rules" 2>&1) &&
    [ -z "$msg" ] && msg=ok
echo "T3-RULES-VERIFY $(printf '%s' "$msg" | tr '\n' ' ')" > "$out"
echo "71-vtium-local.rules: $msg"
seen=" "
for link in /dev/input/by-path/*-event-kbd; do
    node=$(readlink -f "$link")
    case $seen in *" $node "*) continue ;; esac
    seen="$seen$node "
    dev=$(udevadm info --query=path --name="$node")
    props=$(udevadm test --extra-rules-dir="$examples" "/sys$dev" 2>/dev/null |
        sed -n 's/^ *\(VTIUM_[A-Z_]*=.*\)$/\1/p' | tr '\n' ' ')
    echo "T3-UDEV $(basename "$link") $props" > "$out"
    echo "$(basename "$link"): ${props:-no VTIUM_ property}"
done
root=$(mktemp -d)
mkdir -p "$root/etc/udev/hwdb.d"
cp "$examples/vtium.hwdb" "$root/etc/udev/hwdb.d/"
msg=$(systemd-hwdb update --strict --root="$root" 2>&1) && [ -z "$msg" ] && msg=ok
echo "T3-HWDB-BUILD $(printf '%s' "$msg" | tr '\n' ' ')" > "$out"
for modalias in evdev:input:b0003vBBBBp0001e0110 evdev:input:b0003vAAAAp0002e0110 \
        evdev:input:b0003vAAAAp0003e0110; do
    props=$(systemd-hwdb query --root="$root" "$modalias" | tr '\n' ' ')
    echo "T3-HWDB $modalias $props" > "$out"
    echo "$modalias: ${props:-nothing}"
done
rm -rf "$root"
