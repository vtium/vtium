# Where vtium runs

vtium runs on Linux, on x86_64, aarch64, and armv7 processors. It renders through KMS/DRM into memory buffers that the kernel's graphics driver displays. On a machine without DRM, it uses fbdev instead, the kernel's older framebuffer interface. Every build can render on the CPU, so a GPU is optional.

A build with the GPU path renders through OpenGL ES 3.0, using EGL and GBM from Mesa. The `vtium` and `vtium-git` packages turn the GPU path on, and a line `no-hwaccel` in vtium.conf turns it off. When the GPU setup fails for a head, vtium renders that head on the CPU. The GPU path draws the same pixels as the CPU path. vtium's pixel tests check this by rendering each test screen both ways on Mesa's software renderer, and they require identical results.

## How vtium starts

Under systemd, vtium starts on a VT where a getty would. With another init system, vtium starts where that system started a getty. In seat mode, vtium gets its displays and input devices from seatd or logind rather than opening them itself.

When vtium fails under systemd, its unit starts a getty on the VT, so a login prompt returns. There are two exceptions. vtium starts no getty under a closed policy, where you have chosen that no keyboard outside your rules may type. It also starts none when the getty would stop another program, such as a display manager. [systemd, logind and PID 1](../systemd.md) covers each place vtium touches systemd, and what vtium does without it.

## What runs as root

vtium itself runs as root, in direct mode and in seat mode alike. It needs root to open the VT, take control of the displays, read the input devices, and start `/bin/login`. Two helpers parse fonts and images for it, each inside a sandbox, and neither runs as root. The font helper runs as the user `nobody` and refuses to run as root. The image helper runs as the user logged in on that console; before anyone logs in, it reads no files at all. [Security](../concepts/security.md) explains the design and where its protection ends.

## Small machines

The smallest build, minspec, targets a board with 512 MB of memory and a 320×240 display. vtium's virtual machine tests run minspec with one CPU and 512 MB of memory. There its console binary is about 3.1 MiB, and the running console uses at most 5.81 MiB of memory at 320×240. [Small machines](../features.md#small-machines) gives the full measurement and its source.

The `vtium` and `vtium-git` packages build a larger console, which adds images, the power keys, the GPU path, right-to-left text, and seat mode.
