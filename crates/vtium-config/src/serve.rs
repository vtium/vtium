//! The helper's serve mode (ADR-1301 D2): `vtium-config serve`, which
//! the console starts on its first need for glyphs and which generates
//! them from a frit at run time. It sets the parser's sandbox on itself
//! (no-new-privileges, the memory and file limits, no children, refused as
//! root), takes the frit's name and text from the console, so a frit made
//! on the fly needs no file, reads the frit's fonts as the account the
//! console started it as, and answers in the strike chunk format the
//! console already reads.
//!
//! The console sends four requests (the operator's fill model,
//! 2026-10-28). An ask names codepoints and clusters at a cell, those on
//! the screen or newly needed, and is answered whole, ahead of any fill:
//! each cluster shaped with HarfRust from the first outline source in the
//! frit's order that covers its first scalar value other than U+200D, and
//! drawn whole into the frame's cluster map (ADR-1200 D21, ADR-1301 D1); a
//! cluster no source shapes as [`crate::shaping::kind`] requires is left
//! out, and the console draws it its own way. A fill fills a
//! cell's coverage in order: the codepoints it names (the screen's), then
//! printable Latin, punctuation, arrows, box drawing and geometric shapes
//! ([`LATIN_AND_BOX`]), then the rest of
//! the frit's coverage, skipping what the console holds, one [`BATCH`] at
//! a time; between batches the helper takes any waiting request, so a
//! request waits at most one batch. A stop ends a cell's fill, as when the
//! console's store is full or no display draws at the cell, and is answered
//! as it is taken, so the console knows each fill frame before the answer
//! is the stopped fill's, even when the cell is filled again at once. A
//! cell query asks for the cell of a frit that states none, which the
//! frit's first source sets (ADR-1200 D17), before the console's first
//! frame and, for an outline frit, at each zoom; it names the height an
//! outline font would be drawn at, or 0. A bitmap first source answers
//! its own cell, which the console takes at whole multiples, once every
//! later bitmap source's cell fits in it (the operator, 2026-10-28). An
//! outline first source answers the cell the W rule gives it at the height
//! named, which the console takes as it is (the maintainer, 2026-11-01);
//! with no height it has none. Otherwise the helper says why the frit has
//! no cell of its own ([`Engine::own_cell`]). The helper
//! keeps what it sent only for the request that sent it, so it sends a
//! glyph once within one ask or one fill; the console's store is the one
//! record of what the console holds (ADR-1200 D18). The helper exits once
//! it has no request and no fill for [`HELPER_IDLE_MS`], or when the
//! console closes its end and its work is done.
//!
//! A fifth request, a sources query, is vtium-config's own and the console
//! never sends it: it asks which of a frit's sources this machine lacks or
//! refuses, which `vtium config fonts` and the TUI's font list show
//! (ADR-1201 D2, D3), so they learn it without parsing a font in their own
//! process. It is answered at once, like a cell query.
//!
//! The console sends one more request first to each helper it starts: the
//! active fonts, which the helper's sources read alone from then on
//! ([`ActiveAsk`]). It names the paths the batch lists and the account
//! whose files pass the trust rule beside root's. vtium-config's own
//! previews send none, so they read any font a frit names, in
//! vtium-config's window, through the same sandbox.
//!
//! A description is vtium-config's own as well ([`DescribeAsk`]). It names
//! one font file, which the helper reads through its gate and answers at
//! once with what the file holds: its faces, their names, sizes and
//! coverage. The Fonts tab, `fonts --files` and `apply` learn what a font
//! is this way, without parsing it in their own process.
//!
//! An ask that says how its glyphs were made is vtium-config's own too, and
//! the console never sends it. It is answered as an ask is, and before its
//! done frame the helper sends how it made each superscript or subscript
//! glyph it drew ([`Made`]), which the font preview's caption names
//! (ADR-1202 D14).
//!
//! ```text
//! request  magic "VTVSREQA", kind u32 (1 ask, 2 fill, 3 stop, 4 cell
//!          query, 5 sources query, 6 ask that takes parts, 7 ask that
//!          says how its glyphs were made, 8 coverage query; 9, the pointer
//!          ask, 10, the active fonts, and 11, a description, have layouts
//!          of their own, below), frit name length u32, the name (ADR-1200 D1's
//!          characters), frit text length u32, the text (UTF-8), cell width
//!          u32 (0 for a query), cell height u32 (for a sources or
//!          coverage query 0, for a cell query
//!          the height an outline font is drawn at, or 0), the rows line
//!          spacing adds above and below the cell's text band u32 and u32
//!          (both 0 for a query, and together less than the cell's
//!          height), style u32 (0
//!          regular, 1 bold, 2 italic, 3 bold italic; an ask's alone may
//!          name one other than regular), script u32 (0 normal, 1
//!          superscript, 2 subscript; an ask's alone may name one other
//!          than normal), cluster width u32 (0 double, 1 single, the
//!          console's `cluster-width`, ADR-1006 D8; an ask's alone may
//!          name single), line size u32 (0 normal, 1 double width, 2
//!          double height, for a line of double size; an ask's alone may
//!          name one other than normal, in the normal script, and only
//!          where the doubled cell keeps inside CELL_SIDE_MAX),
//!          capabilities
//!          length u32, the capabilities (reserved for the
//!          display's, ADR-1301 D1 and D3: always 0 bytes, and refused
//!          otherwise), codepoint count u32, each codepoint u32 (an ask's
//!          wanted, a fill's first, a coverage query's letters; none for
//!          a cell or sources query), cluster count
//!          u32, each cluster's length u32 and scalar values u32 (an ask's
//!          alone), held range
//!          count u32, each range's first and last codepoint u32 (what the
//!          console holds at the cell, for a fill; none for a cell query),
//!          console font flag u32 (0 none, 1 one):
//!          width u32, height u32, glyph count u32, data length u32, the
//!          data, map count u32, each (codepoint u16, glyph u16)
//! answer   frames: kind u32, length u32, then the payload; kind 1 an
//!          ask's glyphs and 4 a fill's, each the rows line spacing adds
//!          above and below the cell u32 and u32, as the request named
//!          them, then a vitrified set of one cell's
//!          glyphs, its strikes in the style and the script the request
//!          named, an ask's
//!          clusters in its cluster map; kind 2 the oldest ask answered (no payload); kind 5 a
//!          cell's fill done (its width u32, height u32, and rows added
//!          above u32 and below u32); kind 6 a
//!          bitmap frit's own cell (its width u32 and height u32); kind 10
//!          an outline frit's cell at the height asked (its width u32 and
//!          height u32); kind 7 no own cell (UTF-8 reason); kind 8 a stop
//!          taken (its cell's width u32, height u32, and rows added above
//!          u32 and below u32), one for each
//!          stop; kind 9 a warning the frit's text gave as it was read,
//!          once for each frit read (UTF-8); kind 11 a sources query's
//!          answer, one line for each source left out, `missing` or
//!          `refused`, a tab, the source's line in the frit, a tab, its
//!          spelling, a tab and why (UTF-8, empty when none is left out);
//!          kind 12 a sources query's answer for a frit the helper draws
//!          nothing from (UTF-8 reason); kind 13 how an ask of kind 7
//!          made its superscript and subscript glyphs, once before its
//!          done frame ([`Made::encode_all`]); kind 14 the parts of the
//!          helper's sandbox in force, once before its first answer
//!          ([`FRAME_SANDBOX`], a u32 of bits and reasons); kind 15 a coverage
//!          query's answer, the count u32 and each codepoint u32 of the
//!          letters asked that the frit covers, then the family of an
//!          installed font that covers the most of the rest (UTF-8, empty
//!          when none does); kind 16 a pointer picture and 17 a pointer
//!          shape the helper draws nothing for, both answers to a pointer
//!          ask (below); kind 18 a font file's description and 19 no
//!          description, the answers to a description (below); kind 3 a
//!          failure (UTF-8
//!          reason), the helper's own, as when it fails as it starts
//! pointer  magic "VTVSREQA", kind u32 9, file kind u32 (1 SVG, 2 a cursor
//!   ask    theme's Xcursor file), shape u32 (its place in vtium-term's
//!          PointerShape::ALL), pointer-size u32 (0 unset, else 8 to 64; 0
//!          for SVG), height count u32 (1 to 16 for SVG, 0 for Xcursor),
//!          each cell height u32 (1 to CELL_SIDE_MAX), file length u32 (at
//!          most POINTER_SVG_BYTES_MAX for SVG, XCURSOR_FILE_MAX for
//!          Xcursor), the file's bytes
//! pointer  kind 16, one for each picture drawn: shape u32, size u32 (the
//!  answer  cell height asked for SVG, the nominal size for Xcursor), width
//!          u32, height u32, hotspot x u32 and y u32, then width × height
//!          premultiplied RGBA pixels; or kind 17, once: shape u32, why
//!          u32 (0 the file has no picture of the shape, 1 the helper
//!          cannot draw it), then the reason (UTF-8, empty for 0); then
//!          kind 2, done, as every ask ends
//! active   magic "VTVSREQA", kind u32 10, the account u32 whose files
//!  fonts   pass the trust rule beside root's (0, or the helper's own),
//!          path count u32, each path's length u32 (1 to ACTIVE_PATH_MAX)
//!          and the path (absolute, UTF-8, no NUL), the paths' lengths
//!          together at most ACTIVE_BYTES_MAX; no answer
//! describe magic "VTVSREQA", kind u32 11, the font file's path length
//!          u32 (1 to ACTIVE_PATH_MAX) and the path (absolute, UTF-8, no
//!          NUL)
//! describe kind 18, once: the file's description
//!  answer  ([`crate::describe::Description::encode`]); or kind 19, once:
//!          why there is none (UTF-8)
//! ```
//!
//! The console reads each pointer file itself, under the ownership check a
//! frit passes, and sends its bytes in a pointer ask, so the helper opens
//! no pointer file. A pointer ask is answered in its turn among the asks:
//! for an SVG file, the shape's symbol drawn a cell tall at each height
//! asked ([`crate::pointer_svg`]); for a cursor theme's file, every size
//! the reader keeps, or the size nearest `pointer-size`
//! ([`crate::xcursor`]). The console checks each picture before it trusts
//! it: its shape and size among those asked, its sides from 1 to
//! [`POINTER_SIDE_MAX`], its pixels exactly that many, its hotspot inside
//! it, and no color above its alpha.
//!
//! References:
//! - ADR-1309 D5: the pointer ask, its answer frames and the console's checks of them.
//!
//! Little-endian throughout. The console checks every frame before it
//! trusts it: the length against [`FRAME_BYTES_MAX`] before it allocates,
//! a set through vtium-font's reader, every strike at the cell and in the
//! style asked for, an ask's glyphs and clusters among those it asked for,
//! and a fill's among those it does not hold, with no cluster.
//!
//! An ask names a style, regular, bold, italic or bold italic (ADR-1200
//! D21), and its glyphs come in that style. Each codepoint is drawn from
//! the first source in the frit's order that covers it and was given that
//! style (`style=`, ADR-1200 D1). Where no such source covers it, the
//! helper draws the regular glyph and synthesises the style (ADR-1302 D4):
//! an outline glyph slanted by 0.2 about the baseline and emboldened
//! before it is rasterised, and a bitmap glyph widened by a pixel and
//! sheared by 0.2 a pixel row at a time at the font's own size, the
//! widening first. A color glyph is drawn as it is. [`steps`] says what
//! each style is drawn from, in order. A fill, a stop and the queries are
//! regular: the fill fills only the regular glyphs, and the other styles
//! come by ask.
//!
//! An ask also names a script, normal, superscript or subscript (ADR-1202
//! D11), and its glyphs come in strikes of that script, each made small by
//! the source that draws the character's normal glyph (ADR-1202 D15): the
//! same order of steps and sources picks it. The helper never draws a
//! script glyph from another source than that. An outline source's glyph
//! is rasterised at two thirds of the size its normal glyph is drawn at,
//! never scaled from a full-size raster, and stands on the script baseline
//! ([`script_baseline`], ADR-1202 D2). A color glyph is scaled once from
//! its source to two thirds of its cells and placed against the cell's top
//! or bottom (ADR-1202 D5). A bitmap source's glyph is drawn upright and
//! unstyled in the text band and made small by vtium-font's script rule
//! ([`vtium_font::Glyph::scripted`]), each pixel fully on or off, and then
//! takes its bold and italic (ADR-1202 D4, D7). A script glyph's ink stays inside its
//! row (ADR-1202 D8). The fill and the stops draw normal glyphs alone.
//!
//! The kernel's font reaches a frit's `console:` source only as the bytes
//! the console read from its own VT at start and handed over
//! (vtium/vtium#145): the helper opens no VT, and every `console:VT` takes that font.
//!
//! The glyphs are drawn at the cell a request names, the frit's, and
//! never another (#145): an outline font at that cell's height and width,
//! and a bitmap font scaled by the largest whole factor that fits, centred
//! across the cell. The first source's glyphs stand on the cell's bottom
//! row, as `create` places one, and a later source's glyphs stand on the
//! first source's baseline (ADR-1200 D17; see `glyph_top`). A bitmap font
//! whose cell does not fit gives nothing, and the console draws those
//! characters in the built-in font.
//!
//! Where the request names rows that line spacing adds (ADR-1302 D13),
//! the helper draws each glyph in the cell's text band, the cell less
//! those rows, as above, and then places it in the whole cell as the
//! `fit=` of the frit's set that gives it says, or its default
//! ([`Fit::default_for`]): a `fill` glyph stretches to the whole cell, a
//! bitmap glyph by repeating its top and bottom rows and an outline glyph
//! by scaling vertically, and a `natural` or `room` glyph keeps its size
//! with blank rows above and below. A color glyph always keeps its size.
//! An outline glyph's ink may reach past the text band into the added
//! rows and then a row of the whole cell (ADR-1306 D8): its ink in the
//! added rows joins its own pixels, and the rest stays in its ink box
//! ([`spacing::place_rows`]). The strike's baseline and lines move down by
//! the rows added above.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use vtium_font::spacing::Band;
#[cfg(doc)]
use vtium_font::spacing::{self, Fit};
use vtium_font::{Cell, Doubled, Variant};
use vtium_platform::vt::font::ConsoleFont;
#[cfg(doc)]
use vtium_render::POINTER_SIDE_MAX;

pub use crate::made::Made;
pub use crate::seccomp::Missing;
use crate::sources::Gate;

mod bitmap;
mod engine;
mod pointer;
mod sandbox;
mod wire;

use engine::REGULAR;
pub use engine::{
    DrawsNothing, Engine, Found, LeftOut, MadeSet, OwnCell, Paths, Step, default_fit,
    outline_version, steps,
};
pub use sandbox::{NoDomain, SANDBOX_OPTION, SHED_VAR, command, no_domain};
pub use wire::{
    ACTIVE, ACTIVE_BYTES_MAX, ACTIVE_PATH_MAX, ASK, ASK_MADE, ASK_PARTS, ActiveAsk, BATCH,
    COVERAGE, DESCRIBE, DescribeAsk, FILL, FIRST_MAX, FRAME_BYTES_MAX, FRAME_COVERAGE,
    FRAME_DESCRIPTION, FRAME_DONE, FRAME_DRAWS_NOTHING, FRAME_FAILED, FRAME_FILL, FRAME_FILLED,
    FRAME_GLYPHS, FRAME_MADE, FRAME_NO_DESCRIPTION, FRAME_NO_OWN_CELL, FRAME_NO_POINTER,
    FRAME_OUTLINE_CELL, FRAME_OWN_CELL, FRAME_POINTER, FRAME_SANDBOX, FRAME_SOURCES, FRAME_STOPPED,
    FRAME_WARNING, FritText, HELD_RANGES_MAX, LATIN_AND_BOX, LEAVE_OUT, NAME_BYTES_MAX,
    NO_POINTER_MISSING, NO_POINTER_REFUSED, OTHER_BUILD, OWN_CELL, POINTER, POINTER_HEIGHTS_MAX,
    POINTER_SVG, POINTER_XCURSOR, PointerAsk, PointerFile, PointerPicture, REASON_BYTES_MAX,
    REQUEST_MAGIC, Request, SANDBOX_DOMAIN, SANDBOX_DOMAIN_SHIFT, SANDBOX_FALLBACK,
    SANDBOX_SECCOMP_SHIFT, SANDBOX_SOCKETS, SOURCES, STOP, Sandboxed, ServeRequest,
    cluster_width_code, doubled_code, leave_out_request, read_request, script_baseline,
    script_code, script_of, shape_code, shape_of, style_code, style_of, write_frame,
};
use wire::{FRAME_SET, cell_payload, failed, glyph_payload, len32, put32, reason, spaced_payload};

/// The command that makes the binary the helper.
pub const SERVE_ARG: &str = "serve";
/// How long the helper waits for a request before it exits (the operator,
/// 2026-10-28; ADR-1301 D2).
pub const HELPER_IDLE_MS: u64 = 60_000;
/// The helper's CPU time over its life, in seconds: the kernel ends it
/// past this, and the console's next request starts another, a bounded
/// number of times a boot (the console's `RESTARTS_MAX`).
pub const HELPER_SECONDS: u64 = 300;

/// A coverage query's answer ([`FRAME_COVERAGE`]): which of `letters` the
/// frit's sorted `coverage` holds, and the family of an installed font
/// under `dirs` that `gate` lets through that covers the most of the rest.
fn coverage_payload(coverage: &[u32], letters: &[u32], dirs: &[PathBuf], gate: &Gate) -> Vec<u8> {
    let (covered, rest): (Vec<u32>, Vec<u32>) = letters
        .iter()
        .partition(|c| coverage.binary_search(c).is_ok());
    let rest: Vec<char> = rest.into_iter().filter_map(char::from_u32).collect();
    let font = crate::sources::font_covering_in(&rest, dirs, gate).unwrap_or_default();
    let mut out = Vec::new();
    put32(&mut out, len32(covered.len()));
    for c in covered {
        put32(&mut out, c);
    }
    out.extend_from_slice(font.as_bytes());
    out
}

/// One bit for each Unicode codepoint.
struct Bits(Vec<u64>);

impl Bits {
    fn new() -> Bits {
        Bits(vec![0; 0x11_0000 / 64])
    }

    fn has(&self, c: u32) -> bool {
        self.0
            .get((c / 64) as usize)
            .is_some_and(|w| w & (1 << (c % 64)) != 0)
    }

    fn set(&mut self, c: u32) {
        if let Some(w) = self.0.get_mut((c / 64) as usize) {
            *w |= 1 << (c % 64);
        }
    }
}

/// Draws `want` and `clusters` at `cell` in `variant` and writes them as
/// frames of `kind`, a set that passes `max` bytes, [`FRAME_BYTES_MAX`] but in a
/// test, cut in two. Returns whether any glyph went, or the engine's
/// failure; only a write that fails is the outer error.
fn send_glyphs(
    engine: &Engine,
    (cell, band, variant, doubled): (Cell, Band, Variant, Option<Doubled>),
    (want, clusters, parts): (&[u32], &[Vec<u32>], bool),
    kind: u32,
    (out, made): (&mut dyn Write, &mut BTreeMap<u32, Made>),
    max: usize,
) -> io::Result<Result<bool, String>> {
    // no more glyphs drawn at once than a frame holds at their largest, so
    // a cell whose glyphs each pass a set's chunk (ADR-1302 D6) never draws
    // a batch whose bytes would outgrow the helper's memory limit. A line of
    // double width draws its color glyphs and its clusters at double height
    // too, and keeps both sizes until it halves the tall ones, so each of
    // its glyphs counts at both sizes
    let drawn = doubled.and_then(|d| d.cell(cell)).unwrap_or(cell);
    let mut glyph_bytes = engine.glyph_bytes_max(drawn);
    if doubled == Some(Doubled::Width)
        && let Some(tall) = Doubled::Height.cell(cell)
    {
        glyph_bytes = glyph_bytes.saturating_add(engine.glyph_bytes_max(tall));
    }
    let at_once = (max / glyph_bytes.max(1)).max(1);
    let mut queue: VecDeque<(&[u32], &[Vec<u32>])> = VecDeque::new();
    if want.len().saturating_add(clusters.len()) <= at_once {
        queue.push_back((want, clusters));
    } else {
        queue.extend(want.chunks(at_once).map(|w| (w, &[][..])));
        queue.extend(clusters.chunks(at_once).map(|k| (&[][..], k)));
    }
    let mut any = false;
    while let Some((want, clusters)) = queue.pop_front() {
        if want.is_empty() && clusters.is_empty() {
            continue;
        }
        let many = want.len().saturating_add(clusters.len()) > 1;
        match engine.glyphs(
            cell,
            band,
            (variant, doubled),
            want,
            (clusters, parts),
            made,
        ) {
            Ok(None) => {}
            // the codepoints apart from the clusters, then each halved
            Ok(Some(set)) if set.len() > max && many => {
                if !want.is_empty() && !clusters.is_empty() {
                    queue.push_front((&[], clusters));
                    queue.push_front((want, &[]));
                } else if want.len() > 1 {
                    let (a, b) = want.split_at(want.len() / 2);
                    queue.push_front((b, &[]));
                    queue.push_front((a, &[]));
                } else {
                    let (a, b) = clusters.split_at(clusters.len() / 2);
                    queue.push_front((&[], b));
                    queue.push_front((&[], a));
                }
            }
            Ok(Some(set)) if set.len() > max => {}
            Ok(Some(set)) => {
                write_frame(out, kind, &glyph_payload(band, &set))?;
                any = true;
            }
            Err(why) => return Ok(Err(why)),
        }
    }
    Ok(Ok(any))
}

/// A cell's fill: its codepoints in the order drawn, each once, and how
/// far it is.
struct Fill {
    frit: FritText,
    cell: Cell,
    band: Band,
    order: Vec<u32>,
    next: usize,
    /// It began with glyphs held, or has sent some.
    any: bool,
}

impl Fill {
    /// `request`'s fill: its codepoints, then [`LATIN_AND_BOX`], then the
    /// rest of the coverage, each once, those the console holds left out.
    fn new(engine: &Engine, request: &ServeRequest) -> Fill {
        let mut seen = Bits::new();
        for &(a, b) in &request.held {
            (a..=b).for_each(|c| seen.set(c));
        }
        let covered = |c: &u32| engine.coverage().binary_search(c).is_ok();
        let latin = LATIN_AND_BOX.iter().flat_map(|&(a, b)| a..=b);
        let mut order = Vec::new();
        for c in request
            .codepoints
            .iter()
            .copied()
            .chain(latin)
            .chain(engine.coverage().iter().copied())
        {
            if covered(&c) && !seen.has(c) {
                seen.set(c);
                order.push(c);
            }
        }
        Fill {
            frit: request.frit.clone(),
            cell: request.cell,
            band: request.band,
            order,
            next: 0,
            any: !request.held.is_empty(),
        }
    }
}

/// The requests read from `input` on a thread of their own, so the wait for
/// the next one has a deadline; the thread ends with the input, or at the
/// first malformed request.
type Requests = mpsc::Receiver<Result<Option<Request>, String>>;

fn read_requests(input: impl Read + Send + 'static) -> Requests {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut input = input;
        loop {
            let r = read_request(&mut input);
            let end = !matches!(r, Ok(Some(_)));
            if tx.send(r).is_err() || end {
                break;
            }
        }
    });
    rx
}

/// Serves requests read from `input` until it ends and its work is done,
/// or until `idle` passes with no request and no fill, writing the
/// answers to `out`. Returns the exit status: 0 at the input's end or the
/// idle exit, 1 on a malformed request or a write that fails.
pub fn serve(
    input: impl Read + Send + 'static,
    out: &mut dyn Write,
    paths: &Paths,
    idle: Duration,
) -> u8 {
    answer_requests(&read_requests(input), out, paths, idle, None, Gate::Open)
}

/// The helper's work: the asks waiting, in order, and the fills.
struct Work<'p> {
    paths: &'p Paths,
    /// Which font files the frits' sources may read: those the console's
    /// last [`ActiveAsk`] names.
    gate: Gate,
    /// Each frit's engine, by its name and text, so a frit whose text
    /// changes is read again.
    engines: BTreeMap<FritText, Result<Engine, DrawsNothing>>,
    /// The sources each frit leaves out, from the console's last
    /// [`Request::LeaveOut`]: none until one comes.
    left_out: u32,
    console: Option<ConsoleFont>,
    asks: VecDeque<Queued>,
    fills: VecDeque<Fill>,
}

/// An ask waiting its turn: a glyph ask, or a pointer ask, which is
/// answered in the same order so that each done frame answers the oldest
/// ask.
enum Queued {
    Glyphs(ServeRequest),
    Pointer(PointerAsk),
}

impl Work<'_> {
    /// The engine of `frit`, read the first time it is asked for, or why
    /// it draws nothing.
    fn engine(&mut self, frit: &FritText) -> Result<&Engine, &String> {
        self.read(frit).as_ref().map_err(|nothing| &nothing.why)
    }

    /// The engine of `frit`, read the first time it is asked for, or why
    /// it draws nothing and what its read found.
    fn read(&mut self, frit: &FritText) -> &Result<Engine, DrawsNothing> {
        let (paths, console, gate) = (self.paths, self.console.clone(), &self.gate);
        let left_out = self.left_out;
        self.engines
            .entry(frit.clone())
            .or_insert_with(|| Engine::read(frit, paths, console, (gate, left_out)))
    }

    /// Takes the sources the console names to leave out: each frit read
    /// already is read again at its next request, without them.
    fn leave_out(&mut self, mask: u32) {
        self.left_out = mask;
        self.engines.clear();
    }

    /// Takes the active fonts the console names, the gate every frit is
    /// read through from now on. A frit already read is read again at its
    /// next request, through the new gate.
    fn activate(&mut self, active: &ActiveAsk) -> Result<(), String> {
        self.gate = active.gate(rustix::process::geteuid().as_raw())?;
        self.engines.clear();
        Ok(())
    }

    /// Reads `frit` when it is not read yet, and sends each warning its
    /// text and its sources give, so the console logs them. A frit with no
    /// source left sends its warnings too, so the console logs each font
    /// it lacks.
    fn load(&mut self, frit: &FritText, out: &mut dyn Write) -> io::Result<()> {
        if self.engines.contains_key(frit) {
            return Ok(());
        }
        let warnings = match self.read(frit) {
            Ok(engine) => engine.warnings.clone(),
            Err(nothing) => nothing
                .read
                .as_ref()
                .map(|found| found.warnings.clone())
                .unwrap_or_default(),
        };
        for w in warnings {
            reason(out, FRAME_WARNING, &w)?;
        }
        Ok(())
    }

    /// Takes a request in: an ask waits its turn, a fill replaces the
    /// cell's, a stop ends it and is answered, and a cell query is answered
    /// at once. A frit read for the first time sends its warnings first.
    fn take(&mut self, request: ServeRequest, out: &mut dyn Write) -> io::Result<()> {
        if self.console.is_none() {
            self.console.clone_from(&request.console);
        }
        if request.kind != STOP {
            self.load(&request.frit, out)?;
        }
        let same =
            |f: &Fill| f.frit == request.frit && f.cell == request.cell && f.band == request.band;
        match request.kind {
            ASK | ASK_PARTS | ASK_MADE => self.asks.push_back(Queued::Glyphs(request)),
            FILL => {
                self.fills.retain(|f| !same(f));
                let fill = match self.engine(&request.frit) {
                    Ok(engine) => Fill::new(engine, &request),
                    Err(why) => return failed(out, &why.clone()),
                };
                self.fills.push_back(fill);
            }
            OWN_CELL => {
                let own = match self.engine(&request.frit) {
                    Ok(engine) => engine.own_cell(request.cell.height),
                    Err(why) => Err(why.clone()),
                };
                return match own {
                    Ok(OwnCell::Bitmap(cell)) => {
                        write_frame(out, FRAME_OWN_CELL, &cell_payload(cell))
                    }
                    Ok(OwnCell::Outline(cell)) => {
                        write_frame(out, FRAME_OUTLINE_CELL, &cell_payload(cell))
                    }
                    Err(why) => reason(out, FRAME_NO_OWN_CELL, &why),
                };
            }
            COVERAGE => {
                let dirs = self.paths.font_dirs.clone();
                let gate = self.gate.clone();
                return match self.engine(&request.frit) {
                    Ok(engine) => {
                        let payload =
                            coverage_payload(engine.coverage(), &request.codepoints, &dirs, &gate);
                        write_frame(out, FRAME_COVERAGE, &payload)
                    }
                    Err(why) => reason(out, FRAME_DRAWS_NOTHING, &why.clone()),
                };
            }
            SOURCES => {
                return match self.read(&request.frit) {
                    // a frit that reads and has no source left names its
                    // fonts as a frit with some left does, after the line
                    // that says it draws nothing
                    Err(
                        nothing @ DrawsNothing {
                            read: Some(found), ..
                        },
                    ) => {
                        let lines: Vec<String> = std::iter::once(nothing.line_text())
                            .chain(found.left_out.iter().map(LeftOut::line_text))
                            .chain(found.differs.iter().map(crate::needs::Differs::line_text))
                            .collect();
                        write_frame(out, FRAME_SOURCES, lines.join("\n").as_bytes())
                    }
                    Err(nothing) => reason(out, FRAME_DRAWS_NOTHING, &nothing.why.clone()),
                    Ok(engine) => {
                        // the notes on its script-source= lines, which only
                        // a sources query answers (ADR-1202 D12)
                        for note in engine.script_notes() {
                            reason(out, FRAME_WARNING, &note)?;
                        }
                        let lines: Vec<String> = engine
                            .left_out()
                            .iter()
                            .map(LeftOut::line_text)
                            .chain(
                                engine
                                    .differs()
                                    .iter()
                                    .map(crate::needs::Differs::line_text),
                            )
                            .collect();
                        write_frame(out, FRAME_SOURCES, lines.join("\n").as_bytes())
                    }
                };
            }
            _ => {
                self.fills.retain(|f| !same(f));
                return write_frame(
                    out,
                    FRAME_STOPPED,
                    &spaced_payload(request.cell, request.band),
                );
            }
        }
        Ok(())
    }

    /// Answers the oldest ask whole, its codepoints and then its clusters a
    /// batch at a time, then done; or the engine's failure.
    fn ask(&mut self, request: &ServeRequest, out: &mut dyn Write) -> io::Result<()> {
        let mut want = request.codepoints.clone();
        want.sort_unstable();
        want.dedup();
        let mut clusters = request.clusters.clone();
        clusters.sort_unstable();
        clusters.dedup();
        let cell = request.cell;
        let mut result = Ok(());
        // how each script glyph was made, which only an ASK_MADE is sent
        let mut made = BTreeMap::new();
        match self.engine(&request.frit) {
            Ok(engine) => {
                engine.set_cluster_width(request.cluster_width);
                let batches = want
                    .chunks(BATCH)
                    .map(|w| (w, &[][..]))
                    .chain(clusters.chunks(BATCH).map(|k| (&[][..], k)));
                for (w, k) in batches {
                    if let Err(why) = send_glyphs(
                        engine,
                        (
                            cell,
                            request.band,
                            Variant {
                                style: request.style,
                                script: request.script,
                            },
                            request.doubled,
                        ),
                        (w, k, request.kind == ASK_PARTS),
                        FRAME_GLYPHS,
                        (&mut *out, &mut made),
                        FRAME_SET,
                    )? {
                        result = Err(why);
                        break;
                    }
                }
            }
            Err(why) => result = Err(why.clone()),
        }
        match result {
            Ok(()) if request.kind == ASK_MADE => {
                write_frame(out, FRAME_MADE, &Made::encode_all(&made))?;
                write_frame(out, FRAME_DONE, &[])
            }
            Ok(()) => write_frame(out, FRAME_DONE, &[]),
            Err(why) => failed(out, &why),
        }
    }

    /// Draws the next batch of the first fill, which then goes to the back;
    /// a fill at its end is done, or fails when it never had a glyph.
    fn fill(&mut self, out: &mut dyn Write) -> io::Result<()> {
        let Some(mut fill) = self.fills.pop_front() else {
            return Ok(());
        };
        let end = fill.next.saturating_add(BATCH).min(fill.order.len());
        let mut batch = fill.order.get(fill.next..end).unwrap_or_default().to_vec();
        fill.next = end;
        batch.sort_unstable();
        let drawn = match self.engine(&fill.frit) {
            Ok(engine) => send_glyphs(
                engine,
                (fill.cell, fill.band, REGULAR.into(), None),
                (&batch, &[], false),
                FRAME_FILL,
                (&mut *out, &mut BTreeMap::new()),
                FRAME_SET,
            )?,
            Err(why) => Err(why.clone()),
        };
        match drawn {
            Err(why) => return failed(out, &why),
            Ok(any) => fill.any |= any,
        }
        if fill.next < fill.order.len() {
            self.fills.push_back(fill);
            return Ok(());
        }
        if !fill.any {
            let why = match self.engine(&fill.frit) {
                Ok(engine) => engine.none_at(fill.cell),
                Err(why) => why.clone(),
            };
            return failed(out, &why);
        }
        write_frame(out, FRAME_FILLED, &spaced_payload(fill.cell, fill.band))
    }
}

fn answer_requests(
    rx: &Requests,
    out: &mut dyn Write,
    paths: &Paths,
    idle: Duration,
    mut report: Option<Sandboxed>,
    gate: Gate,
) -> u8 {
    let mut work = Work {
        paths,
        gate,
        engines: BTreeMap::new(),
        left_out: 0,
        console: None,
        asks: VecDeque::new(),
        fills: VecDeque::new(),
    };
    let mut ended = false;
    loop {
        // every request waiting, before the next batch; with nothing to do,
        // the next request or the idle exit
        loop {
            let idle_now = work.asks.is_empty() && work.fills.is_empty();
            let next = if ended {
                break;
            } else if idle_now {
                rx.recv_timeout(idle)
            } else {
                rx.try_recv().map_err(|e| match e {
                    TryRecvError::Empty => RecvTimeoutError::Timeout,
                    TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                })
            };
            match next {
                Ok(Ok(Some(r))) => {
                    // the sandbox's report goes before the first answer,
                    // once the request has shown the console to be of this
                    // build, so a console of another build is told so
                    // instead of meeting a frame it does not know
                    if let Some(sandboxed) = report.take()
                        && write_frame(out, FRAME_SANDBOX, &sandboxed.word().to_le_bytes()).is_err()
                    {
                        return 1;
                    }
                    let taken = match r {
                        Request::Serve(r) => work.take(r, out),
                        Request::Pointer(a) => {
                            work.asks.push_back(Queued::Pointer(a));
                            Ok(())
                        }
                        Request::Describe(a) => a.answer(&work.gate, out),
                        Request::LeaveOut(mask) => {
                            work.leave_out(mask);
                            Ok(())
                        }
                        Request::Active(a) => match work.activate(&a) {
                            Ok(()) => Ok(()),
                            Err(why) => {
                                let _ = failed(out, &format!("a malformed request: {why}"));
                                return 1;
                            }
                        },
                    };
                    if taken.is_err() {
                        return 1;
                    }
                }
                Ok(Ok(None)) | Err(RecvTimeoutError::Disconnected) => ended = true,
                Err(RecvTimeoutError::Timeout) if idle_now => return 0,
                Err(RecvTimeoutError::Timeout) => break,
                Ok(Err(why)) => {
                    let _ = failed(out, &format!("a malformed request: {why}"));
                    return 1;
                }
            }
        }
        let wrote = match work.asks.pop_front() {
            Some(Queued::Glyphs(ask)) => work.ask(&ask, out),
            Some(Queued::Pointer(ask)) => ask.answer(out),
            None if !work.fills.is_empty() => work.fill(out),
            None => return 0,
        };
        if wrote.is_err() {
            return 1;
        }
    }
}

#[cfg(test)]
use vtium_testfont::ttf;

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests;
