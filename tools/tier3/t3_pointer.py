"""Tier 3's pointer case: the pointer moved with QMP and the display's cursor
plane read back from the guest's DRM state.

QEMU's screendump leaves the cursor plane out, so the capture cannot show
where the pointer is drawn. The case moves the pointer with QMP's
input-send-event, which q35's VMware mouse (vmport) takes and the guest's
vmmouse driver reports as absolute axes, a virtual mouse to vtium, then has
the guest print the atomic state the kernel keeps in debugfs
(/sys/kernel/debug/dri/N/state) on the event port, and reads the cursor
plane's framebuffer and position from it: the plane shows the arrow with its
hotspot on the pixel vtium put the pointer on. Once the pointer hides, 20 s
after its last move, the plane shows nothing.

The position is worked out as vtium works it out (vtium-platform's absolute
axis, vtium's grid and the renderer's arrow): the mouse's axis scaled to the
grid's pixels, the grid centred on the display, and the arrow's corner its
hotspot up and left of the pointer.
"""

import re

CASE = "pointer-plane"

CHECKS = {
    CASE: "The pointer moved twice with QMP's input-send-event, the display's cursor plane "
          "read from the guest's DRM state after each move, and again once the pointer hid, "
          "with keys typed meanwhile. Correct: vtium logs that the display shows the pointer "
          "on its cursor plane; after each move one plane other than the primary shows a "
          "framebuffer, its corner the arrow's hotspot up and left of the pixel the mouse "
          "points at; once hidden, no such plane shows one.",
}

# QMP's absolute axes run from 0 to 0x7fff (INPUT_EVENT_ABS_MAX); the VMware
# mouse doubles each value, and the guest's vmmouse driver gives each axis
# the range 0 to 0xffff.
ABS_MAX = 0x7FFF
DEVICE_MAX = 0xFFFF
# The two places the pointer is moved to, in QMP's units: a quarter and
# three quarters across, a quarter and a half down.
MOVES = [(0x2000, 0x2000), (0x6000, 0x4000)]
# The pointer hides this long after its last move (vtium's
# pointer-hide-timeout default), and the case waits this long more for it.
HIDE_S = 20
HIDE_SLACK_S = 25

# vtium's log line when a display takes the arrow on its cursor plane
# (crates/vtium/src/pointer.rs).
PLANE_LINE = "shows it on its cursor plane"

# The guest's dump on the event port, between these lines (screens/pointer.sh).
BEGIN, END, ERROR = "T3-PLANE-BEGIN", "T3-PLANE-END", "T3-PLANE-ERROR"

_PLANE = re.compile(r"^plane\[(\d+)\]: (.*)$")
_POS = re.compile(r"^crtc-pos=(\d+)x(\d+)([+-]\d+)([+-]\d+)$")


def move_events(x, y):
    """QMP input-send-event's events putting the tablet at `x`, `y`."""
    return [{"type": "abs", "data": {"axis": "x", "value": x}},
            {"type": "abs", "data": {"axis": "y", "value": y}}]


def hotspot(cell_h):
    """The arrow's hotspot on an unturned display: its outline's width each
    way, one pixel and one more for each 32 of its height, which is the
    cell's, kept from 8 to 64 (crates/vtium-render/src/arrow.rs)."""
    h = min(max(cell_h, 8), 64)
    return h // 32 + 1


def corner(at, display, cell):
    """Where the cursor plane's top left goes for the pointer moved to `at`
    (x, y in QMP's units) on a `display` (w, h) of `cell` (w, h) cells: the
    mouse's axis scaled to the grid's pixels and kept on its last one, the
    grid centred on the display, less the hotspot."""
    out = []
    hot = hotspot(cell[1])
    for v, side, c in zip(at, display, cell):
        grid = side // c * c
        p = min(2 * v * grid // DEVICE_MAX, grid - 1)
        out.append((side - grid) // 2 + p - hot)
    return tuple(out)


def planes(state):
    """The planes of a DRM atomic state dump: [{"id", "name", "crtc", "fb",
    "pos"}], "pos" as (w, h, x, y) or None. A plane's properties are the
    lines indented once under its "plane[N]: NAME" line; its framebuffer's
    details, indented further, are passed over."""
    out, cur = [], None
    for raw in state.splitlines():
        m = _PLANE.match(raw)
        if m:
            cur = {"id": int(m.group(1)), "name": m.group(2), "crtc": None, "fb": 0,
                   "pos": None}
            out.append(cur)
            continue
        if not raw.startswith("\t"):
            cur = None
        if cur is None or raw.startswith("\t\t"):
            continue
        line = raw.strip()
        key, _, value = line.partition("=")
        if key == "crtc":
            cur["crtc"] = value
        elif key == "fb":
            cur["fb"] = int(value) if value.isdigit() else 0
        elif key == "crtc-pos":
            p = _POS.match(line)
            cur["pos"] = tuple(int(g) for g in p.groups()) if p else None
    return out


def shown(state, display):
    """The planes of `state` other than one covering the whole `display`
    that show a framebuffer on a CRTC: the cursor plane while it shows the
    arrow."""
    return [p for p in planes(state)
            if p["fb"] and p["crtc"] not in (None, "(null)") and p["pos"]
            and p["pos"][:2] != tuple(display)]


def dump(events, since=0):
    """The guest's state dump in the event port's text `events` after offset
    `since`: (the state, None), or (None, why) when the guest printed none or
    could not read it."""
    text = events[since:]
    if ERROR in text:
        line = text[text.index(ERROR):].splitlines()[0]
        return None, "the guest could not read the DRM state: %s" % line
    if BEGIN not in text or END not in text:
        return None, "the guest printed no DRM state"
    body = text[text.index(BEGIN) + len(BEGIN):text.index(END)]
    return body.split("\n", 1)[-1], None


def judge(state, display, want):
    """Why a reading failed: `want` is the cursor plane's corner, or None
    for the pointer hidden. None when it passed."""
    seen = shown(state, display)
    if want is None:
        if seen:
            return "the pointer hid, but plane %s still shows framebuffer %d at %s" % (
                seen[0]["name"], seen[0]["fb"], seen[0]["pos"])
        return None
    if len(seen) != 1:
        return "%d planes besides the primary show a framebuffer, not 1: %s" % (
            len(seen), [(p["name"], p["pos"]) for p in seen])
    got = seen[0]["pos"][2:]
    if got != tuple(want):
        return "the cursor plane %s is at %s, not %s" % (seen[0]["name"], got, tuple(want))
    return None
