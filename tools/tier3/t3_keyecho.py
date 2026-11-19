"""Tier 3's key-to-present time on the minspec machine: how long a key
takes to show on the screen, measured by vtium on the real KMS path.

vtium stamps each key it sends to the program. When a display queues the
frame that shows the program's echo, vtium takes the time from the key to
that moment, the key-to-present time. The time ends when the frame is
queued, not when the display flips to it. At debug level, under the render
target, each head's frame-times line once a second gives the count, the
mean and the largest of those times since its last line.

The check `key-echo` runs on 2d-potato-min, whose configuration turns on
debug logging ("conf_extra"). It types `t3 key-echo` in the logged-in
shell. The script (guest/screens/key-echo.sh) puts the terminal in raw mode
with no echo, so the program's own output is the only echo, reports
T3-KEYS on the event port, and runs dd, which reads each key and writes it
back, until it has read len(KEYS) keys. The driver types KEYS through
QEMU's keyboard once it sees T3-KEYS. The script waits a second and a half
before and after, so vtium's frame-times lines stamped inside its window
count the keys' frames and no other, and reports its window as a benchmark
screen does. t3_bench.echo_figures reads the figures from the serial log,
and they go into the variant's records under "bench".

The check passes when the program read every key typed and vtium measured
at least one key echo, and no more echoes than keys. Its capture is the
screen the script leaves, judged by no golden.
"""

import os

import t3_bench
import t3_heads
from t3_qemu import wait_stable

CASE = t3_bench.KEY_ECHO

# The keys the driver types, one QEMU chord each, which the script's
# keys= count matches (test_t3.KeyEchoTest).
KEYS = "the quick brown fox jumps over the lazy dog 0123456789"

CHECKS = {
    CASE: "A program in raw mode echoes each of %d keys typed through the keyboard, and "
          "vtium measures each key from the moment it sends the key to the program until a "
          "display queues the frame that shows the echo. Correct: the typed keys on the "
          "screen, then a line that reads \"key-echo: done\", with the key-to-present "
          "figures in the report." % len(KEYS),
}


def read_events(vm):
    with open(vm.events, "rb") as f:
        return [e.strip() for e in f.read().decode(errors="replace").splitlines()]


def figures(vm, conf):
    """The check's figures (t3_bench.measure) from the serial log and the
    script's T3-BENCH line, or None before the script reported its
    window."""
    line = next((e for e in read_events(vm) if e.startswith("T3-BENCH %s " % CASE)), None)
    if line is None:
        return None
    with open(vm.serial, "rb") as f:
        serial = f.read().decode(errors="replace")
    return t3_bench.measure(serial, line, conf)


def check_key_echo(v, vm, work, caps, errors, log):
    """The key-echo check (CHECKS). As with every check without a golden,
    the case keeps its capture only when it passes."""
    path = os.path.join(work, CASE + ".ppm")
    try:
        last = wait_stable(vm, os.path.join(work, "wait.ppm"))
        vm.type("t3 %s\n" % CASE)
        vm.wait_event("T3-KEYS %s" % CASE, timeout=60)
        vm.type(KEYS)
        vm.wait_event("T3-DONE %s" % CASE, timeout=60)
        wait_stable(vm, path, want=lambda img: img.data != last.data)
        got = figures(vm, t3_heads.conf_lines(v))
        if got is None:
            raise RuntimeError("the screen reported no window on the event port")
    except RuntimeError as e:
        errors[CASE] = str(e)
        return
    log("%s: %s" % (CASE, got))
    if got.get("error"):
        errors[CASE] = got["error"]
        return
    caps[CASE] = path


CHECK_RUNS = {CASE: check_key_echo}
