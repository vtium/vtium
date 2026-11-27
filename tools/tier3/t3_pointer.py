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
grid's pixels, the grid placed on the display as `origin` places it, and the
arrow's corner its hotspot up and left of the pointer.

The showcase draws the pointer into a scene's picture the same way the
display shows it. A scene asks for the pointer on a cell (`pointed`), and the
driver moves it there with QMP (`aim`). It then reads where the cursor plane
is from the guest's DRM state and the plane's image from QEMU's VNC server,
and lays the image over the screendump (`compose`).
"""

import re

import t3_heads
import t3img

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


def origin(display, cell, smooth=False):
    """The display pixel where the grid's top left lies on an unturned
    `display` (w, h) of `cell` (w, h) cells. The grid fills as many whole
    cells as the display holds. With smooth scrolling off, the grid is
    centred on both axes: the top gets half the spare lines, rounded down,
    and the left half the spare columns, so an odd spare pixel goes to the
    bottom or the right. With smooth scrolling on (`smooth`), the grid's
    last row touches the display's bottom edge, so every spare line lies
    above it, and across it stays centred.

    References:
    - ADR-1310 D10: the grid flush to the bottom edge while smooth
      scrolling is on, and centred across.
    - ADR-1310 D12, "With smooth scrolling off": the grid centred on both
      axes."""
    (w, h), (cw, ch) = display, cell
    spare_x, spare_y = w - w // cw * cw, h - h // ch * ch
    return (spare_x // 2, spare_y if smooth else spare_y // 2)


_SECTION = re.compile(r"^\[(\S+)(?:\s+(\S+))?\]$")
_KEY = re.compile(r"^([a-z0-9-]+)\s*=\s*(.*?)\s*$")


def smooth_scrolls(conf, features):
    """Whether smooth scrolling is on for the first head of the vtium.conf
    text `conf`, in a console built with `features`. The value that wins
    is the head's own `smooth-scroll`, else its look's, else the top
    level's, and it is off when none sets it. The head's look is the one
    its `look` names, or `default` when it names none; a configuration
    with no [look default] section takes the settings above its first
    section. Smooth scrolling is on only where that value is `on` and the
    console is built with the feature `smooth-scroll`.

    References:
    - ADR-1310 D7: the key at the top level, in a look and in a head, the
      head's value winning over its look's.
    - ADR-1310 D10, "Every build": smooth scrolling on only in a build
      with the feature `smooth-scroll`."""
    top, looks, heads = {}, {}, []
    keys = top
    for line in conf.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = _SECTION.match(line)
        if m:
            keys = {}
            if m.group(1) == "look" and m.group(2):
                looks[m.group(2)] = keys
            elif m.group(1) == "head" and m.group(2):
                heads.append(keys)
            continue
        m = _KEY.match(line)
        if m:
            keys[m.group(1)] = m.group(2)
    head = heads[0] if heads else {}
    look = looks.get(head.get("look", "default"), {})
    value = head.get("smooth-scroll") or look.get("smooth-scroll") or top.get("smooth-scroll")
    return value == "on" and "smooth-scroll" in features


def variant_smooth(variant):
    """Whether smooth scrolling is on for the console of the tier 3 variant
    `variant` (a variant's dict): `smooth_scrolls` on the vtium.conf the
    variant installs and the features it names, "features" for a console
    built for the variant alone, else "tree_features"."""
    features = variant.get("features")
    features = (features.replace(",", " ").split() if features is not None
                else variant.get("tree_features", ()))
    return smooth_scrolls(t3_heads.guest_conf(variant), features)


def corner(at, display, cell, smooth=False):
    """Where the cursor plane's top left goes for the pointer moved to `at`
    (x, y in QMP's units) on a `display` (w, h) of `cell` (w, h) cells: the
    mouse's axis scaled to the grid's pixels and kept on its last one, from
    the grid's origin on the display, which `smooth` places as `origin`
    says, less the hotspot."""
    out = []
    hot = hotspot(cell[1])
    for v, side, c, o in zip(at, display, cell, origin(display, cell, smooth)):
        grid = side // c * c
        p = min(2 * v * grid // DEVICE_MAX, grid - 1)
        out.append(o + p - hot)
    return tuple(out)


def aim(target, display, cell):
    """QMP's absolute axis values that put the pointer on the middle pixel
    of the character cell `target` (column, row from 0) of a `display` (w,
    h) in `cell` (w, h) cells: corner's scaling run backwards, each value
    the smallest that reaches the pixel."""
    out = []
    for n, side, c in zip(target, display, cell):
        grid = side // c * c
        p = n * c + c // 2
        out.append(-(-p * DEVICE_MAX // (2 * grid)))
    return tuple(out)


# A showcase scene that wants the pointer reports, on the event port, the
# cell to point at and its cell size (screens/showcase.inc, point_at).
_POINT = re.compile(r"^T3-POINT (\d+) (\d+) cell=(\d+)x(\d+)$")


def pointed(events, name):
    """((column, row), (cell w, cell h)) that screen `name` asked the
    pointer onto, from the event port's lines `events` after the screen
    before it ended and up to its own T3-DONE; None when it asked for
    none."""
    seen = None
    for e in events:
        if e == "T3-DONE %s" % name:
            return seen
        if e.startswith("T3-DONE "):
            seen = None
        m = _POINT.match(e)
        if m:
            g = [int(v) for v in m.groups()]
            seen = ((g[0], g[1]), (g[2], g[3]))
    return None


def cursor_plane(state, display):
    """The cursor plane of `state` while it shows the pointer, as `planes`
    gives it, its "pos" (w, h, x, y) on the display; None while no plane
    besides the primary shows a framebuffer, and an error when more than one
    does."""
    seen = shown(state, display)
    if len(seen) > 1:
        raise RuntimeError("%d planes besides the primary show a framebuffer, not 1: %s"
                           % (len(seen), [(p["name"], p["pos"]) for p in seen]))
    return seen[0] if seen else None


def compose(img, cursor, at):
    """`img`, the primary plane's capture, with the cursor plane's image
    `cursor` over it, its corner at `at` (x, y), as the display shows the
    two planes. `cursor` is (width, height, pixels), each pixel 4 bytes B,
    G, R, A of the premultiplied ARGB8888 vtium writes to the cursor plane,
    so a pixel's color is its own plus what lies under it scaled by 255 less
    its alpha. The parts of the image off the display are left out."""
    w, h, pixels = cursor
    if len(pixels) != w * h * 4:
        raise ValueError("cursor image does not match %dx%d" % (w, h))
    data = bytearray(img.data)
    x0, y0 = at
    for y in range(max(0, -y0), min(h, img.height - y0)):
        for x in range(max(0, -x0), min(w, img.width - x0)):
            b, g, r, a = pixels[(y * w + x) * 4:(y * w + x) * 4 + 4]
            if a == 0 and not (r or g or b):
                continue
            o = ((y0 + y) * img.width + x0 + x) * 3
            for i, c in enumerate((r, g, b)):
                data[o + i] = min(255, c + (data[o + i] * (255 - a) + 127) // 255)
    return t3img.Image(img.width, img.height, bytes(data))


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
