# Where vtium runs

vtium runs on Linux, on x86_64, aarch64, and armv7 processors. It renders through KMS/DRM into memory buffers that the kernel's graphics driver displays. On a machine without DRM, it uses fbdev instead, the kernel's older framebuffer interface. Every build can render on the CPU, so a GPU is optional.

A build with the GPU path renders through OpenGL ES 3.0, using EGL and GBM from Mesa. The `vtium` and `vtium-git` packages turn the GPU path on, and a line `no-hwaccel` in vtium.conf turns it off. When the GPU setup fails for a head, vtium renders that head on the CPU. The GPU path draws the same pixels as the CPU path. vtium's pixel tests check this by rendering each test screen both ways on Mesa's software renderer, and they require identical results.

## How vtium starts

Under systemd, vtium starts on a VT where a getty would. With another init system, vtium starts where that system started a getty. In seat mode, vtium gets its displays and input devices from seatd or logind rather than opening them itself.

When vtium fails under systemd, a getty takes the VT back. [Security and the system](security-and-system.md#with-systemd-and-logind-and-without-them) covers the exceptions, sessions that survive a restart, and how vtium works with logind.

## Small machines

The smallest build, minspec, targets a board with 512 MB of memory and a 320×240 display. vtium's virtual machine tests run minspec with one CPU and 512 MB of memory. There its console binary is about 3.1 MiB, and the running console uses at most 5.81 MiB of memory at 320×240. [Small machines](../features.md#small-machines) gives the full measurement and its source.

The `vtium` and `vtium-git` packages build a larger console, which adds images, the power keys, the GPU path, right-to-left text, and seat mode.
