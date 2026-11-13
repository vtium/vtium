"""Tier 3's guest machine: one headless QEMU, driven over QMP.

QEMU runs as the invoking user and is driven over QMP on a unix socket: keys
go in with send-key, screens come back with screendump. Every VM started here
is shut down by the PID it was started with.

The GL variant is the exception for captures: once virgl owns the scanout,
QEMU's screendump reports "no surface", so that variant pairs egl-headless
with a VNC server on a unix socket and reads the screen over RFB instead.
"""

import hashlib
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import tempfile
import time

import t3img
from t3_common import rel

# The machine provisioning boots; each variant names its own (t3_scenario.py).
CPUS = 4
MEMORY_MB = 4096

# A unix socket path, with its terminating NUL, must fit sun_path (108 bytes).
SUN_PATH_MAX = 107

# How the name of each QEMU's socket directory (socket_dir) starts.
SOCKET_DIR_PREFIX = "vtium-t3-"


def socket_dir():
    """A new directory for one QEMU's unix sockets, readable by this user
    only, with a short path of its own: under $XDG_RUNTIME_DIR when the
    session has one, else under the system's temporary directory. A path
    under the checkout passes sun_path's limit when the checkout's own path
    is long, so the sockets never live there."""
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    parent = runtime if runtime and os.path.isabs(runtime) and os.path.isdir(runtime) else None
    return tempfile.mkdtemp(prefix=SOCKET_DIR_PREFIX, dir=parent)

# Seconds a QMP command may take before QEMU counts as wedged.
QMP_TIMEOUT = 30

# Called as on_qemu("start" or "stop", pid) for each QEMU this process starts
# and stops; a worker sets it to report them to the parent, which kills any
# that outlive the worker.
on_qemu = None

# CSI and OSC sequences, which systemd's colored status lines carry.
_ESCAPES = re.compile(r"\x1b\[[0-9;?!]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


class Qemu:
    """One headless QEMU process and its QMP connection."""

    def __init__(self, workdir, disks, device, log, display="none", net=False,
                 capture="screendump", cpus=CPUS, memory_mb=MEMORY_MB, extra=(), heads=(),
                 agent=False):
        os.makedirs(workdir, exist_ok=True)
        self.workdir = workdir
        self.log = log
        self.capture = capture
        # The sockets go in a directory of their own with a short path
        # (socket_dir), and the files QEMU writes go in `workdir`. stop()
        # removes the socket directory, as does a start that fails.
        self.sockets = socket_dir()
        try:
            self._start(disks, device, display, net, cpus, memory_mb, extra, heads, agent)
        except BaseException:
            shutil.rmtree(self.sockets, ignore_errors=True)
            raise

    def _start(self, disks, device, display, net, cpus, memory_mb, extra, heads, agent):
        workdir, log, capture = self.workdir, self.log, self.capture
        self.sock_path = os.path.join(self.sockets, "qmp.sock")
        self.vnc_path = os.path.join(self.sockets, "vnc.sock")
        # A head after a device's first is connected only once a display
        # sets its size, so each in `heads`, (device id, head, (w, h)), gets
        # a VNC server of its own, through which the size is set.
        heads = [(dev, head, size, os.path.join(self.sockets, "vnc%d.sock" % n))
                 for n, (dev, head, size) in enumerate(heads, 1)]
        # ttyS0 is the guest's console. ttyS1 carries the scenario's events
        # (guest/setup), so the driver can wait for a login or a drawn screen.
        self.serial = os.path.join(workdir, "serial.log")
        self.events = os.path.join(workdir, "events.log")
        # ttyS2, for a variant that runs the agent (guest/t3-agent), carries
        # the driver's requests and the agent's answers over a unix socket.
        self.agent_path = os.path.join(self.sockets, "ttyS2") if agent else None
        self.agent_sock = None
        self.agent_buf = b""
        self.asked = 0
        sockets = (self.sock_path, self.vnc_path, *(h[3] for h in heads),
                   *((self.agent_path,) if agent else ()))
        for p in sockets:
            if len(os.fsencode(p)) > SUN_PATH_MAX:
                raise RuntimeError("socket path is %d bytes, over the %d a unix "
                                   "socket allows; set XDG_RUNTIME_DIR or TMPDIR to a "
                                   "shorter directory: %s" % (len(os.fsencode(p)), SUN_PATH_MAX, p))
        for p in (self.serial, self.events):
            if os.path.exists(p):
                os.unlink(p)
        args = ["qemu-system-x86_64", "-machine", "q35,accel=kvm", "-cpu", "host",
                "-smp", str(cpus), "-m", str(memory_mb), "-no-reboot",
                "-display", display, *(("-device", device) if device else ("-vga", "none")),
                "-qmp", "unix:%s,server=on,wait=off" % self.sock_path,
                "-serial", "file:%s" % self.serial,
                "-serial", "file:%s" % self.events]
        if agent:
            args += ["-serial", "unix:%s,server=on,wait=off" % self.agent_path]
        for path, fmt, ro in disks:
            args += ["-drive", "file=%s,if=virtio,format=%s%s"
                     % (path, fmt, ",readonly=on" if ro else "")]
        args += ["-nic", "user,model=virtio-net-pci"] if net else ["-nic", "none"]
        if capture == "vnc":
            args += ["-vnc", "unix:%s" % self.vnc_path]
        for dev, head, _, path in heads:
            args += ["-vnc", "unix:%s,id=vnc-%s-%d,display=%s,head=%d" % (path, dev, head, dev, head)]
        args += list(extra)
        log("starting: " + " ".join(args))
        self.qemu_log = open(os.path.join(workdir, "qemu.log"), "w")
        try:
            self.proc = subprocess.Popen(args, stdout=self.qemu_log,
                                         stderr=subprocess.STDOUT)
        except BaseException:  # no QEMU started, so its log is closed here
            self.qemu_log.close()
            raise
        log("qemu pid %d" % self.proc.pid)
        if on_qemu:
            on_qemu("start", self.proc.pid)
        self.sock = None
        self.buf = b""
        # The QMP events QEMU has sent so far, in order, each {"event",
        # "data"}: SHUTDOWN tells a guest's power-off from its reset.
        self.qmp_events = []
        try:
            self._connect()
            for dev, head, (w, h), path in heads:
                vnc_set_size(path, w, h)
                log("%s head %d connected at %dx%d" % (dev, head, w, h))
        except BaseException:  # a QEMU that cannot be driven is not left running
            self._kill()
            self._stopped()
            self.qemu_log.close()
            raise

    def _connect(self):
        deadline = time.time() + 30
        while self.sock is None:
            if self.proc.poll() is not None:
                raise RuntimeError("qemu exited with %d; see %s"
                                   % (self.proc.returncode, self.qemu_log.name))
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(self.sock_path)
                self.sock = s
            except OSError:
                if time.time() > deadline:
                    raise
                time.sleep(0.2)
        # A wedged QEMU makes a QMP command raise instead of blocking forever.
        self.sock.settimeout(QMP_TIMEOUT)
        self._recv()  # greeting
        self.cmd("qmp_capabilities")

    def _recv(self):
        while b"\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise RuntimeError("QMP connection closed")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def cmd(self, name, **arguments):
        msg = {"execute": name}
        if arguments:
            msg["arguments"] = arguments
        try:
            self.sock.sendall(json.dumps(msg).encode() + b"\n")
            while True:
                reply = self._recv()
                if "event" not in reply:
                    break
                self.qmp_events.append(reply)
        except TimeoutError:
            raise RuntimeError("QMP %s: no reply within %d s; QEMU pid %d is wedged"
                               % (name, QMP_TIMEOUT, self.proc.pid)) from None
        if "error" in reply:
            raise RuntimeError("QMP %s: %s" % (name, reply["error"]))
        return reply.get("return")

    def keys(self, *qcodes):
        """Press qcodes together, as one chord."""
        self.cmd("send-key", keys=[{"type": "qcode", "data": k} for k in qcodes])
        # A fixed pause, kept: QEMU queues each chord's release behind a hold
        # time and drops key events while its bounded input queue is full,
        # and nothing outside QEMU shows how full the queue is. Pacing the
        # chords keeps it short. A dropped key shows on the screen, so it
        # fails the case.
        time.sleep(0.05)

    def type(self, text):
        for ch in text:
            self.keys(*char_qcodes(ch))

    def screendump(self, path, output=None):
        """Capture the screen to a PPM at `path` and return it: the first
        output's, or `output`'s, a {"device", "head"} that QMP's screendump
        takes (a device's qdev id, and the head of a device with several)."""
        if self.capture == "vnc":
            if output:
                raise RuntimeError("a VNC capture reads the first output alone")
            img = vnc_capture(self.vnc_path)
            write_ppm(path, img)
            return img
        tmp = path + ".tmp"
        self.cmd("screendump", filename=tmp, format="ppm", **(output or {}))
        # screendump returns before the file is complete on some backends.
        for _ in range(100):
            try:
                img = t3img.read(tmp)
                os.replace(tmp, path)
                return img
            except (ValueError, IndexError, OSError):
                time.sleep(0.1)
        raise RuntimeError("screendump did not produce a readable image")

    def wait_serial(self, marker, timeout, log_path=None, fail=None, since=0):
        """Wait for a line holding `marker` on the console log (or on
        `log_path`), terminal escape sequences left out, and return it.
        Only lines ending in a newline count: the guest writes a line a few
        bytes at a time, and the part of one that has arrived is not yet
        the line (#61). Only what the guest wrote from byte `since` on
        counts, as after a key the driver pressed. A line holding `fail`,
        when it is given, ends the wait at once with a RuntimeError that
        carries the line, since the guest writes no `marker` after it."""
        log_path = log_path or self.serial
        deadline = time.time() + timeout
        while True:
            with open(log_path, "rb") as f:
                f.seek(since)
                data = f.read()
            data = data[:data.rfind(b"\n") + 1]
            text = _ESCAPES.sub("", data.decode(errors="replace"))
            for line in text.splitlines():
                if fail is not None and fail in line:
                    raise RuntimeError("the guest reported a failure: %s; see %s"
                                       % (line.strip(), rel(log_path)))
                if marker in line:
                    return line.strip()
            if self.proc.poll() is not None or time.time() > deadline:
                break
            time.sleep(0.2)
        raise RuntimeError("no %r within %d s; see %s" % (marker, timeout, rel(log_path)))

    def wait_event(self, marker, timeout):
        """Wait for the guest to report `marker` on its event port."""
        return self.wait_serial(marker, timeout, log_path=self.events)

    def ask(self, request, timeout=30):
        """The agent's answer to `request` (guest/t3-agent): the lines it
        wrote back on its port for it, each without its "T3-AGENT N "
        prefix. The first request also waits for the agent to start: a
        request the guest reads before the agent opens its port is lost,
        so until one is answered `state`, which changes nothing, is asked
        again."""
        if self.agent_sock is None:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.connect(self.agent_path)
            self.agent_sock = s
            deadline = time.time() + 120
            while True:
                try:
                    self._ask("state", 5)
                    break
                except RuntimeError:
                    if time.time() > deadline:
                        raise RuntimeError("the agent did not answer within 120 s; see %s"
                                           % rel(self.serial)) from None
        return self._ask(request, timeout)

    def tell(self, request):
        """Sends `request` to the agent and reads no answer, for a request
        after which the guest may not answer, such as `panic`. The agent
        has answered `state` first, so it reads the request."""
        self.ask("state")
        self.asked += 1
        self.agent_sock.sendall(b"%d %s\n" % (self.asked, request.encode()))

    def _ask(self, request, timeout):
        self.asked += 1
        n = self.asked
        self.agent_sock.sendall(b"%d %s\n" % (n, request.encode()))
        prefix, end = "T3-AGENT %d " % n, "T3-AGENT-END %d" % n
        deadline = time.time() + timeout
        answer = []
        while True:
            while b"\n" in self.agent_buf:
                line, self.agent_buf = self.agent_buf.split(b"\n", 1)
                line = line.decode(errors="replace").strip()
                if line == end:
                    return answer
                if line.startswith(prefix):
                    answer.append(line[len(prefix):])
            left = deadline - time.time()
            if self.proc.poll() is not None or left <= 0:
                raise RuntimeError("the agent did not answer %r within %d s"
                                   % (request, timeout))
            self.agent_sock.settimeout(min(left, 1.0))
            try:
                chunk = self.agent_sock.recv(65536)
            except TimeoutError:
                continue
            if not chunk:
                raise RuntimeError("the agent's socket closed")
            self.agent_buf += chunk

    def status(self):
        """The guest's run state, as QMP's query-status names it ("running",
        "suspended", ...)."""
        return self.cmd("query-status")["status"]

    def wait_status(self, want, timeout):
        """Wait until the guest's run state is `want`."""
        deadline = time.time() + timeout
        while True:
            status = self.status()
            if status == want:
                return
            if time.time() > deadline:
                raise RuntimeError("the guest is %s, not %s, after %d s" % (status, want, timeout))
            time.sleep(0.5)

    def drain_events(self):
        """Read the events QEMU sent before its QMP socket closed, as when
        the guest ended it, and return every event so far."""
        if self.sock is None:
            return self.qmp_events
        try:
            while True:
                event = self._recv()
                if "event" in event:
                    self.qmp_events.append(event)
        except (OSError, RuntimeError, ValueError):  # the socket closed
            pass
        return self.qmp_events

    def shutdown_reason(self):
        """The reason of QEMU's SHUTDOWN event ("guest-shutdown" for an
        ACPI power-off, "guest-reset" for a reset under -no-reboot,
        "guest-panic" ...), or None when it sent none."""
        reasons = [e.get("data", {}).get("reason") for e in self.drain_events()
                   if e.get("event") == "SHUTDOWN"]
        return reasons[-1] if reasons else None

    def shutdown_so_far(self):
        """QEMU's SHUTDOWN reason so far, without waiting for one: None
        while the guest runs and QEMU has sent none; the reason once QEMU
        has ended or sent one; "the guest is STATUS" when the guest stopped
        running some other way, as when it paused."""
        if self.proc.poll() is not None:
            return self.shutdown_reason() or "QEMU exited with %d" % self.proc.returncode
        status = self.status()  # takes in the events QEMU sent before its reply
        reasons = [e.get("data", {}).get("reason") for e in self.qmp_events
                   if e.get("event") == "SHUTDOWN"]
        if reasons:
            return reasons[-1]
        return None if status == "running" else "the guest is %s" % status

    def wait_exit(self, timeout):
        try:
            return self.proc.wait(timeout)
        except subprocess.TimeoutExpired:
            return None

    def _kill(self):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()

    def _stopped(self):
        self.log("qemu pid %d stopped" % self.proc.pid)
        if on_qemu:
            on_qemu("stop", self.proc.pid)

    def stop(self):
        """Power the guest off, then make sure the process we started is gone."""
        if self.proc.poll() is None:
            try:
                self.cmd("quit")
                quit_sent = True
            except (OSError, RuntimeError):  # QMP is gone or wedged
                quit_sent = False
            if not quit_sent or self.wait_exit(15) is None:
                self.log("qemu pid %d did not quit; sending SIGTERM" % self.proc.pid)
                self.proc.terminate()
                if self.wait_exit(10) is None:
                    self.log("qemu pid %d did not exit; sending SIGKILL" % self.proc.pid)
                    self._kill()
        self._stopped()
        if self.sock:
            self.sock.close()
        if self.agent_sock:
            self.agent_sock.close()
        self.qemu_log.close()
        shutil.rmtree(self.sockets, ignore_errors=True)


def _recv_exact(sock, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(min(n - len(buf), 1 << 20))
        if not chunk:
            raise RuntimeError("VNC connection closed")
        buf += chunk
    return bytes(buf)


def _vnc_open(path, encodings):
    """A connection to QEMU's VNC server on the unix socket `path`, past
    RFB 3.8's handshake with no auth, taking `encodings` (SetEncodings):
    (the socket, width, height)."""
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(30)
    s.connect(path)
    try:
        _recv_exact(s, 12)
        s.sendall(b"RFB 003.008\n")
        types = _recv_exact(s, _recv_exact(s, 1)[0])
        if 1 not in types:
            raise RuntimeError("VNC server offers no unauthenticated access")
        s.sendall(b"\x01")
        if struct.unpack(">I", _recv_exact(s, 4))[0] != 0:
            raise RuntimeError("VNC security handshake failed")
        s.sendall(b"\x01")  # shared
        w, h = struct.unpack(">HH", _recv_exact(s, 4))
        _recv_exact(s, 16)
        _recv_exact(s, struct.unpack(">I", _recv_exact(s, 4))[0])
        # 32 bpp little-endian XRGB, so each pixel arrives as B, G, R, X.
        s.sendall(struct.pack(">B3xBBBBHHHBBB3x", 0, 32, 24, 0, 1,
                              255, 255, 255, 16, 8, 0))
        s.sendall(struct.pack(">BxH", 2, len(encodings))
                  + b"".join(struct.pack(">i", e) for e in encodings))
    except BaseException:
        s.close()
        raise
    return s, w, h


# RFB's ExtendedDesktopSize pseudo-encoding and SetDesktopSize message, by
# which a client asks for the display's size; QEMU passes the size to the
# display device as a display window's would be.
EXTENDED_DESKTOP_SIZE = -308
SET_DESKTOP_SIZE = 251


def vnc_set_size(path, width, height):
    """Ask the display of QEMU's VNC server on `path` for `width` by
    `height`, as a client window of that size would. A virtio-gpu head
    after the first reports itself connected only once asked for a size,
    and offers that size as its mode. Raises unless QEMU took the request
    (ExtendedDesktopSize's status "request forwarded")."""
    s, _, _ = _vnc_open(path, [0, EXTENDED_DESKTOP_SIZE])
    try:
        s.sendall(struct.pack(">BxHHBxIHHHHI", SET_DESKTOP_SIZE, width, height, 1,
                              0, 0, 0, width, height, 0))
        while True:
            kind = _recv_exact(s, 1)[0]
            if kind == 2:  # bell
                continue
            if kind == 3:  # cut text
                _recv_exact(s, 3)
                _recv_exact(s, struct.unpack(">I", _recv_exact(s, 4))[0])
                continue
            if kind != 0:
                raise RuntimeError("unexpected VNC message %d" % kind)
            _recv_exact(s, 1)
            (nrects,) = struct.unpack(">H", _recv_exact(s, 2))
            for _ in range(nrects):
                reason, status, rw, rh, enc = struct.unpack(">HHHHi", _recv_exact(s, 12))
                if enc == EXTENDED_DESKTOP_SIZE:
                    screens = _recv_exact(s, 4)[0]
                    _recv_exact(s, 16 * screens)
                    # reason 1: this client's request; status 0 or 4: taken
                    if reason == 1:
                        if status not in (0, 4):
                            raise RuntimeError("the display at %s refused %dx%d: status %d"
                                               % (rel(path), width, height, status))
                        return
                elif enc == 0:
                    _recv_exact(s, rw * rh * 4)
                else:
                    raise RuntimeError("unexpected VNC encoding %d" % enc)
    finally:
        s.close()


def vnc_capture(path):
    """Read one full frame from QEMU's VNC server (RFB 3.8, no auth, raw)."""
    s, w, h = _vnc_open(path, [0])  # raw encoding only
    try:
        s.sendall(struct.pack(">BBHHHH", 3, 0, 0, 0, w, h))
        frame = bytearray(w * h * 3)
        covered = 0
        while covered < w * h:
            kind = _recv_exact(s, 1)[0]
            if kind == 2:  # bell
                continue
            if kind == 3:  # cut text
                _recv_exact(s, 3)
                _recv_exact(s, struct.unpack(">I", _recv_exact(s, 4))[0])
                continue
            if kind != 0:
                raise RuntimeError("unexpected VNC message %d" % kind)
            _recv_exact(s, 1)
            (nrects,) = struct.unpack(">H", _recv_exact(s, 2))
            for _ in range(nrects):
                x, y, rw, rh, enc = struct.unpack(">HHHHi", _recv_exact(s, 12))
                if enc != 0:
                    raise RuntimeError("unexpected VNC encoding %d" % enc)
                raw = _recv_exact(s, rw * rh * 4)
                for row in range(rh):
                    src = raw[row * rw * 4:(row + 1) * rw * 4]
                    line = bytearray(rw * 3)
                    line[0::3], line[1::3], line[2::3] = src[2::4], src[1::4], src[0::4]
                    off = ((y + row) * w + x) * 3
                    frame[off:off + rw * 3] = line
                covered += rw * rh
        return t3img.Image(w, h, bytes(frame))
    finally:
        s.close()


def write_ppm(path, img):
    with open(path, "wb") as f:
        f.write(b"P6\n%d %d\n255\n" % (img.width, img.height))
        f.write(img.data)


_SHIFTED = {'!': '1', '@': '2', '#': '3', '$': '4', '%': '5', '^': '6', '&': '7',
            '*': '8', '(': '9', ')': '0', '_': 'minus', '+': 'equal',
            '{': 'bracket_left', '}': 'bracket_right', '|': 'backslash',
            ':': 'semicolon', '"': 'apostrophe', '~': 'grave_accent',
            '<': 'comma', '>': 'dot', '?': 'slash'}
_PLAIN = {' ': 'spc', '\n': 'ret', '-': 'minus', '=': 'equal',
          '[': 'bracket_left', ']': 'bracket_right', '\\': 'backslash',
          ';': 'semicolon', "'": 'apostrophe', '`': 'grave_accent',
          ',': 'comma', '.': 'dot', '/': 'slash', '\t': 'tab'}


def char_qcodes(ch):
    """The qcodes that type `ch` on a US layout."""
    if ch.isascii() and (ch.islower() or ch.isdigit()):
        return [ch]
    if ch.isascii() and ch.isupper():
        return ["shift", ch.lower()]
    if ch in _PLAIN:
        return [_PLAIN[ch]]
    if ch in _SHIFTED:
        return ["shift", _SHIFTED[ch]]
    raise ValueError("cannot type %r" % ch)


def wait_stable(vm, path, settle=1.0, timeout=60, want=None, output=None):
    """Capture until two captures `settle` seconds apart are identical.

    `want`, if given, is a predicate the stable image must also satisfy.
    `output` names the output captured (Qemu.screendump), by default the
    first. Returns the stable image, saved at `path`.
    """
    deadline = time.time() + timeout
    last = None
    while True:
        img = vm.screendump(path, output)
        digest = hashlib.sha256(img.data).digest()
        if digest == last and (want is None or want(img)):
            return img
        if time.time() > deadline:
            if digest == last:
                raise RuntimeError("screen settled but not in the expected state; "
                                   "last capture at %s" % rel(path))
            raise RuntimeError("screen did not settle; last capture at %s" % rel(path))
        last = digest
        time.sleep(settle)


def not_blank(img):
    return img.data.count(img.data[:3]) * 3 != len(img.data)


def make_iso(srcdir, label, out):
    # xorriso prints a banner even with -quiet; it is kept out of the log,
    # whose lines each name their variant, and shown only on failure.
    r = subprocess.run(["xorriso", "-as", "mkisofs", "-quiet", "-V", label, "-J", "-r",
                        "-o", out, srcdir], capture_output=True, text=True)
    if r.returncode:
        raise RuntimeError("xorriso exited with %d: %s" % (r.returncode, r.stderr.strip()))
