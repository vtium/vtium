# Where vtium runs

vtium runs on Linux PCs and laptops and on small ARM boards. Every build can render on the CPU, so a GPU is optional.

![vtium on a 320×240 display: a ruler of 40 columns, a colored directory listing, the 16 background colors, six color names, bold, faint, underlined, reversed and struck words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, four underline styles, a progress bar at 50%, and a prompt](../showcase/small/small-console.png)

*[The showcase](../showcase/small-machines.md) has more screens like this one.*

The processors are x86_64, aarch64, and armv7. Rendering goes through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager) into dumb buffers, plain memory buffers that the kernel's graphics driver displays. On a machine without DRM, fbdev takes its place, the kernel's older framebuffer interface.

## The GPU path

A build with the GPU path can also render on the machine's GPU, through OpenGL ES 3.0. vtium reaches the GPU through two libraries from [Mesa](https://www.mesa3d.org/), the open-source graphics stack: EGL, which sets up the drawing context, and GBM, which allocates the buffers that the displays show. So the GPU path needs a graphics driver that works with Mesa's EGL and GBM and supports OpenGL ES 3.0. Mesa's own drivers for AMD and Intel graphics are such drivers. vtium's tests run the GPU path on AMD graphics with the `amdgpu` driver, on a virtual machine's GPU through virgl, and on llvmpipe, Mesa's software renderer. vtium is not tested with NVIDIA's own driver.

The `vtium` and `vtium-git` packages include the GPU path and turn it on, and a line `no-hwaccel` in vtium.conf turns it off. When the GPU setup fails for a head, that head renders on the CPU instead, and vtium's log gives the reason in a line that ends `so it draws without the GPU`. A display with 1 bit a pixel always renders on the CPU.

To see which renderer draws each display, read vtium's log. For each display, vtium logs a line that names its renderer, `gpu` or `cpu`, such as `Display DP-1 with backend [drm2d] text renderer [gpu], cell size 10x20`. On VT 2, this command prints those lines:

```
# journalctl -b -u vtium@tty2.service --grep 'text renderer'
```

The GPU path draws the same pixels as the CPU path. vtium's pixel tests check this by rendering each test screen both ways on Mesa's software renderer, and they require identical results.

## How vtium starts

Under systemd, vtium starts on a VT where a getty would. With another init system, it starts where that system started a getty. vtium normally opens the displays and input devices itself, or, in a build with the libseat feature, gets them from seatd where seatd runs. That is direct mode. In seat mode, it gets them from logind instead.

When vtium fails under systemd, a getty takes the VT back. [Security and the system](security-and-system.md#with-systemd-and-logind-and-without-them) covers the exceptions, sessions that survive a restart, and how vtium works with logind.

## Small machines

The smallest build, minspec, targets a board with 512 MB of memory and a 320×240 display. In vtium's virtual machine tests, minspec runs with one CPU and 512 MB of memory. There its binary is about 3.1 MiB, and the running console uses at most 5.81 MiB of memory at 320×240. The full measurement and its source are under [Small machines](../features.md#small-machines).

The `vtium` and `vtium-git` packages build vtium with more features, which adds images, the GPU path, right-to-left text, seat mode, and logind support.
