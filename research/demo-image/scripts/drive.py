#!/usr/bin/env python3
"""Throwaway prototype driver: boots the demo initramfs in QEMU/KVM headless,
logs in on VT 1, runs a few commands, and takes screendumps.

Usage: drive.py OUT_DIR [extra qemu args...]
"""
import json, os, socket, subprocess, sys, time, tempfile

out = sys.argv[1]
extra = sys.argv[2:]
rt = tempfile.mkdtemp(prefix="w6-", dir=os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
qmp_path = os.path.join(rt, "qmp")
serial = os.path.join(out, "serial.log")
cmd = ["qemu-system-x86_64", "-enable-kvm", "-machine", "q35", "-cpu", "host", "-smp", "1", "-m", "512",
       "-kernel", "/boot/vmlinuz-linux", "-initrd", os.path.join(out, "initramfs.cpio.zst"),
       "-append", "console=ttyS0 quiet loglevel=3 rdinit=/init",
       "-device", "virtio-vga,xres=1280,yres=800", "-display", "none",
       "-serial", "file:" + serial, "-qmp", "unix:%s,server=on,wait=off" % qmp_path,
       "-nic", "none"] + extra
t0 = time.monotonic()
q = subprocess.Popen(cmd)
try:
    for _ in range(100):
        if os.path.exists(qmp_path):
            break
        time.sleep(0.05)
    s = socket.socket(socket.AF_UNIX)
    s.connect(qmp_path)
    f = s.makefile("rw")
    f.readline()
    def qmp(execute, **args):
        f.write(json.dumps({"execute": execute, "arguments": args}) + "\n"); f.flush()
        while True:
            r = json.loads(f.readline())
            if "return" in r or "error" in r:
                return r
    qmp("qmp_capabilities")
    def wait_serial(text, timeout=30):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            try:
                if text in open(serial, errors="replace").read():
                    return time.monotonic() - t0
            except FileNotFoundError:
                pass
            time.sleep(0.1)
        raise SystemExit("timeout waiting for %r" % text)
    def shot(name):
        p = os.path.join(out, name + ".ppm")
        qmp("screendump", filename=p)
        return p
    keymap = {" ": "spc", "\n": "ret", "-": "minus", "/": "slash", ".": "dot", "_": "shift-minus", "=": "equal", "|": "shift-backslash", ">": "shift-dot", "'": "apostrophe", ":": "shift-semicolon", ";": "semicolon", ",": "comma"}
    def typ(text):
        for ch in text:
            k = keymap.get(ch)
            if k is None:
                k = ("shift-" + ch.lower()) if ch.isupper() else ch
            keys = [{"type": "qcode", "data": x} for x in k.split("-")]
            qmp("send-key", keys=keys)
            time.sleep(0.03)
    print("rcS done at host %.2f s" % wait_serial("rcS: done"))
    # Wait until vtium on tty1 logs that it drew (its log goes to a file; poll it through the serial shell).
    time.sleep(3)
    shot("01-login")
    typ("demo\n")
    time.sleep(2)
    shot("02-motd")
    typ("vtium-gallery /usr/share/vtium-demo/pictures\n")
    time.sleep(3)
    shot("03-gallery")
    typ("clear\n"); time.sleep(0.5)
    typ("ACCEPT_LABEL=demo bash /usr/share/vtium-review/accept.sh 51 70\n")
    time.sleep(2)
    shot("04-accept-title")
    typ("\n"); time.sleep(2)
    shot("04-accept-page1")
    # Ctrl+Alt+F2: the second vtium.
    qmp("send-key", keys=[{"type": "qcode", "data": "ctrl"}, {"type": "qcode", "data": "alt"}, {"type": "qcode", "data": "f2"}])
    time.sleep(2)
    shot("05-vt2")
    qmp("send-key", keys=[{"type": "qcode", "data": "ctrl"}, {"type": "qcode", "data": "alt"}, {"type": "qcode", "data": "f1"}])
    time.sleep(1)
    shot("06-back-vt1")
    qmp("quit")
finally:
    try:
        q.wait(timeout=10)
    except subprocess.TimeoutExpired:
        q.kill()
    try:
        os.unlink(qmp_path); os.rmdir(rt)
    except OSError:
        pass
