"""Tier 3's benchmark scenarios (ADR-103's S1 to S6 and the acceptance
scenarios that play video): kmscon's render-timer lines on the guest's
console, and the figures of one scenario's window.

A benchmark screen script reports its window on the event port as
"T3-BENCH NAME start=S end=E KEY=N...", the keys in any order, S and E on
the guest's monotonic clock (/proc/uptime), the clock that also stamps
kmscon's console lines. The other keys are the screen's own counts: "lines"
for text it printed, or for a screen that plays video through mpv
"mpv_frames" and "mpv_dropped" (mpv-stats.lua) and "kmscon_cpu_ms",
kmscon's CPU time over the window from /proc, and for the tmux grids
"tmux_sixels", the sixel pictures tmux sent to kmscon while it played.
kmscon, run with render-timer=log, writes one line per display per second,
each covering the second before its stamp; the scenario's figures come from
the lines whose seconds overlap the window.
"""

import re
import statistics

# "[   12.345678] kmscon[505]: terminal: render-timer: display=... key=value ...",
# or vtium[505] for vtium's process. A console line can follow other output
# on the same line, such as a prompt.
_TIMER = re.compile(r"\[\s*(\d+\.\d+)\] (?:kmscon|vtium)\[(\d+)\]: terminal: render-timer: (.*)$")
_BENCH = re.compile(r"T3-BENCH (\S+)((?: \w+=\S+)+)\s*$")

# kmscon logs this for a key it does not know, as a build without the
# render timer (upstream's) does for render-timer.
_UNKNOWN = "unknown config option 'render-timer'"


def timer_lines(text):
    """The render-timer lines in console text, in order, as dicts: "t" (the
    stamp, in seconds), "pid", "display", and each number by its key. A line
    another writer broke into on the serial console, such as the getty's
    terminal queries at boot, is left out."""
    out = []
    for line in text.splitlines():
        m = _TIMER.search(line.strip())
        if not m:
            continue
        entry = {"t": float(m.group(1)), "pid": int(m.group(2))}
        try:
            for field in m.group(3).split():
                key, _, value = field.partition("=")
                entry[key] = value if key == "display" else float(value)
        except ValueError:
            continue
        out.append(entry)
    return out


def bench_window(line):
    """(name, start, end, {key: count}) from a T3-BENCH event line, or None
    when it has no start and end or a count is not a whole number."""
    m = _BENCH.search(line.strip())
    if not m:
        return None
    fields = dict(f.split("=", 1) for f in m.group(2).split())
    try:
        start, end = float(fields.pop("start")), float(fields.pop("end"))
        counts = {k: int(v) for k, v in fields.items()}
    except (KeyError, ValueError):
        return None
    return m.group(1), start, end, counts


def summarize(lines, count, start, end):
    """The figures of the window [start, end] in which `count` lines of text
    were printed (None for a screen that prints none to measure), from the
    render-timer lines that overlap it.

    Flips and frames are counted over the overlapping seconds and divided by
    the window's length; the screen script pauses before and after the
    window, so those seconds hold no other frames. CPU time and damage are
    means per frame over the window; the median is the median of the
    seconds' medians and the 95th percentile the largest second's, as the
    lines carry no single frames.
    """
    seconds = end - start
    window = [x for x in lines if x["t"] > start and x["t"] - x["interval_ms"] / 1000 < end]
    out = {"seconds": round(seconds, 2), "intervals": len(window),
           "displays": sorted({x["display"] for x in window})}
    if count is not None:
        out["lines"] = count
        out["lines_per_s"] = round(count / seconds) if seconds > 0 else None
    if not window or seconds <= 0:
        return out
    flips = sum(x["flips"] for x in window)
    frames = sum(x["frames"] for x in window)
    busy = [x for x in window if x["frames"]]
    # the seconds wholly inside the window, while the screen's output flowed
    inside = [x for x in window if x["t"] - x["interval_ms"] / 1000 >= start and x["t"] <= end]
    skipped = sum(x["skipped"] for x in window)
    out.update({
        "flips": int(flips), "frames": int(frames),
        "presented_per_s": round(flips / seconds, 1),
        "skipped": int(skipped), "skipped_per_s": round(skipped / seconds, 1),
        # Redraws that found nothing to present, and what they cost.
        "skipped_cpu_ms": round(sum(x["skipped_cpu_ms"] for x in window), 1),
        "skipped_cpu_pct": round(sum(x["skipped_cpu_ms"] for x in window) / (seconds * 10), 1),
        "intervals_inside": len(inside), "flips_inside": int(sum(x["flips"] for x in inside)),
    })
    if frames:
        out.update({
            "cpu_ms_mean": round(sum(x["cpu_ms_mean"] * x["frames"] for x in busy) / frames, 3),
            "cpu_ms_median": round(statistics.median(x["cpu_ms_median"] for x in busy), 3),
            "cpu_ms_p95": max(x["cpu_ms_p95"] for x in busy),
            "cpu_ms_max": max(x["cpu_ms_max"] for x in busy),
            "damage_px_mean": round(sum(x["damage_px_mean"] * x["frames"] for x in busy) / frames),
            "damage_pct": round(sum(x["damage_pct"] * x["frames"] for x in busy) / frames, 2),
        })
    return out


def program(counts, seconds):
    """The figures of a screen that plays video through mpv: mpv's frames
    and their rate over the window, and kmscon's CPU time over the window,
    in all and per frame mpv sent (its parsing included, which the render
    timer does not count), and as a share of one CPU."""
    if "mpv_frames" not in counts or seconds <= 0:
        return {}
    frames, ms = counts["mpv_frames"], counts.get("kmscon_cpu_ms")
    out = {"mpv_frames": frames, "mpv_dropped": counts.get("mpv_dropped", 0),
           "mpv_per_s": round(frames / seconds, 1)}
    if ms is not None:
        out["kmscon_cpu_ms"] = ms
        out["kmscon_cpu_pct"] = round(ms / (seconds * 10), 1)
        if frames:
            out["kmscon_cpu_ms_per_mpv_frame"] = round(ms / frames, 3)
    return out


# The clip mpv plays (acceptance.inc) runs at 30 frames per second.
CLIP_FRAME_MS = 1000 / 30


def bottleneck(figures):
    """Why kmscon held a program's video back, as ADR-103 judges A2: its
    CPU time per frame the program sent (parsing included) above the clip's
    frame interval, or fewer frames presented than the program sent; or
    that the program showed nothing. An empty list when it kept up or the
    screen runs no program."""
    if "mpv_frames" not in figures:
        return []
    if not figures["mpv_frames"]:
        return ["mpv showed no frame, so nothing was played to judge"]
    out = []
    per = figures.get("kmscon_cpu_ms_per_mpv_frame")
    if per is not None and per > CLIP_FRAME_MS:
        out.append("kmscon's CPU time per frame mpv sent, %.1f ms, is over the clip's "
                   "%.1f ms frame interval" % (per, CLIP_FRAME_MS))
    shown = figures.get("presented_per_s")
    if shown is not None and shown < 0.9 * figures["mpv_per_s"]:
        out.append("kmscon presented %.1f frames a second, fewer frames than mpv's %.1f"
                   % (shown, figures["mpv_per_s"]))
    return out


# The flood screen's bounds (#147). A stream of sixel pictures changes the
# screen only at each picture's end, so a draw tried after every pty read
# finds nothing to draw and counts as skipped (#139); kmscon draws once per
# loop turn, and only a screen that changed.
FLOOD_MAX_SKIPPED_PER_S = 20


def flood_verdict(figures):
    """Why the flood screen's figures fail, or an empty list: a frame
    presented in a second wholly inside the window, while the stream flowed,
    and at most FLOOD_MAX_SKIPPED_PER_S passes a second that drew nothing."""
    why = []
    if not figures.get("intervals_inside"):
        why.append("no render-timer second lies wholly inside the window")
    elif not figures.get("flips_inside"):
        why.append("no frame was presented while the stream flowed")
    if figures.get("skipped_per_s", 0) > FLOOD_MAX_SKIPPED_PER_S:
        why.append("%.1f passes a second drew nothing, over %d"
                   % (figures["skipped_per_s"], FLOOD_MAX_SKIPPED_PER_S))
    return why


def measure(serial_text, event_line, config):
    """The report entry of one benchmark screen: its figures, or why there
    are none. "error", which fails the case, is set when a build with the
    render timer configured logged no line for the window, when tmux sent
    no sixel picture, and when the flood screen's figures are out of bounds
    (flood_verdict)."""
    window = bench_window(event_line or "")
    if window is None:
        return {"error": "the screen reported no benchmark window: %r" % event_line}
    name, start, end, counts = window
    out = summarize(timer_lines(serial_text), counts.get("lines"), start, end)
    out.update(program(counts, end - start))
    if "tmux_sixels" in counts:
        out["tmux_sixels"] = counts["tmux_sixels"]
        if not counts["tmux_sixels"]:
            out["error"] = ("tmux sent kmscon no sixel picture: it drew placeholders, or "
                            "nothing played")
        elif out.get("kmscon_cpu_ms") is not None:
            # Reported, not judged: A4 is compatibility (ADR-103), and
            # kmscon's streaming is judged apart from tmux (#161).
            out["kmscon_cpu_ms_per_tmux_sixel"] = round(
                out["kmscon_cpu_ms"] / counts["tmux_sixels"], 3)
    if out["intervals"]:
        why = flood_verdict(out) if name == "flood" else []
        if why:
            out["error"] = "; ".join(why)
        return out
    if _UNKNOWN in serial_text:
        out["note"] = "this build has no render timer"
    elif not any(re.fullmatch(r"render-timer=(log|both)", c) for c in config):
        out["note"] = "render-timer is not configured for this variant"
    else:
        out["error"] = "no render-timer line overlaps the window %.2f to %.2f s" % (start, end)
    return out
