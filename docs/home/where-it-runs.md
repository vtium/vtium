# Where vtium runs

vtium runs on Linux, on x86_64, aarch64, and armv7 processors. vtium renders into memory buffers that the kernel's graphics driver displays, or through fbdev on a machine without DRM. Every build can render on the CPU, so a GPU is optional. On supported hardware, a build with the GPU path renders through OpenGL ES 3.0. The GPU path draws the same pixels as the CPU.

## How vtium starts

With systemd, vtium starts on a VT where a getty would start. Without systemd, your init system starts vtium where the init system started a getty. If vtium cannot start, getty and the kernel's console take over the VT. In seat mode, vtium gets the displays and input devices from seatd or logind. See [systemd, logind and PID 1](../systemd.md) for each place vtium touches systemd, and what vtium does without systemd.

## Small machines

The smallest build, minspec, is built for a board with 512 MB of memory and a 320×240 display. vtium's virtual machine tests measure minspec with one CPU and 512 MB of memory. There, the minspec console binary is 3,216,864 bytes, and the console uses at most 5.81 MiB of memory at 320×240. See [Small machines](../features.md#small-machines) for the measurement and its source.

The `vtium` and `vtium-git` packages build a larger console. The larger console adds four features that minspec leaves out: images, right-to-left text, the GPU path, and seat mode.
