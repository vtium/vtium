#!/usr/bin/env python3
"""VT probe for the guided checks (guide.sh): sets a terminal mode, then
prints each sequence the terminal sends, one line each, with the time the
probe read it, in seconds from the start. A read holding several escape
sequences is split at each ESC, and focus reports are labelled. q quits and
resets the mode. With PROBE_LOG set, each line is appended there too.

  probe.py focus         mode 1004: CSI I on focus in, CSI O on focus out
  probe.py focus-later   the same, but sets 1004 only after 10 s
  probe.py decarm        resets DECARM (?8l): a held key gives one event
  probe.py mouse         1000 + 1006 + 1016: SGR mouse reports in pixels
  probe.py bg            1004, then OSC setBackground and, 5 s later,
                         setForeground, reading all the while
  probe.py --list        the modes, one per line
"""
import os
import select
import sys
import termios
import time
import tty

MODES = {
    "focus": ("\033[?1004h", "\033[?1004l"),
    "focus-later": ("\033[?1004h", "\033[?1004l"),
    "decarm": ("\033[?8l", "\033[?8h"),
    "mouse": ("\033[?1000h\033[?1006h\033[?1016h", "\033[?1016l\033[?1006l\033[?1000l"),
    "bg": ("\033[?1004h", "\033[?1004l"),
}

LABELS = {b"\033[I": "FOCUS IN", b"\033[O": "FOCUS OUT"}


def split(data):
    """One read's bytes as sequences: a new one starts at each ESC, so two
    focus reports read together get a line each. Bytes before the first ESC
    stay together."""
    parts = []
    start = 0
    for i in range(1, len(data)):
        if data[i] == 0x1B:
            parts.append(data[start:i])
            start = i
    parts.append(data[start:])
    return [p for p in parts if p]


def label(seq):
    return LABELS.get(seq, "")


class Probe:
    def __init__(self, mode, fd, log):
        self.mode = mode
        self.fd = fd
        self.log = log
        self.t0 = time.monotonic()
        self.n = 0

    def now(self):
        return time.monotonic() - self.t0

    def out(self, s):
        os.write(1, (s + "\r\n").encode())
        if self.log:
            self.log.write("  [%s] %s\n" % (self.mode, s))
            self.log.flush()

    def show(self, data, note=""):
        for seq in split(data):
            self.n += 1
            self.out("%7.3f  #%-4d %-10s %s%r" % (self.now(), self.n, label(seq), note, seq))

    def watch(self, until=None, quit=True):
        """Reads and shows until q (when quit) or the monotonic time until.
        Returns True when q was pressed."""
        while True:
            left = None if until is None else until - time.monotonic()
            if left is not None and left <= 0:
                return False
            if not select.select([self.fd], [], [], left)[0]:
                continue
            data = os.read(self.fd, 256)
            # An empty read is the terminal gone: nothing more will come.
            if not data or quit and data in (b"q", b"Q"):
                return True
            self.show(data, "" if quit else "(before set) ")


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "--list":
        print("\n".join(MODES))
        return 0
    if mode not in MODES:
        print(__doc__, file=sys.stderr)
        return 2
    on, off = MODES[mode]
    fd = sys.stdin.fileno()
    saved = termios.tcgetattr(fd)
    path = os.environ.get("PROBE_LOG")
    log = open(path, "a") if path else None
    tty.setraw(fd)
    p = Probe(mode, fd, log)
    background = False
    try:
        if mode == "focus-later":
            p.out("1004 is set in 10 s: switch away now, come back after")
            p.watch(time.monotonic() + 10, quit=False)
        os.write(1, on.encode())
        p.out("%s on; q quits" % mode)
        if mode == "bg":
            if p.watch(time.monotonic() + 1):
                return 0
            p.out("%7.3f  sending OSC setBackground (the screen may go dark)" % p.now())
            os.write(1, b"\033]setBackground\a")
            background = True
            quit = p.watch(time.monotonic() + 5)
            os.write(1, b"\033]setForeground\a")
            background = False
            p.out("%7.3f  sent OSC setForeground" % p.now())
            if quit:
                return 0
        p.watch()
    finally:
        if background:
            os.write(1, b"\033]setForeground\a")
        os.write(1, off.encode())
        # TCSAFLUSH drops keys typed after q, so none answers guide.sh's prompt.
        termios.tcsetattr(fd, termios.TCSAFLUSH, saved)
        print("%s off" % mode)
    return 0


if __name__ == "__main__":
    sys.exit(main())
