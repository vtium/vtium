"""A stand-in terminal for tests: it runs a command on a pseudo-terminal of a
given size, which becomes the command's controlling terminal, so the command
reads its size with stty and talks to /dev/tty as it does in a real
terminal. It answers the queries a terminal answers and returns everything
the command wrote.

The pseudo-terminal starts raw, as a terminal emulator's is from the
program's side once it turns off the line discipline's processing, so the
bytes come back as the command wrote them, with no carriage return added
before each newline."""

import fcntl
import os
import re
import select
import struct
import subprocess
import termios
import time
import tty

# The queries the stand-in answers: a kitty graphics command, XTWINOPS 16
# (the cell's size in pixels), primary device attributes, the cursor
# position report and OSC 50's font query.
QUERIES = re.compile(rb"\x1b_G[^\x1b]*\x1b\\|\x1b\[16t|\x1b\[c|\x1b\[6n|\x1b\]50;\?\x07")


def answers(kitty=True, cell=(8, 16), font=None):
    """A reply for each query, as a terminal with these traits gives it: one
    that draws kitty graphics answers a query (a=q) with OK, and one that
    does not answers nothing; every one answers XTWINOPS 16 with `cell`
    (width, height), primary device attributes and the cursor position, and
    OSC 50's query with `font` when one is given."""

    def reply(query):
        if query.startswith(b"\x1b_G"):
            keys = dict(k.split(b"=", 1) for k in query[3:-2].split(b";")[0].split(b",")
                        if b"=" in k)
            if kitty and keys.get(b"a") == b"q" and b"i" in keys:
                return b"\x1b_Gi=%s;OK\x1b\\" % keys[b"i"]
            return b""
        if query == b"\x1b[16t":
            return b"\x1b[6;%d;%dt" % (cell[1], cell[0])
        if query == b"\x1b[c":
            return b"\x1b[?62;22c"
        if query == b"\x1b[6n":
            return b"\x1b[1;1R"
        return b"\x1b]50;%s\x07" % font.encode() if font else b""

    return reply


def run(argv, reply, rows=24, cols=80, env=None, timeout=60, cooked=False, interrupt=None):
    """Runs `argv` on a pseudo-terminal of `rows` by `cols`, answering each
    query it writes with `reply(query)`, and returns (its exit status,
    everything it wrote, the terminal's local modes when it ended). The
    terminal starts raw, or `cooked`, with echo and line editing on. With
    `interrupt`, a signal, the command gets that signal in place of the
    answer to its first query for primary device attributes. A command
    still running after `timeout` seconds is killed and the run fails."""
    master, slave = os.openpty()
    if not cooked:
        tty.setraw(slave)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                            start_new_session=True, env=env,
                            preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
    os.close(slave)
    out, scanned = bytearray(), 0
    deadline = time.monotonic() + timeout
    try:
        while True:
            ready, _, _ = select.select([master], [], [], max(0.0, deadline - time.monotonic()))
            if not ready:
                proc.kill()
                raise TimeoutError("%s ran past %d s" % (argv, timeout))
            try:
                data = os.read(master, 1 << 16)
            except OSError:
                break  # every end of the terminal on the command's side is closed
            if not data:
                break
            out += data
            for m in QUERIES.finditer(out, scanned):
                scanned = m.end()
                if interrupt and m.group(0) == b"\x1b[c":
                    os.killpg(proc.pid, interrupt)
                    interrupt = None
                    continue
                answer = reply(m.group(0))
                if answer:
                    os.write(master, answer)
        proc.wait(max(0.0, deadline - time.monotonic()))
        modes = termios.tcgetattr(master)[3]
    finally:
        os.close(master)
    return proc.returncode, bytes(out), modes
