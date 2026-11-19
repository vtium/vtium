# The shader timing harness

The harness times spatial shaders, the GPU path's frame filters, before vtium builds them. It draws each shader over a terminal-like frame at a display's size, off-screen, and reports how long the GPU takes for each frame. The maintainer sets the cap on each pass's reads from these numbers, and they show what the frame-rate floor rejects on each GPU. ADR-1304 D18 asks for the measurement, and its dated note holds the results.

The harness is an OpenGL ES 3.0 program. It runs on a GPU's render node through EGL with no surface, so it needs no DRM master, no display mode and no VT. You can run it on a desktop while the desktop runs.

## Running it

```sh
make shader-timing                                  # the GPU Mesa picks
make shader-timing DEVICE=/dev/dri/renderD128       # the GPU behind a render node
make shader-timing DEVICE=software ARGS='--sizes 1920x1080 --frames 5'
```

`DEVICE` is `surfaceless`, `software` (Mesa's llvmpipe, which runs on the CPU) or a render node's path. `ls -l /dev/dri/by-path` shows which render node belongs to which PCI device. Each run prints its rows and writes them to `target/shader-timing/DEVICE.tsv`. `ARGS` passes more options:

| Option | What it sets |
|---|---|
| `--sizes 1920x1080,3840x2160` | The frame sizes, the default being these two. |
| `--modes plain,smooth,ghostty,gl-linear` | The read modes, described below. All four run by default. |
| `--shaders crt,bloom,crt+bloom` | The plans to time. `crt+bloom` names the two chains. |
| `--frames 31 --warmup 3` | Frames timed for each row, after frames drawn and not timed. |
| `--refresh 60` | The floor in frames a second, and the rate the shaders' time advances at. |

A long draw holds the GPU it runs on. When that GPU also runs your desktop, the desktop stalls for as long as the draw takes. The kernel's amdgpu driver resets a GPU whose job runs past 10 seconds. On a slow GPU, time the costliest plans at the smaller size first, with few frames.

## What it draws

The frame is made-up text: a dark background, with most character cells holding a glyph of lit pixels in one of five colors. Every shader reads that frame and writes a target of the same size, both RGBA8 textures.

The shaders are Ghostty-style files under `shaders/`, each with a `mainImage` that reads the frame through `texture(iChannel0, uv)`. A header of `// timing:` lines states each shader's reads for each pixel, the rest of its work, and where the file comes from. `crt.glsl` (Unlicense) and `vhs.glsl` (MIT) are files of the surveyed collection, kept with their license headers. The only change is one int literal in `vhs.glsl`, which GLSL ES 3.00 does not convert to a float. The other files carry no license that lets vtium's tree hold them. The harness carries a stand-in for each of them, written from the survey's description of the shader. A stand-in keeps the original's reads for each pixel and the shape of its loop, and none of its code. `shader-timing list` prints every plan with its reads, its work and its origin.

The harness rewrites each read of `iChannel0` into a function whose body depends on the read mode:

| Mode | How a read reads the frame |
|---|---|
| `plain` | One `texelFetch` of the pixel whose square holds the place, vtium's `SPATIAL_IN`. |
| `smooth` | Four `texelFetch`es blended in the shader. ADR-1304 D18 proposed this for `SPATIAL_SMOOTH` until the maintainer chose the GPU's filter on 2026-11-18. |
| `ghostty` | The smooth read, with each pixel decoded from sRGB to linear light, and the output encoded again. ADR-1304 D20 proposed this for Ghostty's dialect until the maintainer chose an sRGB copy of the input on 2026-11-18. |
| `gl-linear` | One `texture` read through the GPU's own linear filtering, as Ghostty reads and as vtium's `SPATIAL_SMOOTH` reads. |

Every read turns the place from Ghostty's coordinates, which start at the bottom left, into vtium's, which start at the top left. Each read is also held within 256 pixels of the pixel being written, the cap on reach.

The `reads` column counts each read as 1 in every mode, as vtium's checker counts a smooth read. The results files of 2026-11-18 have a `texels` column in its place, which counts a smooth read in the `smooth` and `ghostty` modes as 4, as ADR-1304 then did.

Two plans chain crt and bloom. In one pass, each read bloom makes runs crt again at that place, so the reads multiply: 8 times 25 makes 200. ADR-1304 D3 therefore gives each filter that reads other pixels a pass of its own, since one shared pass costs about nine times as much as two. In two passes, crt draws into a texture and bloom reads that texture. Their reads add up to 33, and the cap of 25 holds each pass.

## How it times a frame

The harness calls `glFinish`, reads the CPU's clock, draws the frame, calls `glFinish` again and reads the clock again. The second `glFinish` waits for the GPU to finish the frame, so the time covers the work and not only its submission. Where the driver has `GL_EXT_disjoint_timer_query`, a `GL_TIME_ELAPSED_EXT` query around the draws gives the GPU's own time as well.

Each row reports the median and the largest frame on both clocks. The rate in `fps`, and whether it meets the floor in `meets`, come from the median frame on the CPU's clock. That clock is right on every driver the harness has run on. llvmpipe offers timer queries, but they report a fraction of a heavy frame: 8.7 ms for a fireworks-rockets frame that takes 57.5 ms. On the two GPUs measured, the CPU's clock reads at most 0.25 ms more than the GPU's for a frame of one pass, the time to submit it included.

## Its tests

`make test-tools` runs the harness's tests on llvmpipe. The tests check that every plan compiles as GLSL ES 3.00 and draws in every read mode. A test draws the negative shader in three read modes and checks pixels against the frame, which catches a read that flips the picture. Another times a shader that runs 40 rounds of a hash and one that runs 640. It checks that the second takes well over four times as long on both clocks, and that the GPU's clock never reads more than the CPU's. A clock that stops before the GPU finishes fails that test. The test does not hold the GPU's clock to a share of the CPU's. llvmpipe's query covers only part of a frame, and the part depends on how many threads llvmpipe draws with. With 4 threads, as on CI's runners, the query reads about a quarter of the frame.

The harness is a workspace of its own, with its own `Cargo.lock`, so the main workspace and CI do not build it. `make deny` checks its lock file.
