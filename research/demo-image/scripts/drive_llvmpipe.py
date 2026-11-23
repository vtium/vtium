#!/usr/bin/env python3
"""Throwaway: boot the GPU flavour under virgl (virtio-vga-gl, egl-headless on
the host render node), log in, show the gallery, capture over VNC.
Usage: drive_gpu.py OUT_DIR"""
import json, os, socket, subprocess, sys, time, tempfile
sys.path.insert(0, "/scratch/tmp/claude-1000/vtium-w6-rootfs/build/vtium/tools/tier3")
import t3_qemu, t3img  # noqa: E402

out = sys.argv[1]
rt = tempfile.mkdtemp(prefix="w6g-", dir=os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
qp, vp = os.path.join(rt, "qmp"), os.path.join(rt, "vnc")
serial = os.path.join(out, "serial.log")
cmd = ["qemu-system-x86_64", "-machine", "q35,accel=kvm", "-cpu", "host", "-smp", "2", "-m", "1024",
       "-kernel", "/boot/vmlinuz-linux", "-initrd", os.path.join(out, "initramfs.cpio.zst"),
       "-append", "console=ttyS0 quiet loglevel=3 VTIUM_DEBUG=--debug",
       "-device", "virtio-vga,xres=1280,yres=800",
       "-display", "none",
       "-vnc", "unix:" + vp, "-serial", "file:" + serial,
       "-qmp", "unix:%s,server=on,wait=off" % qp, "-nic", "none"]
t0 = time.monotonic()
q = subprocess.Popen(cmd)
try:
    while not os.path.exists(qp):
        time.sleep(0.02)
    s = socket.socket(socket.AF_UNIX); s.connect(qp); f = s.makefile("rw"); f.readline()
    def qmp(e, **a):
        f.write(json.dumps({"execute": e, "arguments": a}) + "\n"); f.flush()
        while True:
            r = json.loads(f.readline())
            if "return" in r or "error" in r:
                return r
    qmp("qmp_capabilities")
    def wait_serial(text, timeout=60):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            try:
                if text in open(serial, errors="replace").read():
                    return time.monotonic() - t0
            except FileNotFoundError:
                pass
            time.sleep(0.1)
        return None
    def typ(text):
        km = {" ": "spc", "\n": "ret", "-": "minus", "/": "slash", ".": "dot"}
        for ch in text:
            qmp("send-key", keys=[{"type": "qcode", "data": km.get(ch, ch)}]); time.sleep(0.03)
    def shot(name):
        img = t3_qemu.vnc_capture(vp)
        with open(os.path.join(out, name + ".ppm"), "wb") as fh:
            fh.write(b"P6\n%d %d\n255\n" % (img.width, img.height) + img.data)
    print("first Display line at %s s" % wait_serial("Display "))
    time.sleep(3)
    shot("g01-login")
    typ("demo\n"); time.sleep(2)
    typ("vtium-gallery /usr/share/vtium-demo/pictures\n"); time.sleep(4)
    shot("g02-gallery")
    qmp("quit")
finally:
    try:
        q.wait(timeout=10)
    except subprocess.TimeoutExpired:
        q.kill()
