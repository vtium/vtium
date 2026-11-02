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
  - operator: aaronsb
    level: guided
    said: "R3, a new record for kitty animation: how frames are stored and charged against the budget (ADR-1100 D5), how they compose (kitty's frame composition rules), who owns the timing (time may enter only in a named place - docs/architecture.md's determinism rules; the record decides how a playing animation stays out of the store's state so replays stay deterministic), and how gaps/loops/stop work."
    via: the team lead's brief for the image records, 2026-11-01, carrying the maintainer's direction for this wave
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol), read 2026-11-01: Animation, Transferring animation frame data, Controlling animations, Composing animation frames, the control data reference's keys for frame loading, composition and control, and d=f and d=F; frames composed onto a canvas of a color or of an earlier frame, by alpha blend or by replacement; gaps in milliseconds, 0 ignored, negative gapless, 40 by default and 0 for the root frame; s=1 stop, s=2 run waiting for more frames, s=3 run and loop; v=0 ignored, v=1 loop forever, v=n loop n-1 times; stopping resets the loop count; a=c answers ENOENT for a missing frame or image and EINVAL for a rectangle out of bounds or overlapping itself in one frame; kitty keeps frame data on disk under a separate quota of five times the base quota"
  - upstream: "libtsm eeaf293 (aaronsb/libtsm), src/tsm/tsm-kitty.c:325: the C reference answered a=f, a=a and a=c with EINVAL, and d=f and d=F deleted nothing, leaving animation to step 5 of kmscon ADR-103"
  - evidence: "docs/architecture.md, Determinism: the screen, the scrollback, the modes, the cursor, the image store and the replies follow from the bytes, with time entering only at named places; the helper's 2-second timer and mode 2026's timeout are named there"
  - evidence: "crates/vtium/src/blink.rs: the blink timers run in the binary, flip a phase only the renderer reads, and redraw, without changing the terminal's state"
  - precedent: ADR-1100
  - precedent: ADR-1101
  - precedent: ADR-1300
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1102: Kitty animation frames are stored whole, and a clock outside the store plays them

## Summary

- **Decided:** vtium builds kitty animation as the kitty spec describes it. Each animation frame is a whole picture the size of its image, held in the store beside the image's own pixels, which are the first frame, the *root frame*. A new frame or an edit is composed into whole pixels when its command arrives, so the store never holds a frame as a recipe of other frames. Every frame is charged to the console's one image budget (ADR-1100 D5), reserved before it is allocated and evicted with its image. The store keeps only what the stream commands: the frames, their gaps, whether the animation is stopped, running or waiting for frames, how many loops to play, and which frame a command last made current. It never records which frame is on the screen now. That is the job of the *animation clock*, which lives in the binary beside the blink timers and is the one place time enters for animation: it reads the monotonic time, picks the frame each playing image shows, and hands that choice to the head's snapshot. So a replay, which has no clock, prints the same store state however long the stream took to arrive.
- **Trades away:** A frame that changes a few pixels costs as many bytes as a full frame, where kitty can keep it as a set of operations. Frames share the budget with every other picture, where kitty gives frames a separate quota five times larger. Which frame a stopped animation keeps on screen depends on when it was stopped, so tier 1 cannot judge what an animation shows, only what it stores.
- **One-way?** No while nothing is built. It becomes expensive once replay goldens print the store's animation state and programs rely on the replies.
- **Probes:**
  - *Not confident (frames in the budget):* an animation's frames count against the same image budget as every other picture, so a long animation can push older pictures out of the scrollback, and one that does not fit is refused with ENOSPC. kitty keeps frames on disk under a separate, larger quota. Is one budget for pictures and frames what you want?
  - *Confident (a stopped animation stays put):* when a program stops an animation, the screen keeps the frame that was showing at that moment, rather than jumping back to a frame the program named earlier. Yes or no?
- **Inversion:** Between "the store plays the animation", which makes its state depend on time, and "the program drives every frame itself", which the spec allows but which leaves `cat`-like programs unable to animate. The answer keeps the store a record of commands and puts playback in a named clock outside it.

## Context

The kitty graphics protocol adds animation to an image: frames sent with `a=f`, controlled with `a=a`, and composed from one another with `a=c`, with `d=f` and `d=F` to delete them. The C reference answered all three actions with EINVAL and deleted nothing for `d=f`, leaving animation to step 5 of its ADR-103, so it has no goldens for animation. ADR-1000 lists `image.kitty-animation` as a supported target, and the maintainer brought it into the wave that makes pictures visible (ADR-1100 D12).

Two of vtium's rules shape it. ADR-1100 D5 gives each console one image budget, which eviction follows by the bytes alone. `docs/architecture.md`'s determinism rules say that the image store follows from the bytes a program sends, with time entering only in named places. A terminal-driven animation, where the program sets gaps and tells the terminal to run, is by nature a matter of time, so this record decides where that time lives.

## Decision

Terms used below: a *frame* is one picture of an animation, numbered from 1; the *root frame*, frame 1, is the image's own pixels; a frame's *gap* is how long it shows before the next one, in milliseconds; a *gapless* frame has a negative gap and is skipped over when the animation plays.

### D1: how frames are stored and charged

- **Whole frames.** The store keeps an image's frames as a list beside its pixels, each frame a whole picture of the image's width and height in premultiplied RGBA, the format of ADR-1100 D2, behind a shared reference as the image's pixels are. Every command that makes or changes a frame composes it into those pixels when the command arrives (D2), so drawing a frame never needs another frame, and a later change to one frame never changes another.
- **Charged to the console's budget.** Each frame past the root costs width × height × 4 bytes plus `FRAME_OVERHEAD`, a named constant of vtium-image's bounds table, starting at 64 bytes, toward the console's image budget (ADR-1100 D5). The store reserves a frame's cost before it allocates it, and when the reservation would pass the budget it evicts in D5's order or refuses, as for any picture. The image the frame belongs to is held while the store makes room, so a frame never evicts its own image. A refused frame answers ENOSPC and leaves the animation as it was.
- **A count bound.** An image holds at most `FRAMES_MAX` frames, a named constant starting at 1,024, the root included; a frame past it answers ENOSPC. Every walk over frames, as when a delete or an eviction frees them, counts as input work (ADR-1100 D5).
- **Eviction takes the frames with the image.** D5's ranks and order apply to the image, and its frames go with it. Nothing evicts a frame alone.
- **The use clock.** A command that names an image to add, change, compose, control or delete its frames moves the store's use clock for that image, as any command that names an existing image does (ADR-1100 D5). Playing an animation never moves it.

### D2: how frames are composed

Composition follows the kitty spec's rules, done by the kitty handler in the console when the command arrives, on pixels already decoded. A frame's data arrives as any transmission's does, raw in the console or as PNG or zlib through the image helper (ADR-1101), and the barrier holds the stream while the helper decodes it (ADR-1100 D7).

- **A new frame** (`a=f` with `i` or `I`, and no `r`) starts from a canvas: the frame that `c` names, or, where `c` is left out, the color `Y` gives as 32-bit RGBA, transparent black by default. The transmitted data covers the rectangle that `x`, `y`, `s` and `v` give, or the whole image where they are left out, and is laid onto the canvas by alpha blending, or by plain replacement where `X=1`. The result is appended as the next frame.
- **An edit** (`a=f` with `r`) lays the data onto frame `r` itself, by the same rules, and changes that frame in place.
- **A composition** (`a=c`) lays the rectangle `w` by `h` at `X`, `Y` of frame `r` onto the rectangle at `x`, `y` of frame `c`, by alpha blending, or by replacement where `C=1`; `w` and `h` default to the whole image.
- **The blend** is premultiplied "source over", in whole numbers, per channel: the result is the source plus the destination × (255 − the source's alpha), plus 127, divided by 255 with the remainder dropped. The same bytes give the same frame on every machine.
- **Refusals,** each of which changes nothing: an image or frame that does not exist answers ENOENT; a rectangle that leaves the image, and a composition whose source and destination are the same frame with rectangles that overlap, answer EINVAL, as the spec requires; a frame that does not fit the budget or the count answers ENOSPC (D1).

### D3: who owns the timing

The figure shows one animation over time: the commands in the stream, what the store records after each, and which frame the clock shows.

![Three lanes over a time axis. The stream lane holds three commands in order: a=f adding frames 2 and 3 with a gap of 40 ms each; a=a with s=3, run and loop; and a=a with s=1, stop. The store lane under them changes only when a command arrives: three frames, stopped; then three frames, running, looping forever, current frame 1, control stamp 1; then stopped, current frame 1, control stamp 2. The clock lane shows frame 1 while the animation is stopped, then, once it runs, frames 2 and 3 in turn every 40 ms, since frame 1's gap is 0 and it is skipped; after the stop it keeps frame 3, the frame that was showing, although the store's current frame is still 1. A note says that a replay prints the store's lane, never the clock's.](figures/ADR-1102-store-and-clock.svg)

- **The store records commands, never the time.** For each animated image the store keeps its frames and their gaps, its run state (stopped, running, or running while waiting for more frames), the number of loops a command asked for, the frame a command last made current with `a=a` and `c`, and a *control stamp*: a count that moves when a command sets the run state, the loops or the current frame, or deletes frames. Nothing in the store depends on when a command arrived or how long ago it was, so the store still follows from the bytes, and the chunking test, which feeds a stream whole and cut at random points, ends in the same store either way.
- **The animation clock is the one place time enters.** It lives in the binary, beside the blink timers (`crates/vtium/src/blink.rs`), and reads the monotonic time that the event loop gives it, as the respawn policy takes its times as arguments. For each animated image of the console a head shows, it keeps what it is showing: the frame, when that frame began, the loops played, and the control stamp it last saw. When the store's control stamp for an image differs from the one it saw, it starts again from the store's current frame and run state at that moment. A new frame or a changed gap moves no control stamp, so an animation that is playing keeps its place while frames arrive.
- **The snapshot carries the choice.** The frame's placement list carries, for an animated image, its frames, their gaps and its animation state, with the pixels shared (ADR-1100 D9). When the binary takes the head's snapshot, it keeps for each animated image only the frame the clock chose, so the render thread composes one picture for each placement, as for any image, and every placement of an image shows the same frame.
- **Redrawing.** When the clock moves an image to another frame, the binary marks changed the cells under that image's placements in the last frame it listed, placeholder pictures included, as the blink timer redraws the cells it blinks. That changes what a display must draw, never the terminal's or the store's state.
- **Its timer.** The clock arms one timer, for the soonest frame change among the animations visible on the console a head shows, and none while no animation plays, the console is not shown, or every display of the head is blanked. It fires at most once every `ANIMATION_FLOOR_MS`, 10 ms, a named constant; a frame whose gap is shorter shows at the next firing or is passed over, as the time since its start decides. When a hidden console is shown again, the clock works out from the time how far each animation has played, rather than replaying every frame it missed.
- **Mode 2026.** A synchronized-output hold (ADR-1100 D9) holds an animation's frame change as it holds any change: a display that still shows its last frame keeps it until the hold ends.
- **The replay interface** prints the store's animation state, never the frame on show, and has no clock. Tier 1 therefore judges what an animation stores, and tier 3 and the maintainer's review on a VT judge what it shows.

### D4: gaps, loops and stopping

- **Gaps.** A frame's gap is set with `z` when it is made or with `a=a`, `r` and `z` later. A `z` of 0 is ignored and keeps the gap the frame had. A positive gap shows the frame that many milliseconds. A negative gap makes the frame gapless: it is never shown while the animation plays, and serves as a canvas for others. A new frame's gap is 40 ms by default, and the root frame's starts at 0, which plays as gapless until a program sets it, as the spec says ("its gap must be set using this control code").
- **Running.** `a=a` with `s=3` runs the animation and loops back to the first frame after the last. `s=2` runs it in loading mode: on reaching the last frame it waits on that frame, and when a later frame arrives it goes on to it once the last frame's gap has passed. `s=1` stops it.
- **Loops.** `v=0` is ignored, `v=1` loops forever, and any larger `v` loops `v` − 1 times. When the loops are spent, the animation stops on its last frame shown. Stopping resets the loop count, as the spec says.
- **Stopping keeps the frame on show.** When a program stops an animation, the screen keeps the frame the clock was showing, and the store's current frame does not move, since the store never knew which frame showed. A later `a=a` with `c` makes a frame current and shows it, running or stopped.
- **Every frame gapless.** An animation whose frames are all gapless shows its current frame and does not advance, so the clock never spins on frames it would skip forever.

### D5: deleting frames

`a=d` with `d=f` or `d=F` deletes animation frames. The spec's table says only "Delete animation frames", so vtium reads it this way: with `i` or `I`, every frame of that image but the root; with neither, every frame but the root of every image of the buffer in use. The animation of each image it reaches stops, and its control stamp moves. Frames hold no placements, so the lowercase and uppercase forms do the same, and the image itself stays. vtium's own replay cases judge this reading, provisional until the maintainer reviews them, and a black-box run of kitty checks it before they are promoted.

### D6: detection and verification

- **Detection.** Kitty's feature query (`a=q` with `e=f`, `e=a` or `e=c`) reports each action, and the keys of it that vtium takes, once the animation build lands; the feature query's keys are the kitty handler's.
- **Replay cases,** vtium's own and provisional until the maintainer reviews them, for each action and delete form: a frame on a color canvas and on an earlier frame, by blend and by replacement; an edit; a composition, with each refusal; gaps, loops, run and stop as the store records them; `d=f` with and without an id.
- **Unit tests** for the budget: a frame reserved before it is allocated, refused with ENOSPC when eviction cannot cover it, never evicting its own image, and the frame count bound, each seen to fail with its check planted out.
- **The clock** takes its times as arguments, so its tests pass the times in: a running animation shows each frame for its gap, loops `v` − 1 times, skips gapless frames, keeps its place as frames arrive in loading mode, keeps the frame on show when stopped, and arms no timer while hidden.
- **Determinism:** a stream of frames and controls fed whole and cut at random points ends in the same store, with frames drawn at different points and none; with the control stamp moved by a drawn frame, planted, the test fails.
- **Tier 3** plays an animation on a VT and captures it at fixed points, its goldens vtium's own and provisional.

### Passages this record changes

- **`docs/architecture.md`, "Time enters in named places only",** names the helper's timer, mode 2026's timeout and the respawn policy; it gains the animation clock, which changes what a display shows and never the store. It changes in its owner's change, when the animation build lands.
- **ADR-1100 D9** says that an entry for an animated image carries its frames and that the snapshot keeps the frame the clock chose; this record's commit edits it, with a dated note.

## C notes: the C reference's flow

The C reference had no animation. Its kitty parser refused `a=f`, `a=a` and `a=c` with EINVAL before any data, in its `refusal` function (`libtsm/src/tsm/tsm-kitty.c:315-340`), and `d=f` and `d=F` deleted nothing.

## Where vtium departs

- **vtium plays kitty animations,** where the C reference answered EINVAL; ADR-1100 lists this among its departures.
- **Frames share the console's image budget,** where kitty keeps them on disk under a separate quota five times its base quota. The spec gives no quota of its own for frames.

## Consequences

### Positive

- The store stays a record of what the program sent, so replays, the chunking test and eviction follow the bytes as they do for every other picture.
- Drawing a frame never needs another frame, so the compositor draws an animated picture as it draws any other.
- A hidden console's animations cost no wake-ups.

### Negative

- A frame that changes a few pixels costs a full frame's bytes, so a long animation fills the budget sooner than in kitty.
- What an animation shows on screen can be judged only by tier 3 and by eye, never by tier 1.
- The binary keeps a little state for each playing image, outside the store.

### Neutral

- Client-driven animations, where a program sends `a=a` with `c` for each frame, work through the store alone and need no clock.
- `d=f`'s reading waits on a black-box check of kitty before its goldens are promoted.

## Alternatives Considered

- **Frames kept as operations on other frames,** composed when drawn, as kitty may keep them. Rejected: drawing would then depend on other frames, the compositor would compose recipes, and the cost a frame charges would change with later edits.
- **A separate frame budget,** as kitty's quota of five times the base. Not taken: two budgets would need their own eviction order, and vtium keeps no picture data on disk; one budget keeps eviction following the bytes, and the probe above asks the maintainer.
- **The store advancing the current frame on a timer.** Rejected: the store's state would then depend on time, and a replay or a stream cut at different points would end in different stores.
- **Stopping on the store's current frame.** Rejected: the picture would jump back to whatever frame a program last named, and the store does not know what showed.
- **Client-driven animation only.** Rejected: the spec asks for terminal-driven animation so that a program like `cat` can show an animated picture and exit.
