---
contract: adr/v1
kind: decision
verb: add
capability: images
basis:
  - operator: aaronsb
    level: directed
    said: "Everything joins this wave: sixel, kitty basic, the image helper, the compositor, AND kitty relative placements, Unicode placeholders, kitty animation, and iTerm2's image format, each after its record."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 2), relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "R3, a new record for kitty animation: how frames are stored and charged against the budget (ADR-1100 D5), how they compose (kitty's frame composition rules), who owns the timing (time may enter only in a named place - docs/architecture.md's determinism rules; the record decides how a playing animation stays out of the store's state so replays stay deterministic), and how gaps/loops/stop work."
    via: the team lead's brief for the image records, 2026-11-01, carrying the maintainer's direction for this wave
  - operator: aaronsb
    level: directed
    said: "Kitty's stored-frame animation gets kitty's separate allowance of five times the console's image budget, so stored frames never evict still pictures. Pictures that programs re-send frame by frame (mpv, timg) stay in the one budget. The C reference never stored frames: it played video only through re-sent frames under one budget of about four screens."
    via: the maintainer, 2026-11-01, in the steering session, relayed by the team lead in fix round 1 of the image records; the wording is the team lead's
    paraphrase: true
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol), read 2026-11-01: Animation, Transferring animation frame data, Controlling animations, Composing animation frames, the control data reference's keys for frame loading, composition and control, and d=f and d=F; every animation escape code must specify i or I; frames composed onto a canvas of a color or of an earlier frame, by alpha blend or by replacement; gaps in milliseconds, 0 ignored, negative gapless, 40 by default and 0 for the root frame; s=1 stop, s=2 run waiting for more frames, s=3 run and loop; v=0 ignored, v=1 loop forever, v=n loop n-1 times; stopping resets the loop count; a=c answers ENOENT for a missing frame or image and EINVAL for a rectangle out of bounds or overlapping itself in one frame; kitty keeps frame data under a separate quota of five times the base quota"
  - upstream: "libtsm eeaf293 (aaronsb/libtsm), src/tsm/tsm-kitty.c:315-340: the C reference answered a=f, a=a and a=c with EINVAL, and d=f and d=F deleted nothing, leaving animation to step 5 of kmscon ADR-103"
  - evidence: "docs/architecture.md, Determinism: the screen, the scrollback, the modes, the cursor, the image store and the replies follow from the bytes, with time entering only at named places; the helper's 2-second timer and mode 2026's timeout are named there"
  - evidence: "crates/vtium/src/blink.rs: the blink timers run in the binary, flip a phase only the renderer reads, and redraw, without changing the terminal's state"
  - precedent: ADR-1100
  - precedent: ADR-1101
  - precedent: ADR-1300
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Kitty's stored-frame animation gets kitty's separate allowance of five times the console's image budget, so stored frames never evict still pictures. Pictures that programs re-send frame by frame, as mpv and timg do, stay in the one budget."
    via: the maintainer, 2026-11-01, in the steering session
    paraphrase: true
    covers: [frames in the budget]
  - operator: aaronsb
    said: "A stopped animation stays put: the record's lean is taken."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
    covers: [a stopped animation stays put]
  - operator: aaronsb
    said: "Yes: a stopped animation keeps the frame that was showing at the moment it was stopped."
    via: the maintainer, 2026-11-02, in the steering session, relayed by the team lead in its brief for the image records' amendments; the wording is the team lead's
    paraphrase: true
    covers: [a stopped animation stays put]
  - operator: aaronsb
    said: "Composition is a hybrid, composed on demand. The store keeps each frame as the program sent it, a change on a base frame with its blend and offset, and composes a frame into whole pixels only when it is about to be shown, keeping a small bounded cache of composed frames. Composition happens once, on the CPU, in the store or the kitty handler's frame code, before any renderer, so the CPU and GPU renderers still draw whole pixels and tier 2's byte-for-byte rule holds. The store may compose a small or short animation at arrival instead. Animated GIFs stay composed by the image helper into whole frames. The reason: memory for animations, while keeping the renderers simple."
    via: the maintainer, in the steering session's interview, 2026-11-02, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "The compose-on-arrival limits (a whole frame of 64 KiB or less, a change covering half the picture or more, more than 8 layers) and a cache of 2 composed frames per image are accepted as starting values, held as named constants."
    via: the maintainer, in the steering session's interview, 2026-11-02
status: accepted
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1102: Kitty animation frames are stored as the program sent them and composed before drawing, and a clock outside the store plays them

## Summary

- **Decided:** vtium builds kitty animation as the kitty spec describes it. An image's own pixels are its first frame, the *root frame*. The store keeps each later frame as the program sent it, as a change laid on an earlier frame or on a color, with each change's blend and offset, and composes the frame into whole pixels only when it is about to be shown, keeping the last few composed frames of each image in a small cache with a fixed bound (the maintainer, 2026-11-02). A small picture, a change that covers most of the picture, and a change that would sit too many layers above whole pixels are composed when they arrive instead (D1). Composition happens once, on the CPU, in vtium-image's frame code beside the store and before any renderer, so the CPU and GPU renderers both draw whole pixels. Frames past the root, with each image's cache at its full bound, are charged to a frame allowance of their own, five times the console's image budget, as kitty keeps its frames under a separate quota of five times its base, so stored frames never evict still pictures. Pictures a program re-sends frame by frame, as mpv and timg do, are ordinary pictures in the one image budget. An animated GIF that a program sends as an iTerm2 picture is stored and played the same way, its frames composed into whole pixels by the image helper (ADR-1103). The store keeps only what the stream commands: the frames, their gaps, whether the animation is stopped, running or waiting for frames, how many loops to play, and which frame a command last made current. It never records which frame is on the screen now. That is the job of the *animation clock*, which lives in the binary beside the blink timers and is the one place time enters for animation: it picks the frame each playing image shows and hands that choice to the head's snapshot, so a replay, which has no clock, prints the same store state however long the stream took to arrive.
- **Trades away:** Showing a frame that is not in the cache costs a composition on the console's main thread, up to nine times the picture's pixels in writes. Each image that holds a change is charged its cache's full bound, two whole frames, whether or not it ever shows. A frame that other frames are laid on cannot change in place: before an edit changes it, the store composes the frames laid on it into whole pixels, which costs memory and work at that command. A console may hold its image budget plus five times that in frames, so a machine with little memory and a large budget can run short. Which frame a stopped animation keeps on screen depends on when it was stopped, so tier 1 cannot judge what an animation shows, only what it stores.
- **One-way?** No while nothing is built. It becomes expensive once replay goldens print the store's animation state and programs rely on the replies.
- **Probes:**
  - *Confident (frames in the budget):* an animation's frames past the root take a frame allowance of their own, five times the console's image budget, as kitty keeps its frames under a separate quota of five times its base quota. A long animation therefore never pushes a still picture out of the scrollback, and a frame that does not fit the allowance is refused with ENOSPC. Pictures that a program re-sends frame by frame, as mpv and timg do, stay in the one image budget. (Answered 2026-11-01; see `considered`.)
  - *Confident (a stopped animation stays put):* when a program stops an animation, the screen keeps the frame that was showing at that moment, rather than jumping back to a frame the program named earlier. Yes or no? (Answered 2026-11-02; see `considered`.)

  > Edited 2026-11-02: the maintainer asked that the probe on frames state the allowance decided on 2026-11-01, which D1 already carries. It read: "*Not confident (frames in the budget):* an animation's frames count against the same image budget as every other picture, so a long animation can push older pictures out of the scrollback, and one that does not fit is refused with ENOSPC. kitty keeps frames on disk under a separate, larger quota. Is one budget for pictures and frames what you want?"
- **Inversion:** Between "the store plays the animation", which makes its state depend on time, and "the program drives every frame itself", which the spec allows but which leaves `cat`-like programs unable to animate. The answer keeps the store a record of commands and puts playback in a named clock outside it.

> Edited 2026-11-02, the maintainer's decision in the steering session's interview (`considered`): the store keeps each frame as the program sent it and composes it into whole pixels when it is about to be shown, in vtium-image's frame code before any renderer, with a small cache of composed frames whose bound is charged to the frame allowance (D1, D2, D3), and composes small pictures, large changes and deep chains on arrival (D1). The Summary, Trades away, D5, D6, the Consequences and the Alternatives follow, and the title says so. The record composed every frame into whole pixels when its command arrived; the earlier text is in git at f5eefcd7.

> Edited 2026-11-02, the maintainer's decision on animated GIFs: frames also arrive whole from an iTerm2 picture's animated GIF, under the same allowance, clock and rules, and D1 maps a GIF's disposal methods onto D2's composition; D1 also says when an image with frames counts as opaque. The Summary named kitty's frames alone.

> Edited 2026-11-02, third round of fixes: D3's first bullet names the root as the current frame after frames are deleted or freed, as D1 and D5 already said.

> Edited 2026-11-02, after the re-check: `d=f`, `d=F` and frames freed for the allowance set the store's current frame to the root and its run state to stopped, and move both stamps (D1, D3, D5), so the clock tells them from `a=a` with `c` and the store never names a deleted frame. They moved only the frame stamp.

> Edited 2026-11-01, after the fresh review and the maintainer's answers (`considered`): frames past the root take a separate allowance of five times the image budget (D1), where they were charged to the one budget; composed pixels count as input work (D2); the clock answers each command by name, through a frame stamp and a run stamp (D3); `d=f` with neither `i` nor `I` does nothing (D5). The earlier text is in git at 991986ba.

## Context

The kitty graphics protocol adds animation to an image: frames sent with `a=f`, controlled with `a=a`, and composed from one another with `a=c`, with `d=f` and `d=F` to delete them. The C reference answered all three actions with EINVAL and deleted nothing for `d=f`, leaving animation to step 5 of its ADR-103, so it has no goldens for animation. It played video only through pictures a program re-sent frame by frame, under one budget of about four screens. ADR-1000 lists `image.kitty-animation` as a supported target, and the maintainer brought it into the wave that makes pictures visible (ADR-1100 D12). vtium presents itself as kitty-compatible (ADR-1100 D14), so where the spec is silent it follows kitty's own behaviour, checked by running kitty.

Two of vtium's rules shape it. ADR-1100 D5 gives each console one image budget, which eviction follows by the bytes alone. `docs/architecture.md`'s determinism rules say that the image store follows from the bytes a program sends, with time entering only in named places. A terminal-driven animation, where the program sets gaps and tells the terminal to run, is by nature a matter of time, so this record decides where that time lives.

## Decision

Terms used below: a *frame* is one picture of an animation, numbered from 1; the *root frame*, frame 1, is the image's own pixels; a frame's *gap* is how long it shows before the next one, in milliseconds; a *gapless* frame has a negative gap and is skipped over when the animation plays.

### D1: how frames are stored and charged

- **Two kinds of stored frame** (the maintainer, 2026-11-02). The store keeps an image's frames past the root as a list beside its pixels, and each frame is one of two kinds. A *whole frame* holds every pixel of the image's width and height in premultiplied RGBA, the format of ADR-1100 D2, behind a shared reference as the image's pixels are. A *change frame* holds its *base*, which is an earlier frame of the image or a color, and its *layers*, in the order they arrived: each layer is a rectangle of premultiplied pixels with its offset in the image and its blend, alpha blending or replacement, as the command that sent it asked. Composing a change frame (D2) starts from its base, composed first when the base is itself a change frame, and lays each layer on in order. The root frame is the image's own pixels and is always whole.
- **When a frame is stored whole.** The store keeps a new or edited frame as a change frame unless one of three rules holds, and then composes it into a whole frame when its command arrives:
  1. the image's whole frame is at most `FRAME_WHOLE_MAX` bytes, 65,536, a named constant, as a picture of 128 by 128 pixels is, where a change saves little;
  2. the frame's layers together cover at least half of the image's area, where a change saves less than half a whole frame;
  3. composing the frame would apply more than `FRAME_CHAIN_MAX` layers, 8, a named constant, counting every layer from the nearest whole frame or color beneath it, so that composing any frame applies at most 8 layers.
- **A base never changes under the frames laid on it.** Before a command changes a frame that change frames are laid on, as an edit (`a=f` with `r`) or a composition (`a=c`) does, the store composes each frame laid on it into a whole frame, so every frame keeps the pixels it had when it was made, and a later change to one frame never changes another. A composition (`a=c`) is always applied to whole pixels: when its destination is a change frame, the store composes that frame into a whole frame first, so a change frame never takes pixels from any frame but its base.
- **The composed cache.** For each image that holds a change frame, the store keeps its last `FRAME_CACHE_FRAMES` composed frames, 2, a named constant: most often the frame on show and the one before it, which is the base of the next. The cache is not part of the store's state. It fills as frames are shown, and a frame leaves it when that frame changes or is deleted, so what it holds depends on what was drawn; nothing that the store compares, prints, answers or decides reads it, eviction included, and the replay interface never sees it.
- **The root frame is the image.** The image's own pixels are charged to the console's image budget, as every picture is (ADR-1100 D5), and its eviction follows D5's ranks and order.
- **A frame allowance of its own.** Frames past the root are charged to the console's *frame allowance*: `FRAME_ALLOWANCE_FACTOR`, 5, times the console's image budget, as kitty's frame quota is five times its base quota (the maintainer, 2026-11-01). A whole frame costs width × height × 4 bytes plus `FRAME_OVERHEAD`, a named constant of vtium-image's bounds table, starting at 64 bytes. A change frame costs, for each of its layers, the layer's width × height × 4 bytes plus `FRAME_OVERHEAD`. An image that holds at least one change frame is also charged its cache's full bound, `FRAME_CACHE_FRAMES` × width × height × 4 bytes, from the moment it takes its first change frame until it holds none, whatever the cache holds at the time, so the charges follow the bytes and never what was drawn. The allowance covers both buffers, follows the budget when `auto` is computed again, and is 0 when the budget is 0. Frames never count toward the image budget, so a frame never evicts a still picture, and a still picture never evicts a frame.
- **When the allowance is full.** The store reserves a command's whole cost before it allocates anything: the new frame or layer, the whole frames that the rules above compose for it, and the cache's bound when the image takes its first change frame. When the reservation would pass the allowance, the store first counts what it could free: the frames past the root of other animated images, the image used least recently first by the use clock (ADR-1100 D5), never the image the frame belongs to and never a root frame. When that falls short, it frees nothing and the command answers ENOSPC, leaving the animation as it was. Otherwise it frees those images' frames and their caches' bounds, whole lists at a time, and for each such image sets its current frame to the root and its run state to stopped and moves both its stamps (D3), so nothing in the store names a frame that is gone.
- **A count bound.** An image holds at most `FRAMES_MAX` frames, a named constant starting at 1,024, the root included; a frame past it answers ENOSPC. Every walk over frames, as when a delete or a reservation frees them, counts as input work (ADR-1100 D5).
- **Eviction takes the frames with the image.** When D5 evicts an image from the image budget, its frames go with it and give back their allowance.
- **Pictures re-sent frame by frame** are ordinary pictures: a program such as mpv or timg that sends a new image, or replaces one, for each frame of a video stores no animation frames and stays wholly in the image budget, as in the C reference (the maintainer, 2026-11-01).
- **Opacity.** An image with frames counts as opaque only when every one of its frames is opaque. The store works out each frame's opacity when the frame is made or changed, without composing it: a whole frame is opaque when every pixel is, and a change frame is opaque when its base is opaque and none of its layers replaces pixels with one that is not opaque, since alpha blending onto an opaque canvas leaves it opaque. The compositor, which draws nothing under the topmost opaque picture (ADR-1100 D4), never leaves out what a frame's transparent pixels should show.
- **Frames from an animated GIF.** An animated GIF that a program sends as an iTerm2 picture (ADR-1103) is stored as an image with frames under this record's rules: the same frame allowance, count bound and eviction (D1), the same clock and redrawing (D3), and the same gaps, loops and stopping (D4). Its frames arrive whole from the image helper (ADR-1101 D6) and are stored as whole frames, so the iTerm2 handler composes nothing and the image has no cache. The handler sets the frames' gaps, the loops and the run state when it stores the picture (ADR-1103 D2). An iTerm2 picture has no id, so no program can send it a later control: it plays until its loops are spent, or until its frames are freed for the allowance, when it shows its root frame, stopped. The helper composes each frame onto a canvas chosen by the disposal method of the frame before it, as the table shows in D2's terms. The first frame is laid on a transparent canvas, as a new frame without `c` is. A GIF pixel is either opaque or wholly transparent, so laying a frame by alpha blending and by replacing only its opaque pixels give the same bytes.

  | The disposal method of the frame before | The canvas the next frame is laid on |
  |---|---|
  | 0 (none given) or 1 (leave in place) | the frame before, as `a=f` with `c` naming it |
  | 2 (restore to background) | the frame before, with its own rectangle cleared to transparent black, as `a=f` with `c` naming it, `X=1` and transparent data over that rectangle; vtium takes the background to be transparent, as web browsers do, and never draws the GIF's background color |
  | 3 (restore to previous) | the canvas the frame before was laid on, as `a=f` with `c` naming that earlier frame, or the transparent canvas when the frame before is the first |

  > Added 2026-11-02: the maintainer decided that an animated GIF sent as an iTerm2 picture plays as an animation through this record's frames and clock. The opacity bullet came with it: the record had not said when an image with frames counts as opaque.
- **The use clock.** A command that names an image to add, change, compose, control or delete its frames moves the store's use clock for that image, as any command that names an existing image does (ADR-1100 D5). Playing an animation never moves it.

> Added 2026-11-02, after the fresh review of the build: a frame's data that the console keeps for the image helper, as for a PNG or zlib frame sent directly, is reserved of the frame allowance as it arrives, in a reservation that holds the frame's image, where ADR-1100 D7 charges a picture's kept payload to the image budget, so a frame's payload never evicts a still picture or its own root frame. The frames that a command's reservation freed for room stay freed when the command is refused after it, as the images that the image budget evicts for a refused picture stay evicted (ADR-1100 D5).

### D2: how frames are composed

Composition follows the kitty spec's rules. vtium-image's frame code, beside the store, does it once, on the CPU, on pixels already decoded, and no renderer ever composes (the maintainer, 2026-11-02): the CPU renderer and the GPU renderer both receive whole pixels, so the GPU path still draws the CPU path's bytes (ADR-1300 D6). The frame code composes at two times: when a command arrives, for a frame D1 stores whole and for the whole frames D1 composes ahead of an edit or a composition; and when the binary takes a head's snapshot, for a change frame that the animation clock chose and that is not in the cache (D3). A frame's data arrives as any transmission's does, raw in the console or as PNG or zlib through the image helper (ADR-1101), and the barrier holds the stream while the helper decodes it (ADR-1100 D7).

- **A new frame** (`a=f` with `i` or `I`, and no `r`) starts from a canvas: the frame that `c` names, or, where `c` is left out, the color `Y` gives as 32-bit RGBA, transparent black by default. The transmitted data covers the rectangle that `x`, `y`, `s` and `v` give, or the whole image where they are left out, and is laid onto the canvas by alpha blending, or by plain replacement where `X=1`. The result is appended as the next frame: a change frame whose base is frame `c` or the color, with the data as its one layer, or the same frame composed whole when D1's rules say so.
- **An edit** (`a=f` with `r`) lays the data onto frame `r` itself, by the same rules. The data becomes another layer of frame `r` when frame `r` is a change frame and D1 keeps it one; otherwise the store composes it into frame `r`'s whole pixels. Either way frame `r` then shows what it would show had its pixels been changed in place.
- **A composition** (`a=c`) lays the rectangle `w` by `h` at `X`, `Y` of frame `r` onto the rectangle at `x`, `y` of frame `c`, by alpha blending, or by replacement where `C=1`; `w` and `h` default to the whole image. It is applied to whole pixels (D1).
- **The blend** is premultiplied "source over", in whole numbers, per channel: `src + (dst × (255 − a) + 127) / 255`, where `a` is the source's alpha and the division drops its remainder. The same bytes give the same frame on every machine, and a frame composed when its command arrives holds the same bytes as the same frame composed when it is shown.
- **Work.** Every pixel that a canvas fill, a blend, a replacement or a composition writes while a command is handled counts as input work, as decoded pixels do (ADR-1100 D5), so a stream of large compositions stops an input call early and the rest is taken by the next. Composing a frame for a snapshot is not input work, since no input call is running; it writes at most (1 + `FRAME_CHAIN_MAX`) × width × height pixels, a copy of the base and at most 8 layers, and the binary composes at most one frame of each animated image for each snapshot.
- **Refusals,** each of which changes nothing: an image or frame that does not exist answers ENOENT; a rectangle that leaves the image, and a composition whose source and destination are the same frame with rectangles that overlap, answer EINVAL, as the spec requires; a frame that does not fit the allowance or the count answers ENOSPC (D1).

### D3: who owns the timing

- **The store records commands, never the time.** For each animated image the store keeps its frames and their gaps, its run state (stopped, running, or running while waiting for more frames), the number of loops a command asked for, its current frame, which `a=a` with `c` sets and which becomes the root when frames are deleted or freed, and two stamps: a *frame stamp*, a count that moves when a command makes a frame current or when frames are deleted or freed, and a *run stamp*, a count that moves when a command sets the run state or the loops, or when frames are deleted or freed. Nothing in the store depends on when a command arrived or how long ago it was, so the store still follows from the bytes, and the chunking test, which feeds a stream whole and cut at random points, ends in the same store either way.
- **The animation clock is the one place time enters.** It lives in the binary, beside the blink timers (`crates/vtium/src/blink.rs`), and reads the monotonic time that the event loop gives it, as the respawn policy takes its times as arguments. For each animated image of the console a head shows, it keeps what it is showing: the frame, when that frame began, the loops played, and the two stamps it last saw.
- **How the clock answers each command.** When it sees a stamp move, the clock answers by what moved, the frame stamp first:

  | Command | What the clock does |
  |---|---|
  | `a=a` with `c` (the frame stamp moves) | shows the store's current frame from that moment, and keeps running or stopped as it was |
  | `d=f` or `d=F`, or frames freed for the allowance (both stamps move; the store's current frame is the root and its run state stopped) | shows the store's current frame, the root, and stops, as the two rows for `c` and `s=1` together give |
  | `a=a` with `s=1` (the run stamp moves) | stops, keeping the frame on show, and sets the loops played back to 0 |
  | `a=a` with `s=2` or `s=3` while stopped (the run stamp moves) | runs from the frame on show |
  | `a=a` with `s=2` or `s=3` while running (the run stamp moves) | keeps its place and takes the new mode |
  | `a=a` with `v` (the run stamp moves) | keeps its place and the loops played, and takes the new limit |
  | `a=f`, a new or changed gap, or `a=c` (no stamp moves) | keeps its place; a frame that changed shows its new pixels |

- **The snapshot carries the choice.** The frame's placement list carries, for an animated image, its image's key and its animation state (ADR-1100 D9). When the binary takes the head's snapshot, it asks vtium-image's frame code for the whole pixels of the frame the clock chose: the root, a whole frame or a cached frame as it is held, or a change frame composed then (D2) and kept in the cache (D1). The snapshot holds that frame alone, so the render thread draws one picture for each placement, as for any image, and every placement of an image shows the same frame. A composed frame that the cache drops while a snapshot still holds it stays in memory until that snapshot is released, as ADR-1100 D9 bounds what a frame in flight holds.
- **What names the pixels a placement shows.** Three values together name the pixels of every frame entry: its image's key; the number of the frame it shows, which is 1 for the root frame and for a picture that is not animated; and that frame's *pixel revision*, the store's revision count at the moment a command last changed what the frame shows. The store gives a frame a new pixel revision whenever a command changes what it shows: a transmission that makes or replaces the image changes the root frame, `a=f` makes a frame or edits it, whether the edit adds a layer or writes whole pixels, and `a=c` composes into one. Composing a change frame, for a snapshot or into whole pixels ahead of an edit, changes nothing the frame shows, so the frame keeps its pixel revision, and a composed frame carries the identity of the frame it was composed from. Two entries with the same three values therefore show the same bytes, and the store's composed cache keys its frames on the same three values. The pixel revision is not D3's frame stamp, which counts commands that change which frame is current. The store keeps each frame's pixel revision beside the frame. The binary's snapshot fills in the frame number and its pixel revision from the frame the animation clock chose, beside that frame's whole pixels; for a picture that is not animated it takes frame 1 and the image's own pixel revision, as the store gives them. The renderer keys what it keeps for a picture on all three values, both its scaled copies and its record of what each display last drew, so when any of the three changes it draws the cells under the placement again from the new pixels and never draws a kept copy of another frame. The CPU renderer and the GPU renderer key on the same three values and read the same frame's pixels from the snapshot, so the GPU path still draws the CPU path's pixels (ADR-1300 D6, ADR-1301).

  > Added 2026-11-02, after the gap-check: the renderer's picture cache named a picture by its image and the store's revision only, so a frame the clock moved to would have drawn the pixels of the frame before it.

  > Added 2026-11-02, after the fresh review of the store's runs: an entry the store makes shows the image's current frame, the one `a=a` with `c` made current, with that frame's number and pixel revision, and the binary's snapshot puts the frame its clock chose in its place. A program that drives its animation itself with `a=a` and `c` therefore draws through the store alone, as the Neutral consequences say.
- **Redrawing.** When the clock moves an image to another frame, the binary marks changed the cells under that image's placements in the last frame it listed, placeholder pictures included, as the blink timer redraws the cells it blinks. That changes what a display must draw, never the terminal's or the store's state.
- **Its timer.** The clock arms one timer, for the soonest frame change among the animations visible on the console a head shows, and none while no animation plays, the console is not shown, or every display of the head is blanked. It fires at most once every `ANIMATION_FLOOR_MS`, 10 ms, a named constant; a frame whose gap is shorter shows at the next firing or is passed over, as the time since its start decides. When a hidden console is shown again, the clock works out from the time how far each animation has played, rather than replaying every frame it missed.
- **Mode 2026.** A synchronized-output hold (ADR-1100 D9) holds an animation's frame change as it holds any change: a display that still shows its last frame keeps it until the hold ends.
- **The replay interface** prints the store's animation state, never the frame on show, and has no clock. Tier 1 therefore judges what an animation stores, and tier 3 and the maintainer's review on a VT judge what it shows.

The figure shows one animation over time: the commands in the stream, what the store records after each, and which frame the clock shows.

![Three lanes over a time axis. The stream lane holds three commands in order: a=f adding frames 2 and 3 with a gap of 40 ms each; a=a with s=3, run and loop; and a=a with s=1, stop. The store lane under them changes only when a command arrives: three frames, stopped; then three frames, running, looping forever, current frame 1, run stamp 1; then stopped, current frame 1, run stamp 2. The clock lane shows frame 1 while the animation is stopped, then, once it runs, frames 2 and 3 in turn every 40 ms, since frame 1's gap is 0 and it is skipped; after the stop it keeps frame 3, the frame that was showing, although the store's current frame is still 1. A note says that a replay prints the store's lane, never the clock's.](figures/ADR-1102-store-and-clock.svg)

### D4: gaps, loops and stopping

- **Gaps.** A frame's gap is set with `z` when it is made or with `a=a`, `r` and `z` later. A `z` of 0 is ignored and keeps the gap the frame had. A positive gap shows the frame that many milliseconds. A negative gap makes the frame gapless: it is never shown while the animation plays, and serves as a canvas for others. A new frame's gap is 40 ms by default, and the root frame's starts at 0, which plays as gapless until a program sets it, as the spec says ("its gap must be set using this control code").
- **Running.** `a=a` with `s=3` runs the animation and loops back to the first frame after the last. `s=2` runs it in loading mode: on reaching the last frame it waits on that frame, and when a later frame arrives it goes on to it once the last frame's gap has passed. `s=1` stops it. D3's table says where each starts.
- **Loops.** `v=0` is ignored, `v=1` loops forever, and any larger `v` loops `v` − 1 times. When the loops are spent, the animation stops on its last frame shown. Stopping resets the loop count, as the spec says.
- **Stopping keeps the frame on show,** and the store's current frame does not move, since the store never knew which frame showed. A later `a=a` with `c` makes a frame current and shows it, running or stopped.
- **Every frame gapless.** An animation whose frames are all gapless shows its current frame and does not advance, so the clock never spins on frames it would skip forever.

### D5: deleting frames

`a=d` with `d=f` or `d=F` and with `i` or `I` deletes every frame of that image but the root, gives back their allowance, sets its current frame to the root and its run state to stopped, and moves both its stamps, so the animation shows its root frame, stopped, and nothing in the store names a frame that is gone. Frames hold no placements, so the lowercase and uppercase forms do the same, and the image itself stays. The spec requires `i` or `I` on every animation command, so `d=f` with neither does nothing until a black-box run of kitty shows what kitty does with it (ADR-1100 D14). vtium's own replay cases judge both, provisional until the maintainer reviews them.

No command deletes one frame alone, so no change frame ever loses its base: `d=f` and `d=F` delete every frame past the root, and the root, the image's own pixels, is always whole. Deleting the frames empties the image's cache and gives back its bound. A command that changes a base, rather than deleting it, first composes the frames laid on it into whole frames (D1).

### D6: detection and verification

- **Detection.** Kitty's published specification has no query for animation: a program detects the graphics protocol with the graphics query (`a=q`, ADR-1100 D13's Detection bullet) and then sends `a=f`, `a=a` and `a=c`, reading each reply, or goes by the terminal's name and version. vtium answers each animation command as this record decides, so a program that tries one learns from its reply whether vtium took it.

  > Edited 2026-11-02: the maintainer read kitty's published specification (sw.kovidgoyal.net/kitty/graphics-protocol) with the steering session. It has no `e` key and no feature query; a program detects the protocol with the graphics query and learns which transmission mediums, formats and compressions a terminal takes by naming them in that query. This bullet named a feature query and no longer does.
- **Replay cases,** vtium's own and provisional until the maintainer reviews them, for each action and delete form: a frame on a color canvas and on an earlier frame, by blend and by replacement; an edit; a composition, with each refusal; gaps, loops, run and stop as the store records them, with both stamps; `d=f` with an id and without one.
- **Unit tests** for the allowance: a frame reserved before it is allocated against the allowance and never the image budget; other images' frames freed least recently used first, never the image's own and never a root; refused with ENOSPC when freeing cannot cover it; the frame count bound; each seen to fail with its check planted out.
- **Unit tests** for change frames: a frame composed when it is shown holds the same bytes as the same frame composed when its command arrives, for each blend and for a color base; an edit or a composition on a base composes the frames laid on it first, so they keep their pixels, and with that step planted out the test fails; each of D1's three rules stores a frame whole; composing any frame applies at most `FRAME_CHAIN_MAX` layers; a change frame's opacity follows its base and its layers; the cache's charge is the same whatever was shown.
- **The clock** takes its times as arguments, so its tests pass the times in and check each row of D3's table: a running animation shows each frame for its gap, loops `v` − 1 times, skips gapless frames, keeps its place as frames arrive in loading mode, keeps the frame on show when stopped, starts from the frame on show, takes the store's frame on `c`, and arms no timer while hidden.
- **Determinism:** a stream of frames and controls fed whole and cut at random points ends in the same store, with frames drawn at different points and none, so the composed cache, which drawing fills, changes no decision; with a stamp moved by a drawn frame, planted, the test fails, and with the cache charged by what it holds, planted, a stream that ends in an eviction fails.
- **The renderer** takes two snapshots of one animated placement that differ only in the frame the clock chose, with the same image key and store revision, and draws each frame's own pixels, on the CPU path, and on the GPU path under llvmpipe once that path is built, byte for byte alike; with the frame number and pixel revision left out of what names a picture, planted, the second draw shows the first frame and the test fails.
- **Tier 3** plays an animation on a VT and captures it at fixed points, its goldens vtium's own and provisional.

### Passages this record changes

- **`docs/architecture.md`, "Time enters in named places only,"** names the helper's timer, mode 2026's timeout and the respawn policy; it gains the animation clock, which changes what a display shows and never the store. It changes in its owner's change, when the animation build lands.
- **ADR-1100 D9** says that an entry for an animated image carries its animation state, that the snapshot takes the whole pixels of the frame the clock chose from vtium-image's frame code, and that every entry carries its frame's number and pixel revision (D3); **ADR-1100 D5** says that frames past the root take this record's allowance. Both carry dated notes.

## C notes: the C reference's flow

The C reference had no animation. Its kitty parser refused `a=f`, `a=a` and `a=c` with EINVAL before any data, in its `refusal` function (`libtsm/src/tsm/tsm-kitty.c:315-340`), and `d=f` and `d=F` deleted nothing.

## Where vtium departs

- **vtium plays kitty animations,** where the C reference answered EINVAL; ADR-1100 lists this among its departures.
- **Frames are held in memory,** where kitty keeps frame data on disk; the allowance is kitty's, five times the base.

## Consequences

### Positive

- The store stays a record of what the program sent, so replays, the chunking test and eviction follow the bytes as they do for every other picture.
- An animation never pushes a still picture out of the scrollback, and a still picture never pushes out an animation's frames.
- The renderers receive whole pixels only, so the compositor draws an animated picture as it draws any other, and the CPU and GPU paths still draw the same bytes.
- A frame that changes a few pixels costs only those pixels, beside its image's cache bound.
- A hidden console's animations cost no wake-ups.

### Negative

- Showing a frame that is not in the cache costs a composition on the console's main thread, up to nine times the picture's pixels.
- Each image that holds a change frame is charged two whole frames for its cache, even when it never shows.
- An edit or a composition on a frame that others are laid on composes those frames whole first.
- A console can hold up to six times its image budget in pictures and frames together: at the largest `auto`, 192 MiB, that is 1,152 MiB, more than a small board has.
- What an animation shows on screen can be judged only by tier 3 and by eye, never by tier 1.
- The binary keeps a little state for each playing image, outside the store, and the store keeps a cache that is not part of its state.

### Neutral

- Client-driven animations, where a program sends `a=a` with `c` for each frame, work through the store alone and need no clock.
- `d=f` without an id waits on a black-box check of kitty.

## Alternatives Considered

- **Every frame composed into whole pixels when its command arrives,** the record's first choice. Not taken (the maintainer, 2026-11-02): every frame then costs a whole frame's bytes even when it changes a few pixels, which spends the frame allowance on animations that change little. Its strengths were real, and vtium keeps them where they matter most: drawing never composes, no frame depends on another, and an edit needs no frames composed ahead of it. D1's three rules keep this for small pictures, for changes that cover most of the picture and for deep chains.
- **Change frames composed by the renderers when drawn.** Rejected (the maintainer, 2026-11-02): the CPU renderer and the GPU renderer would each compose, which makes both less simple and their byte-for-byte agreement harder to keep.
- **A composed cache charged by what it holds.** Not taken: what it holds depends on what was shown, so eviction would follow the drawn frames rather than the bytes.
- **A change to a base that the frames laid on it follow,** with no frames composed ahead of it. Rejected: a frame would change when another frame changed, which no frame did when every frame was whole, and a frame's pixel revision would have to follow every frame beneath it.
- **Frames in the one image budget.** Rejected (the maintainer, 2026-11-01): a long animation would push still pictures out of the scrollback; kitty gives frames a separate quota.
- **One stamp for every animation command.** Rejected: the clock could not tell a stop, which keeps the frame on show, from a `c`, which shows the store's frame, or a change of mode while running, which keeps its place.
- **The store advancing the current frame on a timer.** Rejected: the store's state would then depend on time, and a replay or a stream cut at different points would end in different stores.
- **Stopping on the store's current frame.** Rejected: the picture would jump back to whatever frame a program last named, and the store does not know what showed.
- **Client-driven animation only.** Rejected: the spec asks for terminal-driven animation so that a program like `cat` can show an animated picture and exit.
