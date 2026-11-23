# Demo image research prototype

These files are a research prototype for #466, the demo image that boots into vtium. They are kept for reference and are not built, tested or shipped.

- `report.md`: the research report of 2026-11-22: the options compared, vtium's runtime needs and what provides each, the review inside the image, a motd sketch, the measured results, the risks, and the proposed cards.
- `scripts/mkroot.sh`: packs `make install-tree`, Debian's static busybox, bash and the pinned bookworm libraries into one zstd initramfs. Its paths point at the prototype's scratch folder.
- `scripts/drive*.py` and `scripts/timing.py`: boot the image in QEMU, type through QMP, take screendumps and time the boot, for the CPU image, the GPU image under virgl, and Mesa's software renderer.
- `guest/`: the prototype's `inittab` and `motd`.
- `mesa-list.txt`: the bookworm Mesa packages the GPU flavour needs.
- `shots/`: screendumps from the CPU image in QEMU.

The cards in #466 replace these with a builder under `tools/demo/`, pinned inputs and tests.
