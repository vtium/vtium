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
    said: "If iTerm2's image format (OSC 1337 File=) has no decision in ADR-1100 beyond \"a supported target\", add what it needs as R1 (e) or a short new record: it decodes through the same helper (PNG, and which other formats), its sizing rules (width/height in cells, px, %, auto; preserveAspectRatio), and the OSC string bound."
    via: the team lead's brief for the image records, 2026-11-01, carrying the maintainer's direction for this wave
  - standard: "iTerm2's Images documentation (iterm2.com/documentation-images.html), read 2026-11-01: ESC ] 1337 ; File = [arguments] : base64 file contents, ended by BEL or ST; the keys name, size, width, height, preserveAspectRatio and inline; width and height as N cells, Npx pixels, N% of the session's width or height, or auto, the picture's own size; preserveAspectRatio 0 to stretch and 1, the default, to fit without stretching; inline 1 to show the picture and 0, the default, to download it; MultipartFile, FilePart and FileEnd for transfers in parts; any image format macOS reads"
  - standard: "WezTerm's imgcat documentation (wezterm.org/imgcat.html), read 2026-11-01: WezTerm implements the iTerm2 inline image protocol, and adds doNotMoveCursor=1 as an extension of its own"
  - precedent: ADR-1100
  - precedent: ADR-1101
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "iTerm2 pictures take PNG, JPEG and a GIF's first frame, through png, zune-jpeg and gif, crates approved for the image helper only with what they pull in."
    via: the maintainer, 2026-11-01, in the steering session
    paraphrase: true
    covers: [formats]
  - operator: aaronsb
    said: "iTerm2: take the multipart form (MultipartFile, FilePart, FileEnd) too, with its own bound on the total size and a stated way a part sequence is abandoned, as iTerm2's docs describe; drop the departure."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
    covers: [no transfer in parts]
  - operator: aaronsb
    said: "An animated GIF that a program sends through iTerm2's OSC 1337 File= plays as an animation through the kitty pipeline. The image helper reads the GIF's frames and attributes: each frame's delay, its disposal method, and the loop count from the NETSCAPE2.0 application extension, 0 meaning forever. The frames are stored and played by ADR-1102's machinery: whole frames under the frame allowance, and the animation clock in the binary. Place them with the kitty pipeline."
    via: the maintainer, 2026-11-02, in the steering session, relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1103: iTerm2 pictures are sized by their keys and decoded in the image helper

## Summary

- **Decided:** vtium shows a picture a program sends as iTerm2's `OSC 1337 ; File=` when the string arrives complete, within the 1 MiB bound vtium-parse holds an OSC string to, and carries `inline=1`, and a larger picture sent in parts, as iTerm2's `MultipartFile`, `FilePart` and `FileEnd`, up to 16 MiB of decoded file. The iTerm2 compatibility handler (ADR-1100) decodes the base64 in the console and hands the whole file to the image helper (ADR-1101), which recognizes PNG, JPEG and GIF by their first bytes and decodes them through the pure-Rust crates `png`, `zune-jpeg` and `gif`; any other file is dropped. An animated GIF keeps every frame and plays as an animation through kitty animation's frames and clock (ADR-1102), with the delays, disposal methods and loop count the GIF gives (the maintainer, 2026-11-02). `width` and `height` size the picture in cells, in pixels, in percent of the screen, or at its own size, and `preserveAspectRatio` keeps its shape unless it is 0. The handler places the picture at the cursor under sixel's rules, scrolls as a sixel does, and moves the cursor as a sixel does. `doNotMoveCursor` and every other OSC 1337 command are ignored, and nothing is ever answered.
- **Trades away:** A transfer in parts holds a buffer of up to 16 MiB of decoded file between strings, under the console's image budget. A picture larger than 16 MiB of decoded file is dropped. An animated GIF holds every frame as a whole picture, width × height × 4 bytes a frame, under the console's frame allowance, and one whose frames do not fit shows its first frame, still. No program can stop or step an animated GIF, since an iTerm2 picture has no id. A JPEG's orientation tag is not read, so a photo a camera stored on its side shows on its side. Two more decoders run in the helper, each facing a program's bytes.
- **One-way?** No. The handler is a thin front end; its keys and formats can grow later without touching the store.
- **Probes:**
  - *Not confident (formats):* iTerm2 shows any picture its operating system can read, and `imgcat` is used most on photos, which are JPEGs. The record has the helper decode JPEG and a GIF's first frame beside PNG, through two more pure-Rust crates, `zune-jpeg` and `gif`, linked into the helper only. Do you want those two formats now, or PNG alone, with photos dropped until later?
  - *Not confident (where an animated GIF lives):* you asked that animated GIFs be placed with the kitty pipeline. The record reads that as kitty animation's frame store, frame allowance and clock playing the GIF, while every iTerm2 picture, still or animated, keeps the placement rules it has today: text written over it and erases remove the part they cover, it scrolls with the text, and the cursor ends below it, as `imgcat` expects. The other reading stores every iTerm2 picture as a kitty picture: text and erases would then no longer remove it, only clearing the screen would, it could be pushed out of memory while still on the screen, and a program's kitty command to delete every picture would remove it too. Storing only animated GIFs as kitty pictures would give one protocol two sets of rules, chosen by what the file holds. The record leans to the first reading (D4). Is that what you meant?
  - *Confident (no transfer in parts):* a picture whose file is larger than about 768 KiB does not show, rather than vtium taking iTerm2's newer `MultipartFile` form, which no other terminal vtium is measured against documents. Yes or no?
- **Inversion:** Between "an image protocol of its own" and "ignore OSC 1337 entirely". The answer, as ADR-1100 framed it, is a compatibility front end that maps each picture onto sixel's rules and the kitty path's decoding, so programs that send it work and nothing else in vtium knows it exists.

> Edited 2026-11-02, the maintainer's decision (2026-11-02): an animated GIF plays as an animation through kitty animation's frames and clock. The helper decodes every frame of a GIF and composes the disposal methods into whole frames (D2); the handler stores them under the frame allowance, with gaps from the delays and loops from the loop count (D2); the placement of such a picture is the probe *where an animated GIF lives*, with the record's lean in D4. The Summary, Trades away, the departures, the Consequences, the Alternatives and D5 follow. The record decoded a GIF's first frame only, and listed that as a departure from iTerm2.

> Edited 2026-11-02, third round of fixes: the bound on a transfer is 16 MiB of decoded file throughout, where D1 said "16 MiB after base64"; a part may hold any number of base64 characters (D1); a transfer is also abandoned when the iTerm2 switch goes off and when vtium-term drops an OSC 1337 string while it is open, where D1 left a dropped part to fail at decoding; and the abandonment rules are stated as vtium's own (D1); the helper premultiplies each decoded picture (D2); the Context no longer quotes ADR-1100 D6's earlier wording on the bound.

> Edited 2026-11-02, the maintainer's answer (`considered`): vtium takes iTerm2's transfer in parts, `MultipartFile`, `FilePart` and `FileEnd`, with a bound of its own on the whole file (D1); the departure for a file too large for one sequence is gone. D1 said vtium kept the 1 MiB bound of one sequence and took no transfer in parts.

> Edited 2026-11-01, after the fresh review and the maintainer's answers (`considered`): the 1 MiB bound is recorded as iTerm2's own, so only the transfer in parts departs; `type`, which iTerm2's documentation does not list, is no longer named. The earlier text is in git at 991986ba.

## Context

ADR-1100 carries iTerm2's inline images as a compatibility handler, never as a protocol of equal standing to sixel and kitty (the maintainer, 2026-11-01): it decodes each picture on the kitty path, through the image helper, and places it under the sixel tag, since iTerm2 gives a program no ids and no delete command to manage a picture afterwards. ADR-1100 D6 bounds a picture sent in one string by the 1 MiB OSC bound and routes the transfer in parts to this handler. ADR-1100 leaves open which formats the helper decodes for iTerm2, how the keys size a picture, and where the cursor goes. The C reference had no iTerm2 handler, so there are no goldens; vtium's own replay cases judge the handler, provisional until the maintainer reviews them. iTerm2's documentation and WezTerm's are the specification read here; the protocol is one escape sequence, so a small script in the tree can send it for a known picture in a review on a VT.

## Decision

### D1: what vtium takes

- **The string.** The router hands the handler a complete `OSC 1337 ; File=` string (ADR-1100 D6). The keys run from `File=` to the first `:`, separated by `;`, each a name, `=` and a value; the base64 file follows the `:`. A string with no `:` is dropped. `name` and `size` are read and ignored, since vtium saves no file and draws no progress, and so is any key iTerm2's documentation does not list. WezTerm's `doNotMoveCursor` is ignored too, since vtium adopts no one terminal's extension (ADR-1100 D3).
- **Inline only.** A string without `inline=1` is dropped, since iTerm2 would download its file, and vtium saves nothing.
- **The bound.** vtium-parse holds an OSC string to 1 MiB, the same 1,048,576 bytes iTerm2 holds one of its sequences to, so an iTerm2 picture's file can be about 768 KiB at most after base64 grows it by a third; a longer string is dropped whole, as every OSC string past the bound is, and as iTerm2 refuses it. A larger picture comes in parts (below), which settles the question ADR-1100 D6 left to this handler.
- **A transfer in parts** (the maintainer, 2026-11-02), as iTerm2's documentation describes it. `OSC 1337 ; MultipartFile=` with the same keys as `File=` and no `:` starts a transfer; each `OSC 1337 ; FilePart=` carries a piece of the base64 file, which the handler decodes into the transfer's buffer as it arrives; and `OSC 1337 ; FileEnd` ends it, after which the whole file is decoded and placed as a `File=` picture is (D2 to D4). A part need not hold a multiple of 4 base64 characters, since iTerm2's documentation does not ask for one, so the handler carries up to 3 characters left over at the end of one part into the next. The buffer is reserved under the console's image budget as it grows, as a kitty PNG payload's is (ADR-1100 D5). Output between the parts, text included, leaves the transfer as it is.
- **How a transfer is abandoned.** iTerm2's documentation describes only the three sequences and says nothing of a transfer that never ends, so these rules are vtium's own. The handler drops a transfer in progress and gives its reservation back on a new `MultipartFile=` or `File=`, which then goes on as a string of its own; on RIS, through the handler's reset (ADR-1100 D6); when the file would pass `ITERM2_FILE_MAX`, 16 MiB of decoded file, or the parts would pass `ITERM2_PARTS_MAX`, 65,536, both named constants of vtium-image's bounds table; when a part's base64 does not decode; when the budget refuses the buffer's growth; and when the router's iTerm2 switch goes off (When it runs, below), because sixel is withdrawn, `image-storage-limit` is set to 0 or the console has no image helper any more, as ADR-1100 D6 gives a kitty upload's reservation back when kitty graphics is withdrawn. The switch going off is a host event, which falls between two input calls (ADR-1100 D5), so the transfer ends at a known point in the stream. The handler also drops the transfer when vtium-term drops an OSC 1337 string while it is open: one past 1 MiB, which vtium-parse hands over marked truncated, or one cut by CAN, SUB or another control. The router sees such a string's first bytes, which vtium-parse keeps, and tells the handler. A part lost that way leaves a hole in the file, and the handler does not count on the decoder failing on it, since a lenient decoder may draw the damaged file. A `FilePart=` or `FileEnd` with no transfer in progress is ignored.
- **When it runs.** vtium-term's router has a switch for iTerm2 strings beside sixel's and kitty's. The binary turns it on while sixel is offered (ADR-1100 D10) and an image helper is available, since the handler places its pictures under sixel's rules and decodes them only through the helper. It has no configuration key of its own.
- **No reply.** iTerm2's protocol answers nothing, so the handler writes no reply, and a picture that fails at any step is dropped without a word to the program. The reason goes to vtium's log at debug level.

### D2: decoding

- **In the console,** the handler decodes the base64 into a buffer reserved under the console's image budget, as a kitty PNG payload is (ADR-1100 D5, D7). Base64 that does not decode drops the picture.
- **In the helper,** the handler sends the whole file as inline bytes with the request's any-image flag (ADR-1101 D7), and the barrier holds the stream until the helper answers (ADR-1100 D7). The helper looks at the file's first bytes: a PNG signature decodes through `png` (ADR-1101 D6); a JPEG's start-of-image marker through `zune-jpeg`, to 8-bit RGB with full opacity; a GIF's `GIF87a` or `GIF89a` through `gif`, every frame of it, each composed into a whole picture of the GIF's own size from a transparent canvas, keeping its transparent pixels (ADR-1101 D6, ADR-1102 D1). The handler asks for a GIF's frames by setting the request's frames flag, with `max_frames` and `max_frame_bytes` taken from ADR-1102's bounds (ADR-1101 D7); a GIF of one frame comes back as one picture, as a PNG does. The helper premultiplies each decoded picture into the store's format, as it does every picture it returns (ADR-1101 D6, D7), so a JPEG, which is opaque, is unchanged, and a GIF's transparent pixels become zero pixels. Each decoder checks the picture's size against `max-graphic-size` and the request's `max_bytes` before it allocates a pixel, and a file it cannot decode, or of any other format, is `CORRUPT`. All three are pure Rust and linked into the helper only. Each takes a fuzz target, as ADR-1101 D6 asks of every decoder.
- **The answer.** A decoded picture is stored at its own size, under the sixel tag, charged to the budget as any image is; a picture the budget refuses is dropped, as a sixel is (ADR-1100 D5). Any reply but `OK` drops it.
- **An animated GIF.** A reply with more than one frame is stored as an image with frames (ADR-1102 D1). Its first frame is the image's own pixels, charged to the image budget as any picture is, and the frames past it are reserved under the console's frame allowance before they are stored, by ADR-1102 D1's rules; when that reservation fails, the handler stores the first frame alone, as a still picture. The handler gives each frame its gap from the GIF's delay, 10 ms for each hundredth of a second, and shows a frame with a delay of 0 or 1 hundredth for 100 ms, as web browsers show such frames. It sets the loops from the loop count: a GIF with no NETSCAPE2.0 extension plays its frames once, a count of 0 loops forever, and a count L above 0 plays them L + 1 times in all. Programs that write GIFs disagree on whether L counts every play or the repeats after the first; vtium reads it as the repeats, and a replay case pins that reading. The handler stores the animation running from its first frame, so the picture plays without any command from the program.

### D3: sizing

The handler works out the size the picture draws at, in pixels at the console's cell size, from `width`, `height` and `preserveAspectRatio`:

- **Each of `width` and `height`** is a whole number of cells (`20`), a whole number of pixels (`300px`), a whole percentage of the screen's width or height in pixels (`50%`), or `auto`, the picture's own size. A value left out, a value of 0, and a value vtium cannot read each count as `auto`.
- **`preserveAspectRatio`** is 1 unless the string gives `0`. With it at 1, a size given on one side sets the other in the picture's own proportion, and sizes given on both sides make a box the picture is scaled into, as large as fits with its shape kept. With it at 0, each side takes the size given, or the picture's own where it is `auto`, and the picture is stretched to fit.
- **The box.** The placement covers the cells the drawn size reaches from the cursor's cell, its width and height in pixels each rounded up to whole cells, and the picture draws into the rectangle of that size at the box's top left (ADR-1100 D2). A box past 10,000 cells on either side, ADR-1100's bound, drops the picture. A box wider than the columns from the cursor to the right edge is cut at the right edge when it is drawn, as a sixel is.
- For example, at a 10 by 20 pixel cell, a 640 by 480 picture sent with `width=32` draws 320 by 240 pixels, over 32 columns and 12 rows. Sent with `width=100%;height=6`, on an 80-column screen, it draws inside 800 by 120 pixels as 160 by 120, over 16 columns and 6 rows; with `preserveAspectRatio=0` as well, it draws 800 by 120 over 80 columns and 6 rows.

### D4: placing, and the cursor

- **Where.** The picture's top left is the cursor's cell, under the sixel tag, so every rule in ADR-1100 D3's sixel column applies to it: text written over it and erases cut it, a region scroll splits it, it draws above text at z 0, and it is never evicted while on the screen's own lines.
- **An animated GIF** takes the same placement, under the same sixel tag, as every other iTerm2 picture; only its storage and its playback are kitty animation's (the record's lean on the probe *where an animated GIF lives*). Frames belong to an image and never to a tag, so the rules function needs no new column: text written over the GIF, an erase and a region scroll cut and move its placement as they do a sixel's, and every piece of it shows the frame the clock chose (ADR-1102 D3). Where a picture under the sixel tag covers the older pictures beneath it cell by cell (ADR-1100 D2), an animated GIF covers a cell fully only when every one of its frames covers that cell fully, and leaves a cell empty only when every frame leaves it empty. Its frames past the first can be freed for the frame allowance even while it shows, and it then shows its first frame, still (ADR-1102 D1); its first frame, like any sixel-tag picture, is never evicted while on the screen's own lines.

  > Added 2026-11-02: the maintainer decided that an animated GIF plays through kitty animation's frames and clock, and said to place such pictures with the kitty pipeline. Which part of the kitty pipeline places them is the probe *where an animated GIF lives*; this bullet gives the record's lean.
- **Scrolling.** A box taller than the rows from the cursor to the bottom margin scrolls the screen as a sixel does with sixel scrolling on: the scroll region first, then an index for what the region cannot hold. Mode 80, DECSDM, which turns sixel scrolling off, is a sixel mode and does not apply, so an iTerm2 picture always scrolls.
- **The cursor** ends where a sixel's does with mode 8452 reset: on the row that holds the picture's last row of pixels, at the picture's left column. Mode 8452, which moves the cursor to the right of a sixel, does not apply. A program such as `imgcat` that writes a new line after the picture then continues on the row below it.

### D5: verification

- **Replay cases,** vtium's own and provisional until the maintainer reviews them: a PNG, a JPEG and a GIF each placed; `width` and `height` in cells, pixels, percent and `auto`, with `preserveAspectRatio` at 1 and at 0; a string without `inline=1`, which is dropped; a string past the bound, dropped whole; a picture in parts, and each way a transfer is abandoned; a file the helper cannot decode, dropped; the cursor after a picture, and the scroll for one taller than the rows below the cursor; text written over a picture, which cuts it; an animated GIF stored with its frames, the gaps its delays give and its run state running; a GIF for each disposal method, 1, 2 and 3, whose stored frames are the whole frames ADR-1102 D1's table gives; a GIF with no loop extension, one with a count of 0 and one with a count of 2, stored with the loops each gives; a GIF whose frames pass the frame allowance, and one with more than `FRAMES_MAX` frames, each stored as its first frame, still; text written over an animated GIF, which cuts it as it cuts a still picture.
- **Unit tests in the helper** for each disposal method and each bound on a GIF's frames, each seen to fail with its check planted out.
- **Unit tests** for the sizing rules of D3, each example above among them.
- **The fuzz target** of the image pipeline takes the handler's input, and the helper's fuzz targets take each decoder.
- **On a VT,** a script in the tree sends the sequence for a fixture picture of each format, as `wezterm imgcat` would, for the maintainer's review.

### Passages this record changes

- **ADR-1100 D9** says that a frame entry for an animated kitty image carries its frames; it covers an animated iTerm2 picture too, and its wording changes in an edit of ADR-1100 once the maintainer answers the probe *where an animated GIF lives*.
- **ADR-1000's `osc.1337` row** says OSC 1337 is ignored "apart from File="; it gains `MultipartFile`, `FilePart` and `FileEnd`, in its owner's change.

## Where vtium departs

The C reference had no iTerm2 handler, so nothing here departs from it. From iTerm2:

- **Only PNG, JPEG and GIF show,** where iTerm2 shows any format its operating system reads, PDF among them.
- **An animated GIF whose frames do not fit the console's frame allowance, or that has more than `FRAMES_MAX` frames, shows its first frame, still,** where iTerm2 plays it.
- **A JPEG's orientation tag is not read.**
- **A picture larger than 16 MiB of decoded file does not show,** a bound of vtium's own on a transfer in parts, where iTerm2's documentation gives none.
- **A string without `inline=1` is dropped,** where iTerm2 saves its file to the Downloads folder.

## Consequences

### Positive

- `imgcat`, `wezterm imgcat` and the many programs that send iTerm2's sequence show their pictures on vtium, with no protocol of their own in the store or the compositor.
- A program's file never reaches a decoder in the console's root process.
- Animated GIFs that `imgcat` sends play, as they do in iTerm2.

### Negative

- Pictures larger than 16 MiB of decoded file do not show.
- An animated GIF holds every frame uncompressed, so a long one can fill the frame allowance and push out the frames of other animations.
- The helper carries two more decoders, each a surface a hostile file can reach, held by ADR-1101's sandbox and filters.
- A program learns nothing when its picture is dropped.

### Neutral

- A transfer in parts keeps state between strings in the handler, which RIS clears.

## Alternatives Considered

- **PNG only,** the format kitty's `f=100` takes. Rejected (the maintainer, 2026-11-01): most pictures `imgcat` shows are photos, which are JPEGs.
- **A GIF's first frame only,** the record's first choice (the maintainer, 2026-11-01). Replaced (the maintainer, 2026-11-02): an animated GIF plays through kitty animation's frames and clock (ADR-1102), with the helper composing its disposal methods into whole frames.
- **Every iTerm2 picture stored as a kitty picture.** Not taken while the probe *where an animated GIF lives* is open: text and erases would stop removing iTerm2 pictures, and a kitty delete would reach pictures that a program sent through another protocol.
- **Only animated GIFs stored as kitty pictures.** Not taken: one protocol would follow two sets of rules, chosen by what the file holds, and a GIF that loses its frames to the allowance would still follow kitty's rules as a still picture.
- **No transfer in parts,** keeping one sequence's 1 MiB as the bound. Rejected (the maintainer, 2026-11-02): a photo larger than about 768 KiB would never show.
- **Honoring WezTerm's `doNotMoveCursor`.** Rejected: vtium adopts no one terminal's extension (the maintainer, 2026-11-01, ADR-1100 D3).
- **A configuration key of its own.** Not taken: the handler follows sixel's switch, whose rules its pictures follow, and needs the helper in any case.
