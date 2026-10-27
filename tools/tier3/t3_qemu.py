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
import socket
import struct
import subprocess
import time

import t3img
from t3_common import rel

# The machine provisioning boots; each variant names its own (t3_scenario.py).
CPUS = 4
MEMORY_MB = 4096

# A unix socket path, with its terminating NUL, must fit sun_path (108 bytes).
SUN_PATH_MAX = 107

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
                 capture="screendump", cpus=CPUS, memory_mb=MEMORY_MB):
        os.makedirs(workdir, exist_ok=True)
        self.workdir = workdir
        self.log = log
        self.capture = capture
        self.sock_path = os.path.join(workdir, "qmp.sock")
        self.vnc_path = os.path.join(workdir, "vnc.sock")
        # ttyS0 is the guest's console. ttyS1 carries the scenario's events
        # (guest/setup), so the driver can wait for a login or a drawn screen.
        self.serial = os.path.join(workdir, "serial.log")
        self.events = os.path.join(workdir, "events.log")
        for p in (self.sock_path, self.vnc_path):
            if len(os.fsencode(p)) > SUN_PATH_MAX:
                raise RuntimeError("socket path is %d bytes, over the %d a unix "
                                   "socket allows; use a checkout with a shorter "
                                   "path: %s" % (len(os.fsencode(p)), SUN_PATH_MAX, p))
        for p in (self.sock_path, self.vnc_path, self.serial, self.events):
            if os.path.exists(p):
                os.unlink(p)
        args = ["qemu-system-x86_64", "-machine", "q35,accel=kvm", "-cpu", "host",
                "-smp", str(cpus), "-m", str(memory_mb), "-no-reboot",
                "-display", display, "-device", device,
                "-qmp", "unix:%s,server=on,wait=off" % self.sock_path,
                "-serial", "file:%s" % self.serial,
                "-serial", "file:%s" % self.events]
        for path, fmt, ro in disks:
            args += ["-drive", "file=%s,if=virtio,format=%s%s"
                     % (path, fmt, ",readonly=on" if ro else "")]
        args += ["-nic", "user,model=virtio-net-pci"] if net else ["-nic", "none"]
        if capture == "vnc":
            args += ["-vnc", "unix:%s" % self.vnc_path]
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
        try:
            self._connect()
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

    def screendump(self, path):
        """Capture the screen to a PPM at `path` and return it."""
        if self.capture == "vnc":
            img = vnc_capture(self.vnc_path)
            write_ppm(path, img)
            return img
        tmp = path + ".tmp"
        self.cmd("screendump", filename=tmp, format="ppm")
        # screendump returns before the file is complete on some backends.
        for _ in range(100):
            try:
                img = t3img.read(tmp)
                os.replace(tmp, path)
                return img
            except (ValueError, IndexError, OSError):
                time.sleep(0.1)
        raise RuntimeError("screendump did not produce a readable image")

    def wait_serial(self, marker, timeout, log_path=None):
        """Wait for a line holding `marker` on the console log (or on
        `log_path`), terminal escape sequences left out, and return it.
        Only lines ending in a newline count: the guest writes a line a few
        bytes at a time, and the part of one that has arrived is not yet
        the line (#61)."""
        log_path = log_path or self.serial
        deadline = time.time() + timeout
        while True:
            with open(log_path, "rb") as f:
                data = f.read()
            data = data[:data.rfind(b"\n") + 1]
            text = _ESCAPES.sub("", data.decode(errors="replace"))
            for line in text.splitlines():
                if marker in line:
                    return line.strip()
            if self.proc.poll() is not None or time.time() > deadline:
                break
            time.sleep(0.2)
        raise RuntimeError("no %r within %d s; see %s" % (marker, timeout, rel(log_path)))

    def wait_event(self, marker, timeout):
        """Wait for the guest to report `marker` on its event port."""
        return self.wait_serial(marker, timeout, log_path=self.events)

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
        self.qemu_log.close()


def _recv_exact(sock, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(min(n - len(buf), 1 << 20))
        if not chunk:
            raise RuntimeError("VNC connection closed")
        buf += chunk
    return bytes(buf)


def vnc_capture(path):
    """Read one full frame from QEMU's VNC server (RFB 3.8, no auth, raw)."""
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
        s.sendall(struct.pack(">BxHi", 2, 1, 0))  # raw encoding only
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


def wait_stable(vm, path, settle=1.0, timeout=60, want=None):
    """Capture until two captures `settle` seconds apart are identical.

    `want`, if given, is a predicate the stable image must also satisfy.
    Returns the stable image, saved at `path`.
    """
    deadline = time.time() + timeout
    last = None
    while True:
        img = vm.screendump(path)
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
