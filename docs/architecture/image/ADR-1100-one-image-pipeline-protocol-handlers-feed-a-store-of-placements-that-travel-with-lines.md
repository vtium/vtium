---
contract: adr/v1
kind: decision
verb: add
capability: images
basis:
  - operator: aaronsb
    level: directed
    said: "One image pipeline, shared by both protocols: the store, the protocol tag and its rules, the decode boundary, and the handler trait the protocol front ends implement."
    via: the body of the GitHub issue that asked for one image pipeline, posted from the operator's account; the steering session's brief for this record names that issue's design as the maintainer's direction
  - operator: aaronsb
    level: directed
    said: "The difference is that one function holds every protocol-specific rule, so the store and the compositor never branch on the protocol themselves."
    via: the body of the same issue, the same text
  - operator: aaronsb
    level: directed
    said: "the store's use clock is per store and advances only on stream events (a transmission, a placement, a reference), never on a drawn frame."
    via: a comment on the same issue headed "Determinism follow-up", posted from the operator's account
  - operator: aaronsb
    level: directed
    said: "kitty graphics relative placements (`image.kitty-relative`) and Unicode placeholders (`image.kitty-placeholders`) are both supported targets, as ADR-1000's tables now record (integration 73adb77). Both belong to this issue."
    via: a comment of 2026-10-27 on the GitHub issue for the kitty graphics handler, headed "The operator (2026-10-27)", posted from the operator's account
  - upstream: "kmscon ADR-103 (aaronsb/kmscon b1f7a7c5, docs/architecture/render/): one store per screen buffer; placements anchored to the line of their top row; sixel as cell content and kitty by its spec's erase rules; eviction by rank, then last use, with kitty images on the screen's own lines oldest transmission first and sixels on the screen never evicted; payloads decoded as they stream in; the host hook, the pause and the helper; one budget per process sized from the displays, 16 to 192 MiB. Its Notes record the operator's choices of per-cell sixel coverage, the storage-quota rule and the minimal build without the decoders"
  - upstream: "libtsm eeaf293 (aaronsb/libtsm): src/tsm/tsm-image.c (the store, eviction, the frame's placement list renewing each drawn image's use at line 1120 on one process-wide clock, lines 46-50), tsm-sixel.c, tsm-kitty.c, tsm-vte-image.c (routing, the pause, the work bound), and tsm-replay's fixture host"
  - upstream: "aaronsb/kmscon b1f7a7c5: src/image_host.c, src/image_helper.c, src/image_identity.c, src/render/image_budget.c, src/terminal_image.c, and tests/test_pty.c's test_pause"
  - upstream: "fixes the C reference made and vtium carries: a reservation that cannot fit refused before anything is evicted; scroll and ED 2 scans counted as input work; a refused replacement keeps the old image"
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol), read 2026-11-01: a storage quota (\"the quota in kitty is 320MB per buffer\", kitty's own example) that deletes older images first; \"The clear screen escape code (usually <ESC>[2J) should also clear all images\" and \"The other commands to erase text must have no effect on graphics\"; only images entirely within the margins scroll with them; images in the alternate screen cleared on switching to it; the z bands; and at equal z, \"the image with the lower id is considered to have the lower z-index\""
  - standard: "xterm's ctlseqs and the VT330/VT340 graphics manual: sixel, DECSDM, modes 1070 and 8452, XTSMGRAPHICS"
  - evidence: "docs/c-reference.md §9, Images: its Check (a use clock per store leaves eviction across stores, or a budget split per store, to decide) and its Not planned (whether the compositor's scaled pictures share the image budget)"
  - evidence: "the review titled Keep the GPU and image paths open (2026-11-01): lines carry a LineId for placements, Damage has mark_rect, FrameCell::default_bg serves the lowest layer, Snapshot carries no placements yet and takes them when the image pipeline is built, with their pixels shared"
  - precedent: ADR-1000
  - precedent: ADR-1001
  - precedent: ADR-1003
  - precedent: ADR-1200
  - precedent: ADR-1300
  - precedent: ADR-1400
  - precedent: ADR-1401
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "1. A budget per console: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
    covers: [a budget per console]
  - operator: aaronsb
    said: "2. Scaled copies in the renderer's own cache, never evicting a stored picture: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
    covers: [scaled copies apart]
  - operator: aaronsb
    said: "3. auto keeps four frames of the head's displays, clamped to 16-192 MiB, held as a named constant so it is easy to adjust and can later become a configuration key if that is warranted."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "4. A mode 2026 hold or a frame in progress keeps deleted pixels outside the budget: accepted."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "5. iTerm2 pictures follow sixel's rules, for protocol compatibility: vtium stays close to what the protocols themselves can do and adopts no one terminal's special handling, such as WezTerm's pictures in cells."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "6. The cursor, the pointer and vtium's notices draw above every picture: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "7. Overruled. At equal z the tie is not by protocol: the placement that reached the pipeline first draws below and a later one above, by sequence number, whatever its protocol, so a program can order its pictures on purpose by the order it sends them. List the departure from the C reference's sixel-above-kitty rule; C goldens that mix the two at equal z will differ there."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "8. One premultiplied RGBA format for every protocol: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "9. The C reference's limits as starting values, held as named constants, moved later by measurement: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "10. Kitty ids per buffer, main and alternate screen apart, as kitty does: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "11. A kitty placement that replaces one with the same i and p takes a new sequence number, so it draws on top: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "12. Build the image pipeline now. vtium sets its own performance targets; meeting or measuring the C reference's targets is unnecessary. Remove the dependency on the C reference's A5 and A7 measurement from the record and the build order; vtium's own targets are measured on vtium."
    via: the maintainer's answers in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Q1. Each picture counts its pixels plus 512 bytes, as a named constant, as the C reference counted it: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, second round on 15caffc's questions, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Q2. The minspec build answers 0 (\"I don't know that setting\") for 80, 1070 and 8452, as for any mode it lacks; listed as a departure from the C reference: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, second round on 15caffc's questions, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Q3. Two kitty pictures at equal z follow kitty's id rule: the lower id draws below, as kitty's spec says, whatever their order of arrival. Arrival order (later on top) still governs every other pair at equal z. The record must give a rule that breaks the loop case (a sixel sent between two kitty pictures whose ids run against their arrival) so the draw order is always one deterministic total order; the author proposes it with a lean, in plain words, for the maintainer to confirm."
    via: the maintainer's answers in the steering session, 2026-11-01, second round on 15caffc's questions, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Q4. iTerm2 pictures stay capped by the 1 MiB OSC bound (about 768 KB of file) for now; the iTerm2 handler and the OSC string bound decide whether to raise it: yes."
    via: the maintainer's answers in the steering session, 2026-11-01, second round on 15caffc's questions, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Pending item 1 confirmed as the record leaned: a newly arrived kitty picture slides in just beneath the first higher id, or goes on top when there is none, so nothing already shown moves."
    via: the maintainer's answer in the steering session, 2026-11-01, third round on 0b0b9e9's pending item, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Answer 4, restated: the maintainer accepts the corrected bound. A deleted picture's pixels stay alive outside the budget for as long as any display of the head still holds a frame that shows it: at most one frame for each display of the head still drawing or holding. The frames share pixels, so the extra memory is only the pictures deleted between the oldest and newest of them. This replaces the 'two per head' wording he accepted earlier, which described vtium wrongly."
    via: the maintainer's answer in the steering session, 2026-11-01, fourth round, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "Item 1 confirmed as the record leaned: a kitty picture sent without an id stacks like a sixel, on top when it arrives, and the id rule passes over it."
    via: the maintainer's answer in the steering session, 2026-11-01, fifth round on c6efada's pending item, relayed by the team lead; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "iTerm2's framing: vtium carries OSC 1337 File= as a compatibility handler, so programs that send it work, rather than as an image protocol of equal standing to sixel and kitty. Sixel and kitty are the two protocols the pipeline is designed around; the iTerm2 handler is a thin front end that maps each picture onto sixel's rules. The record should read that way throughout, figures included."
    via: the maintainer's ruling in the steering session, 2026-11-01, relayed by the team lead in scratchpad/answers-2026-11-01.md; the wording is the team lead's
    paraphrase: true
observable:
  - "`cargo tree -p vtium -e normal` with default features lists no vtium-image, and with `--features images` lists it"
  - "The flood test feeds a stream of transmissions and placements past a full store whole and cut at random points, with frames drawn at different points, and every run ends in the same store; with the input-work count removed from a scan, planted, it fails"
  - "A test feeds an interleaved sixel and kitty stream across a pending decode, and the placements land in stream order; with the barrier removed, planted, it fails"
  - "A `compile_fail` doctest in vtium-image shows that code outside the rules module cannot match on a protocol tag"
  - "tier 1's image cases (`img-`, `six-` and `kit-` in `tools/conformance/corpus/cases.list`) pass through vtium-replay built with `images`, apart from the cases the conformance runner's departures list names for the draw order at equal z (D4)"
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1100: One image pipeline: protocol handlers feed a store of placements that travel with lines

## Summary

- **Decided:** vtium keeps the C reference's image flow as one pipeline. The pipeline is designed around two image protocols, sixel and kitty graphics. A handler for each reads its control string, decodes what is safe to decode in the console and hands the rest to the sandboxed image helper, with the console's input paused until the helper answers. A thin front end for iTerm2's inline images (OSC 1337 `File=`) is carried for compatibility, so programs that send them work: it maps each picture onto sixel's rules and is no protocol of its own standing. Each console has one store of images and placements, with a main and an alternate buffer. A placement is anchored to the line holding its top row and travels with the text. Every placement carries the tag of the protocol whose rules it follows, and one function in vtium-image maps a tag to that protocol's rules: what cuts it, how a region scroll moves it, whether it can be evicted while on the screen, and the layer it draws in. Nothing else in vtium can match on a tag. At equal z a later picture draws over an earlier one, except that two kitty pictures keep kitty's own rule, the lower id below; a newly arrived kitty picture that kitty's rule puts under one already shown slides in just beneath it, so the draw order is always one fixed order and nothing already shown moves. The store's use clock moves only on stream events, so eviction follows the bytes and never the drawn frames. Each console has its own budget, and the compositor's scaled copies live in a cache of the renderer's own that never evicts a stored image. A frame carries the placements with their pixels shared. The whole pipeline is the `images` Cargo feature, and the minspec build carries none of it.
- **Trades away:** A sixel is held as premultiplied RGBA, 4 bytes a pixel, where palette indices would take 1 or 2. A file or compressed image costs a round trip to the helper, and that console's output waits for it. With a budget per console, a vtium running several consoles can hold several budgets of images at once. A picture a frame in flight still shows keeps its memory after its image is deleted, until that frame is done. The C reference's goldens that draw a sixel and a kitty picture at equal z in one frame differ from vtium's draw order.
- **One-way?** No while nothing is built. It becomes expensive once tier 1's image goldens are judged against vtium's output and programs rely on the replies, and once the handlers, the compositor and the helper are built against the store's interface.
- **Probes:**
  - *Not confident (a budget per console):* each console gets its own image budget, `image-storage-limit`, covering its main and alternate screens, so what one console keeps depends only on what its own programs sent. The C reference had one budget for the whole process, where one console's pictures could push out another's. With four consoles, vtium may then hold four budgets of pictures at once. Is a budget per console what you want? (Answered 2026-11-01; see `considered`.)
  - *Confident (scaled copies apart):* when a display draws a picture at a size other than its own, as on a zoomed or rotated display, the scaled copy lives in the renderer's own cache, and drawing it never pushes a stored picture out of the scrollback, as the C reference's scaled copy could. Yes or no? (Answered 2026-11-01; see `considered`.)
- **Inversion:** Between "images are cell content" (sixel's own model, where text over a picture replaces it) and "images are a free layer text never touches" (the kitty spec's model). The answer keeps one layer of placements and gives each protocol the erase and scroll rules its own specification describes, held in one function so a third protocol adds a tag and one arm.

> Edited 2026-11-01: the maintainer answered the twelve points this record listed for the operator's check (`considered`), and each answer is written into the decision it names. Point 7 was overruled: the record had sixel draw above kitty at equal z, and kitty placements among themselves by image id; at equal z the placement that reached the pipeline first now draws below (D4). Point 12 removed this record's wait for the C reference's A5 and A7 measurement (D12). The fresh review of 2026-11-01 corrected D1, D2, D3, D5, D6, D8, D9 and D10 in the same edit; the earlier text is in git at 6d87a19.

> Edited 2026-11-01, second round: the maintainer answered the four points the first round raised (`considered`). The 512-byte charge for each image (D2, D5), the minspec answer of 0 for modes 80, 1070 and 8452 (D10) and the 1 MiB bound on iTerm2 pictures (D6) went as the record leaned. The order of two kitty pictures at equal z did not: the record had them in arrival order like every other pair, and they now keep kitty's rule, the lower id below (D4), with an insertion rule that keeps the draw order one fixed order, under For the operator's check. The earlier text is in git at 15caffc.

## Context

vtium needs one image pipeline that the sixel handler, the kitty graphics handler, the iTerm2 compatibility handler, a picture over a picture, pictures in the CPU compositor, the sandboxed image helper, kitty animation and a static image at the login prompt all build on. `crates/vtium-image` holds only its crate documentation today. ADR-1000 lists the rows this record serves: `image.pipeline`, `image.sixel`, `image.kitty`, `image.kitty-animation`, `image.kitty-relative`, `image.kitty-placeholders`, `image.overlap`, `image.compositor`, `image.helper`, `image.iterm2`, `image.login`, `query.xtsmgraphics`, and the modes `DEC-80` (DECSDM), `DEC-1070` (private sixel color registers) and `DEC-8452` (sixel cursor to the right), each in the `images` layer.

The C reference built this in kmscon ADR-103 and its Notes: one store per screen buffer in libtsm, two protocol front ends, a helper process for files and compressed formats, and one budget for the whole process. vtium takes that logic flow and leaves its structure.

Several of vtium's own records already decide parts of the pipeline's surroundings. ADR-1001 gives each line a permanent `LineId` for placements to anchor to, and says that a placement added, moved, clipped or removed marks the cells under its old and new boxes. ADR-1003 decides how an anchor moves through a rewrap. ADR-1300 fixes the color glyph format, premultiplied RGBA, and the row scratch that pictures are composed in. ADR-1400 D2 carries the C reference's helper forward: its own process, the session's user, never root by fallback, a 2-second timeout with input paused. ADR-1401 D1 gives each head its consoles, each a session with its own screen, and D2 gives each head a render thread of its own. `docs/architecture.md` names the eviction rule and the decode timeout among its determinism rules. A review of vtium's frame path found that the line, damage and frame models already have room for placements, and that the head's `Snapshot` carries none yet.

`docs/c-reference.md` §9 left two questions this record answers: how eviction orders images across stores once each store has its own use clock, and whether the compositor's scaled pictures share the image budget.

vtium sets its own performance targets and measures them on vtium (the maintainer, 2026-11-01). Meeting or measuring the C reference's targets, such as ADR-103's A5 and A7, is not a condition of this record or of its build.

## Decision

The figure shows the pipeline: who owns each part, and the path a picture takes from a program's output to each display.

![vtium's image pipeline. One console's pty, read by vtium-platform and paused while a request is out, feeds vtium-parse, which hands on every DCS and APC string in pieces and an OSC string whole, up to 1 MiB. vtium-term's router sends DCS q and APC G to the two handlers, and OSC 1337 File= to the iTerm2 compatibility front end, each only while its switch is on. The two handlers in vtium-image, sixel and kitty, run in stream order and decode sixel, base64 and raw RGB and RGBA themselves; set apart beside them, a thin compatibility front end for iTerm2's OSC 1337 File= places each picture under sixel's rules. For PNG, zlib, files, temporary files, shared memory and iTerm2's formats a handler makes a request, which the binary sends to the image helper and whose reply it hands back, while vtium-platform pauses the pty and, in a box of its own beside the helper, holds the 2-second timer on each request and stops the helper on a failure; input stops after the string's end until the reply, or FAILED after 2 seconds. The image helper runs as the session's user, sandboxed by ADR-1400. The handlers place pictures in the store, one per console, owned by vtium-image: main and alternate buffers of images and placements, each placement with its anchor line and column, box, crop, z, layer, sequence number and protocol tag; a budget for each console, whose limit the binary sets, computing auto from the head's displays with vtium-image's rule; a use clock moved by transmissions, placements and references; each placement's place in the stack at its depth; and placements handed out in draw order, by layer, then z, then that stack, where a later picture goes on top except that a kitty picture slides beneath the first kitty picture with a higher id. rules(tag), the one place each protocol's rules live, is linked to the store and to the vtium-term screen, which both ask it. The vtium-term screen calls the store on each write, erase, scroll, reflow, alternate screen switch and RIS. vtium-term builds the frame: the placements on the visible rows in the store's order, with each image's size and its pixels shared. The binary's snapshot for the head takes the frame at a turn, shared by the displays ready then; during a mode 2026 hold a display keeps its last drawn picture. Each display's compositor in vtium-render draws the placements in the order the frame hands them, at its own scale, and keeps scaled copies in its own cache, outside the budget, which never evicts a stored picture. A legend colors each box by its owner: vtium-platform and the helper, vtium-parse, vtium-term, vtium-image, the vtium binary, and vtium-render.](figures/ADR-1100-pipeline.svg)

### D1: which crate owns what

| Owner | What it holds |
|---|---|
| `vtium-image` | The store: images, placements, the budget and eviction (D2, D5). The protocol tag and its rules function (D3). The handler trait (D6); the two protocol handlers, `protocol::sixel` and `protocol::kitty`; and the iTerm2 compatibility front end, `compat::iterm2`, which implements the same trait and places each picture as a sixel. The decoders that run in the console: sixel, base64, and raw RGB and RGBA into premultiplied RGBA. The decode request and reply types (D7). Its bounds table, which holds the store's limits and the `auto` rule as named constants (D2, D5). It depends on `vtium-parse` alone, for the string pieces and ends the parser hands out, and it names a line by an opaque 64-bit key, so it never depends on `vtium-term`. It forbids `unsafe`, denies the arithmetic, cast and indexing lints `docs/architecture.md` lists, and uses no `HashMap`. |
| `vtium-term`, behind its `images` feature | One store for each terminal it runs. The router that sends DCS `q`, APC `G` and OSC 1337 `File=` to the handlers, and the on and off switch of each protocol. The call into the store on every screen event (D8), with the `LineId` of each line as its key. The marking of `Damage` for each box the store reports changed, as ADR-1001 asks. The anchors carried through a rewrap (ADR-1003). The mode rows 80, 1070 and 8452, DA1's attribute 4, and the XTSMGRAPHICS reply, which the sixel handler's build pins. The `Query` each image reply is written under (D6). The frame's placement list (D9). |
| `vtium-render`, behind its `images` feature | Drawing a frame's placements in the order the frame hands them, the scaled copies and their cache, and the damage pictures cause (D5, D9). |
| `vtium-platform` | The helper's process: starting it as the session's user, its socket, its 2-second timer, and killing it (ADR-1400 D2). Pausing a pty's reads while a request is out. |
| `vtium`, the binary | Sending a request to the helper and handing its reply back to the terminal. The head's snapshot, which takes the frame's placements (D9). Computing `auto` from the head's displays with vtium-image's rule (D5). The configuration keys (D11). Turning the protocols on for a head (D10). |
| The helper | Reading files and shared memory under the file rules, and decoding PNG, zlib and the iTerm2 formats, as the session's user. Whether it is a second executable or the console's own binary is for the image helper's own record to decide, as ADR-1400 D2 leaves it. |

### D2: the store

- **One store for each console,** a console being ADR-1401 D1's session with its own pty, screen and scrollback. The store holds two buffers, main and alternate, and each buffer has its own images, its own kitty ids and numbers, and its own placements, as kitty keeps the two screens apart and the C reference kept one store per screen buffer (the maintainer, 2026-11-01). The two buffers share one budget and one use clock (D5).
- **An image** has a key, a 64-bit number the store counts up and never reuses; its protocol tag; for kitty, its id (1 to 2^32-1) and image number, if it has them; its width and height; its pixels, premultiplied RGBA, 4 bytes a pixel, the color glyph format of ADR-1300, for every protocol (the maintainer, 2026-11-01), held behind a shared reference so a frame can hold them without a copy; whether every pixel is opaque; for a tag whose rules say a new picture covers older ones per cell, which cells it covers fully and which it leaves empty, which a picture drawn over another picture uses; the use stamp and the transmission stamp of D5; and whether kitty's `N=1` marked it transient.
- **A placement** has a key, counted up and never reused; its image's key; its protocol tag; its anchor, which is the key of the line holding its top row and a column; its box in cells; its offset in pixels inside its first cell; its source rectangle in the image's pixels, which is the crop; its z; the layer its tag's rules give it (D4); its sequence number and its place in the stack of placements at its depth (D4); its kitty placement id, or none; and the cell size in pixels it was made in, which is the unit of its pixel values.
- **Cutting splits a placement into pieces.** Each piece is a placement of the same image with a smaller box, and its source rectangle follows from that box. The picture itself is never cut or copied. A piece clipped to nothing is removed, and a sixel's image goes when its last piece goes (D3).
- **The store's maps are ordered.** Images and placements are kept in ordered maps by key, and an index from line key to the placements anchored there is used for lookups only. Nothing orders lines by their key, as ADR-1003 notes that line ids are unique but carry no order. Whatever reaches the screen, a reply or eviction is taken in key order, sequence order or use order, never from a hash.
- **Bounds.** The limits are named constants in vtium-image's bounds table, each row with its reason, starting from the C reference's values and moved later by measurement on vtium (the maintainer, 2026-11-01): 1024 images and 4096 placements in each buffer; an image side of 65,535 pixels, below which `max-graphic-size` sets the working limit (D11); a cell of at most 1024 pixels a side; a box of at most 10,000 cells in each direction, columns and rows each; 512 bytes of the budget charged for each image beside its pixels, which pays for its first placement; and 256 bytes charged for each placement of an image past its first. Every size computed from a program's numbers is checked arithmetic, in 64 bits.
- **A count bound in one buffer.** A new image or a new placement past its buffer's count evicts in D5's order from that buffer alone, and is refused when nothing there may go. A cut or a scroll cannot be refused, since text and erases always land: one that would pass the placement count keeps the largest pieces and drops the smallest, as if their cells were erased, and pieces whose 256-byte charge the budget refuses are dropped the same way, smallest first.

### D3: the protocol tag and the rules function

Every image and placement carries the tag of the protocol whose rules it follows: the protocol that made it, or sixel for a picture the iTerm2 compatibility handler placed. The tag's variants are private to one module of vtium-image, `rules`, and that module's one public function maps a tag to a table of rules. The store and vtium-term's screen ask the table. The frame entry carries what the rules resolved, the layer, the draw order and the kind's printed name, so the compositor reads no tag and asks no rule, and vtium-replay prints a placement's kind from the name the table gives. Code outside the module cannot name a variant to match on it. vtium enforces this by module privacy, where a check that searches the source would be needed in C, and a `compile_fail` doctest shows a match outside the module failing to build. A third protocol adds a tag and one arm of the function.

The store knows two tags, sixel and kitty. The iTerm2 compatibility handler has no tag of its own: it places each picture under the sixel tag, so every rule in the sixel column applies to it, and a replay prints its kind as `sixel`. vtium carries OSC 1337 `File=` so that programs that send it work, never as an image protocol of equal standing to sixel and kitty (the maintainer, 2026-11-01). Mapping its pictures onto sixel's rules follows what the protocols themselves describe and adopts no one terminal's special handling, such as WezTerm's pictures kept in cells (the maintainer, 2026-11-01, for protocol compatibility).

| Rule | Sixel | Kitty graphics |
|---|---|---|
| A character written into a cell it covers | Cuts the part under that cell | No effect |
| ED 0, ED 1, EL, ECH, DECSED, DECSEL, and a rectangle erase or fill (DECERA, DECSERA, DECFRA) over it | Cuts the part under the erased cells | No effect (kitty spec: "The other commands to erase text must have no effect on graphics") |
| A rectangle copy (DECCRA) into cells it covers | Cuts the part under the destination, as a write there does | No effect |
| ICH, DCH, and printing in insert mode on a row it covers | Cuts the part from the cursor to the end of the row, since shifting part of a picture by whole cells would tear it | No effect |
| A new picture of the same tag over it | The parts under the cells the new picture covers fully are removed, and a piece left with no set pixel is removed, the per-cell coverage the operator chose for the C reference | No effect |
| A region scroll (margins, IL, DL) | The part inside the moving band moves with it, the placement splits at the band's edges, and parts pushed out of the region are clipped | A placement wholly inside the band moves and is clipped at its edges (kitty spec: only images "entirely within the page area" scroll, and are clipped). One that crosses an edge keeps its place on the screen: after the scroll it is anchored to the line that then holds its top row, as the C reference did |
| Eviction while placed on the screen's own lines | Never | Allowed, oldest transmission first (D5) |
| Its layer | Above text, at z 0 | From its z (D4) |
| Kitty's delete commands | Never reach it | Reach it, by the spec's table |
| Its image without placements | Is freed, since nothing can name it again | Stays until it is deleted or evicted, so a lowercase delete keeps its data (kitty spec) |
| Its printed kind | `sixel` | `kitty` |

The events every placement meets alike are D8's and are not in the table.

### D4: the order a frame draws placements in

A frame draws, from the bottom: the default background; the layer below backgrounds, kitty placements with z below -1,073,741,824; the cell backgrounds that are not the default; the layer below text, kitty placements with z from -1,073,741,824 to -1; glyphs and decorations; the layer above text, kitty placements with z of 0 or more and every sixel placement, the iTerm2 handler's among them; and last, above every placement, the cursor, the pointer and vtium's own notices and marks, so no program's picture can hide them (the maintainer, 2026-11-01). The lowest layer shows only through cells whose background is the default, which the frame cell's `default_bg` tells the compositor (ADR-1001).

Within a layer, placements draw by z. The placements at one z in one layer form a stack, bottom to top, and the frame draws each stack in that order. The store keeps the stack as placements arrive, with these rules:

- **A later picture goes on top.** A sixel, or a kitty picture that kitty's rule below does not place lower, goes on the top of its stack when it arrives, so a program can order its pictures on purpose by the order it sends them (the maintainer, 2026-11-01). The order of arrival is the sequence number: a per-store count, taken by each placement when it is made, in the order its string reached the pipeline.
- **Two kitty pictures keep kitty's rule.** Between two kitty placements of different image ids, the lower id draws below, whatever their order of arrival, as the kitty spec says: "If two images with the same z-index overlap then the image with the lower id is considered to have the lower z-index" (the maintainer, 2026-11-01). Two placements of the same image keep their order of arrival, which the spec leaves undefined. A kitty picture whose image has no id, sent with neither `i` nor `I`, has no id to compare: it goes on top as a sixel does, and a later kitty picture looking for the first higher id passes over it. The maintainer confirmed this on 2026-11-01.
- **A kitty picture slides in beneath the first higher id.** When a kitty placement arrives, the store looks up its stack from the bottom for the first kitty placement whose image id is higher than the new one's. When there is one, the new placement goes directly beneath it; when there is none, it goes on top. Kitty placements in a stack are therefore always in order of id, and every pair that involves a sixel is in order of arrival, with one exception: a sixel lying above the kitty picture a newcomer slides beneath stays above the newcomer, although it arrived first. Nothing already in the stack moves, so the order is one fixed order, decided by the bytes alone, and never a loop. The maintainer confirmed this rule on 2026-11-01.
- **Pieces of one placement.** A cut or a scroll that splits a placement makes its pieces in a fixed order: for a cut, the part above the cut cells, the part to their left, the part to their right, and the part below; for a region scroll, top to bottom. The first piece that still shows any of the picture keeps the placement's key and its place in the stack; each other piece takes a new key, counted up in the order the pieces were made, and sits directly above the pieces before it. The pieces of one placement therefore stay together at its place, in the order they were made, so a frame numbers them the same way every time, as the C reference's `placement_split` and `frame_rec_cmp` did. Pieces cover different cells, so their order changes no pixel. A piece cut again splits the same way at its own place. Where a cut or a scroll passes the placement count (D2), the pieces that stay are the largest by area, ordered largest first, with pieces of equal area in the order they were made, and the largest keeps the placement's key and its place; below the bound the order they were made in holds. vtium follows the C reference's `placement_split` there so that its goldens at the bound stay valid.
- **Replacements.** A kitty placement that replaces one with the same image and placement id is a new arrival: it takes a new sequence number and goes where a newly arrived kitty picture goes, on top, or beneath the first higher id. The maintainer's answer that a replacement "draws on top" (point 11) is refined by the kitty id rule he chose later the same day (second round, Q3): it draws on top unless a kitty picture with a higher id is already in its stack.

An example. A program shows kitty picture 2, then a sixel chart over it, then kitty picture 1, all overlapping at z 0. Kitty picture 1 slides in just beneath picture 2, so from the back the screen shows picture 1, picture 2, then the chart on top. The chart stays on top although picture 1 arrived after it.

The store hands the frame its placements already in this order (D9), and the compositor draws them as handed.

The C reference drew every sixel above every kitty placement at equal z, and kitty placements among themselves by image id, lower first, as the kitty spec asks. vtium keeps the second and lists the first as a departure. Twenty-eight of the C reference's cases place a sixel before a kitty picture and print both at z 0 in one block, so their goldens can differ from vtium's draw order; the build of the image pipeline confirms each when it runs them and adds those that differ to the conformance runner's departures list: `img-alt-1049`, `img-dch`, `img-decsed`, `img-decsel`, `img-decstr`, `img-discard-alt`, `img-discard-sb`, `img-dl`, `img-ech`, `img-ed0`, `img-ed1`, `img-ed2`, `img-ed3`, `img-el0`, `img-el1`, `img-el2`, `img-ich`, `img-il`, `img-print`, `img-region`, `img-resize`, `img-resize-shrink`, `img-scroll-down`, `img-scroll-down-sb`, `img-scroll-up`, `img-sixel-over`, `kit-evict` and `kit-replace-refused`, in both the screen and the frame view.

The figure shows the layers, the order within a layer, and what one erase does to each kind of placement.

![Two panels. A: the layers of a frame from the bottom: 1, the default background; 2, kitty placements with z below -1,073,741,824; 3, cell backgrounds that are not the default; 4, kitty placements with z from -1,073,741,824 to -1; 5, glyphs and decorations; 6, kitty placements with z of 0 or more, and every sixel at z 0, the iTerm2 handler's pictures among them; 7, the cursor, the pointer, and vtium's own notices and marks. Below the stack: within a layer, placements draw by z; at equal z a later picture draws above an earlier one, except that a newly arrived kitty picture slides in beneath the first kitty picture with a higher id, and nothing already shown moves. B: a grid of ten columns and four rows before and after EL 2 erases its third row. An orange sixel four columns wide covers all four rows, and a teal kitty placement three columns wide beside it covers all four rows. After the erase, the sixel has lost the erased row and remains as two pieces, the top two rows and the bottom row, while the kitty placement is unchanged, since no erase command but ED 2 reaches it. Each piece shows part of the same picture, and the picture itself is never cut.](figures/ADR-1100-layers.svg)

### D5: the budget and eviction

- **A budget for each console** (the maintainer, 2026-11-01). `image-storage-limit` bounds the bytes of every image in one console's store, main and alternate buffers together. An image costs width × height × 4 bytes plus its 512-byte charge, as the C reference counted it (the maintainer, 2026-11-01), and each placement of an image past its first costs 256 bytes (D2). The store reserves an image's whole cost before it allocates a byte of it, and a payload buffered for the helper reserves each growth before it grows (D7).
- **The default, `auto`,** is four frames of the console's head's displays in RGBA, width × height × 4 added over the head's displays, at least 16 MiB and at most 192 MiB: 16 MiB at 1366x768 and 31.6 MiB at 1920x1080. The frame count, the floor and the cap are named constants of vtium-image's bounds table, so they are easy to adjust and can later become configuration keys if that is warranted (the maintainer, 2026-11-01). The binary computes `auto` again when a display joins or leaves the head or changes mode, and a smaller `auto` evicts nothing at once: the next reservation evicts down to the limit it finds, as the C reference settled after a font change briefly uncounted a display. A limit set in the configuration applies as set. `0` turns both protocols off (D10).
- **Host events.** A display joining, leaving or changing mode, a set limit and a protocol offered or withdrawn (D10) are host events. Each falls between two input calls, as a resize does (ADR-1003, "Determinism"), so what a console's store holds follows from its bytes and its host events in order, never from when frames were drawn.
- **Refused before anything is evicted.** When a reservation would pass the budget, the store first counts what eviction may free. When that falls short, it evicts nothing and refuses: kitty answers ENOSPC, and a sixel, the iTerm2 handler's included, is dropped. The C reference made the same fix. The image a kitty transmission replaces is held while the store makes room, so a refused replacement keeps the old image, a fix the C reference made too.
- **The eviction order,** across the console's two buffers:
  1. images with no placement, transient ones before the rest, each least recently used first;
  2. images whose every placement lies in the scrollback, least recently used first, with their placements;
  3. images whose rules allow eviction while placed on the screen's own lines, kitty's, oldest transmission first, with all their placements.

  An image whose rules never allow eviction while placed on the screen's own lines, a sixel, is never evicted while it is, so a store full of them refuses. The screen's own lines are the buffer's visible rows, never the scrollback. A count bound evicts in the same order from one buffer alone (D2).
- **The use clock is the store's own.** It is a count that moves by one at each stream event that touches an image: a transmission, a placement made, and a command that names an existing image, such as a kitty put or query by id or number. Each image takes the clock's value as its use stamp at each such event, and as its transmission stamp when it is transmitted. A drawn frame, a snapshot, a resize, a VT switch and a display's arrival never move it, so eviction follows the bytes. The C reference renewed every drawn image's use on each frame, on a clock shared by the whole process, so its eviction order depended on which frames were drawn and when; vtium lists that as a departure. Each stamp is unique within the store, so the order is total.
- **The renderer's scaled copies are outside this budget** (the maintainer, 2026-11-01). A display that draws a picture at another size keeps the scaled copy in a cache of the renderer's own, bounded per display (the CPU compositor work sets the bound; the C reference's was twice the screen in RGBA). A copy that does not fit is drawn scaled for that frame without being kept, so the cache changes speed and never pixels, the rule ADR-1300 D3 sets for glyphs. No scaled copy ever evicts a stored image. The C reference let a scaled copy for a picture on screen evict pictures in the scrollback (ADR-103's Notes, Q1); vtium lists that as a departure, because the store would then depend on what was drawn.
- **Scans count as input work.** Every walk over placements, on a scroll, an ED 2, a kitty delete or an eviction, adds what it visits to the input call's work, as decoded pixels do, so a flood of line feeds or ED 2 against a full store stops the input call early and the rest is taken by the next call, as the C reference counted them. The work bound is a named constant of vtium-term's bounds table, starting from the C reference's 2^24 a call. It changes when work is done and never the state that results (`docs/architecture.md`, "Chunking invariance").

### D6: the handler trait

A handler turns one protocol's control strings into store and screen changes. vtium-image defines the trait, and each protocol module implements it:

- **`start`** takes the string's introducer: a DCS string's parameters, or the start of an APC or OSC string the router has matched to the handler.
- **`feed`** takes the string's data. A DCS or APC string comes in the pieces the parser hands out as they arrive: a sixel string grows its picture as its bands arrive, and a kitty string decodes its base64 into the pending image, so neither holds a whole payload it can decode as it goes. A PNG or zlib payload sent directly is the exception: its base64-decoded bytes are buffered in the console, under the budget, to be handed to the helper whole (D7). An OSC string reaches its handler in one piece, since the parser collects an OSC string whole (Routing, below). `feed` reports the work it did (D5).
- **`finish`** runs when the string ended with its terminator, which `StringEnd::is_complete` reports. It places the picture, or makes a decode request (D7), and replies.
- **`cancel`** runs when the string was cancelled, by CAN, SUB, another C1 control, or an ESC not followed by `\`. The handler drops what it holds and releases what it reserved; nothing is placed and nothing is answered. Acting only on a complete string is vtium-parse's listed departure from the C reference, which acted on a string however it ended.
- **`decoded`** takes the helper's answer to the handler's request, and places, stores or replies as `finish` would have.

Each method gets one context, which vtium-term implements: the console's store and the buffer in use; a decode request sink; the cell size in pixels and the grid's size; the cursor, which a handler reads and moves through the context; the index the screen does at the bottom margin, which a sixel uses to scroll; the reply sink; the protocol switches; the limits of D11; and the input-work count. A handler never reaches the screen's lines itself, so every scroll it causes passes through the screen and the store hooks of D8.

**Replies.** A handler gives the reply sink its reply with a kind from a closed set vtium-image defines, such as a kitty graphics reply. vtium-term maps each kind to one `Query` variant and writes the reply through `State::reply`, so every image reply is in the generated list the reply rule's test sends hostile text through (`docs/architecture.md`, "No reflection"). A kitty reply carries only numbers it re-encodes and fixed messages.

**Routing.** The router in vtium-term sends DCS `q` with no intermediate to the sixel handler and APC strings whose first byte is `G` to the kitty handler, each only while its protocol is on, as pieces arrive. vtium-parse collects an OSC string whole, up to `OSC_MAX`, 1 MiB, and hands it over once; the router gives a complete `1337;File=` string to the iTerm2 compatibility handler as `start`, one `feed` and `finish`. vtium-term acts on no truncated string, so an iTerm2 picture whose string passes 1 MiB, about 768 KiB of image file after base64, is dropped whole, until the iTerm2 handler and the OSC string bound decide whether to raise it (the maintainer, 2026-11-01). Every other DCS, APC and OSC string stays as ADR-1000's rows say. Kitty's keys section is buffered up to 1 KiB, and a longer one makes the whole string ignored, as in the C reference.

### D7: the decode boundary and the barrier

- **In the console:** sixel, base64, and raw RGB and RGBA (kitty's `f=24` and `f=32`), each converted into premultiplied RGBA as it arrives. These decoders face anything a program writes, and each takes a fuzz target, built with the sixel and kitty handlers.
- **In the helper:** PNG (`f=100`), zlib (`o=z`), file, temporary-file and shared-memory media (`t=f`, `t=t`, `t=s`), and the image formats iTerm2's `File=` sends. The console never opens a file or shared-memory object a program names and never inflates or decodes PNG; it holds the bytes of a directly sent PNG or zlib payload only to hand them over. The helper runs as the session's user in ADR-1400 D2's sandbox, and the image helper's own record decides its file rules, its identity modes and whether a syscall filter narrows it.
- **The request and reply types live in vtium-image,** in the C reference's mechanical terms, so the helper reads no part of any protocol's specification: what to open (a path, a shared-memory name, or the received bytes), where to start, the fewest bytes the image needs and the most that may be read, whether to inflate or decode, and, for a refused temporary file or shared-memory object, a discard that removes it and reads nothing. The largest image the request may return comes from the budget and `max-graphic-size`, and the helper checks a picture's header against it before it allocates. A reply is one outcome: OK with the pixels or the bytes asked for, or `UNREADABLE`, `TOO_SHORT`, `CORRUPT`, `TOO_LARGE`, `OVER_BUDGET` or `FAILED`. The handler turns the outcome into its protocol's reply, such as kitty's EBADF, EINVAL or ENOSPC. The console checks every reply's sizes before it maps or stores anything, as it checks the glyph helper's chunks (ADR-1200 D15).
- **The barrier.** When a handler makes a request, the terminal's input call stops right after the byte that ended the string and returns what it took, and takes nothing more until the reply is handed back. The binary sends the request and keeps the bytes that follow, and vtium-platform stops reading that console's pty; other consoles, drawing, keys and VT switching carry on. One request is out for each console at a time. A later string therefore cannot land before an earlier image, whichever protocol each belongs to, and a client may send a put right after a file transmission without waiting.
- **Time enters in one named place.** A request the helper has not answered in 2 seconds, held by vtium-platform's timer, gets `FAILED`, the helper is stopped, and the next request starts a new one, as `docs/architecture.md` and ADR-1400 D2 already decide. Input stays paused until the reply or the timeout, so no byte reorders around either. A console whose session ends drops its request without a reply.
- **Determinism for the replay interface.** The state after a stream follows from its bytes, its host events and the replies to its requests, in order. The replay interface's fixture host answers each request at once from the case's fixture files (`tools/conformance/corpus/INTERFACE.md`, "The fixture host"), so no case depends on time or on a helper, and the chunking property test runs through it: a stream fed whole, or cut anywhere, including between a request and its reply, ends in the same state and the same replies.
- **Without a helper,** as in a host that installs none, file, temporary-file and shared-memory media answer EBADF and PNG and zlib answer EINVAL, as the C reference answered without its host hook, so the kitty handler can be built before the image helper.

### D8: how a placement travels with text

The events below act on every placement alike; the cutting and region-scroll rules each tag has are D3's.

| Event | What happens to the placements |
|---|---|
| Scrolling the whole screen (LF or IND at the bottom margin, SU, SD, RI at the top) | They move with their lines, and lines entering the scrollback carry them. |
| A line dropped from the scrollback past `sb-size` | Placements anchored there are clipped by that row and anchored on the next line. |
| ED 2 | Every placement on the screen's own lines is removed, as kitty's `a=d` does (kitty spec: the clear screen code "should also clear all images", "so that the clear command works"). A kitty image keeps its data, as a lowercase delete keeps it, which is the C reference's reading where the spec says nothing; a sixel's image goes with its last placement. |
| ED 3 | Placements anchored on the dropped scrollback lines go. |
| A width change on the main screen | The rewrap of ADR-1003 returns, for each anchor in the rows it rewrapped, the row and column now holding that cell, and the store moves the placement there. A placement wider than the columns left keeps its whole picture and overflows the far edge, the right one for text running left to right. Rows not yet rewrapped keep their anchors until they are reached. |
| Any other resize | Lines keep their placements, and drawing clips at the right edge and the bottom; lines the height change moves to or from the scrollback carry theirs. |
| A cell size change (zoom, or a head's grid at another size) | A picture scales with its box, so it stays aligned with the text around it. |
| Entering or leaving the alternate screen (47, 1047, 1049) | The alternate buffer is emptied, on entry as the kitty spec asks ("all images in the alternate screen must be cleared, just as all text is cleared") and on leaving, as the C reference did, so a full-screen program's pictures do not hold memory after it exits. The main buffer's placements show again on leaving. |
| RIS | Both buffers are emptied. |
| DECSTR | No effect, since a soft reset leaves the text. |
| A region scroll that clips the top of a placement or a piece | The part that remains is anchored to the line that holds its new top row after the scroll, as the C reference re-anchored it at its row. |
| Moving the view through the scrollback | No effect on the store. A frame finds placements by walking the visible lines, and as many lines above the top as the tallest placement has rows, so a picture whose top has left the view still draws its lower rows. |
| Every change above | vtium-term marks the cells under each placement's old and new box changed (ADR-1001). |

### D9: the frame, the snapshot, and mode 2026

- **The frame gains a placement list:** every placement with a visible row, in the draw order of D4. Each entry carries its image's key, its visible row (negative when its top is above the view) and column, its box, its pixel offset, its source rectangle, the cell size it was made in, its z, its layer, the kind's printed name from the rules table (D3), its image's width and height, whether its image is opaque, and a shared reference to the image's pixels. The entry carries no tag. vtium-term builds the list from its store.
- **The head's snapshot takes the list** with its pixels shared, never copied, as the frame path review asked. Nothing else in the snapshot's life changes. As `crates/vtium/src/term.rs` builds it today, the first display of a head ready for a frame at a turn of the loop takes a snapshot of the console the head shows, and every other display of the head ready at that turn shares it; a display whose flip is in flight or whose last frame is still composing takes a later one. Each frame sent to the head's render thread holds its snapshot until it is composed. The render thread composes from snapshots alone and never reaches the store, so drawing cannot change a console's state.
- **Mode 2026.** While a hold is in force, a display that still shows its last frame takes no new snapshot and keeps the picture it last drew; a display that lost its picture draws the state as it is, and the hold goes on (`crates/vtium/src/sync.rs`).
- **Memory outside the budget.** An image deleted or evicted stays in memory while a snapshot a frame in flight holds still names it: at most one snapshot for each display of the head, each released when its frame is composed. The snapshots share pixels, so the memory past the budget is only the pictures deleted between the oldest and the newest of them. The budget counts the store's images only, so eviction still follows the bytes. The maintainer accepted pixels held this way outside the budget, and this bound, on 2026-11-01; it replaces the "two snapshots per head" this record first stated, which described vtium wrongly.

### D10: the `images` feature, and what minspec carries

- **One Cargo feature, `images`,** in the binary, which turns on `vtium-term/images` and `vtium-render/images` and links vtium-image. vtium-term takes vtium-image as an optional dependency.
- **The minspec build links none of it.** Without `images` there is no store, no handler and no decoder in the binary, as the operator chose for the C reference's minimal build, where the decoders were absent rather than unreachable. DCS `q`, APC `G` and OSC 1337 `File=` are ignored as every control string vtium does not take is. DA1 reports no attribute 4. A kitty query gets no reply, so a client's detection (a query followed by DA1) finds no support. XTWINOPS 14, 16 and 18 report sizes in every build, as their minspec rows already say. The sixel handler's build pins XTSMGRAPHICS's bytes, with one rule: a build without `images` answers it as a build with sixel switched off does.
- **Modes 80, 1070 and 8452.** In the minspec build they have no row in the mode table, so DECRQM answers 0 for them and SM and RM change nothing, the mode table's rule for a number with no row. The C reference kept the three rows in every build and, while sixel was off, answered DECRQM with each row's fixed state (4, permanently reset, for 80 and 8452; for 1070, 3 or 4 by `private-color-registers`), while SM and RM still stored the mode for when sixel came on. vtium lists the minspec answer as a departure, which the maintainer accepted on 2026-11-01: the build answers 0, "I don't know that setting", as for any mode it lacks. In a build with `images`, while sixel is not offered, vtium answers as the C reference did.
- **A protocol is offered only where it can be drawn,** as the operator decided for the C reference: a head offers sixel and kitty graphics, and stores new pictures, only while `image-storage-limit` is above 0, the protocol's switch is on, and every display of the head has a renderer that draws placements. Until the CPU compositor draws pictures no renderer does, so a build with `images` offers nothing on a real head, while the replay interface's host, which draws nothing, offers both. Pictures stored before a display without placement drawing joined the head stay, counted and not drawn, and show again when it leaves. A protocol offered or withdrawn is a host event (D5).
- **The gates test both builds:** the gate line already runs `cargo test --workspace` with default features beside `--all-features`. Tier 3's install tree takes a feature list, with the size budget applied to the minspec tree alone, as the frame path review asked.

### D11: settings

Each setting takes the name another terminal gives it, as the C reference's did, and becomes a row of the binary's key table (`crates/vtium/src/config/keys.rs`) in the change that first reads it.

| Key | Named after | Default | Meaning |
|---|---|---|---|
| `image-storage-limit` | Ghostty's `image-storage-limit` | `auto` (D5) | The bytes of each console's images, main and alternate together. Plain bytes or K, M and G, up to 1 TiB. `0` turns sixel and kitty graphics off. |
| `max-graphic-size` | xterm's `maxGraphicSize` | `10000x10000` | The largest picture in pixels, for every protocol. |
| `num-color-registers` | xterm's `numColorRegisters` | `1024` | Sixel color registers. |
| `private-color-registers` | xterm's `privateColorRegisters` | `true` | The state of mode 1070 at reset. |
| `sixel`, `kitty-graphics` | the C reference's switches | on | Turn one protocol off: it is neither offered nor decoded. |

The replay interface's reference host sets the budget to 320,000,000 bytes for each screen, which vtium reads as each console's store, and turns both protocols on.

### D12: the order the issues build in

vtium's performance targets are its own, set and measured on vtium (the maintainer, 2026-11-01), so no step waits on a measurement of the C reference.

1. **The image pipeline:** vtium-image's store, tag and rules, budget and eviction, handler trait and decode types; vtium-term's `images` feature, routing, store hooks on every screen event, and the frame's placement list; the head's snapshot taking it; and vtium-replay built with `images`, declaring the `images`, `sixel`, `kitty-graphics` and `image-host` features with its fixture host. The replay interface's `image` and `place` commands drive the store before any handler exists.
2. **The sixel handler and the kitty graphics handler, in parallel,** each in its own module of vtium-image. The kitty handler's media through the helper answer as D7's "Without a helper" says until the image helper is built.
3. **The sandboxed image helper, in parallel with the two handlers,** once D7's request and reply types are fixed, as the C reference built its helper beside its kitty parser against a fixed interface. It needs its own record, which decides its sandbox.
4. **Pictures in the CPU compositor,** after the CPU compositor itself and the image pipeline. A head offers the protocols once the compositor draws placements (D10).
5. **A picture over a picture,** after the pipeline and the sixel handler. **The iTerm2 compatibility handler,** after the pipeline, the image helper and the OSC string bound. **Kitty animation,** after the pipeline and a picture over a picture, with its own record. **A static image at the login prompt and as the console's background,** after the vtium binary, the pipeline, pictures in the CPU compositor and the image helper, with its own design note.

### Verification

- **Store tests for each tag:** a write, each erase, ICH and DCH, a region scroll (a kitty placement crossing an edge included), eviction and the layer, for sixel and kitty, each seen to fail first.
- **The rules function is the only match on a tag:** the `compile_fail` doctest of D3.
- **Stream order across a pending decode:** interleaved sixel and kitty strings, with a request out between them through the fixture host, land in stream order and draw in D4's order at equal z; the same test with the barrier removed, planted, fails.
- **Pieces of one placement:** an erase through the middle of a sixel leaves its pieces numbered in the order of D4, above, left, right, below, the piece above keeping the placement's key; the same test with the pieces ordered by size, planted, fails.
- **The stack at one depth:** kitty picture 2, a sixel, then kitty picture 1, all at z 0, draw from the back as picture 1, picture 2, the sixel; the same stream fed whole and cut at random points gives the same order; with the slide planted as "always on top", the test fails.
- **The budget:** a reservation too large for what eviction could free evicts nothing and is refused, seen to fail with eviction planted before the count; a refused kitty replacement keeps the old image; the eviction order, rank by rank; a cut past the placement count keeps the largest pieces, ordered largest first, the largest keeping the placement's key.
- **The flood:** a stream of transmissions, placements, line feeds and ED 2 against a full store, fed whole and cut at random points, with frames drawn at different points and none, ends in the same store; with a scan's input-work count removed, planted, the bounded input call no longer stops early and the test fails.
- **No order from a hash:** `clippy::disallowed_types` denies `HashMap` and `HashSet` in vtium-image, as in every crate that reaches state.
- **The reply rule:** every image reply kind maps to a `Query` in the reply rule's generated list (D6).
- **The minspec build:** `cargo tree -p vtium -e normal` with default features lists no vtium-image, and the minspec mode table has no rows 80, 1070 or 8452.
- **Tier 1:** the corpus's image cases pass through vtium-replay built with `images`, through the conformance runner, apart from the draw-order cases D4 names, and the capability probes that need `images` pass in that build and fail without it.
- **Coverage:** each test that drives an `image.*` row of ADR-1000 through vtium-image carries its `covers:` tag.

### Passages this record changes once accepted

These passages say something this record decides otherwise. Each changes in its owner's change after ADR-1100 is accepted; this record edits none of them.

- **`docs/architecture.md`, the crate tree (lines 5-13),** draws vtium-image under the binary alone. Under D1 and D10, vtium-term takes it as an optional dependency, and vtium-render reaches its types through vtium-term's frame.
- **`docs/architecture.md`, "Eviction follows the bytes",** names "a transmission or a placement" as the events that advance the use clock. D5 adds a command that names an existing image, as the maintainer's determinism comment does.
- **ADR-1001, "The frame",** says vtium-image "marks its boxes through `Damage`". Under D1 vtium-term marks the boxes the store reports changed, so vtium-image never depends on vtium-term.
- **`docs/c-reference.md` §9** says the C reference bounded "a box of 10,000 cells"; the bound was 10,000 cells a side (`libtsm/src/tsm/tsm-image.c:1899`). Its "Check" and "Not planned" are answered by D5.
- **`tools/conformance/README.md`** says vtium-replay is built without images. D12 gives it a build with `images` and the features that go with it.
- **The conformance runner's departures list** gains the draw-order cases D4 names.

### For the operator's check

No item is pending. Every agent call this record made has been answered by the maintainer and written into the decision it names (`considered`).

> Edited 2026-11-01, third round: the maintainer confirmed the one item left after the second round, where a newly arrived kitty picture goes in its stack (D4), as the record leaned. It read: "1. **Where a newly arrived kitty picture goes in its stack** (D4). Lean: it slides in just beneath the first kitty picture above it in the stack that has a higher id, and goes on top when there is none, so nothing already on the screen moves." The earlier text is in git at 0b0b9e9.

> Edited 2026-11-01, fourth round: the re-check of 870ac65 found D4 silent on a kitty picture sent without an id; item 1 is the agent's call on it, and the section read "No item is pending."

> Edited 2026-11-01, fifth round: the maintainer confirmed item 1 as the record leaned (`considered`). It read: "1. **A kitty picture sent without an id** (D4). Lean: it stacks like a sixel, on top when it arrives, and kitty's id rule leaves it out, since it has no id to compare. In plain words: a program shows kitty picture 5, then a kitty picture it sent without naming it, then kitty picture 3, all overlapping at the same depth. Picture 3 slides in under picture 5, and the unnamed picture, which arrived before picture 3, stays on top of both. The other way would give an unnamed picture the lowest possible id, so it would always sink beneath every named kitty picture at its depth, whenever it arrived." The earlier text is in git at c6efada.

> Edited 2026-11-01, sixth round: the maintainer ruled that vtium carries iTerm2's OSC 1337 `File=` as a compatibility handler, not as an image protocol of equal standing to sixel and kitty (`considered`). The Summary, Context, D1, D3, D4, D5, D6, D8, D12, the departures, the Consequences and both figures now say so. D3's table had an iTerm2 column that repeated sixel's rules, and printed the kind as `iterm2`; the handler now places each picture under the sixel tag. The earlier text is in git at 349fda6.

## C notes: the C reference's flow

How the C reference handled images, as the logical reference for the decisions above, read at kmscon `b1f7a7c5` and libtsm `eeaf293`.

- **Stores.** libtsm kept a `struct tsm_image_store` for each screen buffer and reached the one in use through `cur_store`. Placements were listed on their line (`placement_attach`) and cut by `placement_split` and `placement_cut`; screen events reached the store through `screen_images_scroll`, `screen_images_cut`, `screen_images_erase_screen`, `screen_images_clear_alt`, `screen_images_reset` and `screen_images_discard_line` (`libtsm/src/tsm/tsm-image.c`). In a region scroll, `screen_images_scroll` left a kitty placement that crossed an edge where it was and re-anchored it through `placement_pend` to the line that took its row (`tsm-image.c:777-783`).
- **Eviction.** `image_rank` gave each image its rank, `image_order` its order within the rank (`transmitted` for kitty images on the screen, `last_use` otherwise), and `image_victim` picked the next to go across both buffers, or from one buffer for a count bound (`store_make_room`, `placement_make_room`). `evictable_bytes` counted what could go before anything went. `screen_images_frame` stamped `last_use = ++image_clock` on every placement each frame drew (`tsm-image.c:1120`), and `image_clock` was one static counter for the whole process (`tsm-image.c:46-50`).
- **Bounds and charges.** `TSM_SCREEN_IMAGE_OVERHEAD` (512), `TSM_SCREEN_IMAGES_PER_STORE` (1024), `TSM_SCREEN_PLACEMENT_OVERHEAD` (256), `TSM_SCREEN_PLACEMENTS_PER_STORE` (4096) and `TSM_SCREEN_IMAGE_MAX_CELLS` (10,000, checked against a box's columns and rows each, `tsm-image.c:1899`) in `libtsm.h`; `placement_make_room` charged the placement overhead, and `screen_images_settle` paid a debt the budget refused by dropping the smallest placements first.
- **Draw order.** `frame_rec_cmp` sorted a frame's placements by z, which also set the band, then kitty before sixel, then image id, then creation order, a placement's pieces by key (`tsm-image.c:951-967`).
- **Modes.** `tsm-modes.def` marked 80, 1070 and 8452 `HOST_SIXEL`, and `mode_offered` in `tsm-vte.c` reported their fixed state through `MODE_FIXED` while `vte_sixel_on` was false.
- **Routing and the pause.** `vte_dcs_start`, `vte_dcs_collect` and `vte_dcs_end` fed the sixel decoder (`tsm_sixel_feed_some`, `tsm_sixel_finish`), and `vte_apc_start`, `vte_apc_collect` and `vte_apc_end` the kitty parser (`tsm-vte-image.c`). A bounded input call stopped at `VTE_INPUT_WORK`, 2^24 (`tsm-vte.c:3690`), and took nothing while `vte_image_paused` held. kmscon's `kmscon_pty_pause` and `kmscon_pty_resume` stopped and restarted a pty's reads (`src/misc/pty.c`), tested by `test_pause` in `tests/test_pty.c`, and `tsm_vte_image_reply` delivered the helper's answer.
- **The host.** `image_host.c` started and killed the helper, `image_identity.c` found the session's user, `image_helper.c` applied the file rules and decoded PNG and zlib, and `render/image_budget.c` held the process's one budget, whose `make_room` trimmed the renderers' pictures before evicting libtsm's images.
- **The replay.** `tsm-replay`'s fixture host answered requests from a case's fixture files, which vtium's replay interface carries as "The fixture host".

## Where vtium departs from the C reference

- **The use clock is each store's own and moves only on stream events.** The C reference renewed every drawn image's use on each frame, on one clock for the whole process, so its eviction depended on which frames were drawn (D5).
- **Each console has its own budget,** where the C reference had one for the whole process across every screen (D5).
- **Scaled copies never evict a stored picture.** The C reference's scaled copy for a picture on screen could evict pictures in the scrollback (D5).
- **At equal z, a sixel and a kitty picture draw in the order they arrived,** with a newly arrived kitty picture slid beneath the first higher kitty id. The C reference drew every sixel above every kitty picture (D4); the goldens D4 names can differ.
- **Modes 80, 1070 and 8452 answer DECRQM with 0 in the minspec build,** where the C reference answered their fixed state (D10).
- **A placement's change marks the cells under it,** by ADR-1001's rule, where the C reference left each renderer to damage placement rectangles itself.
- **Rectangle operations cut cell-content pictures** as the erases and writes they are (D3); the C reference had no rectangle operations.
- **A width change rewraps the main screen and carries each anchor** (D8, ADR-1003); the C reference never reflowed.
- **A handler acts only on a string ended by its terminator,** vtium-parse's listed departure.
- **The cursor, the pointer and vtium's notices draw above every placement** (D4); ADR-103 gave them no place in its order.
- **Relative placements, Unicode placeholders and animation are supported targets** (ADR-1000), which the C reference answered with EINVAL; the kitty handler and kitty animation build them on this pipeline.

## Consequences

### Positive

- One store, one budget and one compositor path serve every protocol, so a fix or a speed-up in any of them serves all.
- A protocol's rules sit in one place, and the compiler stops any other code from branching on the protocol.
- A console's pictures follow from its own bytes and host events, whole or chunked, whatever was drawn and however its output interleaved with another console's.
- A program orders its pictures at equal z by the order it sends them, across protocols, except that a kitty picture slides beneath any kitty picture with a higher id, as kitty's own rule asks.
- No file or shared-memory object a program names is opened by the console's root process, and no PNG or zlib payload is decoded there.
- The minspec build carries no image code at all.

### Negative

- A sixel takes two to four times the bytes palette indices would.
- A file or compressed image pauses that console's output for a round trip to the helper, up to 2 seconds when the helper hangs.
- Several consoles can each hold a full budget of pictures, and frames in flight can keep deleted pictures alive past the budgets.
- A picture too large for the scaled-copy cache is scaled again on every frame it draws.
- The C reference's goldens that mix sixel and kitty at equal z judge a different order, so those cases become listed departures.
- Every handler, the compositor and the helper build against the store's interface, so changing it later touches all of them.

### Neutral

- iTerm2's inline images work through a thin compatibility front end that places each picture as a sixel, so the store, its rules and the compositor know only sixel and kitty.
- The replay interface already carries the image commands and the fixture host the C reference's goldens were made with, and vtium-replay gains them with `images`.
- The frame path review's remaining items (a buffer token for each display, drm2d's logic apart from its dumb buffers) are not this record's, and nothing here closes them off.
- The image helper's own record decides the helper's sandbox beyond ADR-1400 D2, and kitty animation's own record decides animation frames.

## Alternatives Considered

- **One budget for the whole process,** as the C reference had. Rejected (the maintainer, 2026-11-01): one console's output could evict another console's pictures, in an order that depends on how the two consoles' outputs interleave in time.
- **Scaled copies inside the image budget,** as the C reference's bbulk kept them. Rejected: a frame drawn or not drawn would then change what the store keeps, which the determinism rules forbid.
- **Sixel above kitty at equal z,** the C reference's order. Rejected (the maintainer, 2026-11-01): a program could not order a sixel and a kitty picture on purpose by sending them in that order.
- **Arrival order for every pair, kitty included.** Rejected (the maintainer, 2026-11-01): it departs from the kitty spec's id rule.
- **A new kitty picture on top, with the higher ids lifted above it.** Rejected (the maintainer, 2026-11-01): it moves pictures already on the screen, so a picture could jump in front of a sixel drawn after it.
- **A sort key of z, then id, then arrival.** Not taken: with a sixel between two kitty pictures whose ids run against their arrival, the three pairwise rules form a loop that no sort key satisfies; a stack the store keeps as placements arrive always has one answer.
- **Branching on the protocol in the store and the compositor,** as the C reference's code did. Rejected: the maintainer asked for one function to hold every protocol rule, and a private tag lets the compiler hold the line.
- **The image store inside vtium-term,** with vtium-image holding only the decoders. Not taken: the store, its rules and its budget are one unit that the handlers act on, and keeping them in vtium-image leaves vtium-term without image code in the minspec build and vtium-image free of vtium-term.
- **vtium-image depending on vtium-term,** with the binary wiring the two together. Not taken: vtium-term would then reach the store on every screen event through a hook object the binary installs, so every scroll and erase would pass through that hook in every build, minspec included, where an optional dependency leaves the minspec build with no image code at all.
- **Inflating zlib in the console,** which already holds an inflate for its built-in fonts (ADR-1200 D6). Not taken: the C reference sent zlib to its helper, and a fault in a decoder facing a program's bytes then costs that request and a helper restart, never the console.
- **Holding sixel pictures as palette indices.** Rejected (the maintainer, 2026-11-01): one pixel format keeps one composition path; the C reference's cost note measured the trade.
- **Waiting for a measurement of the C reference's A5 and A7 targets** before building the store. Rejected (the maintainer, 2026-11-01): vtium sets its own performance targets and measures them on vtium.
