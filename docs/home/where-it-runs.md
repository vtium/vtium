# Where vtium runs

vtium runs on Linux, on x86_64, aarch64, and armv7 processors. It renders into memory buffers that the kernel's graphics driver displays, or through fbdev, the kernel's older framebuffer interface, on a machine without DRM. Every build can render on the CPU, so a GPU is optional. On supported hardware, a build with the GPU path renders through OpenGL ES 3.0 and produces the same pixels as the CPU.

## How vtium starts

Under systemd, vtium starts on a VT where a getty would. With another init system, vtium starts where that system started a getty. If vtium cannot start, getty and the kernel's console take over the VT. In seat mode, vtium gets the displays and input devices from seatd or logind. [systemd, logind and PID 1](../systemd.md) covers each place vtium touches systemd, and what vtium does without it.

## Small machines

The smallest build, minspec, targets a board with 512 MB of memory and a 320×240 display. vtium's virtual machine tests run minspec with one CPU and 512 MB of memory. There the console binary is 3,216,864 bytes, and the running console uses at most 5.81 MiB of memory at 320×240. [Small machines](../features.md#small-machines) gives the measurement and its source.

The `vtium` and `vtium-git` packages build a larger console, which adds four features that minspec leaves out: images, right-to-left text, the GPU path, and seat mode.
