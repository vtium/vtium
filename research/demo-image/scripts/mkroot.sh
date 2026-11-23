#!/bin/bash
# Throwaway prototype: a vtium demo initramfs from vtium's install tree,
# Debian's static busybox and bash, and the pinned Debian sysroot's libraries.
# Usage: mkroot.sh OUT_DIR
set -euo pipefail
W=/scratch/tmp/claude-1000/vtium-w6-rootfs
TREE=$W/build/tree${GPU:+-gpu}
SYS=$(cd $W/build/vtium && tools/arm/sysroot dir amd64)
DL=$W/dl
OUT=${1:-$W/out}
R=$OUT/root
rm -rf "$R"; mkdir -p "$R"
cd "$R"
mkdir -p bin sbin usr/bin usr/sbin lib/x86_64-linux-gnu lib64 etc/init.d etc/vtium proc sys dev run tmp root home/demo var/log lib/modules usr/share/vtium-demo/pictures
chmod 1777 tmp
# vtium's install tree, as make install-tree staged it.
cp -a "$TREE"/. .
# The C libraries vtium, vtium-config and bash need, from the pinned sysroot.
L=$SYS/usr/lib/x86_64-linux-gnu
for f in ld-linux-x86-64.so.2 libc.so.6 libm.so.6 libpthread.so.0 libdl.so.2 librt.so.1 libgcc_s.so.1 libudev.so.1 libcap.so.2 libxkbcommon.so.0; do
	cp -L "$L/$f" lib/x86_64-linux-gnu/
done
ln -s ../lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 lib64/ld-linux-x86-64.so.2
cp -L $DL/x-libtinfo6/lib/x86_64-linux-gnu/libtinfo.so.6 lib/x86_64-linux-gnu/
cp $DL/x-bash/bin/bash bin/bash
cp $DL/bb/bin/busybox bin/busybox
if [ -n "${GPU:-}" ]; then
	# Mesa's EGL, GBM and DRI drivers with their libraries, from bookworm.
	M=$DL/mesa-root
	cp -a $M/usr/lib/x86_64-linux-gnu/. usr/lib/x86_64-linux-gnu/ 2>/dev/null || { mkdir -p usr/lib/x86_64-linux-gnu; cp -a $M/usr/lib/x86_64-linux-gnu/. usr/lib/x86_64-linux-gnu/; }
	[ -d $M/lib/x86_64-linux-gnu ] && cp -a $M/lib/x86_64-linux-gnu/. lib/x86_64-linux-gnu/
	mkdir -p usr/share && cp -a $M/usr/share/glvnd $M/usr/share/drirc.d usr/share/
	printf 'hwaccel\nrender-timer=log\n' > etc/vtium/vtium.conf
fi
ln -s bin/busybox init
ln -s busybox bin/sh
# The host kernel's virtio-gpu driver and its one dependency, unpacked.
K=$(uname -r)
for m in virtio_dma_buf virtio_input virtio_gpu; do
	zstd -qdc "$(modinfo -k $K -n $m)" > lib/modules/$m.ko
done
[ -n "${NO_XKB_DIR:-}" ] || mkdir -p usr/share/X11/xkb
cp $W/build/vtium/tools/tier3/showcase-photos/*.png usr/share/vtium-demo/pictures/
mkdir -p usr/share/vtium-review && cp -a $W/build/vtium/tools/vt/{accept.sh,common.sh,blink.inc,feedback.sh,sections,checks} usr/share/vtium-review/
cat > etc/inittab <<'EOF'
::sysinit:/etc/init.d/rcS
tty1::respawn:/bin/sh -c 'exec /usr/bin/vtium --vt=tty1 --no-switchvt ${VTIUM_DEBUG} >/dev/ttyS0 2>&1'
tty2::respawn:/bin/sh -c 'exec /usr/bin/vtium --vt=tty2 --no-switchvt ${VTIUM_DEBUG} >/dev/ttyS0 2>&1'
::ctrlaltdel:/bin/reboot
::shutdown:/bin/umount -a -r
EOF
cat > etc/init.d/rcS <<'EOF'
#!/bin/sh
/bin/busybox --install -s
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev 2>/dev/null
mkdir -p /dev/pts /dev/shm
ln -s /proc/self/fd /dev/fd; ln -s fd/0 /dev/stdin; ln -s fd/1 /dev/stdout; ln -s fd/2 /dev/stderr
mount -t devpts -o gid=5,mode=620,ptmxmode=666 devpts /dev/pts
mount -t tmpfs tmpfs /dev/shm
mount -t tmpfs -o mode=755 tmpfs /run
for m in virtio_dma_buf virtio_input virtio_gpu; do insmod /lib/modules/$m.ko; done
chown -R 1000:1000 /home/demo
hostname vtium-demo
(sleep 4; { echo DIAG; ps | grep -v '\['; dmesg | grep -i -E 'segfault|vtium|drm'; } >/dev/ttyS0 2>&1) &
echo "rcS: done at $(cut -d' ' -f1 /proc/uptime) s" > /dev/ttyS0
EOF
chmod 755 etc/init.d/rcS
cat > etc/passwd <<'EOF'
root:x:0:0:root:/root:/bin/sh
demo::1000:1000:vtium demo:/home/demo:/bin/bash
vtium-image:x:900:900:vtium image helper:/:/bin/false
nobody:x:65534:65534:nobody:/:/bin/false
EOF
cat > etc/shadow <<'EOF'
root:*:20000:0:99999:7:::
demo::20000:0:99999:7:::
vtium-image:!:20000::::::
nobody:!:20000::::::
EOF
chmod 600 etc/shadow
cat > etc/group <<'EOF'
root:x:0:
tty:x:5:demo
video:x:44:demo
input:x:104:demo
demo:x:1000:
vtium-image:x:900:
nogroup:x:65534:
EOF
echo /bin/bash > etc/shells; echo /bin/sh >> etc/shells
cat > etc/profile <<'EOF'
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
export TERM=${TERM:-xterm-vtium}
PS1='\u@\h:\w\$ '
EOF
cp etc/profile home/demo/.bashrc
cat > etc/motd <<'EOF'

  Welcome to the VTium demo.

  You are logged in on a Linux virtual terminal, and vtium draws it.
  Ctrl+Alt+F1 and Ctrl+Alt+F2 switch between two vtium consoles.

  Try these:
    vtium-gallery /usr/share/vtium-demo/pictures   pictures through kitty graphics
    vtium-config                                    pick a font set and try it live

EOF
chown -R 1000:1000 home/demo 2>/dev/null || true
cd "$R"
find . -print0 | cpio --null -o -H newc -R +0:+0 --quiet | zstd -q -19 -T0 > "$OUT/initramfs.cpio.zst"
# The demo user's home belongs to uid 1000: cpio's -R sets every owner to root,
# so rcS-style fixes would be needed; the prototype accepts root ownership.
ls -la "$OUT/initramfs.cpio.zst"
du -sh "$R"
