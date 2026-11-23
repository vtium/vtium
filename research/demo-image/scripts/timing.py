import json, os, socket, subprocess, sys, time, tempfile
out = sys.argv[1]; mem = sys.argv[2] if len(sys.argv) > 2 else "512"
rt = tempfile.mkdtemp(prefix="w6t-", dir=os.environ.get("XDG_RUNTIME_DIR", "/tmp")); qp = os.path.join(rt, "qmp")
cmd = ["qemu-system-x86_64", "-enable-kvm", "-machine", "q35", "-cpu", "host", "-smp", "1", "-m", mem,
       "-kernel", "/boot/vmlinuz-linux", "-initrd", os.path.join(out, "initramfs.cpio.zst"),
       "-append", "console=ttyS0 quiet loglevel=3", "-device", "virtio-vga,xres=1280,yres=800",
       "-display", "none", "-serial", "null", "-qmp", "unix:%s,server=on,wait=off" % qp, "-nic", "none"]
t0 = time.monotonic(); q = subprocess.Popen(cmd)
while not os.path.exists(qp): time.sleep(0.01)
s = socket.socket(socket.AF_UNIX); s.connect(qp); f = s.makefile("rw"); f.readline()
def qmp(e, **a):
    f.write(json.dumps({"execute": e, "arguments": a}) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r: return r
qmp("qmp_capabilities"); p = os.path.join(rt, "s.ppm")
while time.monotonic() - t0 < 30:
    qmp("screendump", filename=p)
    try:
        d = open(p, "rb").read()
        body = d[d.index(b"255\n") + 4:]
        # vtium's login prompt: some lit pixels in the top row of text, and the 1280x800 mode set.
        if b"1280 800" in d[:20] and body.count(b"\x00") < len(body) - 1000:
            print("login prompt drawn %.2f s after QEMU started (mem %s MB)" % (time.monotonic() - t0, mem)); break
    except (OSError, ValueError): pass
    time.sleep(0.05)
qmp("quit"); q.wait(); os.unlink(p) if os.path.exists(p) else None
