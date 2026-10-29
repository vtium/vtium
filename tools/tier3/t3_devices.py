"""Tier 3's displays and keyboards beyond one of each: a card's second
output and a second card, each captured; USB keyboards in fixed ports; a
keyboard plugged in and pulled out through QMP; and vtium's example udev
rules and hwdb fragment, as the tree installs them, read by the guest's
udev (ADR-1401 D4).

A variant's "outputs" lists the outputs it captures besides the first, each
{"suffix", "device", "head", "size"}: a case drawn on one is the screen's
case with the output's suffix (`cjk@1`), judged by its own golden. A head
after a device's first shows nothing until a display asks for its size, so
an output naming "size" is asked for it over a VNC server of its own
(t3_qemu.vnc_set_size).
"""

import re
import time

from t3_common import rel

# The separator between a screen's case and the output it was drawn on.
SEP = "@"

# The USB keyboards: an xHCI controller at PCI address 0000:15:00.0, a hub in
# its port 3 and a keyboard in the hub's port 2, the port ADR-1401's example
# rule names (pci-0000:15:00.0-usb-0:3.2), so the rule is read as the
# record writes it. The firmware numbers the buses behind bridges depth
# first, so the first root port, holding 19 buses beyond its own (1 to 20),
# leaves bus 21, 0x15, to the second. Both sit at fixed addresses, ahead of
# nothing the variants' other devices take.
USB_BUS = "usb21.0"
USB = ["-device", "pcie-root-port,id=rp1,bus=pcie.0,addr=0x10,chassis=1,bus-reserve=19",
       "-device", "pcie-root-port,id=rp21,bus=pcie.0,addr=0x11,chassis=21",
       "-device", "qemu-xhci,id=usb21,bus=rp21",
       "-device", "usb-hub,id=hub3,bus=%s,port=3" % USB_BUS,
       "-device", "usb-kbd,id=kbd32,bus=%s,port=3.2" % USB_BUS]

CHECKS = {
    "mirror": "Every screen as each output drew it, against the first output's. Correct: "
              "the same grid on every output, each capture identical to the first's.",
    "udev-examples": "vtium's example udev rules and hwdb fragment, as the tree installs "
                     "them under /usr/share/doc/vtium/examples, read by the guest's udev: "
                     "udevadm verify on the rules, udevadm test with them for each "
                     "keyboard, and systemd-hwdb with the fragment for three devices. "
                     "Correct: the rules verify without a message; the keyboard in port "
                     "pci-0000:15:00.0-usb-0:3.2 gets VTIUM_HEAD=panel and every other "
                     "keyboard no VTIUM_ property; the fragment builds without a "
                     "message and marks the token and the numpad it names, and nothing "
                     "else.",
    "keyboard-hotplug": "The boot's USB keyboard pulled out, so the PS/2 keyboard is the "
                        "console's; a USB keyboard plugged into port 3.3 of the hub "
                        "through QMP, typed on, then pulled out, and the console typed on "
                        "again. Correct: typing on the PS/2 keyboard raises its interrupt "
                        "count; vtium takes the new keyboard's event node; its keys reach "
                        "the session and leave the PS/2 count unchanged; the guest lists "
                        "it while plugged and not once pulled out; and the session still "
                        "takes keys.",
}

# The boot's USB keyboard, which keyboard-hotplug pulls out first: QEMU
# sends a key no console binding claims to the keyboard plugged in last, so
# once it is gone the console's keys reach the PS/2 keyboard, whose interrupt
# count tells its keys from a USB keyboard's.
BOOT_ID = "kbd32"
# The keyboard plugged in and pulled out: its QMP id, its port, and the name
# the guest's udev gives it, which the guest lists (screens/keyboard-hotplug.sh).
HOT_ID = "kbd33"
HOT_PORT = "3.3"
HOT_NAME = "pci-0000:15:00.0-usb-0:3.3:1.0-event-kbd"
# vtium's log line for each input device it opens (vtium-platform's input).
KEYBOARD_LINE = re.compile(r"input: using (/dev/input/event\d+) \[([^\]]*)\] as keyboard")

# The port the example rule names, and the property it sets.
PANEL_PORT = "-usb-0:3.2:"
PANEL = ["VTIUM_HEAD=panel"]
# What the example hwdb fragment gives each device the guest queries
# (screens/udev-examples.sh): the token, the numpad, and a device of the
# numpad's vendor it does not name.
HWDB = {"vtium:input:b0003vBBBBp0001e0110": ["VTIUM_ROLE=token"],
        "vtium:input:b0003vAAAAp0002e0110": ["VTIUM_ROLE=numpad"],
        "vtium:input:b0003vAAAAp0003e0110": []}


def split(case):
    """(the screen's case, the output suffix) of `case`; the suffix is ""
    for the first output."""
    screen, sep, output = case.partition(SEP)
    return screen, sep + output


def output_cases(case, outputs):
    """`case` drawn on each of `outputs` after the first."""
    return [case + o["suffix"] for o in outputs]


def capture(output):
    """QMP screendump's arguments for `output`."""
    return {k: output[k] for k in ("device", "head") if k in output}


def heads(outputs):
    """The heads Qemu asks a size for: (device, head, size) of each output
    naming one."""
    return [(o["device"], o["head"], tuple(o["size"])) for o in outputs if "size" in o]


def judge_mirror(images):
    """Why the outputs differ, from {case: (first output's image, [(case on
    another output, its image)])}; None when every capture equals the
    first's."""
    differ = [other for case, (first, rest) in sorted(images.items())
              for other, img in rest if img.data != first.data]
    if differ:
        return "differs from the first output's capture: %s" % ", ".join(differ)
    return None


def events_after(vm, since):
    """The event port's lines after offset `since`."""
    with open(vm.events, errors="replace") as f:
        return f.read()[since:].splitlines()


def events_size(vm):
    with open(vm.events, errors="replace") as f:
        return len(f.read())


def serial_size(vm):
    with open(vm.serial, "rb") as f:
        return len(f.read())


def wait_serial_after(vm, pattern, since, timeout):
    """The first match of `pattern` on the console log after byte `since`,
    in whole lines, waiting up to `timeout` s."""
    deadline = time.time() + timeout
    while True:
        with open(vm.serial, "rb") as f:
            data = f.read()[since:]
        data = data[:data.rfind(b"\n") + 1].decode(errors="replace")
        m = pattern.search(data)
        if m:
            return m
        if vm.proc.poll() is not None or time.time() > deadline:
            raise RuntimeError("no %r on the console within %d s; see %s"
                               % (pattern.pattern, timeout, rel(vm.serial)))
        time.sleep(0.2)


def run_screen(vm, name, timeout, type_line=None):
    """Type `t3 NAME` (with `type_line`, a function typing a line, else the
    console's keyboard) and wait for the screen's end on the event port:
    the lines it printed there."""
    since = events_size(vm)
    (type_line or vm.type)("t3 %s\n" % name)
    deadline = time.time() + timeout
    while True:
        lines = events_after(vm, since)
        if "T3-DONE %s" % name in (l.strip() for l in lines):
            return [l.strip() for l in lines]
        if time.time() > deadline:
            raise RuntimeError("no T3-DONE %s within %d s; see %s" % (name, timeout,
                                                                        rel(vm.events)))
        time.sleep(0.2)


def parse_udev(lines):
    """The guest's account of the examples (screens/udev-examples.sh): ({each
    keyboard's by-path name: its VTIUM_ properties}, {each queried device:
    the hwdb's properties}, what building the hwdb printed, what verifying
    the rules printed), each "ok" when it printed nothing."""
    keyboards, hwdb, built, verified = {}, {}, None, None
    for line in lines:
        fields = line.split()
        if not fields:
            continue
        if fields[0] == "T3-UDEV" and len(fields) >= 2:
            keyboards[fields[1]] = fields[2:]
        elif fields[0] == "T3-HWDB" and len(fields) >= 2:
            hwdb[fields[1]] = fields[2:]
        elif fields[0] == "T3-HWDB-BUILD":
            built = " ".join(fields[1:])
        elif fields[0] == "T3-RULES-VERIFY":
            verified = " ".join(fields[1:])
    return keyboards, hwdb, built, verified


def judge_udev(keyboards, hwdb, built, verified):
    """Why the guest's reading of the examples is wrong (CHECKS
    "udev-examples"); None when it is right."""
    if verified is None:
        return "the guest did not verify the example rules"
    if verified != "ok":
        return "udevadm verify on the example rules printed: %s" % (verified or "nothing, "
                                                                   "and failed")
    if built is None:
        return "the guest did not build the hwdb fragment"
    if built != "ok":
        return "building the hwdb fragment printed: %s" % built
    panel = [k for k in keyboards if PANEL_PORT in k]
    if not panel:
        return "no keyboard in the port the example rule names: %s" % sorted(keyboards)
    for name, props in sorted(keyboards.items()):
        want = PANEL if PANEL_PORT in name else []
        if props != want:
            return "udevadm test gave %s %s, not %s" % (name, props or "nothing", want or "nothing")
    for modalias, want in HWDB.items():
        if modalias not in hwdb:
            return "the guest did not query the hwdb for %s" % modalias
        if hwdb[modalias] != want:
            return "the hwdb gave %s %s, not %s" % (modalias, hwdb[modalias] or "nothing",
                                                     want or "nothing")
    return None


def key_events(qcodes, down):
    """QMP input-send-event's events pressing or releasing `qcodes`."""
    return [{"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": q}}}
            for q in qcodes]


def listed(lines):
    """The keyboards the guest listed (screens/keyboard-hotplug.sh), as {by-path
    name: event node}."""
    return {f[1]: f[2] for f in (l.split() for l in lines) if len(f) == 3 and f[0] == "T3-KBD"}


def i8042(lines):
    """The PS/2 keyboard's interrupt count the guest reported
    (screens/keyboard-hotplug.sh), or None."""
    counts = [f[1] for f in (l.split() for l in lines)
              if len(f) == 2 and f[0] == "T3-I8042" and f[1].isdigit()]
    return int(counts[-1]) if counts else None


def judge_hotplug(control, plugged, typed, pulled, took):
    """Why keyboard-hotplug failed (CHECKS), from the guest's account of each
    run of screens/keyboard-hotplug.sh: `control`, two runs typed on the
    console's keyboard; `plugged` and `typed`, two typed on the plugged-in
    keyboard; `pulled`, one on the console's once it is out; and `took`, the
    event nodes vtium logged as keyboards after the plug. None when it
    passed."""
    first, second = (i8042(r) for r in control)
    if first is None or second is None or second <= first:
        return ("typing on the PS/2 keyboard did not raise its interrupt count (%s, then %s), "
                "so the count cannot tell the keyboards apart" % (first, second))
    node = listed(plugged).get(HOT_NAME)
    if node is None:
        return "plugged in, the guest lists %s, not %s" % (sorted(listed(plugged)), HOT_NAME)
    if node not in took:
        return "vtium took %s after the plug, not %s, the node of %s" % (
            ", ".join(took) or "no keyboard", node, HOT_NAME)
    before, after = i8042(plugged), i8042(typed)
    if before is None or after != before:
        return ("typing on the plugged-in keyboard moved the PS/2 keyboard's interrupt count "
                "from %s to %s: its keys came from the PS/2 keyboard" % (before, after))
    if HOT_NAME in listed(pulled):
        return "pulled out, the guest still lists %s" % HOT_NAME
    return None


def wait_qmp_event(vm, name, device, timeout):
    """Wait for QEMU's QMP event `name` naming `device`."""
    deadline = time.time() + timeout
    while True:
        vm.cmd("query-status")  # reads the events QEMU sent meanwhile
        if any(e.get("event") == name and e.get("data", {}).get("device") == device
               for e in vm.qmp_events):
            return
        if time.time() > deadline:
            raise RuntimeError("no QMP %s for %s within %d s" % (name, device, timeout))
        time.sleep(0.2)
