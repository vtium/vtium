# Where vtium runs

vtium runs on Linux PCs and laptops and on small ARM boards. Every build can render on the CPU, so a GPU is optional.

The processors are x86_64, aarch64, and armv7. Rendering goes through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager) into dumb buffers, plain memory buffers that the kernel's graphics driver displays. On a machine without DRM, fbdev takes its place, the kernel's older framebuffer interface.

## The GPU path

A build with the GPU path can also render on the machine's GPU, through OpenGL ES 3.0, using EGL and GBM from Mesa. The `vtium` and `vtium-git` packages include the GPU path and turn it on, and a line `no-hwaccel` in vtium.conf turns it off. When the GPU setup fails for a head, that head renders on the CPU instead. The GPU path draws the same pixels as the CPU path. vtium's pixel tests check this by rendering each test screen both ways on Mesa's software renderer, and they require identical results.

## How vtium starts

Under systemd, vtium starts on a VT where a getty would. With another init system, it starts where that system started a getty. vtium normally opens the displays and input devices itself, which is direct mode. In seat mode, it gets them from seatd or logind instead.

When vtium fails under systemd, a getty takes the VT back. [Security and the system](security-and-system.md#with-systemd-and-logind-and-without-them) covers the exceptions, sessions that survive a restart, and how vtium works with logind.

## Small machines

The smallest build, minspec, targets a board with 512 MB of memory and a 320×240 display. In vtium's virtual machine tests, minspec runs with one CPU and 512 MB of memory. There its console binary is about 3.1 MiB, and the running console uses at most 5.81 MiB of memory at 320×240. The full measurement and its source are under [Small machines](../features.md#small-machines).

The `vtium` and `vtium-git` packages build a larger console, which adds images, the power keys, the GPU path, right-to-left text, and seat mode.
