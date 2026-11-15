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
//! printable Latin and box drawing ([`LATIN_AND_BOX`]), then the rest of
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
//!          ask, has a layout of its own, below), frit name length u32, the name (ADR-1200 D1's
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
//!          ([`FRAME_SANDBOX`], a u32 of bits); kind 15 a coverage
//!          query's answer, the count u32 and each codepoint u32 of the
//!          letters asked that the frit covers, then the family of an
//!          installed font that covers the most of the rest (UTF-8, empty
//!          when none does); kind 16 a pointer picture and 17 a pointer
//!          shape the helper draws nothing for, both answers to a pointer
//!          ask (below); kind 3 a failure (UTF-8
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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetStatus,
    Scope,
};
use rustix::process::{Resource, Rlimit, setrlimit};
use vtium_font::script::Rect;
use vtium_font::set::{PixelFormat, Set, SetBuilder, SetFace, StrikeBuilder};
use vtium_font::spacing::{self, Band, Fit, InkRows, SpacedCell};
use vtium_font::{
    CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, Cell, Decorations, Doubled, Glyph, Origin, REACH_CELLS,
    REACH_ROWS, SET_CHUNK_BYTES, Script, Style, Variant, Width,
};
use vtium_platform::vt::font::{ConsoleFont, FONT_GLYPHS_MAX, FONT_PITCH_TALL, UniPair};

use crate::child::PARSER_MEMORY;
use crate::font::Font;
use crate::frit::Render;
use crate::frit::{Cover, FRIT_BYTES_MAX, Frit, Hinting, Source, valid_name};
pub use crate::made::Made;
use crate::outline::{OutlineRequest, Placed, family_key, rasterise};
use crate::shaping::ZWJ;
use crate::sources::{Consoles, FONT_DIRS, Host, Input, KBD_DIRS, SourceError, resolve};
use vtium_render::{POINTER_SIDE_MAX, POINTER_SVG_BYTES_MAX, XCURSOR_FILE_MAX};
use vtium_term::{ClusterWidth, PointerShape};

/// The command that makes the binary the helper.
pub const SERVE_ARG: &str = "serve";
/// How long the helper waits for a request before it exits (the operator,
/// 2026-10-28; ADR-1301 D2).
pub const HELPER_IDLE_MS: u64 = 60_000;
/// The helper's CPU time over its life, in seconds: the kernel ends it
/// past this, and the console's next request starts another, a bounded
/// number of times a boot (the console's `RESTARTS_MAX`).
pub const HELPER_SECONDS: u64 = 300;

pub const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQA";
/// Why the helper refuses a request of another layout of its format: the
/// console that sent it comes from another build, and the console logs
/// this as the reason the font helper failed.
pub const OTHER_BUILD: &str = "the console and the font helper come from different builds of vtium, so the font helper cannot read the console's request; reinstall vtium so that both come from one build";
/// A frame of an ask's glyphs: a vitrified set.
pub const FRAME_GLYPHS: u32 = 1;
/// The oldest ask is answered whole.
pub const FRAME_DONE: u32 = 2;
/// The helper failed: the reason, as UTF-8.
pub const FRAME_FAILED: u32 = 3;
/// A frame of a fill's glyphs: a vitrified set.
pub const FRAME_FILL: u32 = 4;
/// A cell's fill is done: its width and height.
pub const FRAME_FILLED: u32 = 5;
/// A bitmap frit's own cell, a cell query's answer when the frit's first
/// source is a bitmap font: its width and height.
pub const FRAME_OWN_CELL: u32 = 6;
/// The frit has no own cell, a cell query's answer: the reason, as UTF-8.
pub const FRAME_NO_OWN_CELL: u32 = 7;
/// A stop is taken, every frame of the fill it ended sent: its cell's
/// width and height.
pub const FRAME_STOPPED: u32 = 8;
/// A warning the frit's text gave as it was read, such as a key that does
/// nothing (ADR-1200 D1): UTF-8, naming the frit and the line.
pub const FRAME_WARNING: u32 = 9;
/// The cell an outline frit takes at the height a cell query names, its
/// answer when the frit's first source is an outline font: its width and
/// height, the height the one asked for.
pub const FRAME_OUTLINE_CELL: u32 = 10;
/// A sources query's answer: each source left out, one a line.
pub const FRAME_SOURCES: u32 = 11;
/// A sources query's answer for a frit the helper draws nothing from: the
/// frit does not read, or none of its sources is on this machine; the
/// reason, as UTF-8. It is a kind of its own so that [`FRAME_FAILED`],
/// which the helper also sends when it fails as it starts, is never taken
/// as a frit's answer.
pub const FRAME_DRAWS_NOTHING: u32 = 12;
/// How an [`ASK_MADE`] made the superscript and subscript glyphs it drew
/// ([`Made::encode_all`]), sent once before its [`FRAME_DONE`].
pub const FRAME_MADE: u32 = 13;
/// The parts of the helper's sandbox in force, sent once before its first
/// answer, which the console logs when the landlock domain is not among
/// them (ADR-1200 D15): a u32 of [`SANDBOX_DOMAIN`], [`SANDBOX_FALLBACK`]
/// and [`SANDBOX_SOCKETS`]. A console of another build never meets it,
/// since its request's magic is refused first.
pub const FRAME_SANDBOX: u32 = 14;
/// A coverage query's answer ([`COVERAGE`], ADR-1401 D7): the count u32
/// and each codepoint u32 of the letters asked that the frit's sources
/// cover, then the family of an installed outline font that covers the
/// most of the rest, UTF-8, empty when none does
/// ([`crate::sources::font_covering`]).
pub const FRAME_COVERAGE: u32 = 15;
/// A pointer picture, an answer to a [`POINTER`] ask
/// ([`PointerPicture::payload`]).
pub const FRAME_POINTER: u32 = 16;
/// A pointer shape the helper draws nothing for, an answer to a
/// [`POINTER`] ask: the shape u32, [`NO_POINTER_MISSING`] or
/// [`NO_POINTER_REFUSED`] u32, and the reason in UTF-8.
pub const FRAME_NO_POINTER: u32 = 17;
/// [`FRAME_NO_POINTER`]'s why: the file has no picture of the shape.
pub const NO_POINTER_MISSING: u32 = 0;
/// [`FRAME_NO_POINTER`]'s why: the helper cannot draw the file's picture.
pub const NO_POINTER_REFUSED: u32 = 1;
/// A pointer ask (ADR-1309 D5): a pointer file's bytes, one shape, and
/// the heights to draw it at, read with a layout of its own
/// ([`PointerAsk`]).
pub const POINTER: u32 = 9;
/// A pointer ask's file kind: an SVG pointer file.
pub const POINTER_SVG: u32 = 1;
/// A pointer ask's file kind: a cursor theme's Xcursor file.
pub const POINTER_XCURSOR: u32 = 2;
/// The most cell heights one pointer ask names: the displays of one head
/// draw at few cell heights.
pub const POINTER_HEIGHTS_MAX: usize = 16;
/// An ask: the codepoints named, answered whole.
pub const ASK: u32 = 1;
/// An ask from a console that also takes the part of a cluster the font
/// draws only without some of its marks, under the part's own key
/// (ADR-1200 D22 step 3): a console built with right-to-left text. It is
/// answered as [`ASK`] is, the parts added.
pub const ASK_PARTS: u32 = 6;
/// An ask from vtium-config's font preview, which also wants to know how
/// each superscript and subscript glyph was made (ADR-1202 D14). It is
/// answered as [`ASK`] is, with a [`FRAME_MADE`] before its
/// [`FRAME_DONE`], and the console never sends it.
pub const ASK_MADE: u32 = 7;
/// A fill: the cell's coverage, in order, around the asks.
pub const FILL: u32 = 2;
/// A coverage query asks which of the letters it names the frit covers,
/// and which installed font covers the rest, for `vtium check-config`'s
/// warning on a font set that leaves the locale's everyday letters to the squeeze
/// (ADR-1401 D7, ADR-1302 D14). It carries a cell of 0 by 0 and the
/// letters as its codepoints.
pub const COVERAGE: u32 = 8;
/// The end of a cell's fill.
pub const STOP: u32 = 3;
/// A cell query asks for the frit's own cell. It carries a width of 0, the
/// height an outline font would be drawn at or 0, and no codepoints.
pub const OWN_CELL: u32 = 4;
/// A sources query asks which of the frit's sources this machine lacks or
/// refuses. It carries a cell of 0 by 0 and no codepoints, and the console
/// never sends it.
pub const SOURCES: u32 = 5;
/// What a fill draws after the screen's codepoints: printable Latin
/// (Basic Latin, Latin-1 and Latin Extended-A), then box drawing, block
/// elements and shades.
pub const LATIN_AND_BOX: [Cover; 3] = [(0x21, 0x7e), (0xa1, 0x17f), (0x2500, 0x259f)];
/// The most ranges a fill's held codepoints take.
pub const HELD_RANGES_MAX: usize = 65_536;
/// The most bytes a frame's payload holds; a batch past it is cut in two
/// until it fits.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// [`FRAME_BYTES_MAX`] as a length.
const FRAME: usize = FRAME_BYTES_MAX as usize;
/// The most bytes a set in a glyph frame holds: the frame's, less the
/// band in front of it.
const FRAME_SET: usize = FRAME - 8;
/// The most bytes a failure's reason holds.
pub const REASON_BYTES_MAX: usize = 4096;
/// The most bytes a frit's name holds, a frit name's own bound
/// (ADR-1200 D1).
pub const NAME_BYTES_MAX: usize = 64;
/// The most codepoints a request names first: a screen of 320 by 100
/// cells, each different.
pub const FIRST_MAX: usize = 32_000;
/// How many codepoints of the rest go in one batch.
pub const BATCH: usize = 1024;

/// A frit as the console hands it over: its name, for the glyphs' set
/// and for messages, and its text, read by the console or made on the fly
/// (ADR-1200 D9).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FritText {
    pub name: String,
    pub text: String,
}

/// What the console sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeRequest {
    /// [`ASK`], [`ASK_PARTS`], [`ASK_MADE`], [`FILL`], [`STOP`],
    /// [`OWN_CELL`] or [`SOURCES`].
    pub kind: u32,
    pub frit: FritText,
    pub cell: Cell,
    /// The rows line spacing adds to the cell above and below its text
    /// band (ADR-1302 D13): the glyphs are drawn in the cell less these
    /// rows and placed in the whole cell by their sets' `fit=`. None for a
    /// query, and for a cell that takes no spacing.
    pub band: Band,
    /// The style an ask's glyphs are drawn in; regular for every other
    /// request.
    pub style: Style,
    /// Where an ask's glyphs sit in their cells (ADR-1202 D11): normal, or
    /// superscript or subscript; normal for every other request.
    pub script: Script,
    /// The double size an ask's glyphs are drawn at, for a line of double
    /// width or double height (DECDWL, DECDHL), or `None` for the normal
    /// size, as every other request draws. A double ask draws only the
    /// outline glyphs of its codepoints, in the cell [`Doubled::cell`]
    /// gives, and leaves out its clusters, the codepoints a bitmap source
    /// draws, and color glyphs. The console doubles the pixels of what is
    /// left out.
    pub doubled: Option<Doubled>,
    /// The cells the console's terminal gives a cluster with a spacing
    /// mark or a virama conjunct (ADR-1006 D8), which the helper places
    /// an ask's clusters in; `double` for every other request.
    pub cluster_width: ClusterWidth,
    /// An ask's codepoints, or the codepoints a fill draws first: those on
    /// the screen.
    pub codepoints: Vec<u32>,
    /// An ask's clusters, each 1 to [`CLUSTER_SCALARS_MAX`] scalar values.
    pub clusters: Vec<Vec<u32>>,
    /// What the console holds at the cell, for a fill, as ranges.
    pub held: Vec<Cover>,
    /// The kernel's font, read by the console from its VT at start.
    pub console: Option<ConsoleFont>,
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Little-endian fields read from a stream, each count bounded before
/// anything is allocated for it.
struct Fields<'a, R>(&'a mut R);

impl<R: Read> Fields<'_, R> {
    fn u32(&mut self) -> Result<u32, String> {
        let mut b = [0u8; 4];
        self.0
            .read_exact(&mut b)
            .map_err(|_| "a request cut short".to_owned())?;
        Ok(u32::from_le_bytes(b))
    }

    fn u16(&mut self) -> Result<u16, String> {
        let mut b = [0u8; 2];
        self.0
            .read_exact(&mut b)
            .map_err(|_| "a request cut short".to_owned())?;
        Ok(u16::from_le_bytes(b))
    }

    fn count(&mut self, max: usize, what: &str) -> Result<usize, String> {
        let n = usize::try_from(self.u32()?).unwrap_or(usize::MAX);
        if n > max {
            return Err(format!("{what} past its bound of {max}"));
        }
        Ok(n)
    }

    fn exact(&mut self, out: &mut [u8]) -> Result<(), String> {
        self.0
            .read_exact(out)
            .map_err(|_| "a request cut short".to_owned())
    }

    fn bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; n];
        self.exact(&mut out)?;
        Ok(out)
    }
}

impl ServeRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        put32(&mut out, self.kind);
        for field in [&self.frit.name, &self.frit.text] {
            put32(&mut out, len32(field.len()));
            out.extend_from_slice(field.as_bytes());
        }
        put32(&mut out, self.cell.width);
        put32(&mut out, self.cell.height);
        put32(&mut out, self.band.above);
        put32(&mut out, self.band.below);
        put32(&mut out, style_code(self.style));
        put32(&mut out, script_code(self.script));
        put32(&mut out, cluster_width_code(self.cluster_width));
        put32(&mut out, doubled_code(self.doubled));
        // the display's capabilities, reserved
        put32(&mut out, 0);
        put32(&mut out, len32(self.codepoints.len()));
        for &c in &self.codepoints {
            put32(&mut out, c);
        }
        put32(&mut out, len32(self.clusters.len()));
        for cluster in &self.clusters {
            put32(&mut out, len32(cluster.len()));
            for &c in cluster {
                put32(&mut out, c);
            }
        }
        put32(&mut out, len32(self.held.len()));
        for &(a, b) in &self.held {
            put32(&mut out, a);
            put32(&mut out, b);
        }
        match &self.console {
            None => put32(&mut out, 0),
            Some(f) => {
                put32(&mut out, 1);
                for v in [f.width, f.height, f.charcount, len32(f.data.len())] {
                    put32(&mut out, v);
                }
                out.extend_from_slice(&f.data);
                put32(&mut out, len32(f.unimap.len()));
                for p in &f.unimap {
                    out.extend_from_slice(&p.unicode.to_le_bytes());
                    out.extend_from_slice(&p.fontpos.to_le_bytes());
                }
            }
        }
        out
    }

    /// Reads one request from `r`: `None` at the end of the input between
    /// requests. Every field is checked: the kind one of the seven, the frit's
    /// name a frit name (ADR-1200 D1), checked before it is allocated, and
    /// its text UTF-8, each inside its bound, the cell inside
    /// [`CELL_SIDE_MAX`], a cell query's at a width of 0, a height inside
    /// [`CELL_SIDE_MAX`] or 0, and naming no codepoint and no range, the
    /// rows line spacing adds none for a query and leaving any other
    /// request's cell a text band at least a row high, the
    /// style one of four and regular but for an ask, the script one of
    /// three and normal but for an ask, the reserved capabilities empty, each codepoint a
    /// scalar value, clusters in an ask alone, each 1 to
    /// [`CLUSTER_SCALARS_MAX`] scalar values, each held range in order and inside Unicode, the
    /// kernel's font inside the kernel's own bounds with its data exactly
    /// as long as its shape.
    pub fn read(r: &mut impl Read) -> Result<Option<ServeRequest>, String> {
        if !read_magic(r)? {
            return Ok(None);
        }
        let mut f = Fields(r);
        let kind = f.u32()?;
        ServeRequest::read_body(kind, &mut f).map(Some)
    }

    /// Reads the fields of a request of `kind` that follow its kind.
    fn read_body<R: Read>(kind: u32, f: &mut Fields<'_, R>) -> Result<ServeRequest, String> {
        if !matches!(
            kind,
            ASK | ASK_PARTS | ASK_MADE | FILL | STOP | OWN_CELL | SOURCES | COVERAGE
        ) {
            return Err("an unknown request kind".to_owned());
        }
        let n = f.count(NAME_BYTES_MAX, "a frit's name")?;
        let mut buf = [0u8; NAME_BYTES_MAX];
        let raw = buf.get_mut(..n).unwrap_or_default();
        f.exact(raw)?;
        let name = std::str::from_utf8(raw)
            .ok()
            .filter(|name| valid_name(name))
            .ok_or("a frit's name outside A-Z a-z 0-9 . _ + -")?
            .to_owned();
        let n = f.count(FRIT_BYTES_MAX, "a frit's text")?;
        let text = String::from_utf8(f.bytes(n)?).map_err(|_| "a frit's text that is not UTF-8")?;
        let cell = Cell {
            width: f.u32()?,
            height: f.u32()?,
        };
        let query = matches!(kind, OWN_CELL | SOURCES | COVERAGE);
        if query && (cell.width != 0 || cell.height > CELL_SIDE_MAX) {
            return Err(format!(
                "a cell query at a cell, or at a height of more than {CELL_SIDE_MAX} pixels"
            ));
        }
        if matches!(kind, SOURCES | COVERAGE) && cell.height != 0 {
            return Err("a sources or coverage query at a height".to_owned());
        }
        if !query
            && (!(1..=CELL_SIDE_MAX).contains(&cell.width)
                || !(1..=CELL_SIDE_MAX).contains(&cell.height))
        {
            return Err(format!(
                "a cell side of 0 or of more than {CELL_SIDE_MAX} pixels"
            ));
        }
        let band = Band {
            above: f.u32()?,
            below: f.u32()?,
        };
        if query && !band.is_none() {
            return Err("line spacing on a cell or sources query".to_owned());
        }
        if !band.is_none() && band.added() >= cell.height {
            return Err("line spacing that leaves the cell no text band".to_owned());
        }
        let style = style_of(f.u32()?).ok_or("an unknown style")?;
        let ask = matches!(kind, ASK | ASK_PARTS | ASK_MADE);
        if !ask && style != Style::default() {
            return Err("a style other than regular on a request that is not an ask".to_owned());
        }
        let script = script_of(f.u32()?).ok_or("an unknown script")?;
        if !ask && script != Script::Baseline {
            return Err("a superscript or subscript on a request that is not an ask".to_owned());
        }
        let cluster_width = cluster_width_of(f.u32()?).ok_or("an unknown cluster width")?;
        if !ask && cluster_width != ClusterWidth::Double {
            return Err("a cluster width on a request that is not an ask".to_owned());
        }
        let doubled = doubled_of(f.u32()?).ok_or("an unknown line size")?;
        if !ask && doubled.is_some() {
            return Err("a double size on a request that is not an ask".to_owned());
        }
        if doubled.is_some() && script != Script::Baseline {
            return Err("a superscript or subscript at a double size".to_owned());
        }
        if doubled.is_some_and(|d| d.cell(cell).is_none()) {
            return Err(format!(
                "a double size whose cell has a side of more than {CELL_SIDE_MAX} pixels"
            ));
        }
        if f.u32()? != 0 {
            return Err("display capabilities this helper does not read".to_owned());
        }
        let n = f.count(
            if query && kind != COVERAGE {
                0
            } else {
                FIRST_MAX
            },
            "the codepoints asked for",
        )?;
        let mut codepoints = Vec::with_capacity(n);
        for _ in 0..n {
            let c = f.u32()?;
            if char::from_u32(c).is_none() {
                return Err("a codepoint that is not a Unicode scalar value".to_owned());
            }
            codepoints.push(c);
        }
        let n = f.count(if ask { FIRST_MAX } else { 0 }, "the clusters asked for")?;
        let mut clusters = Vec::with_capacity(n);
        for _ in 0..n {
            let len = f.count(CLUSTER_SCALARS_MAX as usize, "a cluster's scalar values")?;
            let mut cluster = Vec::with_capacity(len);
            for _ in 0..len {
                let c = f.u32()?;
                if char::from_u32(c).is_none() {
                    return Err(
                        "a cluster holding a codepoint that is not a scalar value".to_owned()
                    );
                }
                cluster.push(c);
            }
            if cluster.is_empty() {
                return Err("an empty cluster".to_owned());
            }
            clusters.push(cluster);
        }
        let n = f.count(if query { 0 } else { HELD_RANGES_MAX }, "the ranges held")?;
        let mut held = Vec::with_capacity(n);
        for _ in 0..n {
            let (a, b) = (f.u32()?, f.u32()?);
            if a > b || b > 0x10_ffff {
                return Err("a held range out of order or past Unicode".to_owned());
            }
            held.push((a, b));
        }
        let console = match f.u32()? {
            0 => None,
            1 => {
                let (width, height, charcount) = (f.u32()?, f.u32()?, f.u32()?);
                if !(1..=32).contains(&width)
                    || !(1..=FONT_PITCH_TALL).contains(&height)
                    || !(1..=FONT_GLYPHS_MAX).contains(&charcount)
                {
                    return Err("a console font past the kernel's bounds".to_owned());
                }
                let want = width.div_ceil(8) * height * charcount;
                let n = f.count(want as usize, "a console font's data")?;
                if n != want as usize {
                    return Err("a console font's data not as long as its shape".to_owned());
                }
                let data = f.bytes(n)?;
                let n = f.count(usize::from(u16::MAX), "a console font's map")?;
                let mut unimap = Vec::with_capacity(n);
                for _ in 0..n {
                    let (unicode, fontpos) = (f.u16()?, f.u16()?);
                    unimap.push(UniPair { unicode, fontpos });
                }
                Some(ConsoleFont {
                    width,
                    height,
                    charcount,
                    data,
                    unimap,
                })
            }
            _ => return Err("an unknown console font flag".to_owned()),
        };
        Ok(ServeRequest {
            kind,
            frit: FritText { name, text },
            cell,
            band,
            style,
            script,
            doubled,
            cluster_width,
            codepoints,
            clusters,
            held,
            console,
        })
    }
}

/// Reads the magic of the next request from `r`: `false` at the end of the
/// input between requests.
fn read_magic(r: &mut impl Read) -> Result<bool, String> {
    let mut magic = [0u8; 8];
    let (head, rest) = magic.split_at_mut(1);
    match r.read(head) {
        Ok(0) => return Ok(false),
        Ok(_) => {}
        Err(e) => return Err(format!("cannot read a request: {e}")),
    }
    r.read_exact(rest)
        .map_err(|_| "a request cut short".to_owned())?;
    if &magic != REQUEST_MAGIC {
        // a request of another layout of this format comes from a
        // console of another build
        return Err(if magic.starts_with(b"VTVSREQ") {
            OTHER_BUILD.to_owned()
        } else {
            "not a serve request".to_owned()
        });
    }
    Ok(true)
}

/// Any request the helper reads: one of the glyph requests, or a pointer
/// ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Serve(ServeRequest),
    Pointer(PointerAsk),
}

/// Reads one request of any kind from `r`, as [`ServeRequest::read`] and
/// [`PointerAsk::read`] check them: `None` at the end of the input between
/// requests.
pub fn read_request(r: &mut impl Read) -> Result<Option<Request>, String> {
    if !read_magic(r)? {
        return Ok(None);
    }
    let mut f = Fields(r);
    let kind = f.u32()?;
    if kind == POINTER {
        PointerAsk::read_body(&mut f).map(|a| Some(Request::Pointer(a)))
    } else {
        ServeRequest::read_body(kind, &mut f).map(|r| Some(Request::Serve(r)))
    }
}

/// The kind of pointer file a pointer ask carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerFile {
    /// An SVG pointer file, whose symbols the helper draws a cell tall.
    Svg,
    /// One cursor theme file, in the Xcursor format.
    Xcursor,
}

/// A pointer ask (ADR-1309 D5): a pointer file's bytes, which the console
/// read, and one shape to draw from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerAsk {
    pub file: PointerFile,
    pub shape: PointerShape,
    /// `pointer-size`, the nominal size a cursor theme's picture is kept
    /// at; `None` keeps every size within the budget. Always `None` for an
    /// SVG file.
    pub size: Option<u32>,
    /// The cell heights the displays draw at, each of which an SVG file's
    /// symbol is drawn for; none for a cursor theme's file, which answers
    /// with the sizes it keeps.
    pub heights: Vec<u32>,
    pub bytes: Vec<u8>,
}

/// A shape's place in [`PointerShape::ALL`], as a pointer ask and its
/// answers carry it.
pub fn shape_code(shape: PointerShape) -> u32 {
    PointerShape::ALL
        .iter()
        .position(|&s| s == shape)
        .and_then(|i| u32::try_from(i).ok())
        .unwrap_or(0)
}

/// The shape at `code` in [`PointerShape::ALL`].
pub fn shape_of(code: u32) -> Option<PointerShape> {
    PointerShape::ALL.get(usize::try_from(code).ok()?).copied()
}

impl PointerAsk {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        put32(&mut out, POINTER);
        put32(
            &mut out,
            match self.file {
                PointerFile::Svg => POINTER_SVG,
                PointerFile::Xcursor => POINTER_XCURSOR,
            },
        );
        put32(&mut out, shape_code(self.shape));
        put32(&mut out, self.size.unwrap_or(0));
        put32(&mut out, len32(self.heights.len()));
        for &h in &self.heights {
            put32(&mut out, h);
        }
        put32(&mut out, len32(self.bytes.len()));
        out.extend_from_slice(&self.bytes);
        out
    }

    /// Reads one pointer ask from `r`, as [`read_request`] does, refusing
    /// any other request.
    pub fn read(r: &mut impl Read) -> Result<Option<PointerAsk>, String> {
        match read_request(r)? {
            None => Ok(None),
            Some(Request::Pointer(a)) => Ok(Some(a)),
            Some(Request::Serve(_)) => {
                Err("a glyph request where a pointer ask was expected".to_owned())
            }
        }
    }

    /// Reads the fields of a pointer ask that follow its kind, each checked
    /// before anything is allocated for it: the file kind one of two, the
    /// shape one of the ten, `pointer-size` 0 or 8 to 64 and 0 for SVG,
    /// the heights 1 to [`POINTER_HEIGHTS_MAX`] of them for SVG and none
    /// for Xcursor, each 1 to [`CELL_SIDE_MAX`], and the file within its
    /// kind's bound.
    fn read_body<R: Read>(f: &mut Fields<'_, R>) -> Result<PointerAsk, String> {
        let file = match f.u32()? {
            POINTER_SVG => PointerFile::Svg,
            POINTER_XCURSOR => PointerFile::Xcursor,
            _ => return Err("a pointer ask of an unknown file kind".to_owned()),
        };
        let shape = shape_of(f.u32()?).ok_or("a pointer ask for an unknown shape")?;
        let size = match f.u32()? {
            0 => None,
            s if file == PointerFile::Xcursor && (8..=POINTER_SIDE_MAX).contains(&s) => Some(s),
            _ => {
                return Err(
                    "a pointer ask whose pointer-size is not 8 to 64, or set for an SVG file"
                        .to_owned(),
                );
            }
        };
        let most = match file {
            PointerFile::Svg => POINTER_HEIGHTS_MAX,
            PointerFile::Xcursor => 0,
        };
        let n = f.count(most, "a pointer ask's heights")?;
        if file == PointerFile::Svg && n == 0 {
            return Err("a pointer ask for an SVG file with no height".to_owned());
        }
        let mut heights = Vec::with_capacity(n);
        for _ in 0..n {
            let h = f.u32()?;
            if !(1..=CELL_SIDE_MAX).contains(&h) {
                return Err(format!(
                    "a pointer ask's height of 0 or of more than {CELL_SIDE_MAX} pixels"
                ));
            }
            heights.push(h);
        }
        let n = f.count(
            match file {
                PointerFile::Svg => POINTER_SVG_BYTES_MAX,
                PointerFile::Xcursor => XCURSOR_FILE_MAX,
            },
            "a pointer file's bytes",
        )?;
        let bytes = f.bytes(n)?;
        Ok(PointerAsk {
            file,
            shape,
            size,
            heights,
            bytes,
        })
    }

    /// Draws the ask's pictures and writes the answer: a [`FRAME_POINTER`]
    /// for each picture, or one [`FRAME_NO_POINTER`], then
    /// [`FRAME_DONE`].
    fn answer(&self, out: &mut dyn Write) -> io::Result<()> {
        let code = shape_code(self.shape);
        let none = |out: &mut dyn Write, why: u32, reason: &str| {
            let mut payload = Vec::new();
            put32(&mut payload, code);
            put32(&mut payload, why);
            let mut reason = reason.to_owned();
            while reason.len() > REASON_BYTES_MAX {
                reason.pop();
            }
            payload.extend_from_slice(reason.as_bytes());
            write_frame(out, FRAME_NO_POINTER, &payload)
        };
        match self.file {
            PointerFile::Svg => {
                let mut heights = self.heights.clone();
                heights.sort_unstable();
                heights.dedup();
                for h in heights {
                    match crate::pointer_svg::draw(&self.bytes, self.shape.name(), h) {
                        Ok(p) => write_frame(out, FRAME_POINTER, &p.payload(self.shape))?,
                        Err(crate::pointer_svg::NoPicture::Missing) => {
                            none(out, NO_POINTER_MISSING, "")?;
                            break;
                        }
                        Err(crate::pointer_svg::NoPicture::Refused(why)) => {
                            none(out, NO_POINTER_REFUSED, &why)?;
                            break;
                        }
                    }
                }
            }
            PointerFile::Xcursor => match crate::xcursor::read(&self.bytes, self.size) {
                Ok(pictures) => {
                    for p in pictures {
                        write_frame(out, FRAME_POINTER, &p.payload(self.shape))?;
                    }
                }
                Err(why) => none(out, NO_POINTER_REFUSED, &why)?,
            },
        }
        write_frame(out, FRAME_DONE, &[])
    }
}

/// One pointer picture the helper drew: premultiplied RGBA rows, `width`
/// pixels a row, with the pixel the pointer names at `hot`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerPicture {
    /// The cell height it was drawn for, from an SVG file, or its nominal
    /// size, from a cursor theme's file.
    pub size: u32,
    pub width: u32,
    pub height: u32,
    pub hot: (u32, u32),
    pub rgba: Vec<u8>,
}

impl PointerPicture {
    /// The picture as [`FRAME_POINTER`]'s payload, for `shape`.
    pub fn payload(&self, shape: PointerShape) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rgba.len().saturating_add(24));
        for v in [
            shape_code(shape),
            self.size,
            self.width,
            self.height,
            self.hot.0,
            self.hot.1,
        ] {
            put32(&mut out, v);
        }
        out.extend_from_slice(&self.rgba);
        out
    }
}

/// A style as a request carries it: bold 1, italic 2, as a set's strike
/// carries its style.
pub fn style_code(style: Style) -> u32 {
    u32::from(style.bold) | u32::from(style.italic) << 1
}

/// A line size as a request carries it: the normal size 0, double width
/// 1, double height 2.
pub fn doubled_code(doubled: Option<Doubled>) -> u32 {
    match doubled {
        None => 0,
        Some(Doubled::Width) => 1,
        Some(Doubled::Height) => 2,
    }
}

/// The double size a request's line size `code` names: `Some(None)` for
/// the normal size, and `None` past double height's 2.
fn doubled_of(code: u32) -> Option<Option<Doubled>> {
    match code {
        0 => Some(None),
        1 => Some(Some(Doubled::Width)),
        2 => Some(Some(Doubled::Height)),
        _ => None,
    }
}

/// A cluster width as a request carries it: `double` 0, `single` 1.
pub fn cluster_width_code(rule: ClusterWidth) -> u32 {
    match rule {
        ClusterWidth::Double => 0,
        ClusterWidth::Single => 1,
    }
}

/// The cluster width a request's code names.
fn cluster_width_of(code: u32) -> Option<ClusterWidth> {
    match code {
        0 => Some(ClusterWidth::Double),
        1 => Some(ClusterWidth::Single),
        _ => None,
    }
}

/// A script as a request carries it: normal 0, superscript 1, subscript
/// 2, as a set's strike carries its script.
pub fn script_code(script: Script) -> u32 {
    match script {
        Script::Baseline => 0,
        Script::Super => 1,
        Script::Sub => 2,
    }
}

/// The script a request's `code` names, `None` past subscript's 2.
pub fn script_of(code: u32) -> Option<Script> {
    match code {
        0 => Some(Script::Baseline),
        1 => Some(Script::Super),
        2 => Some(Script::Sub),
        _ => None,
    }
}

/// The baseline a superscript or subscript glyph stands on in a text band
/// `height` rows high, where the first source's own box (ADR-1202 D1), its
/// glyphs' rows from its ascent to its descent, lies `own.1` rows high from
/// row `own.0`, and its normal glyphs stand on row `base`. vtium-font's
/// script rule sizes that box to two thirds and places it against the
/// band's top for a superscript and its bottom for a subscript
/// ([`vtium_font::script::size`], [`vtium_font::script::place`]); the
/// baseline lies the same share of the small box down as of the own box,
/// rounded to the nearest row (D2).
pub fn script_baseline(script: Script, own: (u32, u32), base: u32, height: u32) -> u32 {
    let base = base.min(height);
    let (top, tall) = (own.0.min(height), own.1.min(height - own.0.min(height)));
    if script == Script::Baseline || tall == 0 {
        return base;
    }
    let (_, small) = vtium_font::script::size((1, tall));
    let (_, at) = vtium_font::script::place(script, (1, small), (1, height), Band::NONE);
    let down = u64::from(base.saturating_sub(top).min(tall));
    let down = (down * u64::from(small) + u64::from(tall) / 2) / u64::from(tall);
    at.saturating_add(u32::try_from(down).unwrap_or(small))
}

/// The style a request's `code` names, `None` past bold italic's 3.
pub fn style_of(code: u32) -> Option<Style> {
    (code <= 3).then_some(Style {
        bold: code & 1 != 0,
        italic: code & 2 != 0,
    })
}

/// Writes one frame.
pub fn write_frame(out: &mut dyn Write, kind: u32, payload: &[u8]) -> io::Result<()> {
    let mut head = Vec::with_capacity(8);
    put32(&mut head, kind);
    put32(&mut head, len32(payload.len()));
    out.write_all(&head)?;
    out.write_all(payload)?;
    out.flush()
}

/// Where the helper finds what a frit names.
#[derive(Clone, Debug)]
pub struct Paths {
    pub kbd_dirs: Vec<PathBuf>,
    pub font_dirs: Vec<PathBuf>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
}

impl Default for Paths {
    /// The system's.
    fn default() -> Paths {
        Paths {
            kbd_dirs: KBD_DIRS.iter().map(PathBuf::from).collect(),
            font_dirs: FONT_DIRS.iter().map(PathBuf::from).collect(),
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            kernel_cmdline: PathBuf::from("/proc/cmdline"),
        }
    }
}

/// The kernel's font as the console handed it, for every `console:VT`.
struct Handed(Option<ConsoleFont>);

impl Consoles for Handed {
    fn font(&self, _vt: u16) -> Result<ConsoleFont, String> {
        self.0
            .clone()
            .ok_or_else(|| "the console read no font from its VT at start".to_owned())
    }
}

/// A source read and parsed once for the helper's life: a bitmap font's
/// glyphs, or an outline font's bytes with the codepoints it maps.
enum Loaded {
    Bitmap(Font),
    Outline {
        font: Arc<[u8]>,
        family: String,
        mapped: Vec<u32>,
    },
}

/// A frit's own cell, as its first source sets it (ADR-1200 D17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnCell {
    /// A bitmap font's own cell, which the console takes at the largest
    /// whole multiple not over the height it would draw at, and a zoom
    /// steps by its height.
    Bitmap(Cell),
    /// An outline font's cell at the height asked, taken as it is; a zoom
    /// asks again at the new height.
    Outline(Cell),
}

/// One step of what a style's glyphs are drawn from (ADR-1200 D21): the
/// sources the frit gave the style `face`, in the frit's order, with the
/// style `synth` drawn over their glyphs (ADR-1302 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub face: Style,
    pub synth: Style,
}

const REGULAR: Style = Style {
    bold: false,
    italic: false,
};
const BOLD: Style = Style {
    bold: true,
    italic: false,
};

/// What the glyphs of `style` are drawn from, in order: first the sources
/// the frit gave that style, drawn as they are; then, for a codepoint none
/// of them covers, the regular sources with the style synthesised over
/// them (ADR-1200 D21). Bold italic with no bold-italic face is the
/// regular glyph emboldened and then slanted, even where the frit names a
/// bold or an italic face (the maintainer, 2026-11-04).
pub fn steps(style: Style) -> Vec<Step> {
    let mut out = vec![Step {
        face: style,
        synth: REGULAR,
    }];
    if style != REGULAR {
        out.push(Step {
            face: REGULAR,
            synth: style,
        });
    }
    out
}

/// How `c` uses a row made taller by line spacing when the frit's set
/// that gives it has no `fit=`. The console places the glyphs it draws
/// itself by the same function ([`Fit::default_for`]), so the helper and
/// the console agree on every character's default, combining marks
/// included: vtium-font's one table of marks decides which characters are
/// marks for both.
///
/// References:
/// - ADR-1302 D13: the default fit of each kind of character.
pub fn default_fit(c: u32) -> Fit {
    Fit::default_for(c)
}

/// Whether `c` is a private-use character: of the Private Use Area,
/// U+E000 to U+F8FF, or of Supplementary Private Use Area-A or -B. Unicode
/// assigns these no meaning, and symbol fonts such as Nerd Fonts and
/// Powerline put their symbols there.
fn private_use(c: u32) -> bool {
    matches!(c, 0xe000..=0xf8ff | 0xf_0000..=0xf_fffd | 0x10_0000..=0x10_fffd)
}

/// A source [`Engine::load`] left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftOut {
    /// This machine does not have it; otherwise it has it and refused it.
    pub missing: bool,
    /// Its line in the frit.
    pub line: usize,
    /// Its value as the frit spells it, such as `font:JetBrains Mono`.
    pub spelling: String,
    pub why: String,
}

impl LeftOut {
    /// The line a sources query answers for it.
    pub fn line_text(&self) -> String {
        let kind = if self.missing { "missing" } else { "refused" };
        let clean = |t: &str| t.replace(['\t', '\n'], " ");
        format!(
            "{kind}\t{}\t{}\t{}",
            self.line,
            clean(&self.spelling),
            clean(&self.why)
        )
    }

    /// The source a sources query's line `text` names, `None` for a line
    /// that is not one.
    pub fn parse(text: &str) -> Option<LeftOut> {
        let mut f = text.splitn(4, '\t');
        let missing = match f.next()? {
            "missing" => true,
            "refused" => false,
            _ => return None,
        };
        Some(LeftOut {
            missing,
            line: f.next()?.parse().ok()?,
            spelling: f.next()?.to_owned(),
            why: f.next()?.to_owned(),
        })
    }
}

/// A frit's sources, ready to draw at any cell.
pub struct Engine {
    name: String,
    sources: Vec<(Source, Loaded)>,
    /// Every codepoint some source gives, sorted.
    coverage: Vec<u32>,
    /// The sources that failed as they drew, by their place in `sources`,
    /// and why: left out for the engine's life (ADR-1200 D14).
    failed: RefCell<BTreeMap<usize, String>>,
    /// What the frit's text gave as warnings, each naming its line.
    warnings: Vec<String>,
    /// The sources [`Engine::load`] left out, each as a sources query
    /// answers it.
    left_out: Vec<LeftOut>,
    /// Each outline source's baseline at a cell's height, by its place in
    /// `sources` and the height (ADR-1302 D3): found once, from the
    /// source's printable ASCII, so every request draws it on one
    /// baseline whatever the request holds.
    baselines: RefCell<BTreeMap<(usize, u32), u32>>,
    /// How the outline sources' glyphs are hinted: the frit's `hinting=`
    /// (ADR-1200 D1).
    hinting: Hinting,
    /// The cells the console's terminal gives a cluster with a spacing
    /// mark or a virama conjunct (ADR-1006 D8), which the ask being
    /// answered names ([`Engine::set_cluster_width`]).
    cluster_width: std::cell::Cell<ClusterWidth>,
    /// Each bitmap source's smaller sizes by its place in `sources`, in
    /// the order D3 names them: a CPI source's other heights, then the
    /// fonts its `script-source=` lines name (ADR-1202 D3, D12). Its own
    /// strike, which comes first, is not repeated here.
    smaller: Vec<Vec<SmallerFont>>,
    /// Each outline source's strikes by its place in `sources`, each a
    /// [`Loaded::Bitmap`]: the bitmap files its `strike=` lines name that
    /// this machine has and that read. Empty for a bitmap source.
    strikes: Vec<Vec<Loaded>>,
    /// The frit's `cell=`, which sets its own cell where it states one.
    cell: Option<Cell>,
    /// The spelling of the frit's text font, its first regular source,
    /// when [`Engine::load`] left it out: then the built-in font takes its
    /// place, and the frit has no own cell.
    text_left_out: Option<String>,
    /// The frit's `render=` lines, by the line of the source each is for
    /// ([`Engine::draws_as_bitmap`]).
    render: Vec<(usize, Render)>,
}

/// A vitrified set of glyphs, `None` when no source gives any, and how the
/// engine made each superscript or subscript glyph in it, by its codepoint
/// ([`Engine::draw_made`]).
pub type MadeSet = (Option<Vec<u8>>, BTreeMap<u32, Made>);

/// A smaller size of a bitmap source (ADR-1202 D3): its font, and for one
/// that `script-source=` names, that line and the font's spelling.
struct SmallerFont {
    font: Font,
    named: Option<(usize, String)>,
}

/// The codepoints `font` maps in the face of `family` (its first face for
/// an empty family), as the rasteriser takes them: no control character
/// and no glyph 0.
fn outline_coverage(font: &[u8], family: &str) -> Result<Vec<u32>, String> {
    use swash::{FontDataRef, StringId};
    let fonts = FontDataRef::new(font).ok_or("not a TrueType or OpenType font")?;
    let want = family_key(family);
    let face = fonts
        .fonts()
        .find(|f| {
            want.is_empty() || {
                let strings = f.localized_strings();
                [StringId::TypographicFamily, StringId::Family]
                    .into_iter()
                    .filter_map(|id| strings.find_by_id(id, Some("en")))
                    .any(|s| family_key(&s.to_string()) == want)
            }
        })
        .ok_or_else(|| format!("the font holds no face of family {family}"))?;
    let mut out = Vec::new();
    face.charmap().enumerate(|c, g| {
        let control = c < 0x20 || (0x7f..0xa0).contains(&c);
        if g != 0 && !control && char::from_u32(c).is_some() {
            out.push(c);
        }
    });
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// Sorted codepoints as ranges, runs joined.
fn ranges(cs: &[u32]) -> Vec<Cover> {
    let mut out: Vec<Cover> = Vec::new();
    for &c in cs {
        match out.last_mut() {
            Some((_, b)) if b.checked_add(1) == Some(c) => *b = c,
            _ => out.push((c, c)),
        }
    }
    out
}

fn bit(bits: &[u8], row: usize, x: u32, y: u32) -> bool {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    bits.get(at).is_some_and(|b| b & (0x80 >> (x % 8)) != 0)
}

fn set_bit(bits: &mut [u8], row: usize, x: u32, y: u32) {
    let at = (y as usize)
        .saturating_mul(row)
        .saturating_add((x / 8) as usize);
    if let Some(b) = bits.get_mut(at) {
        *b |= 0x80 >> (x % 8);
    }
}

/// The cell's row where the top row of a bitmap glyph scaled by `scale`
/// goes, so that the font's baseline `own`,
/// counted in the font's rows, meets the cell's baseline `base` (ADR-1200
/// D17). The first source's glyphs then fill the cell down to its bottom
/// row, and a later source's glyphs stand on the first source's baseline;
/// the ink a later source's glyph then has above or below the cell goes to
/// its ink box ([`place_styled`]).
fn glyph_top(scale: u32, own: u32, base: u32) -> i64 {
    i64::from(base) - i64::from(own) * i64::from(scale)
}

/// A bitmap glyph placed in the cells of the text band, as coverage, and
/// its ink box where its ink passes them (ADR-1302 D6, ADR-1306 D1).
type Drawn = (Vec<u8>, Option<InkRows>);

/// A 1-bit glyph `width` cells wide of a font of cell `from`, drawn in the
/// style `synth` asks (ADR-1302 D4), scaled by `scale` into `cell`, the
/// text band's cell, centred across, its top row at row `top` of the cell
/// ([`glyph_top`]), as coverage of 0 or 255. Bold widens each row, a pixel
/// set when it or its left neighbour is; italic then shears the rows about
/// `baseline` at the font's own size, before the scale, by vtium-font's
/// rule ([`vtium_font::italic::shear_shift`]), so the console's own bitmap
/// faces and the helper draw the same glyph. A glyph whose ink passes its
/// cells, a sheared one beside them or a later source's raised onto the
/// first source's baseline above or below them, carries its ink box: its
/// cells and its ink together, cut to its reach, [`REACH_CELLS`] cells past
/// its cells on each side and `reach` rows above and below the text band.
fn place_styled(
    glyph: &[u8],
    width: Width,
    from: Cell,
    (cell, top): (Cell, i64),
    scale: u32,
    synth: Style,
    (baseline, reach): (u32, (u32, u32)),
) -> Drawn {
    let cells = width.cells();
    let src_w = from.width * cells;
    let src_row = src_w.div_ceil(8) as usize;
    let lit = |x: u32, y: u32| {
        bit(glyph, src_row, x, y) || (synth.bold && x > 0 && bit(glyph, src_row, x - 1, y))
    };
    let mask: Vec<u8> = (0..from.height)
        .flat_map(|y| (0..src_w).map(move |x| if lit(x, y) { 255 } else { 0 }))
        .collect();
    let sheared = synth
        .italic
        .then(|| vtium_font::italic::shear(&mask, src_w, from.height, baseline))
        .flatten();
    // the drawn glyph at the font's own size: its pixels, their width and
    // the column of the font's cell its first one lies at
    let (pixels, stride, shift) = match &sheared {
        Some(s) => (&s.pixels, s.width as usize, i64::from(s.left)),
        None => (&mask, src_w as usize, 0),
    };
    let dst_w = cell.width * cells;
    let x0 = i64::from((dst_w - src_w * scale) / 2);
    let scale = i64::from(scale);
    // the glyph's pixel under column `x` from the cells' left edge and row
    // `y` from the text band's top
    let at = |x: i64, y: i64| -> bool {
        let (Ok(sy), Ok(sx)) = (
            usize::try_from((y - top).div_euclid(scale)),
            usize::try_from((x - x0).div_euclid(scale) - shift),
        ) else {
            return false;
        };
        sy < from.height as usize
            && sx < stride
            && pixels.get(sy * stride + sx).is_some_and(|&p| p != 0)
    };
    let rect = |(x0, x1): (i64, i64), (y0, y1): (i64, i64)| -> Vec<u8> {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| if at(x, y) { 255 } else { 0 }))
            .collect()
    };
    let (span, height) = (i64::from(dst_w), i64::from(cell.height));
    let own = rect((0, span), (0, height));
    let across = i64::from(cell.width) * i64::from(REACH_CELLS);
    let (columns, rows) = (
        -across..span + across,
        -i64::from(reach.0)..height + i64::from(reach.1),
    );
    let inked_column = |x: i64| rows.clone().any(|y| at(x, y));
    let inked_row = |y: i64| columns.clone().any(|x| at(x, y));
    let ink = (|| {
        let (first, last) = (
            columns.clone().find(|&x| inked_column(x))?,
            columns.clone().rev().find(|&x| inked_column(x))?,
        );
        let (highest, lowest) = (
            rows.clone().find(|&y| inked_row(y))?,
            rows.clone().rev().find(|&y| inked_row(y))?,
        );
        let (left, right) = (first.min(0), (last + 1).max(span));
        let (up, down) = (highest.min(0), (lowest + 1).max(height));
        if (left, right, up, down) == (0, span, 0, height) {
            return None;
        }
        Some(InkRows {
            left: i32::try_from(left).ok()?,
            top: i32::try_from(up).ok()?,
            width: u32::try_from(right - left).ok()?,
            height: u32::try_from(down - up).ok()?,
            pixels: rect((left, right), (up, down)),
        })
    })();
    (own, ink)
}

/// What a bitmap glyph drawn in the cells of a text band `span` by
/// `height` pixels is made from (ADR-1202 D1, D4, D7): a 1-bit source,
/// whose own box is its font's cell `font`, `cells` cells wide, drawn at
/// `scale`, centred across with its top row at `top` as [`place_styled`]
/// places it and cut to the band; sheared about the row under the font's
/// baseline `baseline`; and with `synth` left to the small glyph.
fn bitmap_origin(
    (font, cells, scale): (Cell, Width, u32),
    (span, height): (u32, u32),
    (top, baseline): (i64, u32),
    synth: Style,
) -> Origin {
    let w = font.width * cells.cells() * scale;
    let h = i64::from(font.height * scale);
    let row = |y: i64| u32::try_from(y.clamp(0, i64::from(height))).unwrap_or(0);
    let (first, last) = (row(top), row(top + h));
    Origin {
        own: Some(Rect {
            left: span.saturating_sub(w) / 2,
            top: first,
            width: w.min(span),
            height: last.saturating_sub(first),
        }),
        ..Origin::bitmap(row(top + i64::from(baseline * scale)), synth)
    }
}

/// A bitmap glyph's coverage `own`, drawn upright and unstyled in the
/// cells of a text band `size` pixels across and down and `width` cells
/// wide, as `script` draws it from `origin`: made small, styled and placed
/// by vtium-font's script rule ([`Glyph::scripted`], ADR-1202 D1, D4, D7),
/// the one the console's own bitmap fonts take. `None` for coverage that
/// is not the size's.
fn script_bitmap(
    own: &[u8],
    size: (u32, u32),
    width: Width,
    (script, origin): (Script, &Origin),
) -> Option<Vec<u8>> {
    let glyph = Glyph::from_pixels(size.0, size.1, vtium_font::Format::Mask, own.to_vec())?
        .with_cells(width);
    Some(
        glyph
            .scripted(script, origin, Band::NONE)?
            .pixels()
            .to_vec(),
    )
}

/// Coverage of 0 or 255, `width` pixels across, as 1-bit rows.
fn pack_bits(coverage: &[u8], width: u32) -> Vec<u8> {
    let w = width as usize;
    let row = w.div_ceil(8);
    let mut out = vec![0u8; coverage.len().checked_div(w).unwrap_or(0) * row];
    for (y, line) in coverage.chunks(w.max(1)).enumerate() {
        for (x, &p) in line.iter().enumerate() {
            if p != 0 {
                set_bit(&mut out, row, x as u32, y as u32);
            }
        }
    }
    out
}

/// The row under the baseline `font` is sheared about for italic, from its
/// cell's top: the baseline the font states, as a built-in set's strike
/// and a BDF font's ascent do, else the row under the lowest ink of its
/// `H`, else its cell's bottom edge ([`vtium_font::italic::bitmap_baseline`]).
fn bitmap_baseline(font: &Font) -> u32 {
    if let Some(b) = font.baseline {
        return b.min(font.cell.height);
    }
    let h = font
        .map
        .get(&u32::from('H'))
        .and_then(|&g| font.glyphs.get(g as usize))
        .map(|(width, rows)| {
            let w = font.cell.width * width.cells();
            let row = w.div_ceil(8) as usize;
            (0..font.cell.height)
                .map(|y| (0..w).any(|x| bit(rows, row, x, y)))
                .collect::<Vec<bool>>()
        });
    vtium_font::italic::bitmap_baseline(h.as_deref(), font.cell.height)
}

/// A glyph from a smaller size of a bitmap source (ADR-1202 D3), as
/// [`Engine::smaller_glyph`] finds it.
#[derive(Clone, Copy)]
struct SmallerGlyph<'a> {
    /// The strike's place: 0 for the source's own, else its smaller
    /// size's place in [`Engine`]'s list, from 1.
    strike: usize,
    /// The glyph's number in the strike's font.
    glyph: u32,
    /// The whole multiple the strike is drawn at.
    multiple: u32,
    font: &'a Font,
    rows: &'a [u8],
}

/// The smaller sizes of the bitmap source `source` (ADR-1202 D3, D12),
/// read and parsed as [`crate::sources::smaller`] names them. A font this
/// machine does not have is left out quietly, as a source is. A font that
/// is here and is refused is left out with a warning naming its line in
/// the frit `name`. A `script-source=` font keeps its own Unicode table
/// where it has one, and takes the source's `unimap=` where it has none.
fn smaller_fonts(
    source: &Source,
    host: &Host<'_>,
    name: &str,
    warnings: &mut Vec<String>,
) -> Vec<SmallerFont> {
    let mut fonts = Vec::new();
    for (named, input) in crate::sources::smaller(source, host) {
        let font = input.and_then(|input| {
            bitmap_of(input, "this is an outline font, not a smaller bitmap font")
                .map_err(SourceError::Failed)
        });
        match (font, &named) {
            (Ok(font), _) => fonts.push(SmallerFont { font, named }),
            (Err(SourceError::Failed(e)), Some((line, spelling))) => warnings.push(format!(
                "{name}:{line}: script-source={spelling}: {e}; the font is left out"
            )),
            // a font this machine lacks, and a CPI height that does not
            // decode, are left out quietly
            (Err(_), _) => {}
        }
    }
    fonts
}

/// The glyphs of a bitmap font that a source names besides its own: a
/// smaller size or a strike. A font with a Unicode table of its own keeps
/// it, and one with none takes the source's `unimap=`, else kbd's
/// `def.uni`. An outline font is refused with `outline`.
fn bitmap_of(input: Input, outline: &str) -> Result<Font, String> {
    match input {
        Input::Bitmap(font) => Ok(font),
        Input::Bdf(bytes) => crate::bdf::font(&bytes),
        Input::Font(req) => req
            .unimap
            .as_ref()
            .and_then(|_| crate::decode::font(&req.font, None, None).ok())
            .map_or_else(
                || crate::decode::font(&req.font, req.unimap.as_deref(), req.def_uni.as_deref()),
                Ok,
            )
            .map(Font::from_glyphs),
        Input::Outline(_) => Err(outline.to_owned()),
    }
}

/// The strikes that the outline `source`'s `strike=` lines name, read and
/// parsed, each a [`Loaded::Bitmap`]. A strike this machine does not have
/// is left out quietly, as a source is, and one that is here and is
/// refused is left out with a warning naming its line in the frit `name`.
///
/// References:
/// - ADR-1204 D2: the `strike=` key under an outline source.
fn strike_fonts(
    source: &Source,
    host: &Host<'_>,
    name: &str,
    warnings: &mut Vec<String>,
) -> Vec<Loaded> {
    let mut fonts = Vec::new();
    for strike in &source.strikes {
        let file = Source {
            spelling: strike.spelling.clone(),
            origin: strike.origin.clone(),
            unimap: None,
            cover: Vec::new(),
            cp_size: None,
            script_sources: Vec::new(),
            strikes: Vec::new(),
            style: source.style,
            fits: Vec::new(),
            line: strike.line,
        };
        let font = resolve(&file, host).and_then(|input| {
            bitmap_of(input, "this is an outline font, not a bitmap strike")
                .map_err(SourceError::Failed)
        });
        match font {
            Ok(font) => fonts.push(Loaded::Bitmap(font)),
            Err(SourceError::Failed(e)) => warnings.push(format!(
                "{name}:{}: strike={}: {e}; the strike is left out",
                strike.line, strike.spelling
            )),
            Err(SourceError::Missing(_)) => {}
        }
    }
    fonts
}

impl Engine {
    /// Places the clusters the engine draws next in the cells that `rule`
    /// gives a cluster with a spacing mark or a virama conjunct, as the
    /// console's terminal gives them (ADR-1006 D8): the ask being answered
    /// names it, and every other drawing takes `double`, the default.
    pub fn set_cluster_width(&self, rule: ClusterWidth) {
        self.cluster_width.set(rule);
    }

    /// Parses `frit` and reads each source it names, as this process's
    /// account; a source missing or unreadable is left out, and the frit
    /// fails only when none is left. A source that is on this machine and
    /// is refused, such as a BDF file the reader refuses, is also a
    /// warning when the frit loads without it, so the console logs it; a
    /// source this machine does not have is left out quietly. Its messages
    /// name the frit by its name.
    pub fn load(
        frit: &FritText,
        paths: &Paths,
        console: Option<ConsoleFont>,
    ) -> Result<Engine, String> {
        let file = Path::new(&frit.name);
        let frit =
            Frit::parse(frit.text.as_bytes(), file, &frit.name).map_err(|e| e.to_string())?;
        let mut warnings: Vec<String> = frit.warnings.iter().map(ToString::to_string).collect();
        let handed = Handed(console);
        let host = Host {
            kbd_dirs: paths.kbd_dirs.clone(),
            font_dirs: paths.font_dirs.clone(),
            vconsole_conf: paths.vconsole_conf.clone(),
            kernel_cmdline: paths.kernel_cmdline.clone(),
            consoles: &handed,
        };
        let mut sources = Vec::new();
        let mut smaller = Vec::new();
        let mut strikes = Vec::new();
        let mut why = Vec::new();
        let mut left_out = Vec::new();
        for source in &frit.sources {
            let loaded = resolve(source, &host).and_then(|input| {
                match input {
                    Input::Bitmap(font) => Ok(Loaded::Bitmap(font)),
                    Input::Bdf(bytes) => crate::bdf::font(&bytes).map(Loaded::Bitmap),
                    Input::Font(req) => crate::decode::font(
                        &req.font,
                        req.unimap.as_deref(),
                        req.def_uni.as_deref(),
                    )
                    .map(|g| Loaded::Bitmap(Font::from_glyphs(g))),
                    Input::Outline(req) => {
                        outline_coverage(&req.font, &req.family).map(|mapped| Loaded::Outline {
                            font: req.font,
                            family: req.family,
                            mapped,
                        })
                    }
                }
                .map_err(SourceError::Failed)
            });
            match loaded {
                Ok(l) => {
                    strikes.push(match l {
                        Loaded::Outline { .. } => {
                            strike_fonts(source, &host, &frit.name, &mut warnings)
                        }
                        Loaded::Bitmap(_) => {
                            if let Some(strike) = source.strikes.first() {
                                warnings.push(format!(
                                    "{}:{}: strike={} is ignored, because the source above it is a bitmap font",
                                    frit.name, strike.line, strike.spelling
                                ));
                            }
                            Vec::new()
                        }
                    });
                    let small = match l {
                        Loaded::Bitmap(_) => {
                            smaller_fonts(source, &host, &frit.name, &mut warnings)
                        }
                        Loaded::Outline { .. } => {
                            if let Some(named) = source.script_sources.first() {
                                warnings.push(format!(
                                    "{}:{}: script-source={} is ignored: the source above it is an outline font, which draws its own superscripts and subscripts at a smaller size",
                                    frit.name, named.line, named.spelling
                                ));
                            }
                            Vec::new()
                        }
                    };
                    sources.push((source.clone(), l));
                    smaller.push(small);
                }
                Err(SourceError::Missing(e)) => {
                    why.push(format!("source={}: {e}", source.spelling));
                    left_out.push(LeftOut {
                        missing: true,
                        line: source.line,
                        spelling: source.spelling.clone(),
                        why: e,
                    });
                }
                Err(SourceError::Failed(e)) => {
                    left_out.push(LeftOut {
                        missing: false,
                        line: source.line,
                        spelling: source.spelling.clone(),
                        why: e.clone(),
                    });
                    let line = format!("source={}: {e}", source.spelling);
                    warnings.push(format!(
                        "{}:{}: {line}; the source is left out",
                        frit.name, source.line
                    ));
                    why.push(line);
                }
            }
        }
        // a font: source that turns out to be a console font draws 1-bit
        // whatever its render= line says, so the line does nothing
        for (source, _) in sources
            .iter()
            .filter(|(_, l)| matches!(l, Loaded::Bitmap(_)))
        {
            if frit.render_for(source.line).is_some() {
                warnings.push(format!(
                    "{}:{}: source={}: render= is ignored, because this font is a bitmap font, which always draws 1-bit",
                    frit.name, source.line, source.spelling
                ));
            }
        }
        let text_left_out = frit
            .sources
            .iter()
            .find(|s| s.style == REGULAR)
            .filter(|text| {
                !sources
                    .iter()
                    .any(|(s, _): &(Source, _)| s.line == text.line)
            })
            .map(|text| text.spelling.clone());
        if sources.is_empty() {
            return Err(if why.is_empty() {
                format!("{}: the frit names no source", file.display())
            } else {
                why.join("; ")
            });
        }
        let mut coverage: Vec<u32> = sources
            .iter()
            .zip(&strikes)
            .flat_map(|((source, l), strikes)| {
                let cs: Vec<u32> = std::iter::once(l)
                    .chain(strikes)
                    .flat_map(|l| match l {
                        Loaded::Bitmap(f) => f.map.keys().copied().collect::<Vec<u32>>(),
                        Loaded::Outline { mapped, .. } => mapped.clone(),
                    })
                    .collect();
                cs.into_iter().filter(|&c| source.covers(c))
            })
            .collect();
        coverage.sort_unstable();
        coverage.dedup();
        // `Frit::parse` refuses a name that is not valid
        Ok(Engine {
            cluster_width: std::cell::Cell::default(),
            name: frit.name,
            sources,
            coverage,
            failed: RefCell::default(),
            warnings,
            left_out,
            baselines: RefCell::default(),
            hinting: frit.hinting,
            smaller,
            strikes,
            cell: frit.cell,
            text_left_out,
            render: frit.render,
        })
    }

    /// Whether the glyphs of the outline source at `i` in `sources` draw as
    /// a bitmap font's glyphs draw: 1-bit and hinted to the pixel grid both
    /// ways. The source's `render=` line decides where it has one. A source
    /// with none draws as a bitmap when the frit's first source is a bitmap
    /// font, so that the outline glyphs that fill its gaps look like the
    /// rest of it. The first source is the first regular one the engine can
    /// still draw from, the one that sets the frit's cell
    /// ([`Engine::own_cell`]).
    ///
    /// References:
    /// - ADR-1200 D1, the `render=` row: the values and the default.
    fn draws_as_bitmap(&self, i: usize) -> bool {
        let Some((source, _)) = self.sources.get(i) else {
            return false;
        };
        if let Some(&(_, render)) = self.render.iter().find(|&&(at, _)| at == source.line) {
            return render == Render::Bitmap;
        }
        let failed = self.failed.borrow();
        self.sources
            .iter()
            .enumerate()
            .filter(|(at, (s, _))| s.style == REGULAR && !failed.contains_key(at))
            .map(|(_, (_, loaded))| loaded)
            .next()
            .is_some_and(|loaded| matches!(loaded, Loaded::Bitmap(_)))
    }

    /// `request`, for the outline source at `i` in `sources`, asking for
    /// that source's glyphs 1-bit where they draw as a bitmap
    /// ([`Engine::draws_as_bitmap`]). Such a request carries
    /// [`Hinting::Bitmap`] in place of the frit's `hinting=`.
    fn rendered(&self, request: OutlineRequest, i: usize) -> OutlineRequest {
        match self.draws_as_bitmap(i) {
            true => OutlineRequest {
                hinting: Hinting::Bitmap,
                ..request
            },
            false => request,
        }
    }

    /// The frit text's warnings and one for each source the helper
    /// refused, each naming its line.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The sources the engine left out: those this machine does not have,
    /// and those it has and refused.
    pub fn left_out(&self) -> &[LeftOut] {
        &self.left_out
    }

    /// Every codepoint the frit's sources give, sorted.
    pub fn coverage(&self) -> &[u32] {
        &self.coverage
    }

    /// The frit's own cell (ADR-1200 D17), which its first source sets and
    /// later sources draw into. The first source is the frit's text font,
    /// its first regular source. When [`Engine::load`] left the text font
    /// out, because this machine does not have it or cannot read it, the
    /// built-in font takes its place and the frit has no own cell, so the
    /// console takes the cell of its configuration, as for a console with
    /// no font named, and a later source, such as a layer's font, never
    /// sets the cell. A text font that has failed as it drew is left out
    /// for the engine's life, as it is for glyphs, and the next source
    /// sets the cell. A bitmap first
    /// source gives its own cell, once every later bitmap source's cell
    /// fits in it, so each draws at a whole factor (the operator,
    /// 2026-10-28). An outline first source gives the cell `height` pixels
    /// high that the W rule gives it (the maintainer, 2026-11-01); the
    /// console names the height, and at a height of 0 there is none. An outline
    /// source whose W rule fails fails as a source that fails to draw does.
    /// Else why the frit has none: a missing text font, a later bitmap
    /// source whose cell does not fit, no height to draw an outline font
    /// at, or no source left.
    ///
    /// References:
    /// - ADR-1204 D5: a missing text font leaves the cell to the
    ///   configuration, and a layer's font never sets the cell.
    pub fn own_cell(&self, height: u32) -> Result<OwnCell, String> {
        if let Some(text) = &self.text_left_out {
            return Err(format!(
                "the text font source={text} is missing, so the built-in font takes its place, and the cell comes from the configuration"
            ));
        }
        let mut why = Vec::new();
        let mut sources = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, (source, _))| source.style == REGULAR);
        let (first, cell) = loop {
            let Some((i, (source, loaded))) = sources.next() else {
                why.push("the frit has no source left to take a cell from".to_owned());
                return Err(why.join("; "));
            };
            if let Some(failed) = self.failed.borrow().get(&i) {
                why.push(format!("source={}: {failed}", source.spelling));
                continue;
            }
            match loaded {
                Loaded::Bitmap(font) => break (source, font.cell),
                Loaded::Outline { .. } if height == 0 => {
                    return Err(format!(
                        "source={} is an outline font, whose cell needs the height it is drawn at",
                        source.spelling
                    ));
                }
                Loaded::Outline { font, family, .. } => {
                    // the frit's bold face joins the W rule (ADR-1200 D17)
                    let mut faces: Vec<(&[u8], &str)> = vec![(font, family)];
                    faces.extend(self.bold_face());
                    match crate::outline::cell_width(&faces, height) {
                        Ok(width) => return Ok(OwnCell::Outline(Cell { width, height })),
                        Err(e) => {
                            why.push(format!("source={}: {e}", source.spelling));
                            self.failed.borrow_mut().insert(i, e);
                        }
                    }
                }
            }
        };
        let first_at = self
            .sources
            .iter()
            .position(|(s, _)| std::ptr::eq(s, first))
            .unwrap_or(0);
        for (_, (source, loaded)) in self.sources.iter().enumerate().skip(first_at + 1) {
            match loaded {
                Loaded::Bitmap(font)
                    if font.cell.width > cell.width || font.cell.height > cell.height =>
                {
                    return Err(format!(
                        "source={}'s {}x{} cell does not fit source={}'s {}x{}",
                        source.spelling,
                        font.cell.width,
                        font.cell.height,
                        first.spelling,
                        cell.width,
                        cell.height
                    ));
                }
                Loaded::Bitmap(_) | Loaded::Outline { .. } => {}
            }
        }
        Ok(OwnCell::Bitmap(cell))
    }

    /// The frit's bold face, which the W rule measures beside the regular
    /// face (ADR-1200 D17), so a bold face wider than the regular one
    /// still fits the cell: the first outline source the frit gave
    /// `style=bold` that has not failed, its bytes and its family.
    /// `None` when the frit names none.
    fn bold_face(&self) -> Option<(&[u8], &str)> {
        let failed = self.failed.borrow();
        self.sources
            .iter()
            .enumerate()
            .filter(|(i, (source, _))| source.style == BOLD && !failed.contains_key(i))
            .find_map(|(_, (_, loaded))| match loaded {
                Loaded::Outline { font, family, .. } => Some((&font[..], family.as_str())),
                Loaded::Bitmap(_) => None,
            })
    }

    /// The size, in pixels to the em, source `i` draws at in a cell
    /// `height` pixels high when it is an outline source after an outline
    /// first source: the first source's size (ADR-1200 D17). The first
    /// source is the frit's first regular one, the one whose metrics every
    /// strike reports ([`Engine::metrics_at`]). `None` draws the source at
    /// its own size, so its ascent and descent span the height, as the
    /// first source itself and every source behind a bitmap first source
    /// do.
    fn size(&self, i: usize, height: u32) -> Option<f32> {
        let first = self.sources.iter().position(|(s, _)| s.style == REGULAR)?;
        if i == first {
            return None;
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(first) else {
            return None;
        };
        crate::outline::size(font, family, height).ok()
    }

    /// Source `i`'s baseline in a cell `height` pixels high, found once for
    /// the engine's life (ADR-1302 D3); `None` for a bitmap source or one
    /// whose font cannot be read, which the rasteriser then refuses. An
    /// outline source that draws at the first source's size
    /// ([`Engine::size`]) draws on the first source's baseline too
    /// (ADR-1200 D17), so a fallback font whose ascent reaches higher than
    /// the first source's, as Noto Sans Hebrew's and Noto Sans Arabic's
    /// reach above DejaVu Sans Mono's, stands on the same row as the first
    /// source's letters. Any other outline source stands on its own
    /// baseline.
    fn baseline(&self, i: usize, height: u32) -> Option<u32> {
        if let Some(&b) = self.baselines.borrow().get(&(i, height)) {
            return Some(b);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        if self.size(i, height).is_some() {
            let first = self.sources.iter().position(|(s, _)| s.style == REGULAR)?;
            return self.baseline(first, height);
        }
        let b = crate::outline::baseline(font, family, height, None, self.hinting).ok()?;
        self.baselines.borrow_mut().insert((i, height), b);
        Some(b)
    }

    /// Source `i`'s own baseline in a cell `height` pixels high: where the
    /// source stands when it draws at its own size, its ascent and descent
    /// spanning the cell ([`Engine::fitted`]). Found once for the engine's
    /// life, as [`Engine::baseline`] finds the others.
    fn own_baseline(&self, i: usize, height: u32) -> Option<u32> {
        if self.size(i, height).is_none() {
            return self.baseline(i, height);
        }
        // the cache holds (i, height) for a source that draws at its own
        // size only, so a source at the first source's size keeps its own
        // baseline under a key no other lookup uses
        let key = (i.checked_add(self.sources.len())?, height);
        if let Some(&b) = self.baselines.borrow().get(&key) {
            return Some(b);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        let b = crate::outline::baseline(font, family, height, None, self.hinting).ok()?;
        self.baselines.borrow_mut().insert(key, b);
        Some(b)
    }

    /// Whether source `i`'s glyph for `c` is a merged symbol, fitted to the
    /// first source's cell: `c` is a private-use character, where Nerd
    /// Fonts and Powerline put their symbols, and source `i` is an outline
    /// source with `cover=` lines that would otherwise draw at the first
    /// source's size ([`Engine::size`]). Such a glyph draws at the source's
    /// own size, at which the source's ascent and descent span the cell,
    /// and on the source's own baseline at that size. A symbol font made
    /// for merging, such as Symbols Nerd Font Mono, lays out its symbols
    /// in that box, so they fill the cell's height as the font means them
    /// to: a Powerline separator runs from the cell's top to its bottom,
    /// and an icon stands in the middle of the cell. A symbol whose ink
    /// still passes its cells is drawn smaller, as any later source's
    /// glyph is.
    ///
    /// Letters a `cover=` source gives, such as the ideographs of
    /// `cover=gb2312`, keep the first source's size and baseline, so every
    /// script on a line shares the first source's baseline.
    ///
    /// References:
    /// - ADR-1200 D17: later sources draw at the first source's size, on its baseline.
    /// - ADR-1302 D1: font merging fits a merged symbol before ink reaches past its cell.
    fn fitted(&self, i: usize, c: u32) -> bool {
        if !private_use(c) {
            return false;
        }
        let first = self.sources.iter().position(|(s, _)| s.style == REGULAR);
        let outline = |i: usize| matches!(self.sources.get(i), Some((_, Loaded::Outline { .. })));
        let merged = self
            .sources
            .get(i)
            .is_some_and(|(source, _)| !source.cover.is_empty());
        merged && first.is_some_and(|first| first != i && outline(first)) && outline(i)
    }

    /// The baseline and the underline and strike-through a strike at
    /// `cell` reports (ADR-1302 D3, ADR-1200 D2): those of the frit's
    /// first regular source, the font that sets the cell, for an outline
    /// font, so every answer at one cell reports the same ones even after
    /// that source fails as it draws. A bitmap first source reports the
    /// lines of a face with no metrics of its own, and the baseline it is
    /// sheared about ([`bitmap_baseline`]) as its glyphs sit in the cell,
    /// scaled and on the cell's bottom row, so a reader that shears its
    /// glyphs for italic (vtium-font's set reader) leans them as the helper
    /// does. One whose font cannot be read, or that fits the cell at no
    /// whole factor, reports the cell's bottom row.
    /// The strike that source `i` draws from in a text band `cell`: the
    /// tallest of its strikes that fits the cell, when the cell is no
    /// taller than the tallest strike. Above that height, and in a cell
    /// that no strike fits, the source draws its outline. A character the
    /// strike lacks draws from the outline at every height.
    ///
    /// References:
    /// - ADR-1204 D2: the largest strike that fits a cell up to the
    ///   tallest strike's height, and the outline above it.
    fn strike_at(&self, i: usize, cell: Cell) -> Option<&Loaded> {
        let strikes = self.strikes.get(i)?;
        let height = |l: &Loaded| match l {
            Loaded::Bitmap(font) => Some(font.cell),
            Loaded::Outline { .. } => None,
        };
        let tallest = strikes.iter().filter_map(height).map(|c| c.height).max()?;
        if cell.height > tallest {
            return None;
        }
        strikes
            .iter()
            .filter(|l| height(l).is_some_and(|c| c.width <= cell.width && c.height <= cell.height))
            .max_by_key(|l| height(l).map_or(0, |c| c.height))
    }

    /// The frit's first regular source, the one that sets the cell, as it
    /// draws in a text band `cell`: its place in `sources`, and its strike
    /// at that cell where it has one ([`Engine::strike_at`]), else the
    /// source itself. Its baseline and lines are the strike's where the
    /// strike draws, as a bitmap first source's are.
    fn first_drawn(&self, cell: Cell) -> Option<(usize, &Loaded)> {
        let first = self.sources.iter().position(|(s, _)| s.style == REGULAR)?;
        let (_, own) = self.sources.get(first)?;
        Some((first, self.strike_at(first, cell).unwrap_or(own)))
    }

    /// The first regular source's own box in a text band `cell` (ADR-1202
    /// D1), as its first row and its rows: a bitmap source's cell at the
    /// whole factor it draws at, on the band's bottom row; an outline
    /// source's rows from its ascent to its descent about its baseline;
    /// else the whole band.
    fn own_box(&self, cell: Cell) -> (u32, u32) {
        let band = (0, cell.height);
        let Some((first, drawn)) = self.first_drawn(cell) else {
            return band;
        };
        match drawn {
            Loaded::Bitmap(font) => {
                let scale = (cell.width / font.cell.width.max(1))
                    .min(cell.height / font.cell.height.max(1));
                let tall = font.cell.height * scale;
                if scale == 0 {
                    return band;
                }
                (cell.height - tall, tall)
            }
            Loaded::Outline { font, family, .. } => {
                let (Some(base), Ok((above, below))) = (
                    self.baseline(first, cell.height),
                    crate::outline::own_box(font, family, cell.height, None),
                ) else {
                    return band;
                };
                let top = base.saturating_sub(above);
                (top, (base + below).min(cell.height) - top)
            }
        }
    }

    fn metrics_at(&self, cell: Cell) -> (u32, Decorations) {
        let plain = (cell.height, Decorations::for_height(cell.height));
        let Some((first, drawn)) = self.first_drawn(cell) else {
            return plain;
        };
        if let Loaded::Bitmap(font) = drawn {
            let scale =
                (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
            if scale == 0 {
                return plain;
            }
            let top = cell.height - font.cell.height * scale;
            return (top + bitmap_baseline(font) * scale, plain.1);
        }
        let Loaded::Outline { font, family, .. } = drawn else {
            return plain;
        };
        let Some(base) = self.baseline(first, cell.height) else {
            return plain;
        };
        let lines =
            crate::outline::decorations(font, family, (cell.height, base), None, self.hinting)
                .unwrap_or(plain.1);
        (base, lines)
    }

    /// The lines of a cell in `script` at `cell`, the text band, whose
    /// normal glyphs stand on row `base` with the lines `normal`
    /// ([`Engine::metrics_at`]): the frit's first source's, found by its
    /// kind (ADR-1202 D8 step 3). An outline source's are measured at the
    /// script size from the script baseline ([`script_baseline`]), with the
    /// overline on the top row of the small own box. A bitmap source with a
    /// smaller size in D3's range takes that size's lines, the lines of its
    /// height at the multiple it is drawn at, placed whole against the
    /// band; one with none takes its normal lines through D4's map. Both
    /// come from vtium-font's script rule ([`vtium_font::script::lines`]),
    /// which the console's own fonts take too. The baseline's lines are
    /// `normal`.
    fn script_lines(
        &self,
        cell: Cell,
        script: Script,
        (base, normal): (u32, Decorations),
    ) -> Decorations {
        use vtium_font::script::lines;
        if script == Script::Baseline {
            return normal;
        }
        let own = self.own_box(cell);
        let mapped = lines::mapped(normal, script, own, cell.height, Band::NONE);
        let Some((first, drawn)) = self.first_drawn(cell) else {
            return mapped;
        };
        match drawn {
            Loaded::Bitmap(font) => {
                let scale = (cell.width / font.cell.width.max(1))
                    .min(cell.height / font.cell.height.max(1));
                let others = self.smaller.get(first).map_or(&[][..], Vec::as_slice);
                let strikes: Vec<Cell> = std::iter::once(font.cell)
                    .chain(others.iter().map(|f| f.font.cell))
                    .collect();
                let chosen =
                    font.cell.height.checked_mul(scale).and_then(|normal| {
                        vtium_font::script::smaller(normal, cell.width, &strikes)
                    });
                let Some(chosen) = chosen else {
                    return mapped;
                };
                let Some(&own) = strikes.get(chosen.strike) else {
                    return mapped;
                };
                let j = chosen.multiple;
                let size = (own.width * j, own.height * j);
                let at = lines::at_multiple(Decorations::for_height(own.height), own.height, j);
                lines::placed(at, script, size, (cell.width, cell.height), Band::NONE)
            }
            Loaded::Outline { font, family, .. } => {
                let Ok(size) = crate::outline::size(font, family, cell.height) else {
                    return mapped;
                };
                let (num, den) = vtium_font::SCRIPT_SCALE;
                let small = size * num as f32 / den as f32;
                let baseline = script_baseline(script, own, base, cell.height);
                let measured = crate::outline::decorations(
                    font,
                    family,
                    (cell.height, baseline),
                    Some(small),
                    self.hinting,
                );
                let Ok(measured) = measured else {
                    return mapped;
                };
                let (_, tall) = vtium_font::script::size((1, own.1));
                let (_, top) =
                    vtium_font::script::place(script, (1, tall), (1, cell.height), Band::NONE);
                Decorations {
                    overline: top.min(cell.height.saturating_sub(measured.underline_thickness)),
                    ..measured
                }
            }
        }
    }

    /// Why no source gives a glyph at `cell`: each bitmap source whose own
    /// cell does not fit it, and each source that failed as it drew.
    pub fn none_at(&self, cell: Cell) -> String {
        let failed = self.failed.borrow();
        let unfit: Vec<String> = self
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, (source, loaded))| match (failed.get(&i), loaded) {
                (Some(why), _) => Some(format!("source={}: {why}", source.spelling)),
                (None, Loaded::Bitmap(f))
                    if f.cell.width > cell.width || f.cell.height > cell.height =>
                {
                    Some(format!(
                        "source={}: its {}x{} cell does not fit",
                        source.spelling, f.cell.width, f.cell.height
                    ))
                }
                _ => None,
            })
            .collect();
        let mut why = format!("the frit gives no glyph at {}x{}", cell.width, cell.height);
        if !unfit.is_empty() {
            why.push_str(": ");
            why.push_str(&unfit.join("; "));
        }
        why
    }

    /// The most bytes one glyph at `cell` can take: two cells of
    /// premultiplied RGBA when a source is an outline font, which may draw
    /// in color, else two cells of 1-bit rows.
    fn glyph_bytes_max(&self, cell: Cell) -> usize {
        let outline = self
            .sources
            .iter()
            .any(|(_, l)| matches!(l, Loaded::Outline { .. }));
        let (w, h) = (cell.width.saturating_mul(2), cell.height);
        let bytes = if outline {
            w.saturating_mul(h).saturating_mul(4)
        } else {
            w.div_ceil(8).saturating_mul(h)
        };
        usize::try_from(bytes).unwrap_or(usize::MAX)
    }

    /// The regular glyphs at `cell` for `want`, sorted codepoints, as
    /// [`Engine::glyphs`] draws them.
    pub fn draw(&self, cell: Cell, want: &[u32]) -> Result<Option<Vec<u8>>, String> {
        let mut made = BTreeMap::new();
        self.glyphs(
            cell,
            Band::NONE,
            (REGULAR.into(), None),
            want,
            (&[], false),
            &mut made,
        )
    }

    /// The regular glyphs at `cell` for `clusters`, sorted, as
    /// [`Engine::glyphs`] draws them.
    pub fn draw_clusters(
        &self,
        cell: Cell,
        clusters: &[Vec<u32>],
    ) -> Result<Option<Vec<u8>>, String> {
        let mut made = BTreeMap::new();
        self.glyphs(
            cell,
            Band::NONE,
            (REGULAR.into(), None),
            &[],
            (clusters, false),
            &mut made,
        )
    }

    /// The glyphs of `style` at `at`, a cell and the rows its line spacing
    /// adds (a plain cell adds none), for `want`, sorted codepoints, and
    /// `clusters`, sorted, as [`Engine::glyphs`] draws them, with the parts
    /// of clusters where `parts` says, as an [`ASK_PARTS`] gets them.
    pub fn draw_style(
        &self,
        at: impl Into<SpacedCell>,
        style: Style,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_variant(at, style.into(), want, (clusters, parts))
    }

    /// The glyphs of `variant`, a style and a script, at `at`, as
    /// [`Engine::draw_style`] draws a style's: a superscript or subscript
    /// ask's answer (ADR-1202 D11).
    pub fn draw_variant(
        &self,
        at: impl Into<SpacedCell>,
        variant: Variant,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_made(at, variant, want, (clusters, parts))
            .map(|(set, _)| set)
    }

    /// The glyphs of `variant` at `at` for `want` and `clusters`, as
    /// [`Engine::draw_variant`] draws them, and how the engine made each
    /// superscript or subscript glyph of a codepoint it drew, by the
    /// codepoint ([`Made`], ADR-1202 D14): none for normal glyphs, and none
    /// for clusters.
    pub fn draw_made(
        &self,
        at: impl Into<SpacedCell>,
        variant: Variant,
        want: &[u32],
        (clusters, parts): (&[Vec<u32>], bool),
    ) -> Result<MadeSet, String> {
        let at = at.into();
        let mut made = BTreeMap::new();
        let set = self.glyphs(
            at.cell,
            at.band,
            (variant, None),
            want,
            (clusters, parts),
            &mut made,
        )?;
        Ok((set, made))
    }

    /// The glyphs of `variant` at `at` for `want`, sorted codepoints, drawn
    /// at the double size `doubled`, as a double ask's answer
    /// ([`ServeRequest::doubled`]): a set whose strikes are at the cell
    /// [`Doubled::cell`] gives for `at`'s cell.
    pub fn draw_doubled(
        &self,
        at: impl Into<SpacedCell>,
        kind: (Variant, Doubled),
        want: &[u32],
    ) -> Result<Option<Vec<u8>>, String> {
        self.draw_doubled_clusters(at, kind, want, &[])
    }

    /// [`Engine::draw_doubled`] for `want` and for `clusters`, sorted, as a
    /// double ask that names clusters is answered: each cluster shaped
    /// whole at the double size, its color glyphs among them.
    pub fn draw_doubled_clusters(
        &self,
        at: impl Into<SpacedCell>,
        (variant, doubled): (Variant, Doubled),
        want: &[u32],
        clusters: &[Vec<u32>],
    ) -> Result<Option<Vec<u8>>, String> {
        let at = at.into();
        let mut made = BTreeMap::new();
        self.glyphs(
            at.cell,
            at.band,
            (variant, Some(doubled)),
            want,
            (clusters, false),
            &mut made,
        )
    }

    /// One note for each `script-source=` font that gives no superscript
    /// or subscript in the frit's own cell (ADR-1202 D12): the font whose
    /// height, at no whole multiple that fits the cell, lies from one half
    /// to three quarters of the height its source draws at there. The own
    /// cell is the frit's `cell=`, else its first source's when that is a
    /// bitmap font; a frit whose own cell needs a height has no notes. Each
    /// note names the frit and the line, as a warning does.
    pub fn script_notes(&self) -> Vec<String> {
        let own = self.cell.or_else(|| match self.own_cell(0) {
            Ok(OwnCell::Bitmap(cell)) => Some(cell),
            _ => None,
        });
        let Some(cell) = own else {
            return Vec::new();
        };
        let mut notes = Vec::new();
        for ((_, loaded), smaller) in self.sources.iter().zip(&self.smaller) {
            let Loaded::Bitmap(font) = loaded else {
                continue;
            };
            let scale =
                (cell.width / font.cell.width.max(1)).min(cell.height / font.cell.height.max(1));
            let normal = font.cell.height * scale;
            for small in smaller {
                let Some((line, spelling)) = &small.named else {
                    continue;
                };
                if scale > 0
                    && vtium_font::script::smaller(normal, cell.width, &[small.font.cell]).is_some()
                {
                    continue;
                }
                let lowest = normal.div_ceil(2);
                let highest = normal * 3 / 4;
                notes.push(format!(
                    "{}:{line}: script-source={spelling} gives no superscripts or subscripts in the font set's own {}x{} character cell. Its characters are {} pixels high, and there a smaller size must be from {lowest} to {highest} pixels high, from half to three quarters of the {normal}-pixel characters of the font above it.",
                    self.name, cell.width, cell.height, small.font.cell.height
                ));
            }
        }
        notes
    }

    /// The glyph that a smaller size of bitmap source `i` gives `c` for a
    /// superscript or subscript (ADR-1202 D3): the strike that
    /// [`vtium_font::script::smaller`] chooses among the source's own
    /// strike, `font` drawn at `scale`, and its smaller sizes, for a normal
    /// glyph `font`'s height times `scale` and cells `cell_width` across.
    /// The source's own strike at a smaller multiple draws `c` as its glyph
    /// `g`. `None` when no strike lies in the range, and when the chosen
    /// strike lacks `c` or draws it at another width than `g`; the glyph
    /// is then made small by D4. Only the source's own font and the
    /// smaller sizes it names are asked (D15).
    fn smaller_glyph<'a>(
        &'a self,
        i: usize,
        (font, scale): (&'a Font, u32),
        cell_width: u32,
        c: u32,
        g: u32,
    ) -> Option<SmallerGlyph<'a>> {
        let others = self.smaller.get(i).map_or(&[][..], Vec::as_slice);
        let strikes: Vec<Cell> = std::iter::once(font.cell)
            .chain(others.iter().map(|f| f.font.cell))
            .collect();
        let normal = font.cell.height.checked_mul(scale)?;
        let chosen = vtium_font::script::smaller(normal, cell_width, &strikes)?;
        let f = match chosen.strike {
            0 => font,
            n => &others.get(n - 1)?.font,
        };
        let glyph = match chosen.strike {
            0 => g,
            _ => *f.map.get(&c)?,
        };
        let (width, rows) = f.glyphs.get(glyph as usize)?;
        let (own, _) = font.glyphs.get(g as usize)?;
        (width == own).then_some(SmallerGlyph {
            strike: chosen.strike,
            glyph,
            multiple: chosen.multiple,
            font: f,
            rows,
        })
    }

    /// How a script glyph that a smaller size `small` of bitmap source `i`
    /// gives was made, for a source drawn at `scale` (ADR-1202 D3, D14):
    /// the source's own font at a smaller multiple, another height of a
    /// CPI source, or a `script-source=` font, named by its spelling.
    fn made_smaller(&self, i: usize, scale: u32, small: SmallerGlyph<'_>) -> Made {
        let (height, multiple) = (small.font.cell.height, small.multiple);
        let named = small
            .strike
            .checked_sub(1)
            .and_then(|n| self.smaller.get(i)?.get(n))
            .map(|f| f.named.as_ref());
        match named {
            None => Made::Multiple {
                normal: scale,
                small: multiple,
            },
            Some(None) => Made::Height { height, multiple },
            Some(Some((_, spelling))) => Made::Named {
                spelling: spelling.clone(),
                height,
                multiple,
            },
        }
    }

    /// How the glyph that source `source` gives for `c` uses a row made
    /// taller by line spacing (ADR-1302 D13): the `fit=` of the source's
    /// set that holds `c`, else `c`'s default ([`default_fit`]).
    fn fit(source: &Source, c: u32) -> Fit {
        source.fit(c).unwrap_or_else(|| default_fit(c))
    }

    /// `request`, for source `i`'s normal glyphs at `cell` in the text band,
    /// asking for its glyphs in `script` instead (ADR-1202 D1, D2, D5,
    /// D8): drawn at two thirds of the size its normal glyphs are drawn at
    /// ([`vtium_font::SCRIPT_SCALE`]), rasterised at that size rather than
    /// scaled from a full-size raster, on the script baseline that the
    /// first source's own box ([`Engine::own_box`]) and its baseline `base`
    /// give ([`script_baseline`]), with no
    /// ink above or below the whole cell. `None` when the size the source
    /// draws at cannot be read.
    fn script_request(
        &self,
        request: OutlineRequest,
        (i, fitted): (usize, bool),
        (cell, band, base): (Cell, Band, u32),
        script: Script,
    ) -> Option<OutlineRequest> {
        let size = self.drawn_size((i, fitted), cell.height)?;
        let (num, den) = vtium_font::SCRIPT_SCALE;
        let small = size * num as f32 / den as f32;
        Some(OutlineRequest {
            cluster_width: Default::default(),
            sizes: vec![small.to_bits()],
            baselines: vec![script_baseline(
                script,
                self.own_box(cell),
                base,
                cell.height,
            )],
            reaches: vec![(band.above, band.below)],
            script,
            ..request
        })
    }

    /// The size, in pixels to the em, outline source `i` draws its normal
    /// glyphs at in a text band `height` pixels high: the first source's
    /// size for a later source ([`Engine::size`]), and its own size for the
    /// first source and for a merged symbol (`fitted`, [`Engine::fitted`]).
    /// `None` for a bitmap source, or one whose font cannot be read.
    fn drawn_size(&self, (i, fitted): (usize, bool), height: u32) -> Option<f32> {
        if let Some(size) = self.size(i, height).filter(|_| !fitted) {
            return Some(size);
        }
        let Some((_, Loaded::Outline { font, family, .. })) = self.sources.get(i) else {
            return None;
        };
        crate::outline::size(font, family, height).ok()
    }

    /// `request`, source `i`'s request at the text band `cell`, drawn at
    /// the double size `doubled` instead. The glyphs take the size and the
    /// baseline that the normal cell gives them. On a line of double height
    /// both are twice as large. On a line of double width the request
    /// stretches each outline to twice its width
    /// ([`OutlineRequest::double_width`]). Each glyph is then the normal
    /// glyph at twice its width, and twice its height on a line of double
    /// height, rendered from its outlines at that size. `None` when the
    /// source has no size or no baseline at the cell.
    fn doubled_request(
        &self,
        request: OutlineRequest,
        (i, fitted): (usize, bool),
        cell: Cell,
        doubled: Doubled,
    ) -> Option<OutlineRequest> {
        let rows = doubled.rows();
        let size = self.drawn_size((i, fitted), cell.height)? * rows as f32;
        let baseline = match fitted {
            true => self.own_baseline(i, cell.height),
            false => self.baseline(i, cell.height),
        }?;
        Some(OutlineRequest {
            sizes: vec![size.to_bits()],
            baselines: vec![baseline.checked_mul(rows)?],
            ..request
        })
    }

    /// The glyphs of `variant` at `cell` for `want`, sorted codepoints, and
    /// `clusters`, sorted, as a vitrified set of that one cell whose
    /// strikes are of the variant's style and script; `None` when no source
    /// gives any. A superscript or subscript glyph comes from the source
    /// the steps pick for the normal glyph (ADR-1202 D15), made small by
    /// that source's kind ([`Engine::script_request`],
    /// [`script_bitmap`]). Where line
    /// spacing adds `band` to the cell, each glyph is drawn in the cell's
    /// text band and placed in the whole cell ([`spaced`]). Each
    /// [`Step`] of the style ([`steps`]) draws in turn what the steps
    /// before it left: each codepoint from the first source in the frit's
    /// order that the step's face is and that gives it, and each cluster
    /// from the first such outline source that covers its first scalar
    /// value other than U+200D and shapes it (ADR-1200 D21), with the
    /// step's synthesis drawn over the glyph (ADR-1302 D4). A codepoint or
    /// cluster that asks for emoji presentation
    /// ([`crate::shaping::asks_for_emoji`]) is first drawn, by the same
    /// order, from the first source that gives it a color glyph, and only
    /// when none does from the first that gives it any (ADR-1200 D21). A source that
    /// fails as it draws is left out, and the next one gives its glyphs.
    /// Where `parts_asked`, as for an [`ASK_PARTS`], the set also holds the
    /// part of a cluster asked for that a source draws only without some of
    /// its marks, under the part's own key (ADR-1200 D22 step 3). Each
    /// superscript or subscript glyph of a codepoint goes into `made` with
    /// the way the engine made it (ADR-1202 D14).
    fn glyphs(
        &self,
        cell: Cell,
        band: Band,
        (variant, doubled): (Variant, Option<Doubled>),
        want: &[u32],
        (clusters, parts_asked): (&[Vec<u32>], bool),
        made: &mut BTreeMap<u32, Made>,
    ) -> Result<Option<Vec<u8>>, String> {
        let Variant { style, script } = variant;
        // the glyphs are drawn in the text band, and the strike's baseline
        // and lines move down by the rows added above it
        let whole = cell;
        let cell = Cell {
            width: whole.width,
            height: whole
                .height
                .checked_sub(band.added())
                .filter(|&h| h > 0)
                .ok_or("line spacing that leaves the cell no text band")?,
        };
        let (base, lines) = self.metrics_at(cell);
        // a bitmap source's glyphs stand on this baseline in the text band
        let text_base = base;
        // a superscript or subscript strike reports the lines of its own
        // size, where its glyphs lie (ADR-1202 D8); a normal strike's
        // overline stays on the cell's top row
        let script_lines = self.script_lines(cell, script, (base, lines));
        let (base, lines) = (
            base.saturating_add(band.above),
            Decorations {
                underline: script_lines.underline.saturating_add(band.above),
                strike: script_lines.strike.saturating_add(band.above),
                overline: match script {
                    Script::Baseline => 0,
                    _ => script_lines.overline.saturating_add(band.above),
                },
                ..script_lines
            },
        );
        // a double ask draws each glyph in the cell its size gives, and its
        // strikes report the normal cell's baseline and lines at that size;
        // the sources are measured at the normal cell all the same
        if doubled.is_some() && script != Script::Baseline {
            return Err("a superscript or subscript at a double size".to_owned());
        }
        let rows = doubled.map_or(1, Doubled::rows);
        let (drawn, drawn_whole, drawn_band) = match doubled {
            None => (cell, whole, band),
            Some(d) => (
                d.cell(cell).ok_or("a double size past the largest cell")?,
                d.cell(whole).ok_or("a double size past the largest cell")?,
                d.band(band),
            ),
        };
        let (base, lines) = (
            base.saturating_mul(rows),
            Decorations {
                underline: lines.underline.saturating_mul(rows),
                underline_thickness: lines.underline_thickness.saturating_mul(rows),
                strike: lines.strike.saturating_mul(rows),
                strike_thickness: lines.strike_thickness.saturating_mul(rows),
                overline: lines.overline.saturating_mul(rows),
            },
        );
        let strike = |format| {
            StrikeBuilder::new(format, drawn_whole, style)
                .script(script)
                .baseline(base)
                .decorations(lines)
                .compress(false)
        };
        let (mut bits, mut r8, mut rgba) = (
            strike(PixelFormat::Bits),
            strike(PixelFormat::R8),
            strike(PixelFormat::Rgba),
        );
        let mut mapped: BTreeSet<u32> = BTreeSet::new();
        // color glyphs in one cell held back for a text glyph from a later
        // source, the first source's for each character
        let mut in_one_cell: Vec<(char, Placed, Fit)> = Vec::new();
        let mut shaped: BTreeSet<&[u32]> = BTreeSet::new();
        let mut parts: BTreeSet<Vec<u32>> = BTreeSet::new();
        let steps = steps(style);
        // an outline source with a strike at this cell draws the strike's
        // glyphs first, and its outline the characters the strike lacks
        // (ADR-1204 D2)
        let order = steps.iter().flat_map(|step| {
            self.sources
                .iter()
                .enumerate()
                .filter(move |(_, (source, _))| source.style == step.face)
                .flat_map(move |(i, (source, loaded))| {
                    self.strike_at(i, cell)
                        .map(|strike| (i, (source, strike), step.synth))
                        .into_iter()
                        .chain(std::iter::once((i, (source, loaded), step.synth)))
                })
        });
        // what asks for emoji presentation goes first to the sources in
        // order that give it a color glyph, then everything left to the
        // sources in order (ADR-1200 D21)
        let passes = [true, false]
            .into_iter()
            .flat_map(|color| order.clone().map(move |source| (color, source)));
        // a merged symbol font's private-use symbols are drawn in a request
        // of their own, at the font's own size ([`Engine::fitted`]), before
        // the rest of what the source gives
        let passes = passes.flat_map(|pass| [true, false].map(|fitted| (pass, fitted)));
        // what a double ask leaves to the console, which doubles the pixels
        // of its glyph at the normal size: the codepoints a bitmap source
        // draws; and on a line of double width, the codepoints a color
        // glyph draws, which `squashed` holds and which are drawn again
        // below from a line of double height
        let mut left_out: BTreeSet<u32> = BTreeSet::new();
        let mut squashed: BTreeSet<u32> = BTreeSet::new();
        let wide_line = doubled == Some(Doubled::Width);
        for ((color_only, (i, (source, loaded), synth)), fitted) in passes {
            if self.failed.borrow().contains_key(&i) {
                continue;
            }
            let asked = |k: &[u32]| !color_only || crate::shaping::asks_for_emoji(k);
            let left: Vec<u32> = want
                .iter()
                .copied()
                .filter(|&c| {
                    !mapped.contains(&c)
                        && !left_out.contains(&c)
                        && source.covers(c)
                        && asked(&[c])
                        && self.fitted(i, c) == fitted
                })
                .collect();
            let lead = |k: &[u32]| k.iter().copied().find(|&c| c != ZWJ);
            // a cluster is shaped whole with the rest of the source's glyphs
            let unshaped: Vec<&[u32]> = clusters
                .iter()
                .map(Vec::as_slice)
                .filter(|k| {
                    !fitted
                        && !wide_line
                        && !shaped.contains(k)
                        && lead(k).is_some_and(|c| source.covers(c))
                        && asked(k)
                })
                .collect();
            match loaded {
                // a bitmap font has no color glyph
                Loaded::Bitmap(_) if left.is_empty() || color_only => {}
                Loaded::Bitmap(font) => {
                    let scale = (cell.width / font.cell.width.max(1))
                        .min(cell.height / font.cell.height.max(1));
                    if scale == 0 {
                        continue;
                    }
                    // a bitmap font's glyph doubles its pixels on a line of
                    // double size, which the console does
                    if doubled.is_some() {
                        left_out.extend(left.iter().filter(|c| font.map.contains_key(c)));
                        continue;
                    }
                    let baseline = bitmap_baseline(font);
                    // a glyph's ink may use the rows spacing adds and then a
                    // row of the whole cell (ADR-1306 D1, D8)
                    let reach = (
                        band.above
                            .saturating_add(whole.height.saturating_mul(REACH_ROWS)),
                        band.below
                            .saturating_add(whole.height.saturating_mul(REACH_ROWS)),
                    );
                    // each glyph drawn, by the strike it comes from (0 for
                    // the source's own, else a smaller size's place, 1 on),
                    // its glyph there and its fit: whether it went to the R8
                    // strike rather than the 1-bit one, and its number there
                    let mut placed: BTreeMap<(usize, u32, Fit), (bool, u32)> = BTreeMap::new();
                    for c in left {
                        let (Some(&g), Some(ch)) = (font.map.get(&c), char::from_u32(c)) else {
                            continue;
                        };
                        let fit = Engine::fit(source, c);
                        // a script glyph comes from a smaller size of the
                        // source where one lies in D3's range
                        let small = match script {
                            Script::Baseline => None,
                            _ => self.smaller_glyph(i, (font, scale), cell.width, c, g),
                        };
                        let how = match (script, small) {
                            (Script::Baseline, _) => None,
                            (_, Some(s)) => Some(self.made_smaller(i, scale, s)),
                            (_, None) => Some(Made::Scaled),
                        };
                        let key = match small {
                            Some(s) => (s.strike, s.glyph, fit),
                            None => (0, g, fit),
                        };
                        let (gray, index) = match placed.get(&key) {
                            Some(&at) => at,
                            None => {
                                let Some((width, rows)) = font.glyphs.get(g as usize) else {
                                    continue;
                                };
                                let span = cell.width * width.cells();
                                let drawn = match small {
                                    // the smaller size's glyph whole, in its
                                    // style, against the text band, its ink
                                    // cut to the row (ADR-1202 D3, D7, D8)
                                    Some(s) => {
                                        let f = s.font;
                                        let size = (
                                            f.cell.width * width.cells() * s.multiple,
                                            f.cell.height * s.multiple,
                                        );
                                        let (_, top) = vtium_font::script::place(
                                            script,
                                            size,
                                            (span, cell.height),
                                            Band::NONE,
                                        );
                                        Some(place_styled(
                                            s.rows,
                                            *width,
                                            f.cell,
                                            (cell, i64::from(top)),
                                            s.multiple,
                                            synth,
                                            (bitmap_baseline(f), (0, 0)),
                                        ))
                                    }
                                    None => None,
                                };
                                let top = glyph_top(scale, baseline, text_base);
                                // a script glyph is drawn upright and
                                // unstyled, and takes its style once it is
                                // small (ADR-1202 D7)
                                let drawn_synth = match script {
                                    Script::Baseline => synth,
                                    _ => Style::default(),
                                };
                                let (own, ink) = match drawn {
                                    Some(drawn) => drawn,
                                    None => place_styled(
                                        rows,
                                        *width,
                                        font.cell,
                                        (cell, top),
                                        scale,
                                        drawn_synth,
                                        (baseline, reach),
                                    ),
                                };
                                // a script glyph with no smaller size is the
                                // normal glyph in the text band made small,
                                // and keeps no ink past its cells (ADR-1202
                                // D4, D8)
                                let (own, ink) = match script {
                                    Script::Baseline => (own, ink),
                                    _ if small.is_some() => (own, ink),
                                    _ => {
                                        let origin = bitmap_origin(
                                            (font.cell, *width, scale),
                                            (span, cell.height),
                                            (top, baseline),
                                            synth,
                                        );
                                        match script_bitmap(
                                            &own,
                                            (span, cell.height),
                                            *width,
                                            (script, &origin),
                                        ) {
                                            Some(small) => (small, None),
                                            None => continue,
                                        }
                                    }
                                };
                                let Some((own, ink)) = spacing::place_rows(
                                    &own,
                                    (span, cell.height),
                                    vtium_font::Format::Mask,
                                    ink.as_ref(),
                                    (band, fit, true),
                                ) else {
                                    continue;
                                };
                                // a glyph of 0 and 255 alone is one bit; a
                                // script glyph scaled with gray edges is R8
                                if !own.iter().all(|&p| p == 0 || p == 255) {
                                    let i = r8.glyph(*width, own);
                                    placed.insert(key, (true, i));
                                    r8.map(ch, i);
                                    mapped.insert(c);
                                    made.extend(how.map(|how| (c, how)));
                                    continue;
                                }
                                let i = bits.glyph(*width, pack_bits(&own, span));
                                // a box past a set's chunk is not sent, and
                                // the glyph is cut to its cells (ADR-1306 D3)
                                let ink = ink
                                    .map(|b| (pack_bits(&b.pixels, b.width), b))
                                    .filter(|(packed, _)| packed.len() <= SET_CHUNK_BYTES as usize);
                                if let Some((packed, b)) = ink {
                                    bits.ink(i, (b.left, b.top), (b.width, b.height), packed);
                                }
                                placed.insert(key, (false, i));
                                (false, i)
                            }
                        };
                        if gray {
                            r8.map(ch, index);
                        } else {
                            bits.map(ch, index);
                        }
                        mapped.insert(c);
                        made.extend(how.map(|how| (c, how)));
                    }
                }
                Loaded::Outline { .. } if left.is_empty() && unshaped.is_empty() => {}
                Loaded::Outline { font, family, .. } => {
                    // a cluster's scalar values join the cover, as an empty
                    // cover is every codepoint; only those wanted are taken
                    let mut cover: Vec<u32> = left.clone();
                    cover.extend(unshaped.iter().flat_map(|k| k.iter().copied()));
                    cover.retain(|&c| source.covers(c));
                    cover.sort_unstable();
                    cover.dedup();
                    if cover.is_empty() {
                        continue;
                    }
                    let request = OutlineRequest {
                        font: Arc::clone(font),
                        family: family.clone(),
                        cells: vec![(drawn.width, drawn.height)],
                        cover: ranges(&cover),
                        clusters: unshaped.iter().map(|k| crate::shaping::ask(k)).collect(),
                        cluster_width: self.cluster_width.get(),
                        baselines: match fitted {
                            true => self.own_baseline(i, cell.height),
                            false => self.baseline(i, cell.height),
                        }
                        .into_iter()
                        .collect(),
                        sizes: self
                            .size(i, cell.height)
                            .filter(|_| !fitted)
                            .map(f32::to_bits)
                            .into_iter()
                            .collect(),
                        // a glyph's ink may use the rows spacing adds and
                        // then a row of the whole cell (ADR-1306 D1, D8)
                        reaches: vec![(
                            drawn_band
                                .above
                                .saturating_add(drawn_whole.height.saturating_mul(REACH_ROWS)),
                            drawn_band
                                .below
                                .saturating_add(drawn_whole.height.saturating_mul(REACH_ROWS)),
                        )],
                        script: Script::Baseline,
                        hinting: self.hinting,
                        synth,
                        // a fallback source: any source but the first of its
                        // style, the frit's own face for that style, draws
                        // a glyph too big for its cells smaller (ADR-1200 D17)
                        fit: self
                            .sources
                            .iter()
                            .position(|(s, _)| s.style == source.style)
                            != Some(i),
                        double_width: doubled == Some(Doubled::Width),
                    };
                    let request = self.rendered(request, i);
                    let request = match doubled {
                        None => request,
                        Some(d) => match self.doubled_request(request, (i, fitted), cell, d) {
                            Some(request) => request,
                            None => continue,
                        },
                    };
                    let request = match script {
                        Script::Baseline => request,
                        _ => match self.script_request(
                            request,
                            (i, fitted),
                            (cell, band, text_base),
                            script,
                        ) {
                            Some(request) => request,
                            None => continue,
                        },
                    };
                    let outline = match rasterise(&request) {
                        Ok(outline) => outline,
                        Err(why) => {
                            self.failed.borrow_mut().insert(i, why);
                            continue;
                        }
                    };
                    let Some(s) = outline.strikes.first() else {
                        continue;
                    };
                    let mut placed: BTreeMap<(u32, Fit), Option<u32>> = BTreeMap::new();
                    let mut index =
                        |(g, fit): (u32, Fit), target: &mut StrikeBuilder, glyph: &Placed| {
                            *placed.entry((g, fit)).or_insert_with(|| {
                                put_placed(target, glyph, (drawn, drawn_band, fit))
                            })
                        };
                    for &(c, g) in &s.map {
                        let (Some(glyph), Some(ch)) = (s.glyphs.get(g as usize), char::from_u32(c))
                        else {
                            continue;
                        };
                        if mapped.contains(&c)
                            || left.binary_search(&c).is_err()
                            || (color_only && !glyph.color)
                        {
                            continue;
                        }
                        if glyph.color && wide_line {
                            left_out.insert(c);
                            squashed.insert(c);
                            continue;
                        }
                        // a character that presents as text by default
                        // takes a later source's text glyph before a color
                        // glyph in one cell (ADR-1302 D14, step 2)
                        if glyph.color
                            && !glyph.wide
                            && !color_only
                            && script == Script::Baseline
                            && !crate::shaping::emoji_by_default(c)
                        {
                            if !in_one_cell.iter().any(|(d, ..)| *d == ch) {
                                in_one_cell.push((ch, glyph.clone(), Engine::fit(source, c)));
                            }
                            continue;
                        }
                        let target = if glyph.color { &mut rgba } else { &mut r8 };
                        let Some(at) = index((g, Engine::fit(source, c)), target, glyph) else {
                            continue;
                        };
                        target.map(ch, at);
                        mapped.insert(c);
                        if script != Script::Baseline {
                            let how = if glyph.color {
                                Made::Color
                            } else {
                                Made::Outline
                            };
                            made.insert(c, how);
                        }
                    }
                    for (key, g) in &s.clusters {
                        let Some(glyph) = s.glyphs.get(*g as usize) else {
                            continue;
                        };
                        // a cluster asked for, or the part of one the font
                        // draws without the marks it has no glyph for,
                        // which the console lays them over (ADR-1200 D22
                        // step 3); a part goes once, and never under the
                        // key of a cluster asked for
                        let asked = unshaped.iter().find(|k| **k == key.as_slice()).copied();
                        let part = parts_asked
                            && asked.is_none()
                            && !parts.contains(key)
                            && !clusters.contains(key)
                            && unshaped.iter().any(|k| crate::shaping::is_part(key, k));
                        if asked.is_none() && !part {
                            continue;
                        }
                        if color_only && !glyph.color {
                            continue;
                        }
                        let chars: Option<Vec<char>> =
                            key.iter().map(|&c| char::from_u32(c)).collect();
                        let Some(chars) = chars else {
                            continue;
                        };
                        let target = if glyph.color { &mut rgba } else { &mut r8 };
                        let fit = Engine::fit(source, lead(key).unwrap_or_default());
                        let Some(at) = index((*g, fit), target, glyph) else {
                            continue;
                        };
                        target.cluster(&chars, at);
                        match asked {
                            Some(asked) => {
                                shaped.insert(asked);
                            }
                            None => {
                                parts.insert(key.clone());
                            }
                        }
                    }
                }
            }
        }
        for (ch, glyph, fit) in in_one_cell {
            let c = u32::from(ch);
            if mapped.contains(&c) {
                continue;
            }
            if let Some(at) = put_placed(&mut rgba, &glyph, (drawn, drawn_band, fit)) {
                rgba.map(ch, at);
                mapped.insert(c);
            }
        }
        // a line of double width takes its color glyphs and its clusters
        // from a line of double height, each row pair made one row
        let mut halved = false;
        if wide_line && (!squashed.is_empty() || !clusters.is_empty()) {
            let want: Vec<u32> = squashed.into_iter().collect();
            let tall = self.glyphs(
                whole,
                band,
                (variant, Some(Doubled::Height)),
                &want,
                (clusters, parts_asked),
                made,
            )?;
            if let Some(tall) = tall {
                let tall = tall_set(tall, whole)?;
                halved = halve_into(&tall, (&want, variant), (&mut r8, &mut rgba));
            }
        }
        if mapped.is_empty() && shaped.is_empty() && parts.is_empty() && !halved {
            return Ok(None);
        }
        let mut set = SetBuilder::new(&self.name);
        for s in [bits, r8, rgba] {
            if !s.is_empty() {
                set = set.strike(s);
            }
        }
        set.write().map(Some).map_err(|e| e.to_string())
    }
}

/// `glyph`, placed by the helper's outline engine in cells of `cell`,
/// added to `target` placed in a cell taller by `band` as `fit` says
/// (ADR-1302 D13), with its ink box where it has one that fits a set's
/// chunk; a box past a chunk is not sent, and the glyph is cut to its cells
/// (ADR-1306 D3). Gives the glyph's number in `target`.
fn put_placed(
    target: &mut StrikeBuilder,
    glyph: &Placed,
    (cell, band, fit): (Cell, Band, Fit),
) -> Option<u32> {
    let width = if glyph.wide {
        Width::Wide
    } else {
        Width::Narrow
    };
    let format = if glyph.color {
        vtium_font::Format::Rgba
    } else {
        vtium_font::Format::Mask
    };
    let size = (cell.width * width.cells(), cell.height);
    let (pixels, ink) = spacing::place_rows(
        &glyph.pixels,
        size,
        format,
        glyph.ink.as_ref(),
        (band, fit, false),
    )?;
    let at = target.glyph(width, pixels);
    let ink = ink.filter(|i| i.pixels.len() <= SET_CHUNK_BYTES as usize);
    if let Some(ink) = ink {
        target.ink(at, (ink.left, ink.top), (ink.width, ink.height), ink.pixels);
    }
    Some(at)
}

/// The set `bytes` that the engine drew for a line of double height at
/// `whole`, the cell of the normal size, read back as the face that draws
/// it, so that a line of double width can take its glyphs.
fn tall_set(bytes: Vec<u8>, whole: Cell) -> Result<SetFace, String> {
    let set = Set::from_bytes(bytes).map_err(|e| format!("{e:?}"))?;
    let tall = Doubled::Height
        .cell(whole)
        .ok_or("a double size past the largest cell")?;
    SetFace::new(Arc::new(set), tall.height)
        .ok_or_else(|| format!("no strike at {} pixels", tall.height))
}

/// Adds to the strikes of a line of double width, `r8` for coverage and
/// `rgba` for color, each glyph that `tall` holds in `variant`: the glyph
/// of each codepoint of `want` and of each cluster its cluster map holds,
/// drawn for a line of double height. Each glyph comes in at half its
/// height ([`halved`]), so it is the glyph of the normal size at twice its
/// width, as a line of double width shows it. Gives whether any glyph
/// went in.
///
/// The engine draws an outline glyph for a line of double width by
/// stretching its outline sideways before it rasterises it. It cannot do
/// that for a color glyph, which is a bitmap or layers of color, or for a
/// cluster, whose glyphs the shaper places. So those two come from the
/// glyphs of a line of double height, which are twice as wide and twice as
/// tall, made half as tall.
fn halve_into(
    tall: &SetFace,
    (want, variant): (&[u32], Variant),
    (r8, rgba): (&mut StrikeBuilder, &mut StrikeBuilder),
) -> bool {
    let mut put = |glyph: &Glyph| -> Option<(bool, u32)> {
        let pixels = halved(glyph)?;
        let color = glyph.format() == vtium_font::Format::Rgba;
        let target = if color { &mut *rgba } else { &mut *r8 };
        Some((color, target.glyph(glyph.cells(), pixels)))
    };
    let mut maps: Vec<(char, bool, u32)> = Vec::new();
    for ch in want.iter().filter_map(|&c| char::from_u32(c)) {
        if let Some(glyph) = tall.draw_variant(&[ch], variant)
            && let Some((color, at)) = put(&glyph)
        {
            maps.push((ch, color, at));
        }
    }
    let mut keys: Vec<(Vec<char>, bool, u32)> = Vec::new();
    for (_, key) in tall.set().strike_clusters() {
        if let Some(glyph) = tall.draw_variant(&key, variant)
            && let Some((color, at)) = put(&glyph)
        {
            keys.push((key, color, at));
        }
    }
    for &(ch, color, at) in &maps {
        if color { &mut *rgba } else { &mut *r8 }.map(ch, at);
    }
    for (key, color, at) in &keys {
        if *color { &mut *rgba } else { &mut *r8 }.cluster(key, *at);
    }
    !maps.is_empty() || !keys.is_empty()
}

/// The pixels of `glyph` at half its height: each pair of rows made one
/// row by the mean of each pixel's bytes, rounded to the nearest. A color
/// glyph's colors are premultiplied by their alpha, so their mean is the
/// mean color. `None` for a glyph of an odd height, which no cell of a
/// line of double height has.
fn halved(glyph: &Glyph) -> Option<Vec<u8>> {
    let height = glyph.height();
    if !height.is_multiple_of(2) {
        return None;
    }
    let row = usize::try_from(glyph.width())
        .ok()?
        .checked_mul(usize::try_from(glyph.format().bytes_per_pixel()).ok()?)?;
    let mut out = Vec::with_capacity(glyph.pixels().len() / 2);
    for pair in glyph.pixels().chunks_exact(row.checked_mul(2)?) {
        let (upper, lower) = pair.split_at(row);
        out.extend(
            upper
                .iter()
                .zip(lower)
                .map(|(&a, &b)| (u16::from(a) + u16::from(b)).div_ceil(2) as u8),
        );
    }
    Some(out)
}

/// A coverage query's answer ([`FRAME_COVERAGE`]): which of `letters` the
/// frit's sorted `coverage` holds, and the family of an installed font
/// under `dirs` that covers the most of the rest.
fn coverage_payload(coverage: &[u32], letters: &[u32], dirs: &[PathBuf]) -> Vec<u8> {
    let (covered, rest): (Vec<u32>, Vec<u32>) = letters
        .iter()
        .partition(|c| coverage.binary_search(c).is_ok());
    let rest: Vec<char> = rest.into_iter().filter_map(char::from_u32).collect();
    let font = crate::sources::font_covering(&rest, dirs).unwrap_or_default();
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
    // a batch whose bytes would outgrow the helper's memory limit
    let drawn = doubled.and_then(|d| d.cell(cell)).unwrap_or(cell);
    let at_once = (max / engine.glyph_bytes_max(drawn).max(1)).max(1);
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

/// A frame's payload of `cell`: its width and height.
fn cell_payload(cell: Cell) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    put32(&mut payload, cell.width);
    put32(&mut payload, cell.height);
    payload
}

/// A frame's payload of `cell` whose line spacing adds `band`: its width,
/// its height, and the rows added above and below (ADR-1302 D13). Two
/// heads can draw one cell size with different bands, so a fill's end and
/// a stop name both.
fn spaced_payload(cell: Cell, band: Band) -> Vec<u8> {
    let mut payload = cell_payload(cell);
    put32(&mut payload, band.above);
    put32(&mut payload, band.below);
    payload
}

/// A frame of glyphs: the rows line spacing adds to their cell, then
/// `set`, so the console holds them by the cell and its band.
fn glyph_payload(band: Band, set: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(set.len().saturating_add(8));
    put32(&mut payload, band.above);
    put32(&mut payload, band.below);
    payload.extend_from_slice(set);
    payload
}

fn failed(out: &mut dyn Write, why: &str) -> io::Result<()> {
    reason(out, FRAME_FAILED, why)
}

/// Writes `why`, cut at [`REASON_BYTES_MAX`], as a frame of `kind`.
fn reason(out: &mut dyn Write, kind: u32, why: &str) -> io::Result<()> {
    let mut why = why.to_owned();
    while why.len() > REASON_BYTES_MAX {
        why.pop();
    }
    write_frame(out, kind, why.as_bytes())
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

/// The variable the helper sets when it starts itself again without the
/// descriptors it inherited, so it does so once.
pub const SHED_VAR: &str = "VTIUM_CONFIG_SERVE_SHED";

/// The descriptors above standard error this process holds: each number
/// `/proc/self/fd` lists that still names a file once the listing's own
/// descriptor is closed.
fn inherited() -> Vec<u32> {
    let listed: Vec<u32> = std::fs::read_dir("/proc/self/fd")
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
                .filter(|&fd| fd > 2)
                .collect()
        })
        .unwrap_or_default();
    listed
        .into_iter()
        .filter(|fd| std::fs::read_link(format!("/proc/self/fd/{fd}")).is_ok())
        .collect()
}

/// Leaves the helper holding only its three pipes, whatever its starter
/// left open, as a terminal a font could write to: when it holds any
/// descriptor above standard error, it marks every one close-on-exec and
/// starts itself again, once, with the same arguments. Returns why it could
/// not; `None` when it holds only its pipes, or after the one restart.
fn shed_inherited() -> Option<String> {
    use std::os::unix::process::CommandExt;
    if std::env::var_os(SHED_VAR).is_some() || inherited().is_empty() {
        return None;
    }
    if let Err(e) = vtium_platform::cloexec_above_stdio() {
        return Some(format!("the helper cannot close what it inherited: {e}"));
    }
    let why = std::process::Command::new("/proc/self/exe")
        .args(std::env::args_os().skip(1))
        .env(SHED_VAR, "1")
        .exec();
    Some(format!(
        "the helper cannot start again without what it inherited: {why}"
    ))
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
    answer_requests(&read_requests(input), out, paths, idle, None)
}

/// The helper's work: the asks waiting, in order, and the fills.
struct Work<'p> {
    paths: &'p Paths,
    /// Each frit's engine, by its name and text, so a frit whose text
    /// changes is read again.
    engines: BTreeMap<FritText, Result<Engine, String>>,
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
    fn engine(&mut self, frit: &FritText) -> &Result<Engine, String> {
        let (paths, console) = (self.paths, self.console.clone());
        self.engines
            .entry(frit.clone())
            .or_insert_with(|| Engine::load(frit, paths, console))
    }

    /// Reads `frit` when it is not read yet, and sends each warning its
    /// text gives, so the console logs them.
    fn load(&mut self, frit: &FritText, out: &mut dyn Write) -> io::Result<()> {
        if self.engines.contains_key(frit) {
            return Ok(());
        }
        if let Ok(engine) = self.engine(frit) {
            for w in engine.warnings.clone() {
                reason(out, FRAME_WARNING, &w)?;
            }
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
                return match self.engine(&request.frit) {
                    Ok(engine) => {
                        let payload =
                            coverage_payload(engine.coverage(), &request.codepoints, &dirs);
                        write_frame(out, FRAME_COVERAGE, &payload)
                    }
                    Err(why) => reason(out, FRAME_DRAWS_NOTHING, &why.clone()),
                };
            }
            SOURCES => {
                return match self.engine(&request.frit) {
                    Ok(engine) => {
                        // the notes on its script-source= lines, which only
                        // a sources query answers (ADR-1202 D12)
                        for note in engine.script_notes() {
                            reason(out, FRAME_WARNING, &note)?;
                        }
                        let lines: Vec<String> =
                            engine.left_out().iter().map(LeftOut::line_text).collect();
                        write_frame(out, FRAME_SOURCES, lines.join("\n").as_bytes())
                    }
                    Err(why) => reason(out, FRAME_DRAWS_NOTHING, &why.clone()),
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
    mut report: Option<u32>,
) -> u8 {
    let mut work = Work {
        paths,
        engines: BTreeMap::new(),
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
                    if let Some(parts) = report.take()
                        && write_frame(out, FRAME_SANDBOX, &parts.to_le_bytes()).is_err()
                    {
                        return 1;
                    }
                    let taken = match r {
                        Request::Serve(r) => work.take(r, out),
                        Request::Pointer(a) => {
                            work.asks.push_back(Queued::Pointer(a));
                            Ok(())
                        }
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

/// The limits the helper sets on itself before it reads anything: the
/// parser's, with a CPU limit over the helper's life. With no process
/// left to start, it starts no thread either, so its reader starts first.
fn limits() -> Result<(), String> {
    for (resource, limit) in [
        (Resource::As, PARSER_MEMORY),
        (Resource::Cpu, HELPER_SECONDS),
        (Resource::Fsize, 0),
        (Resource::Nproc, 0),
        (Resource::Core, 0),
    ] {
        let limit = Rlimit {
            current: Some(limit),
            maximum: Some(limit),
        };
        setrlimit(resource, limit).map_err(|e| format!("the helper cannot set its limits: {e}"))?;
    }
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|e| format!("the helper cannot read its ids from /proc/self/status: {e}"))?;
    refuse_root(&status)
}

/// The ids a `/proc/<pid>/status` line names after its `key`, such as
/// `Uid:`'s real, effective, saved and filesystem uids, or `Groups:`'s
/// supplementary groups; `None` when the line is missing or a field is not
/// a number.
fn status_ids(status: &str, key: &str) -> Option<Vec<u32>> {
    let line = status.lines().find_map(|l| l.strip_prefix(key))?;
    line.split_whitespace().map(|id| id.parse().ok()).collect()
}

/// The helper never runs as root, nor with root's group (ADR-1200 D15).
/// `status` is its `/proc/self/status`, and the helper refuses to run when
/// any of its real, effective, saved and filesystem uids is 0, when any of
/// those gids is 0, or when its supplementary groups include 0. A saved id
/// of 0 would let it take root's id back, and group 0 reads what root's
/// group may read. A status it cannot read is refused too.
fn refuse_root(status: &str) -> Result<(), String> {
    let ids = |key: &str| {
        status_ids(status, key)
            .filter(|ids| key == "Groups:" || ids.len() == 4)
            .ok_or_else(|| {
                format!(
                    "the helper cannot read its ids: /proc/self/status has no {key} line it reads"
                )
            })
    };
    if ids("Uid:")?.contains(&0) {
        return Err("the helper refuses to run as root".to_owned());
    }
    if ids("Gid:")?.contains(&0) || ids("Groups:")?.contains(&0) {
        return Err("the helper refuses to run with group 0, root's group".to_owned());
    }
    Ok(())
}

/// The newest landlock version this build knows; the helper's domain
/// handles every write right and every network right of the kernel's
/// version up to it.
const LANDLOCK_NEWEST: ABI = ABI::V9;

/// Enters a landlock domain for the calling thread and the threads it
/// starts later (ADR-1200 D15, the maintainer, 2026-11-04). The domain
/// handles every right to write, create, remove, rename or truncate a
/// file and grants none, refuses binding and connecting TCP sockets, and
/// is scoped, so the helper connects to no abstract Unix socket made
/// outside it and signals no process outside it, each as far as the
/// kernel's landlock version knows it. It leaves reads open, since a frit
/// may name a font by an absolute path anywhere. Returns whether the
/// domain is in force; on a kernel without landlock the helper runs as
/// before, with none.
fn lock_out() -> Result<bool, String> {
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_write(LANDLOCK_NEWEST))
        .and_then(|r| r.handle_access(AccessNet::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.scope(Scope::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.create())
        .and_then(|r| r.restrict_self())
        .map_err(|e| format!("the helper cannot enter its landlock domain: {e}"))?;
    Ok(status.ruleset != RulesetStatus::NotEnforced)
}

/// Installs the helper's socket filter on the calling thread and the
/// threads it starts later (ADR-1200 D15, the maintainer, 2026-11-04): a
/// seccomp filter under which `socket`, `socketpair` and the three
/// io_uring calls answer EPERM and every other call goes on, so the helper
/// makes no socket of any kind on any kernel, the UDP and Unix sockets
/// landlock leaves open included ([`crate::seccomp::socket_filter`]).
/// Returns whether the filter is in force; on an architecture that
/// [`crate::seccomp`] has no table of call numbers for, there is none.
fn refuse_sockets() -> Result<bool, String> {
    #[cfg(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )
    ))]
    {
        let filter = crate::seccomp::socket_filter()
            .map_err(|e| format!("the helper cannot build its socket filter: {e}"))?;
        crate::seccomp::install(&filter)
            .map_err(|e| format!("the helper cannot install its socket filter: {e}"))?;
    }
    Ok(crate::seccomp::BUILT)
}

/// Installs the helper's fallback filter on the calling thread and the
/// threads it starts later, in place of the landlock domain on a kernel
/// without landlock (ADR-1200 D15, the maintainer, 2026-11-13): a seccomp
/// filter that refuses the file, signal and tracing calls the domain
/// refuses ([`crate::seccomp::fallback_filter`]). Returns whether the
/// filter is in force; on an architecture that [`crate::seccomp`] has no
/// table of call numbers for, there is none.
fn fall_back() -> Result<bool, String> {
    #[cfg(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )
    ))]
    {
        let pid = rustix::process::getpid().as_raw_nonzero().get();
        let pid = u32::try_from(pid).map_err(|_| "the helper's process id is negative")?;
        let filter = crate::seccomp::fallback_filter(pid)
            .map_err(|e| format!("the helper cannot build its fallback filter: {e}"))?;
        crate::seccomp::install(&filter)
            .map_err(|e| format!("the helper cannot install its fallback filter: {e}"))?;
    }
    Ok(crate::seccomp::BUILT)
}

/// The parts of the helper's sandbox in force, as the bits of its
/// [`FRAME_SANDBOX`]: its landlock domain, its fallback filter, which
/// stands in for the domain on a kernel without landlock, and its socket
/// filter.
pub const SANDBOX_DOMAIN: u32 = 1;
pub const SANDBOX_FALLBACK: u32 = 2;
pub const SANDBOX_SOCKETS: u32 = 4;

/// Sandboxes the calling thread and the threads it starts later: the
/// socket filter, then the landlock domain, or the fallback filter where
/// the kernel offers no landlock. Returns the parts in force, as
/// [`FRAME_SANDBOX`] reports them, or why a part failed to install.
fn sandbox() -> Result<u32, String> {
    let mut parts = 0;
    if refuse_sockets()? {
        parts |= SANDBOX_SOCKETS;
    }
    if lock_out()? {
        parts |= SANDBOX_DOMAIN;
    } else if fall_back()? {
        parts |= SANDBOX_FALLBACK;
    }
    Ok(parts)
}

/// `vtium-config serve`: no-new-privileges, the sandbox, the requests'
/// reader, the limits, then the requests on stdin answered on stdout until
/// [`HELPER_IDLE_MS`] passes with none, the first answer led by the
/// sandbox's report. A failure to sandbox is written as a failed frame, so
/// the console logs it.
pub fn command(paths: &Paths) -> u8 {
    let mut out = io::stdout().lock();
    if let Some(why) = shed_inherited() {
        let _ = failed(&mut out, &why);
        return 1;
    }
    if let Err(e) = rustix::thread::set_no_new_privs(true) {
        let _ = failed(
            &mut out,
            &format!("the helper cannot set no-new-privileges: {e}"),
        );
        return 1;
    }
    // before the reader's thread starts, which inherits the filters and
    // the domain, and before any font is read
    let parts = match sandbox() {
        Ok(parts) => parts,
        Err(why) => {
            let _ = failed(&mut out, &why);
            return 1;
        }
    };
    let requests = read_requests(io::stdin());
    if let Err(why) = limits() {
        let _ = failed(&mut out, &why);
        return 1;
    }
    answer_requests(
        &requests,
        &mut out,
        paths,
        Duration::from_millis(HELPER_IDLE_MS),
        Some(parts),
    )
}

#[cfg(test)]
use vtium_testfont::ttf;

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use std::time::Instant;
    use vtium_font::Face;
    use vtium_font::Format;
    use vtium_font::set::{Set, SetFace};

    /// A directory of a test's own, `vtium-serve-<name>-<pid>` in the
    /// temporary directory, removed when the test ends, whether it passes
    /// or fails.
    fn scratch(name: &str) -> Scratch {
        Scratch::new(&format!("serve-{name}"))
    }

    /// Each frame in `bytes`, a glyph frame's payload without the band in
    /// front of its set, which [`bands`] gives.
    fn frames(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        raw_frames(bytes)
            .into_iter()
            .map(|(kind, payload)| match kind {
                FRAME_GLYPHS | FRAME_FILL => (kind, payload[8..].to_vec()),
                _ => (kind, payload),
            })
            .collect()
    }

    /// Each frame in `bytes`, as the helper wrote it.
    fn raw_frames(mut bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let kind = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            out.push((kind, bytes[8..8 + len].to_vec()));
            bytes = &bytes[8 + len..];
        }
        out
    }

    /// The band each glyph frame in `bytes` names, in order.
    fn bands(bytes: &[u8]) -> Vec<Band> {
        raw_frames(bytes)
            .into_iter()
            .filter(|f| matches!(f.0, FRAME_GLYPHS | FRAME_FILL))
            .map(|(_, p)| Band {
                above: u32::from_le_bytes(p[..4].try_into().unwrap()),
                below: u32::from_le_bytes(p[4..8].try_into().unwrap()),
            })
            .collect()
    }

    /// The frit `t` of `text`, handed over with no file.
    fn frit(text: &str) -> FritText {
        FritText {
            name: "t".to_owned(),
            text: text.to_owned(),
        }
    }

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    fn request(kind: u32, frit: FritText, codepoints: &[char]) -> ServeRequest {
        ServeRequest {
            cluster_width: Default::default(),
            kind,
            frit,
            cell: CELL,
            band: Band::NONE,
            style: Style::default(),
            script: Script::Baseline,
            doubled: None,
            codepoints: codepoints.iter().map(|&c| u32::from(c)).collect(),
            clusters: Vec::new(),
            held: Vec::new(),
            console: None,
        }
    }

    fn run(input: &[ServeRequest]) -> Vec<(u32, Vec<u8>)> {
        frames(&run_bytes(input))
    }

    /// What the helper writes for `input`, as it writes it.
    fn run_bytes(input: &[ServeRequest]) -> Vec<u8> {
        let bytes: Vec<u8> = input.iter().flat_map(ServeRequest::encode).collect();
        let mut out = Vec::new();
        assert_eq!(
            serve(
                std::io::Cursor::new(bytes),
                &mut out,
                &Paths::default(),
                Duration::from_secs(5)
            ),
            0
        );
        out
    }

    /// Each glyph frame's set as a face at the 8x16 cell, with its kind.
    fn faces(frames: &[(u32, Vec<u8>)]) -> Vec<(u32, SetFace)> {
        frames
            .iter()
            .filter(|f| matches!(f.0, FRAME_GLYPHS | FRAME_FILL))
            .map(|f| {
                let set = Arc::new(Set::from_bytes(f.1.clone()).unwrap());
                assert_eq!(set.cells(), vec![CELL]);
                (f.0, SetFace::new(set, 16).unwrap())
            })
            .collect()
    }

    #[test]
    fn a_request_reads_back() {
        let r = ServeRequest {
            cluster_width: Default::default(),
            held: vec![(0x41, 0x5a), (0x4e00, 0x4e00)],
            console: Some(ConsoleFont {
                width: 8,
                height: 2,
                charcount: 2,
                data: vec![1, 2, 3, 4],
                unimap: vec![UniPair {
                    unicode: 0x41,
                    fontpos: 1,
                }],
            }),
            ..request(FILL, frit("name=a\nsource=builtin:unifont\n"), &['A', '中'])
        };
        let bytes = r.encode();
        assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(r)));
        assert_eq!(ServeRequest::read(&mut &b""[..]), Ok(None));
    }

    // ADR-1202 D14: an ask that says how its glyphs were made reads back,
    // and its answer holds a made frame just before its done frame, naming
    // each script glyph the helper drew and how; a plain ask's holds none
    // covers: sgr.script
    #[test]
    fn an_ask_made_says_how_each_script_glyph_was_made() {
        let sup = ServeRequest {
            cluster_width: Default::default(),
            script: Script::Super,
            ..request(ASK_MADE, frit("source=builtin:unifont\n"), &['x', '2'])
        };
        assert_eq!(
            ServeRequest::read(&mut &sup.encode()[..]),
            Ok(Some(sup.clone()))
        );
        let answer = run(std::slice::from_ref(&sup));
        let kinds: Vec<u32> = answer.iter().map(|f| f.0).collect();
        assert_eq!(kinds.first(), Some(&FRAME_GLYPHS), "{kinds:?}");
        assert_eq!(kinds[kinds.len() - 2..], [FRAME_MADE, FRAME_DONE]);
        assert_eq!(
            Made::decode_all(&answer[kinds.len() - 2].1),
            Some(vec![(0x32, Made::Scaled), (0x78, Made::Scaled)])
        );
        let plain = ServeRequest { kind: ASK, ..sup };
        assert!(!run(&[plain]).iter().any(|f| f.0 == FRAME_MADE));
    }

    const ITALIC: Style = Style {
        bold: false,
        italic: true,
    };
    const BOLD_ITALIC: Style = Style {
        bold: true,
        italic: true,
    };

    // the line size of a line of double size moved the magic from
    // VTVSREQ9 to VTVSREQA, as ADR-1006 D8's cluster width moved it from
    // VTVSREQ8 to VTVSREQ9, as ADR-1401 D7's coverage query and ADR-1302 D5's color
    // emoji's box moved it from VTVSREQ7 to VTVSREQ8, as ADR-1200 D15's report
    // of the sandbox moved it from VTVSREQ6 to VTVSREQ7, as ADR-1202 D11's script moved it from VTVSREQ5 to VTVSREQ6, as
    // ADR-1302 D13's band moved it from VTVSREQ4 to VTVSREQ5 and the style
    // from VTVSREQ3 to VTVSREQ4: a request of an older layout
    // is refused whole, with the reason a person can act on, and one of no
    // layout of this format as not a request; an ask alone names a style
    // (ADR-1200 D21)
    #[test]
    fn the_old_magic_and_a_style_off_an_ask_are_refused() {
        let ask = ServeRequest {
            cluster_width: Default::default(),
            style: BOLD_ITALIC,
            script: Script::Baseline,
            ..request(ASK, frit("source=builtin:unifont\n"), &['A'])
        };
        let bytes = ask.encode();
        assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask)));
        for magic in [
            b"VTVSREQ3",
            b"VTVSREQ4",
            b"VTVSREQ5",
            b"VTVSREQ6",
            b"VTVSREQ7",
            b"VTVSREQ8",
            b"VTVSREQ9",
        ] {
            let mut old = bytes.clone();
            old[..8].copy_from_slice(magic);
            assert_eq!(
                ServeRequest::read(&mut &old[..]),
                Err(OTHER_BUILD.to_owned())
            );
        }
        let mut other = bytes.clone();
        other[..8].copy_from_slice(b"GARBAGE!");
        assert_eq!(
            ServeRequest::read(&mut &other[..]),
            Err("not a serve request".to_owned())
        );
        for kind in [FILL, STOP, OWN_CELL, SOURCES] {
            let query = matches!(kind, OWN_CELL | SOURCES);
            let r = ServeRequest {
                cluster_width: Default::default(),
                style: ITALIC,
                script: Script::Baseline,
                cell: if query {
                    Cell {
                        width: 0,
                        height: 0,
                    }
                } else {
                    CELL
                },
                band: Band::NONE,
                ..request(kind, frit("source=builtin:unifont\n"), &[])
            };
            let e = ServeRequest::read(&mut &r.encode()[..]).unwrap_err();
            assert!(e.contains("style"), "{kind}: {e}");
            let regular = ServeRequest {
                cluster_width: Default::default(),
                style: REGULAR,
                script: Script::Baseline,
                ..r
            };
            assert!(
                ServeRequest::read(&mut &regular.encode()[..]).is_ok(),
                "{kind}"
            );
        }
    }

    #[test]
    fn a_malformed_request_is_refused_before_allocation() {
        // the frit `a` with no text: its cell at 21, its line spacing at 29,
        // its style at 37, its script at 41, its cluster width at 45, its
        // line size at 49, its capabilities at 53, its codepoint at 61
        let a = FritText {
            name: "a".to_owned(),
            text: String::new(),
        };
        let good = request(ASK, a.clone(), &['A']).encode();
        assert_eq!(ServeRequest::read(&mut &good[..]).unwrap().unwrap().frit, a);
        // the name's and the text's lengths past their bounds, then
        // nothing: refused on the count, before a buffer of that size is
        // made
        let head = |n: &[u32]| {
            let mut b = REQUEST_MAGIC.to_vec();
            b.extend_from_slice(&ASK.to_le_bytes());
            for v in n {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b
        };
        for huge in [head(&[u32::MAX]), {
            let mut b = head(&[1]);
            b.push(b'a');
            b.extend_from_slice(&u32::MAX.to_le_bytes());
            b
        }] {
            let e = ServeRequest::read(&mut &huge[..]).unwrap_err();
            assert!(e.contains("past its bound"), "{e}");
        }
        let mut many = head(&[1]);
        many.push(b'a');
        for v in [0u32, 8, 16, 0, 0, 0, 0, 0, 0, 0, u32::MAX] {
            many.extend_from_slice(&v.to_le_bytes());
        }
        let e = ServeRequest::read(&mut &many[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
        // the kind, the cell's width and height, line spacing above and
        // below that leaves no text band, a style past bold italic, a
        // script past subscript, a cluster width past single, a line size
        // past double height, capabilities not yet read, the codepoint, the
        // cluster count
        for (at, value) in [
            (8, 9u32),
            (21, 0),
            (25, CELL_SIDE_MAX + 1),
            (29, 16),
            (33, 16),
            (37, 4),
            (41, 3),
            (45, 2),
            (49, 3),
            (53, 1),
            (61, 0xd800),
            (65, u32::MAX),
        ] {
            let mut bad = good.clone();
            bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(ServeRequest::read(&mut &bad[..]).is_err(), "{at}");
        }
        // ADR-1006 D8: an ask carries its cluster width, and a request
        // that is not an ask carries `double`
        let single = ServeRequest {
            cluster_width: ClusterWidth::Single,
            ..request(ASK, a.clone(), &['A'])
        };
        let read = ServeRequest::read(&mut &single.encode()[..])
            .unwrap()
            .unwrap();
        assert_eq!(read.cluster_width, ClusterWidth::Single);
        let fill = ServeRequest {
            cluster_width: ClusterWidth::Single,
            ..request(FILL, a.clone(), &[])
        };
        let e = ServeRequest::read(&mut &fill.encode()[..]).unwrap_err();
        assert!(e.contains("cluster width"), "{e}");
        let backwards = ServeRequest {
            cluster_width: Default::default(),
            held: vec![(0x42, 0x41)],
            ..request(FILL, a, &[])
        };
        assert!(ServeRequest::read(&mut &backwards.encode()[..]).is_err());
        for name in ["", "a\nb"] {
            let named = ServeRequest {
                cluster_width: Default::default(),
                frit: FritText {
                    name: name.to_owned(),
                    text: String::new(),
                },
                ..request(ASK, frit(""), &[])
            };
            assert!(
                ServeRequest::read(&mut &named.encode()[..]).is_err(),
                "{name:?}"
            );
        }
        let mut not_utf8 = good.clone();
        not_utf8[16] = 0xff;
        assert!(ServeRequest::read(&mut &not_utf8[..]).is_err());
        assert!(ServeRequest::read(&mut &good[..good.len() - 1]).is_err());
    }

    /// A sources query is answered for every frit: the sources left out of
    /// one that reads, and for one the helper draws nothing from, because
    /// no source of it is on this machine or it does not read, a frame of
    /// its own, never the failed frame the helper sends for its own
    /// failure.
    #[test]
    fn a_frit_that_draws_nothing_has_an_answer_of_its_own() {
        let ask = |text: &str| ServeRequest {
            cluster_width: Default::default(),
            cell: Cell {
                width: 0,
                height: 0,
            },
            band: Band::NONE,
            ..request(SOURCES, frit(text), &[])
        };
        let kinds: Vec<u32> = run(&[
            ask("source=builtin:unifont\n"),
            ask("source=file:/nonexistent/vtium.psf\n"),
            ask("no-such-key=1\n"),
        ])
        .iter()
        .map(|f| f.0)
        .filter(|&k| k != FRAME_WARNING)
        .collect();
        assert_eq!(
            kinds,
            [FRAME_SOURCES, FRAME_DRAWS_NOTHING, FRAME_DRAWS_NOTHING]
        );
    }

    /// A sources query carries a cell of 0 by 0 and names nothing: one
    /// at a width or a height, or naming a codepoint or a held range, is
    /// refused as it is read.
    #[test]
    fn a_sources_query_naming_anything_is_refused() {
        let empty = Cell {
            width: 0,
            height: 0,
        };
        let query = ServeRequest {
            cluster_width: Default::default(),
            cell: empty,
            band: Band::NONE,
            ..request(SOURCES, frit("source=builtin:unifont\n"), &[])
        };
        assert_eq!(
            ServeRequest::read(&mut &query.encode()[..]),
            Ok(Some(query.clone()))
        );
        for (what, bad) in [
            (
                "a width",
                ServeRequest {
                    cluster_width: Default::default(),
                    cell: Cell {
                        width: 1,
                        height: 0,
                    },
                    band: Band::NONE,
                    ..query.clone()
                },
            ),
            (
                "a height",
                ServeRequest {
                    cluster_width: Default::default(),
                    cell: Cell {
                        width: 0,
                        height: 16,
                    },
                    band: Band::NONE,
                    ..query.clone()
                },
            ),
            (
                "a codepoint",
                ServeRequest {
                    cluster_width: Default::default(),
                    codepoints: vec![0x41],
                    ..query.clone()
                },
            ),
            (
                "a held range",
                ServeRequest {
                    cluster_width: Default::default(),
                    held: vec![(0x41, 0x42)],
                    ..query.clone()
                },
            ),
        ] {
            assert!(
                ServeRequest::read(&mut &bad.encode()[..]).is_err(),
                "{what}"
            );
        }
    }

    // ADR-1200 D21: an ask names clusters whole, each checked as it is
    // read, and no other request names one
    #[test]
    fn an_asks_clusters_read_back_and_a_malformed_one_is_refused() {
        let acute = vec![0x65, 0x301];
        let family = vec![0x1f468, 0x200d, 0x1f469, 0x200d, 0x1f467];
        let ask = ServeRequest {
            cluster_width: Default::default(),
            clusters: vec![acute.clone(), family],
            ..request(ASK, frit("source=builtin:unifont\n"), &['A'])
        };
        let bytes = ask.encode();
        assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask.clone())));
        let longest = CLUSTER_SCALARS_MAX as usize;
        for bad in [vec![], vec![0x41; longest + 1], vec![0x65, 0xd800]] {
            let r = ServeRequest {
                cluster_width: Default::default(),
                clusters: vec![bad.clone()],
                ..ask.clone()
            };
            assert!(
                ServeRequest::read(&mut &r.encode()[..]).is_err(),
                "{bad:x?}"
            );
        }
        let longest_ok = ServeRequest {
            cluster_width: Default::default(),
            clusters: vec![vec![0x41; longest]],
            ..ask.clone()
        };
        assert!(ServeRequest::read(&mut &longest_ok.encode()[..]).is_ok());
        for kind in [FILL, STOP] {
            let r = ServeRequest {
                cluster_width: Default::default(),
                clusters: vec![acute.clone()],
                ..request(kind, frit("source=builtin:unifont\n"), &[])
            };
            let e = ServeRequest::read(&mut &r.encode()[..]).unwrap_err();
            assert!(e.contains("past its bound"), "{e}");
        }
    }

    // ADR-1200 D1, S5: a frit's warnings, such as compress='s, are sent
    // as the helper reads the frit, once for its life, so the console logs
    // them
    #[test]
    fn a_frits_warnings_are_sent_once_as_it_is_read() {
        let warned = frit("source=builtin:unifont\ncompress=yes\n");
        let ask = request(ASK, warned.clone(), &['A']);
        let frames = run(&[ask.clone(), request(FILL, warned, &[]), ask]);
        let warnings: Vec<String> = frames
            .iter()
            .filter(|(k, _)| *k == FRAME_WARNING)
            .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
            .collect();
        assert_eq!(
            warnings,
            ["t:2: compress= has no effect here, and is ignored"]
        );
        assert_eq!(frames.first().map(|f| f.0), Some(FRAME_WARNING));
        let quiet = run(&[request(ASK, frit("source=builtin:unifont\n"), &['A'])]);
        assert!(quiet.iter().all(|(k, _)| *k != FRAME_WARNING));
    }

    // a bdf: source is parsed where the other fonts are, not as it is read
    #[test]
    fn a_bdf_source_is_parsed_when_the_frit_loads() {
        let dir = scratch("bdf");
        let (good, bad) = (dir.join("good.bdf"), dir.join("bad.bdf"));
        std::fs::write(&good, crate::bdf::tests::bdf("")).unwrap();
        std::fs::write(&bad, "STARTFONT 2.1\nCHARS 1\n").unwrap();
        let load = |p: &std::path::Path| {
            Engine::load(
                &frit(&format!("source=bdf:{}\n", p.display())),
                &Paths::default(),
                None,
            )
        };
        let (good, bad) = (load(&good), load(&bad));
        assert!(good.unwrap().coverage().contains(&0x41));
        let e = bad.err().unwrap();
        assert!(e.contains("CHARSET_REGISTRY"), "{e}");
    }

    // a source on this machine that the helper refuses, such as a BDF
    // file its reader refuses, is a warning the console logs when the
    // frit loads without it; a source this machine does not have is left
    // out quietly, as a frit names fonts a machine may not install
    #[test]
    fn a_source_refused_is_a_warning_and_a_missing_one_is_not() {
        let dir = scratch("refused");
        let bad = dir.join("bad.bdf");
        std::fs::write(&bad, "STARTFONT 2.1\nCHARS 1\n").unwrap();
        let text = format!(
            "source=builtin:unifont\nsource=bdf:{}\nsource=bdf:{}\n",
            bad.display(),
            dir.join("missing.bdf").display()
        );
        let engine = Engine::load(&frit(&text), &Paths::default(), None);
        let frames = run(&[request(ASK, frit(&text), &['A'])]);
        let warnings = engine.unwrap().warnings().to_vec();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let w = &warnings[0];
        assert!(
            w.starts_with(&format!("t:2: source=bdf:{}: ", bad.display())),
            "{w}"
        );
        assert!(w.contains("CHARSET_REGISTRY"), "{w}");
        assert!(w.ends_with("; the source is left out"), "{w}");
        let sent: Vec<String> = frames
            .iter()
            .filter(|(k, _)| *k == FRAME_WARNING)
            .map(|(_, p)| String::from_utf8(p.clone()).unwrap())
            .collect();
        assert_eq!(sent, warnings, "sent to the console as the frit loads");
    }

    // A zstd font whose window is past ZSTD_WINDOW_MAX is refused with a
    // warning that names its window, the 16 MiB limit and what to do
    #[test]
    fn a_zstd_font_with_too_large_a_window_warns_how_to_fix_it() {
        let dir = scratch("window");
        let wide = dir.join("wide.bdf.zst");
        let bdf = crate::bdf::tests::bdf("");
        std::fs::write(
            &wide,
            crate::decode::tests::zstd_frame(0x78, 0, bdf.as_bytes()),
        )
        .unwrap();
        let text = format!("source=builtin:unifont\nsource=bdf:{}\n", wide.display());
        let engine = Engine::load(&frit(&text), &Paths::default(), None);
        let warnings = engine.unwrap().warnings().to_vec();
        assert_eq!(
            warnings,
            [format!(
                "t:2: source=bdf:{}: zstd: the font is compressed with a 32 MiB window and \
                 vtium-config reads at most 16 MiB: recompress it with a window of 16 MiB or \
                 less; the source is left out",
                wide.display()
            )]
        );
    }

    // `Frit::parse` refuses a name that is not valid, so no engine is
    // named anything but the frit's own name
    #[test]
    fn a_frit_with_an_invalid_name_does_not_load() {
        let bad = FritText {
            name: "a b".to_owned(),
            text: "source=builtin:unifont\n".to_owned(),
        };
        let e = Engine::load(&bad, &Paths::default(), None).err().unwrap();
        assert!(e.contains("cannot be a font set's name"), "{e}");
    }

    // a set past its frame's bound, of one codepoint and a cluster, is cut
    // into the codepoint's and the cluster's, each sent, and ends
    #[test]
    fn an_oversize_set_of_a_codepoint_and_clusters_is_cut_and_ends() {
        let dir = scratch("cut");
        let font = dir.join("shaping.ttf");
        std::fs::write(&font, super::ttf::test_shaping().build()).unwrap();
        let engine = Engine::load(
            &frit(&format!("source=outline:{}\n", font.display())),
            &Paths::default(),
            None,
        );
        let engine = engine.unwrap();
        let cell = Cell {
            width: 12,
            height: 20,
        };
        let (want, clusters) = (vec![0xe9], vec![vec![0x65, 0x301]]);
        let whole = engine
            .glyphs(
                cell,
                Band::NONE,
                (REGULAR.into(), None),
                &want,
                (&clusters, false),
                &mut BTreeMap::new(),
            )
            .unwrap()
            .unwrap();
        let mut out = Vec::new();
        let sent = send_glyphs(
            &engine,
            (cell, Band::NONE, REGULAR.into(), None),
            (&want, &clusters, false),
            FRAME_GLYPHS,
            (&mut out, &mut BTreeMap::new()),
            whole.len() - 1,
        )
        .unwrap();
        assert_eq!(sent, Ok(true));
        let sets: Vec<Set> = frames(&out)
            .into_iter()
            .map(|(k, p)| {
                assert_eq!(k, FRAME_GLYPHS);
                Set::from_bytes(p).unwrap()
            })
            .collect();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].clusters().count(), 0);
        assert_eq!(
            sets[1].clusters().map(|(_, _, k)| k).collect::<Vec<_>>(),
            [vec!['e', '\u{301}']]
        );
    }

    // an 8x16 bitmap font scaled 33 times: each glyph takes 17,424 bytes cut
    // to its cell of 264x528, past a set's 16 KiB chunk, and the answer
    // carries every glyph, each in a chunk of its own, never a failure
    // (issue 272, ADR-1302 D6)
    #[test]
    fn a_bitmap_font_scaled_33_times_is_answered_whole() {
        let dir = scratch("huge");
        let path = dir.join("8x16.psf");
        let font = crate::decode::tests::psf2(8, 16, 3, Some(&[(1, 'A'), (2, 'B')]));
        std::fs::write(&path, font).unwrap();
        let frit = frit(&format!("source=file:{}\n", path.display()));
        let huge = Cell {
            width: 264,
            height: 528,
        };
        let asked = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell: huge,
            band: Band::NONE,
            ..request(ASK, frit.clone(), &['A', 'B'])
        }]);
        let kinds: Vec<u32> = asked.iter().map(|f| f.0).collect();
        assert_eq!(kinds, [FRAME_GLYPHS, FRAME_DONE]);
        let set = Arc::new(Set::from_bytes(asked[0].1.clone()).unwrap());
        assert_eq!(set.cells(), vec![huge]);
        let face = SetFace::new(set, huge.height).unwrap();
        // glyph 1's rows are 0x01, its last column lit, and glyph 2's 0x02;
        // each scaled 33 times and placed across the cell's full width
        for (c, column) in [('A', 7), ('B', 6)] {
            let g = face.draw(&[c], Style::default()).unwrap();
            assert_eq!((g.width(), g.height()), (264, 528));
            for y in [0, 263, 527] {
                for x in 0..264 {
                    let lit = g.pixel(x, y).unwrap()[0] != 0;
                    assert_eq!(lit, x / 33 == column, "{c:?} at {x},{y}");
                }
            }
        }
        // a fill at that cell is answered too
        let filled = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell: huge,
            band: Band::NONE,
            ..request(FILL, frit, &[])
        }]);
        assert!(
            filled.iter().all(|f| f.0 != FRAME_FAILED),
            "{:?}",
            filled.iter().map(|f| f.0).collect::<Vec<_>>()
        );
        assert!(filled.iter().any(|f| f.0 == FRAME_FILL));
    }

    // a batch is drawn no larger than a frame holds at its glyphs' largest:
    // with room for one 264x528 glyph a frame, each glyph is drawn and sent
    // alone
    #[test]
    fn a_batch_of_large_glyphs_is_drawn_a_frame_at_a_time() {
        let dir = scratch("batch");
        let path = dir.join("8x16.psf");
        let font = crate::decode::tests::psf2(8, 16, 3, Some(&[(1, 'A'), (2, 'B')]));
        std::fs::write(&path, font).unwrap();
        let engine = Engine::load(
            &frit(&format!("source=file:{}\n", path.display())),
            &Paths::default(),
            None,
        )
        .unwrap();
        let huge = Cell {
            width: 264,
            height: 528,
        };
        assert_eq!(engine.glyph_bytes_max(huge), 66 * 528);
        let mut out = Vec::new();
        let sent = send_glyphs(
            &engine,
            (huge, Band::NONE, REGULAR.into(), None),
            (&[u32::from('A'), u32::from('B')], &[], false),
            FRAME_GLYPHS,
            (&mut out, &mut BTreeMap::new()),
            66 * 528 + 4096,
        )
        .unwrap();
        assert_eq!(sent, Ok(true));
        let sets: Vec<Set> = frames(&out)
            .into_iter()
            .map(|(_, p)| Set::from_bytes(p).unwrap())
            .collect();
        assert_eq!(sets.len(), 2);
    }

    /// A cell query for `frit`: at no cell, naming nothing.
    fn query(frit: FritText) -> ServeRequest {
        ServeRequest {
            cluster_width: Default::default(),
            cell: Cell {
                width: 0,
                height: 0,
            },
            band: Band::NONE,
            ..request(OWN_CELL, frit, &[])
        }
    }

    /// The frit `t` whose sources are `sources`, each a PSF2 font written
    /// to a file of its own as `(width, height)`, or a source's spelling.
    fn psf_frit(name: &str, sources: &[Result<(u32, u32), &str>]) -> (FritText, Scratch) {
        let dir = scratch(name);
        let mut text = String::new();
        for (i, source) in sources.iter().enumerate() {
            match source {
                Ok((w, h)) => {
                    let path = dir.join(format!("{i}.psf"));
                    let font = crate::decode::tests::psf2(*w, *h, 1, Some(&[(0, 'A')]));
                    std::fs::write(&path, font).unwrap();
                    text.push_str(&format!("source=file:{}\n", path.display()));
                }
                Err(spelling) => text.push_str(&format!("source={spelling}\n")),
            }
        }
        (frit(&text), dir)
    }

    fn cell_bytes(width: u32, height: u32) -> Vec<u8> {
        [width.to_le_bytes(), height.to_le_bytes()].concat()
    }

    // the operator's decision on #145, 2026-10-28: a frit of bitmap sources
    // with no cell= takes its first source's own cell, asked of the helper
    #[test]
    fn a_cell_query_answers_the_frits_own_cell() {
        let (psf, _dir) = psf_frit("own-psf", &[Ok((8, 16))]);
        let (mixed, _mixed_dir) = psf_frit("own-mixed", &[Ok((8, 16)), Err("builtin:unscii-8")]);
        let frames = run(&[query(psf), query(mixed)]);
        assert_eq!(
            frames,
            [
                (FRAME_OWN_CELL, cell_bytes(8, 16)),
                (FRAME_OWN_CELL, cell_bytes(8, 16))
            ]
        );
        let frames = run(&[query(frit(
            "source=builtin:unscii-8\nsource=builtin:unscii-8\n",
        ))]);
        assert_eq!(frames, [(FRAME_OWN_CELL, cell_bytes(8, 8))]);
    }

    // a later source whose cell does not fit the first's leaves the frit
    // with no own cell, and the reason names it
    #[test]
    fn a_frit_whose_sources_do_not_fit_its_first_has_no_own_cell() {
        let (frit, _dir) = psf_frit("own-unfit", &[Ok((8, 8)), Err("builtin:unifont")]);
        let frames = run(&[
            query(frit),
            query(self::frit("source=file:/nonexistent/x.psf\n")),
        ]);
        assert_eq!(
            frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            [FRAME_NO_OWN_CELL, FRAME_NO_OWN_CELL]
        );
        let why = String::from_utf8_lossy(&frames[0].1);
        assert!(
            why.contains("source=builtin:unifont's 8x16 cell does not fit"),
            "{why}"
        );
        assert!(String::from_utf8_lossy(&frames[1].1).contains("not on this machine"));
    }

    /// A cell query for `frit` at a cell `height` pixels high, the height
    /// an outline first source is drawn at.
    fn query_at(frit: FritText, height: u32) -> ServeRequest {
        ServeRequest {
            cluster_width: Default::default(),
            cell: Cell { width: 0, height },
            band: Band::NONE,
            ..query(frit)
        }
    }

    /// The frit of `text` and then a `source=outline:` line for each of
    /// `fonts`, each written to a file of its own in a directory named
    /// for `name`, or a source's spelling.
    fn outline_frit(
        name: &str,
        text: &str,
        fonts: &[Result<super::ttf::Font, &str>],
    ) -> (FritText, Scratch) {
        let dir = scratch(name);
        let mut text = text.to_owned();
        for (i, font) in fonts.iter().enumerate() {
            match font {
                Ok(font) => {
                    let path = dir.join(format!("{i}.ttf"));
                    std::fs::write(&path, font.build()).unwrap();
                    text.push_str(&format!("source=outline:{}\n", path.display()));
                }
                Err(spelling) => text.push_str(&format!("source={spelling}\n")),
            }
        }
        (frit(&text), dir)
    }

    // ADR-1200 D17 (the maintainer, 2026-11-01): an outline frit that
    // states no cell takes its fonts' own, the W rule's width at the
    // height the console asks at. Test Mono's W advances 600 units of
    // 1000 and its ink fills the advance: 15.5 pixels to the em at 16
    // give 9.3, inked over 10 pixels, and 19.5 at 20 give 11.7, over 12
    #[test]
    fn an_outline_frits_cell_query_answers_the_w_rule_at_the_height_asked() {
        let (frit, _dir) = outline_frit("own-outline", "", &[Ok(super::ttf::test_mono())]);
        let frames = run(&[
            query_at(frit.clone(), 16),
            query_at(frit.clone(), 20),
            query(frit),
        ]);
        assert_eq!(
            frames[..2],
            [
                (FRAME_OUTLINE_CELL, cell_bytes(10, 16)),
                (FRAME_OUTLINE_CELL, cell_bytes(12, 20)),
            ]
        );
        // with no height there is no outline cell to give
        assert_eq!(frames[2].0, FRAME_NO_OWN_CELL);
        let why = String::from_utf8_lossy(&frames[2].1);
        assert!(why.contains("needs the height"), "{why}");
        assert_eq!(frames.len(), 3);
    }

    // ADR-1200 D17: the first source sets the cell, and later sources draw
    // into it. A bitmap first source gives its own cell whatever outline
    // source follows, and an outline first source the W rule's, a later
    // bitmap source refusing nothing
    #[test]
    fn the_first_source_sets_the_cell_whatever_kind_follows() {
        let (psf, psf_dir) = psf_frit("own-first-psf", &[Ok((8, 16))]);
        let font = |dir: &Path| {
            let path = dir.join("later.ttf");
            std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
            path
        };
        let bitmap_first = frit(&format!(
            "{}source=outline:{}\n",
            psf.text,
            font(&psf_dir).display()
        ));
        let (outline_first, _dir) = outline_frit(
            "own-first-outline",
            "",
            &[Ok(super::ttf::test_mono()), Err("builtin:unifont")],
        );
        let frames = run(&[
            query_at(bitmap_first.clone(), 20),
            query(bitmap_first),
            query_at(outline_first, 10),
        ]);
        assert_eq!(
            frames,
            [
                (FRAME_OWN_CELL, cell_bytes(8, 16)),
                (FRAME_OWN_CELL, cell_bytes(8, 16)),
                // 9.5 pixels to the em: W advances 5.7, inked over 6; unifont's
                // 8x16 does not fit and fills nothing
                (FRAME_OUTLINE_CELL, cell_bytes(6, 10)),
            ]
        );
    }

    // ADR-1204 D5: when the set's text font, its first source, is missing
    // or refused, the built-in font takes its place, and the frit gives no
    // own cell, so the console takes the cell of its configuration. A later
    // source, such as a layer's font, never sets the cell
    #[test]
    fn a_missing_text_font_leaves_the_cell_to_the_configuration() {
        let (layered, dir) = outline_frit(
            "own-missing-text",
            "source=font:Nonesuch Mono\n",
            &[Ok(super::ttf::test_mono())],
        );
        let broken = dir.join("broken.psf");
        std::fs::write(&broken, b"not a font").unwrap();
        let refused = frit(&format!(
            "source=file:{}\nsource=builtin:unscii-8\n",
            broken.display()
        ));
        let missing_bitmap = frit("source=kbd:nonesuch\nsource=builtin:unscii-8\n");
        let frames: Vec<(u32, Vec<u8>)> =
            run(&[query_at(layered, 16), query(refused), query(missing_bitmap)])
                .into_iter()
                .filter(|f| f.0 != FRAME_WARNING)
                .collect();
        assert_eq!(
            frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            [FRAME_NO_OWN_CELL, FRAME_NO_OWN_CELL, FRAME_NO_OWN_CELL]
        );
        let why = String::from_utf8_lossy(&frames[0].1);
        assert!(
            why.contains("the text font source=font:Nonesuch Mono is missing, so the built-in font takes its place"),
            "{why}"
        );
    }

    // A source that has failed is left out of the cell as it is out of the
    // glyphs, and the next source sets the cell: one whose W rule fails, a
    // font giving no ascent or descent, and one that failed as it drew
    #[test]
    fn a_failed_first_source_leaves_the_cell_to_the_next() {
        let flat = super::ttf::Font {
            ascent: 0,
            descent: 0,
            ..super::ttf::test_mono()
        };
        let (broken, _dir) = outline_frit("own-broken", "", &[Ok(flat)]);
        let broken = frit(&format!("{}source=builtin:unscii-8\n", broken.text));
        let frames = run(&[query_at(broken.clone(), 16), query_at(broken, 16)]);
        assert_eq!(
            frames,
            [
                (FRAME_OWN_CELL, cell_bytes(8, 8)),
                (FRAME_OWN_CELL, cell_bytes(8, 8)),
            ]
        );
        let (good, _dir) = outline_frit(
            "own-failed",
            "",
            &[Ok(super::ttf::test_mono()), Err("builtin:unscii-8")],
        );
        let engine = Engine::load(&good, &Paths::default(), None).unwrap();
        assert_eq!(
            engine.own_cell(16),
            Ok(OwnCell::Outline(Cell {
                width: 10,
                height: 16
            }))
        );
        engine
            .failed
            .borrow_mut()
            .insert(0, "failed as it drew".to_owned());
        assert_eq!(
            engine.own_cell(16),
            Ok(OwnCell::Bitmap(Cell {
                width: 8,
                height: 8
            }))
        );
        // with no source left, the reason names each one's failure
        engine.failed.borrow_mut().insert(1, "planted".to_owned());
        let why = engine.own_cell(16).unwrap_err();
        assert!(
            why.contains("failed as it drew") && why.contains("no source left"),
            "{why}"
        );
    }

    // ADR-1302 D6: the helper sends a glyph's ink box in the set beside its
    // pixels cut to its cell, and the console's reader gives it back. Test
    // Mono's L here spans -2.9 to 8.8 pixels of a 12-pixel cell
    // covers: text.overhang
    #[test]
    fn an_asks_glyph_carries_its_ink_box() {
        let mut font = super::ttf::test_mono();
        font.glyphs
            .push(super::ttf::Glyph::rect(600, -150, 0, 450, 700));
        font.cmap.push((0x4c, 8));
        font.cmap.sort_unstable();
        let (frit, _dir) = outline_frit("ink", "", &[Ok(font)]);
        let cell = Cell {
            width: 12,
            height: 20,
        };
        let frames = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band: Band::NONE,
            ..request(ASK, frit, &['L', 'A'])
        }]);
        assert_eq!(frames[0].0, FRAME_GLYPHS);
        let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
        let face = SetFace::new(set, 20).unwrap();
        let l = face.draw(&['L'], Style::default()).unwrap();
        let ink = l.ink().unwrap();
        assert_eq!((ink.left(), ink.width()), (-3, 15));
        assert!(face.draw(&['A'], Style::default()).unwrap().ink().is_none());
    }

    /// A tall main font, whose ascent and descent span 1.5 em, and a short
    /// symbol font, whose span 1 em, merged by `cover=` for the Private
    /// Use Area and Cyrillic: the symbol font's U+E0B0, a bar from its
    /// descent to its ascent, and U+0416, the same bar as a letter.
    fn merged_frit(name: &str) -> (FritText, Scratch) {
        let dir = scratch(name);
        let main = super::ttf::Font {
            family: "Tall Mono".to_owned(),
            ascent: 1100,
            descent: -400,
            ..super::ttf::test_mono()
        };
        let symbols = super::ttf::Font {
            family: "Short Symbols".to_owned(),
            ascent: 800,
            descent: -200,
            glyphs: vec![
                super::ttf::Glyph::empty(400),
                super::ttf::Glyph::rect(400, 0, -200, 400, 800),
            ],
            cmap: vec![(0x0416, 1), (0xe0b0, 1)],
            colr: Vec::new(),
            palette: Vec::new(),
            ..super::ttf::test_mono()
        };
        let (main_path, symbols_path) = (dir.join("main.ttf"), dir.join("symbols.ttf"));
        std::fs::write(&main_path, main.build()).unwrap();
        std::fs::write(&symbols_path, symbols.build()).unwrap();
        let text = format!(
            "source=outline:{}\nsource=outline:{}\ncover=U+E000-U+F8FF\ncover=U+0400-U+04FF\n",
            main_path.display(),
            symbols_path.display()
        );
        (frit(&text), dir)
    }

    // #64: a merged symbol font's private-use glyph is fitted to the main
    // font's cell. It draws at the size at which the symbol font's own
    // ascent and descent span the cell, so a bar the font draws from its
    // descent to its ascent, as a Powerline separator runs, fills the
    // cell from its top row to its bottom row. At the main font's size it
    // would span only two thirds of the cell. A letter the same font gives
    // through cover= keeps the main font's size, on its baseline, as every
    // later source's letters do (ADR-1200 D17)
    // covers: text.font-merge
    #[test]
    fn a_merged_symbol_fills_the_main_fonts_cell() {
        let (frit, _dir) = merged_frit("merged-fit");
        let cell = Cell {
            width: 12,
            height: 30,
        };
        let frames = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band: Band::NONE,
            ..request(ASK, frit, &['\u{e0b0}', '\u{416}'])
        }]);
        assert_eq!(frames[0].0, FRAME_GLYPHS);
        let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
        let face = SetFace::new(set, 30).unwrap();
        let rows = |c: char| -> Vec<u32> {
            let g = face.draw(&[c], Style::default()).unwrap();
            (0..g.height())
                .filter(|&y| (0..g.width()).any(|x| g.pixel(x, y).unwrap()[0] > 0))
                .collect()
        };
        assert_eq!(
            rows('\u{e0b0}'),
            (0..30).collect::<Vec<_>>(),
            "the separator"
        );
        let letter = rows('\u{416}');
        assert!(
            letter.len() <= 21,
            "the letter at the main font's size: {letter:?}"
        );
    }

    // ADR-1006 D8: an ask under `cluster-width = single` has the conjunct
    // ka, virama, ssa drawn in one cell, as the console's terminal gives it,
    // and the same ask under `double` in two
    #[test]
    fn an_ask_places_its_clusters_by_its_cluster_width() {
        let mut font = super::ttf::test_mono();
        let first = u16::try_from(font.glyphs.len()).unwrap();
        for (i, c) in [0x915u32, 0x94d, 0x937].into_iter().enumerate() {
            font.glyphs
                .push(super::ttf::Glyph::rect(900, 0, 0, 900, 500));
            font.cmap.push((c, first + u16::try_from(i).unwrap()));
        }
        font.cmap.sort_unstable();
        let (frit, _dir) = outline_frit("cluster-width", "", &[Ok(font)]);
        let key = vec![0x915, 0x94d, 0x937];
        let cells = |rule: ClusterWidth| {
            let frames = run(&[ServeRequest {
                cluster_width: rule,
                clusters: vec![key.clone()],
                ..request(ASK, frit.clone(), &[])
            }]);
            let set = frames
                .iter()
                .find(|(k, _)| *k == FRAME_GLYPHS)
                .map(|(_, p)| Arc::new(Set::from_bytes(p.clone()).unwrap()))
                .unwrap();
            let face = SetFace::new(set, 16).unwrap();
            face.cluster(&['\u{915}', '\u{94d}', '\u{937}'], Style::default())
                .unwrap()
                .cells()
        };
        assert_eq!(cells(ClusterWidth::Single), Width::Narrow);
        assert_eq!(cells(ClusterWidth::Double), Width::Wide);
    }

    // ADR-1302 D3: every request draws a font on one baseline, found from
    // its printable ASCII, whatever the request holds, and the strike
    // reports it with lines that follow it. Test Mono's underscore passes
    // a 12-pixel cell by a row, so its baseline rises from 10 to 9
    // covers: text.overhang
    #[test]
    fn every_request_draws_a_font_on_one_baseline() {
        let mut font = super::ttf::test_mono();
        font.glyphs
            .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
        font.glyphs
            .push(super::ttf::Glyph::rect(600, 100, 0, 500, 500));
        font.cmap.push((0x5f, 8));
        font.cmap.push((0x61, 9));
        font.cmap.sort_unstable();
        let (frit, _dir) = outline_frit("baseline", "", &[Ok(font)]);
        let cell = Cell {
            width: 7,
            height: 12,
        };
        let ask = |c: char| ServeRequest {
            cluster_width: Default::default(),
            cell,
            band: Band::NONE,
            ..request(ASK, frit.clone(), &[c])
        };
        // each in a helper of its own, so no earlier request found it
        let sets: Vec<Set> = ['a', '_', '\u{4e00}']
            .iter()
            .map(|&c| Set::from_bytes(run(&[ask(c)])[0].1.clone()).unwrap())
            .collect();
        for set in &sets {
            let strike = set.strikes().next().unwrap();
            assert_eq!(strike.baseline, 9, "{:?}", set.clusters().count());
            let d = strike.decorations;
            assert!(
                d.underline > strike.baseline,
                "the underline below the baseline"
            );
            assert!(d.strike < strike.baseline, "the strike-through above it");
        }
        let face = SetFace::new(Arc::new(sets.into_iter().next().unwrap()), 12).unwrap();
        let a = face.draw(&['a'], Style::default()).unwrap();
        let inked = |y: usize| a.pixels()[y * 7..(y + 1) * 7].iter().any(|&p| p > 0);
        assert!(inked(8) && !inked(9), "a stands on row 9 asked alone");
    }

    // ADR-1200 D1: a frit's `hinting=` reaches the glyphs the helper draws
    // and the baseline it finds, light where the frit says nothing. In
    // DejaVu Sans Mono at 16 pixels, light hinting puts the underscore a
    // row past the cell at the ascent's baseline, so the baseline rises
    // from 13 to 12, and full hinting keeps it at 13
    #[test]
    fn a_frits_hinting_reaches_its_outline_glyphs() {
        let font = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/DejaVuSansMono-ascii.ttf"
        );
        let cell = Cell {
            width: 9,
            height: 16,
        };
        for (key, hinting, base) in [
            ("", Hinting::Light, 12),
            ("hinting=light\n", Hinting::Light, 12),
            ("hinting=full\n", Hinting::Full, 13),
        ] {
            let text = format!("{key}source=outline:{font}\n");
            let frames = run(&[ServeRequest {
                cluster_width: Default::default(),
                cell,
                band: Band::NONE,
                ..request(ASK, frit(&text), &['H'])
            }]);
            assert_eq!(frames[0].0, FRAME_GLYPHS, "{key:?}");
            let set = Set::from_bytes(frames[0].1.clone()).unwrap();
            assert_eq!(set.strikes().next().unwrap().baseline, base, "{key:?}");
            let face = SetFace::new(Arc::new(set), 16).unwrap();
            let drawn = face.draw(&['H'], Style::default()).unwrap();
            let want = rasterise(&OutlineRequest {
                cluster_width: Default::default(),
                font: std::fs::read(font).unwrap().into(),
                cells: vec![(9, 16)],
                cover: vec![(0x48, 0x48)],
                hinting,
                ..OutlineRequest::default()
            })
            .unwrap();
            let s = &want.strikes[0];
            assert_eq!(s.baseline, base, "{key:?}");
            assert_eq!(drawn.pixels(), &s.glyphs[0].pixels[..], "{key:?}");
        }
    }

    // Outline glyphs behind a bitmap first source draw 1-bit, so they look
    // like the bitmap font's own; render=smooth draws them antialiased, and
    // render=bitmap draws even a first outline source 1-bit. The frit's
    // first source is an 8x16 PSF font that holds only A, and DejaVu Sans
    // Mono fills a, g and @; a glyph drawn 1-bit holds only fully lit and
    // fully dark pixels, and some of them lit. A font: source that turns
    // out to be a console font draws 1-bit whatever its render= says, so
    // the helper warns that the line does nothing.
    //
    // References:
    // - ADR-1200 D1, the render= row: the default and its override.
    // covers: text.outline
    #[test]
    fn outline_glyphs_behind_a_bitmap_first_source_draw_one_bit() {
        let dejavu = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/DejaVuSansMono-ascii.ttf"
        );
        let (psf, _dir) = psf_frit("render-psf", &[Ok((8, 16))]);
        let outline = format!("source=outline:{dejavu}\n");
        let cases = [
            (format!("{}{outline}", psf.text), true),
            (format!("{}{outline}render=smooth\n", psf.text), false),
            (format!("{}{outline}render=bitmap\n", psf.text), true),
            (outline.clone(), false),
            (format!("{outline}render=bitmap\n"), true),
            (format!("{outline}{}", psf.text), false),
        ];
        let cell = Cell {
            width: 8,
            height: 16,
        };
        for (text, one_bit) in cases {
            let frames = run(&[ServeRequest {
                cell,
                ..request(ASK, frit(&text), &['a', 'g', '@'])
            }]);
            assert_eq!(frames[0].0, FRAME_GLYPHS, "{text}");
            let set = Set::from_bytes(frames[0].1.clone()).unwrap();
            let face = SetFace::new(Arc::new(set), 16).unwrap();
            let mut partial = false;
            for c in ['a', 'g', '@'] {
                let drawn = face.draw(&[c], Style::default()).unwrap();
                let px = drawn.pixels();
                assert!(px.iter().any(|&p| p > 0), "{c:?} has ink: {text}");
                partial |= px.iter().any(|&p| p != 0 && p != 255);
            }
            assert_eq!(partial, !one_bit, "{text}");
        }
        let kbd = scratch("render-kbd");
        std::fs::create_dir_all(kbd.join("consolefonts")).unwrap();
        let font = crate::decode::tests::psf2(8, 16, 1, Some(&[(0, 'A')]));
        std::fs::write(kbd.join("consolefonts/x.psf"), font).unwrap();
        let paths = Paths {
            kbd_dirs: vec![kbd.to_path_buf()],
            font_dirs: Vec::new(),
            ..Paths::default()
        };
        let engine = Engine::load(&frit("source=font:x\nrender=bitmap\n"), &paths, None).unwrap();
        assert_eq!(
            engine.warnings(),
            [
                "t:1: source=font:x: render= is ignored, because this font is a bitmap font, which always draws 1-bit"
            ]
        );
    }

    // one helper keeps a source's baseline for each height apart (review
    // finding A7): Test Mono's underscore leaves its baseline at 8 in a
    // 10-pixel cell and raises it from 10 to 9 in a 12-pixel one
    #[test]
    fn the_baseline_is_kept_for_each_height() {
        let mut font = super::ttf::test_mono();
        font.glyphs
            .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
        font.cmap.push((0x5f, 8));
        font.cmap.sort_unstable();
        let (frit, _dir) = outline_frit("two-heights", "", &[Ok(font)]);
        let at = |h| ServeRequest {
            cluster_width: Default::default(),
            cell: Cell {
                width: 7,
                height: h,
            },
            band: Band::NONE,
            ..request(ASK, frit.clone(), &['A'])
        };
        let frames = run(&[at(10), at(12), at(10)]);
        let bases: Vec<u32> = frames
            .iter()
            .filter(|f| f.0 == FRAME_GLYPHS)
            .map(|f| {
                Set::from_bytes(f.1.clone())
                    .unwrap()
                    .strikes()
                    .next()
                    .unwrap()
                    .baseline
            })
            .collect();
        assert_eq!(bases, [8, 9, 8]);
    }

    // a strike reports its frit's first source's baseline even once that
    // source has failed as it drew, so the answers at one cell never change
    // mid-session: Test Mono with an underscore that raises its baseline at
    // 12 pixels from 10 to 9
    #[test]
    fn a_strike_keeps_its_first_sources_baseline_after_it_fails() {
        let mut font = super::ttf::test_mono();
        font.glyphs
            .push(super::ttf::Glyph::rect(600, 0, -200, 600, -100));
        font.cmap.push((0x5f, 8));
        font.cmap.sort_unstable();
        let (frit, _dir) = outline_frit("first-baseline", "", &[Ok(font), Err("builtin:unscii-8")]);
        let engine = Engine::load(&frit, &Paths::default(), None).unwrap();
        let cell = Cell {
            width: 7,
            height: 12,
        };
        assert_eq!(engine.metrics_at(cell).0, 9);
        engine
            .failed
            .borrow_mut()
            .insert(0, "failed as it drew".to_owned());
        assert_eq!(engine.metrics_at(cell).0, 9, "after the source fails");
    }

    // ADR-1200 D17: a frit that states its cell is drawn at it, the W
    /// The glyphs an ask for `chars` draws from `frit` at `cell`, whose
    /// line spacing adds `band`, as rows, each glyph's in order, with the
    /// first strike's lines and baseline.
    fn spaced_rows(
        frit: &FritText,
        cell: Cell,
        band: Band,
        chars: &[char],
    ) -> (Vec<Vec<Vec<u8>>>, (Decorations, u32)) {
        let bytes = run_bytes(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band,
            ..request(ASK, frit.clone(), chars)
        }]);
        // every glyph frame names the band its request named
        assert!(
            bands(&bytes).iter().all(|&b| b == band),
            "{:?}",
            bands(&bytes)
        );
        let frames = frames(&bytes);
        let faces: Vec<SetFace> = frames
            .iter()
            .filter(|f| f.0 == FRAME_GLYPHS)
            .map(|f| {
                SetFace::new(Arc::new(Set::from_bytes(f.1.clone()).unwrap()), cell.height).unwrap()
            })
            .collect();
        let glyph = |c: char| {
            faces
                .iter()
                .find_map(|face| face.draw(&[c], Style::default()))
                .unwrap()
        };
        let rows = chars
            .iter()
            .map(|&c| {
                let g = glyph(c);
                assert_eq!(g.height(), cell.height, "{c:?}");
                g.rows().map(<[u8]>::to_vec).collect()
            })
            .collect();
        let baseline = faces[0].set().strikes().next().unwrap().baseline;
        (rows, (faces[0].decorations(), baseline))
    }

    /// `rows` with `above` blank rows over them and `below` under them.
    fn padded(rows: &[Vec<u8>], above: usize, below: usize) -> Vec<Vec<u8>> {
        let blank = vec![0u8; rows[0].len()];
        let mut out = vec![blank.clone(); above];
        out.extend(rows.iter().cloned());
        out.extend(vec![blank; below]);
        out
    }

    // ADR-1204 D2: an outline source's strike= lines name bitmap files of
    // its design. A cell up to the tallest strike's height draws the
    // largest strike that fits it, placed on the cell's bottom row as a
    // bitmap source is placed; above that height, and in a cell no strike
    // fits, the outline draws. A character the strikes lack, W here, draws
    // from the outline at every height. The 12-pixel strike draws its A in
    // the cell's last column and the 16-pixel strike in the one before it
    #[test]
    fn an_outline_sources_strikes_draw_up_to_the_tallest_strikes_height() {
        let dir = scratch("strikes");
        let outline = dir.join("mono.ttf");
        std::fs::write(&outline, super::ttf::test_mono().build()).unwrap();
        let strike = |height: u32, glyph: u32| {
            let path = dir.join(format!("{height}.psf"));
            let font = crate::decode::tests::psf2(8, height, 3, Some(&[(glyph, 'A')]));
            std::fs::write(&path, font).unwrap();
            path
        };
        let (twelve, sixteen) = (strike(12, 1), strike(16, 2));
        let plain = frit(&format!("source=outline:{}\n", outline.display()));
        let struck = frit(&format!(
            "source=outline:{}\nstrike=file:{}\nstrike=file:{}\n",
            outline.display(),
            sixteen.display(),
            twelve.display()
        ));
        // the strike's A: its column lit on its rows, at the cell's bottom
        let strike_a = |height: u32, tall: u32, column: usize| -> Vec<Vec<u8>> {
            (0..height)
                .map(|y| {
                    let mut row = vec![0u8; 8];
                    if y >= height.saturating_sub(tall) {
                        row[column] = 255;
                    }
                    row
                })
                .collect()
        };
        for height in [10, 12, 14, 15, 16, 17, 20] {
            let cell = Cell { width: 8, height };
            let (drawn, _) = spaced_rows(&struck, cell, Band::NONE, &['A', 'W']);
            let (outline_drawn, _) = spaced_rows(&plain, cell, Band::NONE, &['A', 'W']);
            let want_a = match height {
                12..=15 => strike_a(height, 12, 7),
                16 => strike_a(height, 16, 6),
                _ => outline_drawn[0].clone(),
            };
            assert_ne!(strike_a(height, 12, 7), outline_drawn[0], "{height}");
            assert_eq!(drawn[0], want_a, "A at {height} pixels");
            assert_eq!(drawn[1], outline_drawn[1], "W at {height} pixels");
        }
    }

    // ADR-1204 D2: strike= follows an outline source, names a bitmap file,
    // and a strike this machine lacks is left out quietly while one it
    // refuses is a warning
    #[test]
    fn a_strike_this_machine_lacks_is_left_out_and_a_bad_one_warns() {
        let dir = scratch("strikes-bad");
        let outline = dir.join("mono.ttf");
        std::fs::write(&outline, super::ttf::test_mono().build()).unwrap();
        let broken = dir.join("broken.psf");
        std::fs::write(&broken, b"not a font").unwrap();
        let text = format!(
            "source=outline:{}\nstrike=file:{}/gone.psf\nstrike=file:{}\n",
            outline.display(),
            dir.display(),
            broken.display()
        );
        let engine = Engine::load(&frit(&text), &Paths::default(), None).unwrap();
        assert_eq!(engine.warnings().len(), 1, "{:?}", engine.warnings());
        assert!(
            engine.warnings()[0].starts_with(&format!("t:3: strike=file:{}: ", broken.display())),
            "{:?}",
            engine.warnings()
        );
        assert!(engine.warnings()[0].ends_with("; the strike is left out"));
        assert!(engine.strikes.iter().all(Vec::is_empty));
    }

    // ADR-1302 D13: where line spacing adds rows to the cell, the helper
    // draws each glyph in the text band and places it in the whole cell.
    // A bitmap letter keeps its rows with blank rows above and below, a
    // box-drawing line stretches by repeating its top and bottom rows, and
    // the strike's lines move down by the rows added above. A frit's
    // `fit=natural` keeps the line at its size too.
    #[test]
    fn a_spaced_bitmap_glyph_is_padded_or_stretched_by_its_fit() {
        let unscii = frit("source=builtin:unscii-8\n");
        let text = Cell {
            width: 8,
            height: 8,
        };
        let cell = Cell {
            width: 8,
            height: 11,
        };
        let band = Band { above: 2, below: 1 };
        let chars = ['A', '\u{2502}', '\u{2588}'];
        let (plain, (plain_lines, plain_base)) = spaced_rows(&unscii, text, Band::NONE, &chars);
        let (rows, (lines, base)) = spaced_rows(&unscii, cell, band, &chars);
        assert_eq!(rows[0], padded(&plain[0], 2, 1), "A keeps its size");
        for (i, c) in [(1, "a box line"), (2, "a full block")] {
            let p = &plain[i];
            let mut want = vec![p[0].clone(), p[0].clone()];
            want.extend(p.iter().cloned());
            want.push(p[7].clone());
            assert_eq!(rows[i], want, "{c} fills the cell");
        }
        assert_eq!(lines.underline, plain_lines.underline + 2);
        assert_eq!(lines.strike, plain_lines.strike + 2);
        // the baseline moves down with the text band, so a bitmap glyph
        // sheared for italic about it leans as it does with no spacing
        assert_eq!(base, plain_base + 2);
        let lean = |cell: Cell, band: Band| -> Vec<Vec<u8>> {
            let frames = run(&[ServeRequest {
                cluster_width: Default::default(),
                cell,
                band,
                ..request(ASK, unscii.clone(), &['A'])
            }]);
            let set = Arc::new(Set::from_bytes(frames[0].1.clone()).unwrap());
            let face = SetFace::new(set, cell.height).unwrap();
            let italic = Style {
                bold: false,
                italic: true,
            };
            let g = face.draw(&['A'], italic).unwrap();
            g.rows().map(<[u8]>::to_vec).collect()
        };
        let (plain_italic, spaced_italic) = (lean(text, Band::NONE), lean(cell, band));
        assert_eq!(
            spaced_italic,
            padded(&plain_italic, 2, 1),
            "the italic A leans alike"
        );
        let natural = frit("source=builtin:unscii-8\nfit=natural\n");
        let (rows, _) = spaced_rows(&natural, cell, band, &chars);
        assert_eq!(
            rows[1],
            padded(&plain[1], 2, 1),
            "fit=natural keeps the line's size"
        );
    }

    // ADR-1302 D13: an outline letter keeps its size in the text band; the
    // same glyph mapped from a Powerline codepoint, which is `fill`, scales
    // vertically to the whole cell; a color glyph keeps its size whatever
    // `fit=` says
    #[test]
    fn a_spaced_outline_glyph_is_padded_scaled_or_kept_by_its_fit() {
        let (mono, _dir) = outline_frit("spaced", "", &[Ok(super::ttf::test_mono())]);
        let text = Cell {
            width: 12,
            height: 20,
        };
        let cell = Cell {
            width: 12,
            height: 26,
        };
        let band = Band { above: 3, below: 3 };
        let chars = ['A', '\u{e0a0}', '\u{1f600}'];
        let (plain, (_, plain_base)) = spaced_rows(&mono, text, Band::NONE, &chars);
        let (rows, (_, base)) = spaced_rows(&mono, cell, band, &chars);
        assert_eq!(
            base,
            plain_base + 3,
            "the baseline moves down with the text band"
        );
        assert_eq!(rows[0], padded(&plain[0], 3, 3), "A keeps its size");
        assert_ne!(
            rows[1],
            padded(&plain[1], 3, 3),
            "the Powerline glyph is stretched"
        );
        assert_eq!(
            spacing::scale_rows(&plain[1].concat(), 12, band).unwrap(),
            rows[1].concat(),
            "the Powerline glyph is scaled vertically"
        );
        assert_eq!(rows[2], padded(&plain[2], 3, 3), "the emoji keeps its size");
        let (fill, _dir) = outline_frit("spaced-fill", "", &[Ok(super::ttf::test_mono())]);
        let fill = frit(&format!("{}fit=fill\n", fill.text));
        let (rows, _) = spaced_rows(&fill, cell, band, &chars);
        assert_eq!(
            rows[2],
            padded(&plain[2], 3, 3),
            "fit=fill does not stretch a color glyph"
        );
        assert_eq!(rows[0], rows[1], "fit=fill stretches the letter");
    }

    // ADR-1306 D8: under line spacing a natural glyph's ink may reach past
    // its text band through the rows the band adds and then a row of the
    // whole cell. Test Mono's U+2191 here is a bar from 1400 to 2200 units
    // above the baseline: at 19.5 pixels to the em in the 12x20 text band,
    // baseline 16, it reaches 42.9 pixels up, 26.9 over the text band's top,
    // more than the whole cell's 26 rows but less than the 3 added rows and
    // those 26. Drawn with a band of 3 above and 3 below, its box starts 24
    // rows over the 26-row cell's top, where a reach of a row alone, without
    // the added rows, would have cut it at 23
    #[test]
    fn a_spaced_glyphs_ink_reaches_through_the_added_rows_and_a_row_more() {
        let mut font = super::ttf::test_mono();
        font.glyphs
            .push(super::ttf::Glyph::rect(600, 100, 1400, 500, 2200));
        let bar = u16::try_from(font.glyphs.len() - 1).unwrap();
        font.cmap.push((0x2191, bar));
        font.cmap.sort_unstable();
        let (mono, _dir) = outline_frit("spaced-reach", "", &[Ok(font)]);
        let cell = Cell {
            width: 12,
            height: 26,
        };
        let band = Band { above: 3, below: 3 };
        let bytes = run_bytes(&[ServeRequest {
            cluster_width: Default::default(),
            cell,
            band,
            ..request(ASK, mono, &['\u{2191}'])
        }]);
        let g = frames(&bytes)
            .iter()
            .filter(|f| f.0 == FRAME_GLYPHS)
            .find_map(|f| {
                let set = Set::from_bytes(f.1.clone()).unwrap();
                SetFace::new(Arc::new(set), cell.height)
                    .unwrap()
                    .draw(&['\u{2191}'], Style::default())
            })
            .unwrap();
        // the bar's ink over the cell
        let ink = g.ink().unwrap();
        assert_eq!((ink.left(), ink.top(), ink.width()), (0, -24, 12));
        let row = |y: usize| &ink.pixels()[y * 12..(y + 1) * 12];
        assert!(row(0).iter().any(|&p| p > 0), "the bar's top row");
        assert!(row(1)[4] == 255, "the bar under its top row");
    }

    // ADR-1302 D13: two fills at one cell size with two bands are two
    // fills, each drawn at its band, each ending with a frame that names
    // its band; neither replaces the other
    #[test]
    fn one_cell_size_with_two_bands_fills_twice() {
        let unscii = frit("source=builtin:unscii-8\n");
        let band = Band { above: 2, below: 2 };
        let fill = |band| ServeRequest {
            cluster_width: Default::default(),
            band,
            ..request(FILL, unscii.clone(), &['A'])
        };
        let bytes = run_bytes(&[fill(Band::NONE), fill(band)]);
        let ends: Vec<Vec<u8>> = raw_frames(&bytes)
            .into_iter()
            .filter(|f| f.0 == FRAME_FILLED)
            .map(|f| f.1)
            .collect();
        assert_eq!(
            ends,
            [spaced_payload(CELL, Band::NONE), spaced_payload(CELL, band)]
        );
        let bands = bands(&bytes);
        assert!(
            bands.contains(&Band::NONE) && bands.contains(&band),
            "{bands:?}"
        );
    }

    // ADR-1302 D13: a stop at a cell with a band ends that band's fill
    // alone, and the helper's word that it took it names the band
    #[test]
    fn a_stop_at_a_band_is_taken_naming_the_band() {
        let unscii = frit("source=builtin:unscii-8\n");
        let band = Band { above: 2, below: 1 };
        let at = |kind| ServeRequest {
            cluster_width: Default::default(),
            band,
            ..request(kind, unscii.clone(), &['A'])
        };
        let frames = run(&[at(FILL), at(STOP)]);
        let stops: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|f| f.0 == FRAME_STOPPED)
            .map(|f| &f.1)
            .collect();
        assert_eq!(stops, [&spaced_payload(CELL, band)]);
    }

    // ADR-1302 D13: a request whose line spacing leaves its cell no text
    // band, or a query that names any, is refused
    #[test]
    fn line_spacing_that_leaves_no_text_band_is_refused() {
        let (unscii, base) = (
            frit("source=builtin:unscii-8\n"),
            request(ASK, frit("source=builtin:unscii-8\n"), &['A']),
        );
        for (cell_h, above, below, ok) in [
            (11, 2, 1, true),
            (11, 6, 5, false),
            (11, 11, 0, false),
            (11, 0, 11, false),
        ] {
            let r = ServeRequest {
                cluster_width: Default::default(),
                cell: Cell {
                    width: 8,
                    height: cell_h,
                },
                band: Band { above, below },
                ..base.clone()
            };
            let read = ServeRequest::read(&mut std::io::Cursor::new(r.encode()));
            assert_eq!(read.is_ok(), ok, "{above}+{below} of {cell_h}");
        }
        let query = ServeRequest {
            cluster_width: Default::default(),
            band: Band { above: 1, below: 0 },
            ..super::tests::query(unscii)
        };
        assert!(ServeRequest::read(&mut std::io::Cursor::new(query.encode())).is_err());
    }

    // rule never widening it; the console reads cell= itself and asks the
    // helper only where D17 has it ask, so a cell query answers the fonts'
    // own cell whatever cell= says
    #[test]
    fn a_frit_that_states_its_cell_is_drawn_at_it() {
        let (frit, _dir) =
            outline_frit("own-stated", "cell=10x20\n", &[Ok(super::ttf::test_mono())]);
        let stated = Cell {
            width: 10,
            height: 20,
        };
        let asked = run(&[ServeRequest {
            cluster_width: Default::default(),
            cell: stated,
            band: Band::NONE,
            ..request(ASK, frit.clone(), &['W'])
        }]);
        let queried = run(&[query_at(frit, 20)]);
        assert_eq!(
            asked.iter().map(|f| f.0).collect::<Vec<_>>(),
            [FRAME_GLYPHS, FRAME_DONE]
        );
        let set = Set::from_bytes(asked[0].1.clone()).unwrap();
        assert_eq!(set.cells(), vec![stated]);
        assert_eq!(queried, [(FRAME_OUTLINE_CELL, cell_bytes(12, 20))]);
    }

    // a cell query names no cell, no codepoint and no range
    #[test]
    fn a_cell_query_at_a_cell_or_naming_codepoints_is_refused() {
        let good = query(frit("")).encode();
        assert!(ServeRequest::read(&mut &good[..]).unwrap().is_some());
        let high = query_at(frit(""), CELL_SIDE_MAX).encode();
        let read = ServeRequest::read(&mut &high[..]).unwrap().unwrap();
        assert_eq!((read.cell.width, read.cell.height), (0, CELL_SIDE_MAX));
        for bad in [
            ServeRequest {
                cluster_width: Default::default(),
                cell: CELL,
                band: Band::NONE,
                ..query(frit(""))
            },
            ServeRequest {
                cluster_width: Default::default(),
                cell: Cell {
                    width: 8,
                    height: 0,
                },
                band: Band::NONE,
                ..query(frit(""))
            },
            query_at(frit(""), CELL_SIDE_MAX + 1),
            ServeRequest {
                cluster_width: Default::default(),
                codepoints: vec![0x41],
                clusters: Vec::new(),
                ..query(frit(""))
            },
            ServeRequest {
                cluster_width: Default::default(),
                held: vec![(0x41, 0x41)],
                ..query(frit(""))
            },
        ] {
            assert!(
                ServeRequest::read(&mut &bad.encode()[..]).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_frit_name_outside_d1s_set_is_refused() {
        let named = |name: &str| {
            ServeRequest {
                cluster_width: Default::default(),
                frit: FritText {
                    name: name.to_owned(),
                    text: String::new(),
                },
                ..request(ASK, frit(""), &[])
            }
            .encode()
        };
        for name in ["a/b", "../a", ".a", "a b", "é", "a\u{7f}"] {
            let e = ServeRequest::read(&mut &named(name)[..]).unwrap_err();
            assert!(e.contains("a frit's name"), "{name:?}: {e}");
        }
        let fine = "Ter-v16n_1.2+x";
        let read = ServeRequest::read(&mut &named(fine)[..]).unwrap().unwrap();
        assert_eq!(read.frit.name, fine);
        let longest = "a".repeat(NAME_BYTES_MAX);
        assert!(ServeRequest::read(&mut &named(&longest)[..]).is_ok());
    }

    #[test]
    fn an_ask_is_answered_whole_with_only_what_it_names() {
        let frit = frit("source=builtin:unscii-8\n");
        let frames = run(&[request(ASK, frit, &['Z', 'A', '\u{10fffd}'])]);
        assert_eq!(
            frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            [FRAME_GLYPHS, FRAME_DONE]
        );
        let faces = faces(&frames);
        let f = &faces[0].1;
        assert!(f.has_glyph('A') && f.has_glyph('Z') && !f.has_glyph('B'));
        // Unscii-8's 8x8 sits on the bottom half of an 8x16 cell
        let a = f.draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.width(), a.height()), (8, 16));
        assert!((0..8).all(|y| (0..8).all(|x| a.pixel(x, y).unwrap()[0] == 0)));
    }

    #[test]
    fn a_fill_draws_the_screen_then_latin_and_box_then_the_rest_once() {
        let frit = frit("source=builtin:unscii-8\n");
        let fill = ServeRequest {
            cluster_width: Default::default(),
            held: vec![(0x42, 0x42)],
            ..request(FILL, frit, &['α'])
        };
        let frames = run(&[fill]);
        assert_eq!(frames.last().map(|f| f.0), Some(FRAME_FILLED));
        assert_eq!(
            frames.last().unwrap().1,
            [8, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let faces = faces(&frames);
        assert!(faces.len() > 1, "{}", faces.len());
        assert!(faces.iter().all(|f| f.0 == FRAME_FILL));
        let first = &faces[0].1;
        // the screen's α, then Latin and box drawing, then the rest in
        // codepoint order: Unscii-8's last, U+1FBF9, comes later
        for c in ['α', 'A', 'é', '─', '█'] {
            assert!(first.has_glyph(c), "{c}");
        }
        assert!(!first.has_glyph('\u{1fbf9}'));
        // every glyph once, and none the console holds
        for c in ['A', 'α', 'β', '─', '\u{1fbf9}'] {
            assert_eq!(faces.iter().filter(|f| f.1.has_glyph(c)).count(), 1, "{c}");
        }
        assert!(faces.iter().all(|f| !f.1.has_glyph('B')));
    }

    #[test]
    fn an_ask_waits_at_most_one_batch_of_a_fill() {
        // unifont's coverage is some sixty batches; an ask sent behind the
        // fill is answered after at most one of them
        let frit = frit("source=builtin:unifont\n");
        let frames = run(&[
            request(FILL, frit.clone(), &[]),
            request(ASK, frit, &['中']),
        ]);
        let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
        let asked = kinds.iter().position(|&k| k == FRAME_GLYPHS).unwrap();
        assert!(asked <= 1, "{:?}", &kinds[..asked + 1]);
        assert_eq!(kinds[asked + 1], FRAME_DONE);
        assert!(kinds.iter().filter(|&&k| k == FRAME_FILL).count() > 10);
        assert_eq!(kinds.last(), Some(&FRAME_FILLED));
        // the ask and the fill are two requests: each sends 中 once, and
        // the console drops the fill's
        let faces = faces(&frames);
        let sent = |kind| {
            faces
                .iter()
                .filter(|f| f.0 == kind && f.1.has_glyph('中'))
                .count()
        };
        assert_eq!((sent(FRAME_GLYPHS), sent(FRAME_FILL)), (1, 1));
    }

    #[test]
    fn a_stop_ends_its_cells_fill_and_a_fill_replaces_it() {
        let frit = frit("source=builtin:unscii-8\n");
        let paths = Paths::default();
        let mut work = Work {
            paths: &paths,
            engines: BTreeMap::new(),
            console: None,
            asks: VecDeque::new(),
            fills: VecDeque::new(),
        };
        let mut out = Vec::new();
        work.take(request(FILL, frit.clone(), &[]), &mut out)
            .unwrap();
        work.take(request(FILL, frit.clone(), &['α']), &mut out)
            .unwrap();
        assert_eq!(work.fills.len(), 1);
        assert_eq!(work.fills[0].order[0], u32::from('α'));
        assert!(out.is_empty());
        work.take(request(STOP, frit, &[]), &mut out).unwrap();
        assert!(work.fills.is_empty());
        assert_eq!(
            frames(&out),
            [(FRAME_STOPPED, spaced_payload(CELL, Band::NONE))]
        );
    }

    // a stop is answered as it is taken, between the stopped fill's last
    // frame and the next fill's first, and answered with no fill too
    #[test]
    fn a_stop_is_answered_between_the_fill_it_ends_and_the_next() {
        let frit = frit("source=builtin:unscii-8\n");
        let frames = run(&[
            request(STOP, frit.clone(), &[]),
            request(FILL, frit.clone(), &['A']),
            request(STOP, frit.clone(), &[]),
            request(FILL, frit, &['A']),
        ]);
        let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
        let stops: Vec<usize> = (0..kinds.len())
            .filter(|&i| kinds[i] == FRAME_STOPPED)
            .collect();
        assert_eq!(stops.len(), 2, "{kinds:?}");
        assert!(
            stops
                .iter()
                .all(|&i| frames[i].1 == spaced_payload(CELL, Band::NONE))
        );
        // the first stop, with no fill, comes first; the first fill's
        // frames, and its end if it reached it, come before the second,
        // and the second fill runs whole after it
        assert_eq!(stops[0], 0);
        assert!(
            kinds[..stops[1]]
                .iter()
                .filter(|&&k| k == FRAME_FILLED)
                .count()
                <= 1
        );
        let after = &kinds[stops[1] + 1..];
        assert_eq!(after.first(), Some(&FRAME_FILL), "{kinds:?}");
        assert_eq!(after.last(), Some(&FRAME_FILLED), "{kinds:?}");
        assert_eq!(after.iter().filter(|&&k| k == FRAME_FILLED).count(), 1);
    }

    // a frit is its name and its text: the same name with new text is read
    // again
    #[test]
    fn a_frit_whose_text_changes_is_read_again() {
        let frames = run(&[
            request(ASK, frit("source=builtin:unscii-8\n"), &['中']),
            request(ASK, frit("source=builtin:unifont\n"), &['中']),
        ]);
        let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
        // Unscii-8 lacks 中 and unifont has it
        assert_eq!(kinds, [FRAME_DONE, FRAME_GLYPHS, FRAME_DONE]);
    }

    #[test]
    fn a_frit_with_no_source_here_fails_each_request() {
        let frit = frit("source=file:/nonexistent/x.psf\n");
        let frames = run(&[
            request(ASK, frit.clone(), &['A']),
            request(FILL, frit, &['B']),
        ]);
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().all(|f| f.0 == FRAME_FAILED));
        assert!(String::from_utf8_lossy(&frames[0].1).contains("not on this machine"));
    }

    #[test]
    fn a_fill_no_source_can_draw_fails_naming_why() {
        // unifont's 8x16 does not fit a 4x8 cell: an ask is answered with
        // nothing, and a fill fails with the reason
        let frit = frit("source=builtin:unifont\n");
        let small = Cell {
            width: 4,
            height: 8,
        };
        let frames = run(&[
            ServeRequest {
                cluster_width: Default::default(),
                cell: small,
                band: Band::NONE,
                ..request(ASK, frit.clone(), &['A'])
            },
            ServeRequest {
                cluster_width: Default::default(),
                cell: small,
                band: Band::NONE,
                ..request(FILL, frit, &['A'])
            },
        ]);
        assert_eq!(
            frames.iter().map(|f| f.0).collect::<Vec<_>>(),
            [FRAME_DONE, FRAME_FAILED]
        );
        let why = String::from_utf8_lossy(&frames[1].1);
        assert!(why.contains("no glyph at 4x8"), "{why}");
        assert!(why.contains("8x16"), "{why}");
    }

    #[test]
    fn a_malformed_request_ends_the_helper_with_a_failure() {
        let mut out = Vec::new();
        let status = serve(
            std::io::Cursor::new(b"VTVSREQ1\xff\xff\xff\xff".to_vec()),
            &mut out,
            &Paths::default(),
            Duration::from_secs(5),
        );
        assert_eq!(status, 1);
        let frames = frames(&out);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, FRAME_FAILED);
    }

    /// An SVG pointer file with a white 8 × 16 `default` symbol, its
    /// hotspot at its corner.
    fn pointer_svg() -> Vec<u8> {
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><symbol id=\"default\" viewBox=\"0 0 8 16\" data-hotspot=\"0 0\"><rect width=\"8\" height=\"16\" fill=\"#fff\"/></symbol></svg>".to_vec()
    }

    fn svg_ask(shape: PointerShape, heights: Vec<u32>) -> PointerAsk {
        PointerAsk {
            file: PointerFile::Svg,
            shape,
            size: None,
            heights,
            bytes: pointer_svg(),
        }
    }

    // A pointer ask reads back as it was encoded, and the glyph requests'
    // reader refuses it as a kind it does not know.
    #[test]
    fn a_pointer_ask_reads_back_and_glyph_reading_refuses_it() {
        let ask = svg_ask(PointerShape::Text, vec![16, 32]);
        let bytes = ask.encode();
        assert_eq!(
            read_request(&mut &bytes[..]),
            Ok(Some(Request::Pointer(ask.clone())))
        );
        assert_eq!(PointerAsk::read(&mut &bytes[..]), Ok(Some(ask)));
        assert!(ServeRequest::read(&mut &bytes[..]).is_err());
        let x = PointerAsk {
            file: PointerFile::Xcursor,
            shape: PointerShape::NsResize,
            size: Some(24),
            heights: Vec::new(),
            bytes: b"Xcur".to_vec(),
        };
        assert_eq!(
            read_request(&mut &x.encode()[..]),
            Ok(Some(Request::Pointer(x)))
        );
    }

    // Each field of a pointer ask is checked before it is trusted.
    #[test]
    fn a_pointer_ask_refuses_each_field_out_of_bounds() {
        let good = svg_ask(PointerShape::Default, vec![16]);
        let svg_sized = PointerAsk {
            size: Some(24),
            ..good.clone()
        };
        let no_height = svg_ask(PointerShape::Default, Vec::new());
        let many = svg_ask(PointerShape::Default, vec![16; POINTER_HEIGHTS_MAX + 1]);
        let zero = svg_ask(PointerShape::Default, vec![0]);
        let tall = svg_ask(PointerShape::Default, vec![CELL_SIDE_MAX + 1]);
        let xcursor_height = PointerAsk {
            file: PointerFile::Xcursor,
            ..good.clone()
        };
        let small = PointerAsk {
            file: PointerFile::Xcursor,
            heights: Vec::new(),
            size: Some(7),
            ..good.clone()
        };
        let large_svg = PointerAsk {
            bytes: vec![b' '; POINTER_SVG_BYTES_MAX + 1],
            ..good.clone()
        };
        for (what, ask) in [
            ("size on SVG", svg_sized),
            ("no height", no_height),
            ("too many heights", many),
            ("height 0", zero),
            ("height too tall", tall),
            ("height on Xcursor", xcursor_height),
            ("size under 8", small),
            ("SVG too large", large_svg),
        ] {
            assert!(read_request(&mut &ask.encode()[..]).is_err(), "{what}");
        }
        let mut file_kind = good.encode();
        file_kind[12] = 3;
        assert!(read_request(&mut &file_kind[..]).is_err());
        let mut shape = good.encode();
        shape[16] = 10;
        assert!(read_request(&mut &shape[..]).is_err());
        let bytes = good.encode();
        assert!(read_request(&mut &bytes[..bytes.len() - 1]).is_err());
    }

    // The helper answers a pointer ask with a picture at each height
    // asked, then done; a shape the file lacks with a frame that says so;
    // and a cursor file it cannot read with the reason.
    #[test]
    fn the_helper_answers_a_pointer_ask_with_its_pictures() {
        let mut input = svg_ask(PointerShape::Default, vec![20, 16, 16]).encode();
        input.extend(svg_ask(PointerShape::Wait, vec![16]).encode());
        input.extend(
            PointerAsk {
                file: PointerFile::Xcursor,
                shape: PointerShape::Text,
                size: None,
                heights: Vec::new(),
                bytes: b"not a cursor".to_vec(),
            }
            .encode(),
        );
        let mut out = Vec::new();
        let status = serve(
            std::io::Cursor::new(input),
            &mut out,
            &Paths::default(),
            Duration::from_secs(5),
        );
        assert_eq!(status, 0);
        let frames = raw_frames(&out);
        let kinds: Vec<u32> = frames.iter().map(|f| f.0).collect();
        assert_eq!(
            kinds,
            [
                FRAME_POINTER,
                FRAME_POINTER,
                FRAME_DONE,
                FRAME_NO_POINTER,
                FRAME_DONE,
                FRAME_NO_POINTER,
                FRAME_DONE
            ]
        );
        let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
        let first = &frames[0].1;
        // shape 0, drawn for 16, 8 × 16, hotspot at 0, 0
        assert_eq!(
            (0..6).map(|i| word(first, i)).collect::<Vec<_>>(),
            [0, 16, 8, 16, 0, 0]
        );
        assert_eq!(first.len(), 24 + 8 * 16 * 4);
        assert_eq!(word(&frames[1].1, 1), 20);
        assert_eq!(
            (word(&frames[3].1, 0), word(&frames[3].1, 1)),
            (shape_code(PointerShape::Wait), NO_POINTER_MISSING)
        );
        let why = &frames[5].1;
        assert_eq!(
            (word(why, 0), word(why, 1)),
            (shape_code(PointerShape::Text), NO_POINTER_REFUSED)
        );
        assert!(String::from_utf8_lossy(&why[8..]).contains("Xcur"));
    }

    /// Input that never ends and never gives a byte.
    struct Silent;

    impl Read for Silent {
        #[expect(clippy::disallowed_methods, reason = "input that never comes")]
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(Duration::from_secs(3600));
            Ok(0)
        }
    }

    #[test]
    #[expect(clippy::disallowed_methods, reason = "the idle exit is timed")]
    fn the_helper_exits_after_its_idle_interval() {
        let started = Instant::now();
        let mut out = Vec::new();
        let status = serve(
            Silent,
            &mut out,
            &Paths::default(),
            Duration::from_millis(200),
        );
        let took = started.elapsed();
        assert_eq!(status, 0);
        assert!(out.is_empty());
        assert!(
            took >= Duration::from_millis(200) && took < Duration::from_secs(5),
            "{took:?}"
        );
        assert_eq!(HELPER_IDLE_MS, 60_000);
    }

    // the helper's landlock domain refuses every file write and every TCP
    // bind and connect, and an abstract Unix socket made outside it, and
    // leaves reads open (ADR-1200 D15, the maintainer, 2026-11-04); a
    // thread of its own enters it, as the helper's main thread does, so the
    // rest of the test process stays outside it
    #[test]
    fn the_helpers_landlock_domain_refuses_writes_and_the_network() {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
        let dir = scratch("landlock");
        let existing = dir.join("existing");
        std::fs::write(&existing, b"font").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let tcp = listener.local_addr().unwrap();
        let name = format!("vtium-landlock-{}", std::process::id());
        let abstract_addr = SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
        let _unix = UnixListener::bind_addr(&abstract_addr).unwrap();
        let (existing_in, dir_in) = (existing.clone(), dir.to_path_buf());
        let locked = std::thread::spawn(move || {
            let enforced = lock_out().unwrap();
            if !enforced {
                return None;
            }
            let read = std::fs::read(&existing_in).ok();
            let created = std::fs::write(dir_in.join("new"), b"x").is_ok();
            let opened = std::fs::OpenOptions::new()
                .write(true)
                .open(&existing_in)
                .is_ok();
            let removed = std::fs::remove_file(&existing_in).is_ok();
            let bound = std::net::TcpListener::bind("127.0.0.1:0").is_ok();
            let connected = std::net::TcpStream::connect(tcp).is_ok();
            let reached = UnixStream::connect_addr(&abstract_addr).is_ok();
            Some((read, created, opened, removed, bound, connected, reached))
        })
        .join()
        .unwrap();
        let Some((read, created, opened, removed, bound, connected, reached)) = locked else {
            eprintln!(
                "skipped: this kernel offers no landlock, so the helper runs without its domain"
            );
            return;
        };
        assert_eq!(read.as_deref(), Some(&b"font"[..]), "reads stay open");
        assert!(!created, "a file created");
        assert!(!opened, "a file opened for writing");
        assert!(!removed, "a file removed");
        assert!(!bound, "a TCP socket bound");
        assert!(!connected, "a TCP socket connected");
        assert!(
            !reached,
            "an abstract Unix socket made outside the domain reached"
        );
        // the test's own threads are outside the domain
        assert!(std::fs::write(dir.join("outside"), b"x").is_ok());
    }

    // the helper's socket filter refuses every new socket on any kernel,
    // UDP and Unix ones included, which landlock leaves open, and lets
    // every other call through (ADR-1200 D15, the maintainer, 2026-11-04);
    // a thread of its own installs it, as the helper's main thread does
    #[test]
    fn the_helpers_socket_filter_refuses_every_new_socket() {
        use std::os::unix::net::{UnixDatagram, UnixStream};
        let dir = scratch("sockets");
        // qemu-user implements no seccomp, since a filter would bind qemu
        // itself, so there the test cannot judge the filter, and it skips
        // and says so
        let enosys = std::io::Error::from_raw_os_error(libc::ENOSYS).to_string();
        let qemu = std::env::var_os("VTIUM_QEMU_USER");
        let made = std::thread::spawn(move || {
            match refuse_sockets() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(e) => {
                    assert!(qemu.is_some() && e.contains(&enosys), "{e}");
                    eprintln!(
                        "skipped: {} implements no seccomp: {e}",
                        qemu.unwrap_or_default().display()
                    );
                    return None;
                }
            }
            let read = std::fs::read("/proc/self/status").is_ok();
            let written = std::fs::write(dir.join("allowed"), b"x").is_ok();
            Some((
                read && written,
                std::net::UdpSocket::bind("127.0.0.1:0").is_ok(),
                std::net::TcpListener::bind("127.0.0.1:0").is_ok(),
                UnixDatagram::unbound().is_ok(),
                UnixStream::pair().is_ok(),
            ))
        })
        .join()
        .unwrap();
        let Some((others, udp, tcp, unix, pair)) = made else {
            eprintln!("skipped: no socket filter on this architecture or under qemu-user");
            return;
        };
        assert!(others, "a call other than socket refused");
        assert!(!udp, "a UDP socket made");
        assert!(!tcp, "a TCP socket made");
        assert!(!unix, "a Unix socket made");
        assert!(!pair, "a socket pair made");
        // the test's own threads are outside the filter
        assert!(std::net::UdpSocket::bind("127.0.0.1:0").is_ok());
    }

    // the socket filter refuses io_uring, whose IORING_OP_SOCKET makes a
    // socket without calling socket (Linux 5.19). The test cannot make the
    // calls itself, since the crate forbids unsafe code, and std cannot
    // start a process under the filter, so it hands the compiled filter to
    // a python3, which installs it through ctypes, as the helper's
    // seccompiler does, and then makes each io_uring call with arguments
    // that fail on their own (EFAULT or EBADF) and prints its errno. The
    // same python3 without the filter shows that the kernel offers
    // io_uring, else the test skips and says so
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    ))]
    #[test]
    fn the_helpers_socket_filter_refuses_io_uring() {
        const SCRIPT: &str = "import ctypes, struct, sys\n\
            libc = ctypes.CDLL(None, use_errno=True)\n\
            code = bytes.fromhex(sys.argv[4])\n\
            if code:\n\
            \x20   class Prog(ctypes.Structure):\n\
            \x20       _fields_ = [('len', ctypes.c_ushort), ('filter', ctypes.c_char_p)]\n\
            \x20   prog = Prog(len(code) // 8, code)\n\
            \x20   assert libc.prctl(38, 1, 0, 0, 0) == 0\n\
            \x20   assert libc.prctl(22, 2, ctypes.byref(prog), 0, 0) == 0\n\
            out = []\n\
            for call, args in ((sys.argv[1], (1, None)), (sys.argv[2], (-1, 0, 0, 0, None, 0)), (sys.argv[3], (-1, 0, None, 0))):\n\
            \x20   ctypes.set_errno(0)\n\
            \x20   r = libc.syscall(ctypes.c_long(int(call)), *[ctypes.c_long(a) if isinstance(a, int) else a for a in args])\n\
            \x20   out.append(str(ctypes.get_errno() if r < 0 else 0))\n\
            print(' '.join(out))\n";
        let errnos = |filter: &[seccompiler::sock_filter]| -> Option<Vec<i32>> {
            let code: String = filter
                .iter()
                .flat_map(|f| {
                    let mut b = f.code.to_ne_bytes().to_vec();
                    b.extend([f.jt, f.jf]);
                    b.extend(f.k.to_ne_bytes());
                    b
                })
                .map(|b| format!("{b:02x}"))
                .collect();
            let calls = [
                libc::SYS_io_uring_setup,
                libc::SYS_io_uring_enter,
                libc::SYS_io_uring_register,
            ];
            let out = std::process::Command::new("python3")
                .args(["-I", "-c", SCRIPT])
                .args(calls.map(|n| n.to_string()))
                .arg(code)
                .env_clear()
                .output()
                .ok()?;
            String::from_utf8(out.stdout)
                .ok()?
                .split_whitespace()
                .map(|e| e.parse().ok())
                .collect::<Option<Vec<i32>>>()
                .filter(|e| e.len() == 3)
        };
        let Some(outside) = errnos(&[]) else {
            eprintln!("skipped: no python3 to make the io_uring calls with");
            return;
        };
        // Under qemu-user, as make arm-test runs the tests, python3 is the
        // host's, of another architecture than the filter, which then
        // matches none of its calls.
        let machine = std::process::Command::new("python3")
            .args(["-I", "-c", "import platform; print(platform.machine())"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|m| m.trim().to_owned())
            .unwrap_or_default();
        if machine != std::env::consts::ARCH {
            eprintln!(
                "skipped: python3 runs on {machine}, and the filter is for {}",
                std::env::consts::ARCH
            );
            return;
        }
        if outside.contains(&libc::EPERM) || outside.contains(&libc::ENOSYS) {
            eprintln!("skipped: this kernel refuses io_uring already: {outside:?}");
            return;
        }
        let inside = errnos(&crate::seccomp::socket_filter().unwrap());
        assert_eq!(
            inside,
            Some(vec![libc::EPERM; 3]),
            "outside the filter: {outside:?}"
        );
    }

    #[test]
    fn the_helper_refuses_to_run_as_root() {
        let status = |uid: &str, gid: &str, groups: &str| {
            format!("Name:\tvtium-config\nUid:\t{uid}\nGid:\t{gid}\nGroups:\t{groups}\nNgid:\t0\n")
        };
        let nobody = "65534\t65534\t65534\t65534";
        assert_eq!(refuse_root(&status(nobody, nobody, "")), Ok(()));
        assert_eq!(refuse_root(&status(nobody, nobody, "1000 998 ")), Ok(()));
        // root in any of the four uids, its saved uid included, which would
        // let the helper take root's id back
        for uid in [
            "0\t0\t0\t0",
            "65534\t0\t65534\t65534",
            "65534\t65534\t0\t65534",
            "65534\t65534\t65534\t0",
        ] {
            let why = refuse_root(&status(uid, nobody, "")).unwrap_err();
            assert!(why.contains("refuses to run as root"), "{why}");
        }
        // group 0 as any of the four gids, or among the supplementary groups
        for (gid, groups) in [
            ("0\t0\t0\t0", ""),
            ("65534\t65534\t0\t65534", ""),
            (nobody, "0"),
            (nobody, "1000 0 998"),
        ] {
            let why = refuse_root(&status(nobody, gid, groups)).unwrap_err();
            assert!(why.contains("group 0"), "{why}");
        }
        // a status it cannot read is refused
        for broken in [
            String::new(),
            status("65534\t65534", nobody, ""),
            status(nobody, "x\t65534\t65534\t65534", ""),
            format!("Uid:\t{nobody}\nGid:\t{nobody}\n"),
        ] {
            let why = refuse_root(&broken).unwrap_err();
            assert!(why.contains("cannot read its ids"), "{why}");
        }
        // the helper's own status reads, as the test's account
        let own = std::fs::read_to_string("/proc/self/status").unwrap();
        assert!(status_ids(&own, "Uid:").is_some_and(|ids| ids.len() == 4));
        assert!(status_ids(&own, "Groups:").is_some());
    }

    /// DejaVu Sans Mono's ASCII, whose cell at 16 pixels is 9x16.
    const DEJAVU: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/DejaVuSansMono-ascii.ttf"
    );
    const DEJAVU_CELL: Cell = Cell {
        width: 9,
        height: 16,
    };

    /// An ask of `frit` at DejaVu Sans Mono's cell for `chars`, drawn at
    /// `doubled`.
    fn doubled_ask(frit: FritText, chars: &[char], doubled: Option<Doubled>) -> ServeRequest {
        ServeRequest {
            cell: DEJAVU_CELL,
            doubled,
            ..request(ASK, frit, chars)
        }
    }

    /// The set of each glyph frame `frames` holds, and the cell of each of
    /// its strikes.
    fn answered(frames: &[(u32, Vec<u8>)]) -> Vec<(Arc<Set>, Vec<Cell>)> {
        frames
            .iter()
            .filter(|f| f.0 == FRAME_GLYPHS)
            .map(|f| {
                let set = Arc::new(Set::from_bytes(f.1.clone()).unwrap());
                let cells = set.cells();
                (set, cells)
            })
            .collect()
    }

    /// Where a request about `frit` holds its line size: after the magic,
    /// the kind, the frit's name and text with their lengths, the cell, the
    /// rows line spacing adds, the style, the script and the cluster width.
    fn line_size_at(frit: &FritText) -> usize {
        8 + 4 + 4 + frit.name.len() + 4 + frit.text.len() + 8 + 8 + 4 + 4 + 4
    }

    /// The one glyph frame `frames` holds, as [`answered`] reads it; fails,
    /// saying `what`, unless there is exactly one.
    fn one_answer(frames: &[(u32, Vec<u8>)], what: &str) -> (Arc<Set>, Vec<Cell>) {
        let mut all = answered(frames);
        assert_eq!(all.len(), 1, "{what}");
        all.remove(0)
    }

    /// The coverage in each of `g`'s rows, added up.
    fn row_ink(g: &Glyph) -> Vec<u64> {
        g.pixels()
            .chunks(g.width() as usize)
            .map(|row| row.iter().map(|&p| u64::from(p)).sum())
            .collect()
    }

    // an ask's line size reads back; a double size on any other request,
    // a superscript or subscript at a double size, a line size past double
    // height's 2, and a cell whose double passes CELL_SIDE_MAX are refused
    #[test]
    fn a_double_ask_reads_back_and_its_bounds_are_checked() {
        let font = frit(&format!("source=outline:{DEJAVU}\n"));
        for doubled in [Doubled::Width, Doubled::Height] {
            let ask = ServeRequest {
                style: BOLD_ITALIC,
                ..doubled_ask(font.clone(), &['H'], Some(doubled))
            };
            let bytes = ask.encode();
            let at = line_size_at(&font);
            assert_eq!(
                &bytes[at..at + 4],
                &doubled_code(Some(doubled)).to_le_bytes()
            );
            assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(ask)));
        }
        let fill = ServeRequest {
            doubled: Some(Doubled::Width),
            ..request(FILL, font.clone(), &['H'])
        };
        let e = ServeRequest::read(&mut &fill.encode()[..]).unwrap_err();
        assert!(
            e.contains("double size on a request that is not an ask"),
            "{e}"
        );
        let sup = ServeRequest {
            script: Script::Super,
            ..doubled_ask(font.clone(), &['H'], Some(Doubled::Height))
        };
        let e = ServeRequest::read(&mut &sup.encode()[..]).unwrap_err();
        assert!(
            e.contains("superscript or subscript at a double size"),
            "{e}"
        );
        let mut past = doubled_ask(font.clone(), &['H'], None).encode();
        let at = line_size_at(&font);
        past[at..at + 4].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(
            ServeRequest::read(&mut &past[..]),
            Err("an unknown line size".to_owned())
        );
        let tall = Cell {
            width: 9,
            height: CELL_SIDE_MAX / 2 + 1,
        };
        let height = ServeRequest {
            cell: tall,
            ..doubled_ask(font.clone(), &['H'], Some(Doubled::Height))
        };
        let e = ServeRequest::read(&mut &height.encode()[..]).unwrap_err();
        assert!(e.contains("double size whose cell"), "{e}");
        // the same cell doubles across at double width, which keeps its height
        let width = ServeRequest {
            cell: tall,
            doubled: Some(Doubled::Width),
            ..height
        };
        assert!(ServeRequest::read(&mut &width.encode()[..]).is_ok());
    }

    // issue #61, the maintainer's direction of 2026-11-14: an outline font
    // draws a line of double size at true double size. A double-width
    // ask's glyph is drawn in a cell twice as wide, from the outline
    // stretched to twice its width at the normal size: each row holds
    // twice the normal row's coverage, the rows unchanged, and the glyph is
    // not the normal one with each pixel doubled. A double-height ask's
    // glyph is drawn at twice the size, in a cell twice as wide and twice
    // as tall, its ink over twice the rows and four times the coverage,
    // and it too is not the normal glyph with its pixels doubled
    #[test]
    fn a_double_ask_renders_its_outline_glyphs_at_double_size() {
        let font = frit(&format!("source=outline:{DEJAVU}\n"));
        let chars = ['H', 'g', 'o'];
        let normal = run(&[doubled_ask(font.clone(), &chars, None)]);
        let (normal, _) = &one_answer(&normal, "one glyph frame at the normal size");
        let normal = SetFace::new(Arc::clone(normal), 16).unwrap();
        let wide = run(&[doubled_ask(font.clone(), &chars, Some(Doubled::Width))]);
        let (wide, cells) = &one_answer(&wide, "one glyph frame at double width");
        assert_eq!(
            cells,
            &[Cell {
                width: 18,
                height: 16
            }]
        );
        let wide = SetFace::new(Arc::clone(wide), 16).unwrap();
        let tall = run(&[doubled_ask(font, &chars, Some(Doubled::Height))]);
        let (tall, cells) = &one_answer(&tall, "one glyph frame at double height");
        assert_eq!(
            cells,
            &[Cell {
                width: 18,
                height: 32
            }]
        );
        let tall = SetFace::new(Arc::clone(tall), 32).unwrap();
        let ink_rows = |g: &Glyph| {
            let rows = row_ink(g);
            let first = rows.iter().position(|&n| n > 0).unwrap();
            let last = rows.iter().rposition(|&n| n > 0).unwrap();
            (first, last)
        };
        for c in chars {
            let n = normal.draw(&[c], Style::default()).unwrap();
            let w = wide.draw(&[c], Style::default()).unwrap();
            let t = tall.draw(&[c], Style::default()).unwrap();
            assert_eq!((w.width(), w.height()), (18, 16), "{c}");
            assert_eq!((t.width(), t.height()), (18, 32), "{c}");
            for (y, (&a, &b)) in row_ink(&n).iter().zip(&row_ink(&w)).enumerate() {
                // a row's ink moves by a fraction of a pixel as the pen
                // is placed in the wider cell, which changes its sum by a
                // few per cent where the glyph meets its cell's edges
                assert!(
                    b.abs_diff(2 * a) <= 2 * a / 20 + 18,
                    "{c} row {y}: {b} against twice {a}"
                );
            }
            let doubled_pairs = w.pixels().chunks(2).all(|p| p[0] == p[1]);
            assert!(!doubled_pairs, "{c}: each pixel doubled across");
            let (top, bottom) = ink_rows(&n);
            let (t_top, t_bottom) = ink_rows(&t);
            assert!(t_top.abs_diff(2 * top) <= 1, "{c}: {t_top} against {top}");
            assert!(
                t_bottom.abs_diff(2 * bottom + 1) <= 1,
                "{c}: {t_bottom} against {bottom}"
            );
            let (sum_n, sum_t): (u64, u64) = (row_ink(&n).iter().sum(), row_ink(&t).iter().sum());
            assert!(
                sum_t.abs_diff(4 * sum_n) <= 4 * sum_n / 10,
                "{c}: {sum_t} against four times {sum_n}"
            );
            let blocks = (0..16).all(|y| {
                (0..9).all(|x| {
                    let at = |dx: usize, dy: usize| t.pixels()[(2 * y + dy) * 18 + 2 * x + dx];
                    at(0, 0) == at(1, 0) && at(0, 0) == at(0, 1) && at(0, 0) == at(1, 1)
                })
            });
            assert!(!blocks, "{c}: each pixel doubled across and down");
        }
    }

    // a double ask leaves to the console what a bitmap source draws, even
    // where a later outline source covers it too, so the console doubles
    // its pixels as the DEC terminals did; the outline source draws the
    // rest at double size
    #[test]
    fn a_double_ask_leaves_a_bitmap_sources_glyphs_to_the_console() {
        let dir = scratch("doubled-bitmap");
        let path = dir.join("9x16.psf");
        let psf = crate::decode::tests::psf2(9, 16, 2, Some(&[(1, 'A')]));
        std::fs::write(&path, psf).unwrap();
        let font = frit(&format!(
            "source=file:{}\nsource=outline:{DEJAVU}\n",
            path.display()
        ));
        let normal = run(&[doubled_ask(font.clone(), &['A', 'B'], None)]);
        let (normal, _) = &one_answer(&normal, "one glyph frame at the normal size");
        let normal = SetFace::new(Arc::clone(normal), 16).unwrap();
        assert!(normal.has_glyph('A') && normal.has_glyph('B'));
        let frames = run(&[doubled_ask(font, &['A', 'B'], Some(Doubled::Width))]);
        let (wide, _) = &one_answer(&frames, "one glyph frame at double width");
        let wide = SetFace::new(Arc::clone(wide), 16).unwrap();
        assert!(!wide.has_glyph('A'));
        assert!(wide.has_glyph('B'));
        assert_eq!(frames.last().map(|f| f.0), Some(FRAME_DONE));
        // a frit of bitmap fonts alone answers a double ask with nothing
        let bitmap = frit(&format!("source=file:{}\n", path.display()));
        let frames = run(&[doubled_ask(bitmap, &['A'], Some(Doubled::Height))]);
        assert_eq!(frames.iter().map(|f| f.0).collect::<Vec<_>>(), [FRAME_DONE]);
    }

    /// The face of the one glyph set `frames` answer with, at `height`.
    fn answered_face(frames: &[(u32, Vec<u8>)], what: &str) -> (SetFace, Vec<Cell>) {
        let (set, cells) = one_answer(frames, what);
        let height = cells[0].height;
        (SetFace::new(set, height).unwrap(), cells)
    }

    // covers: text.line-size
    // issue #61: a color glyph on a line of double size comes from the
    // helper at that size, never with its pixels doubled. Test Mono's
    // U+1F600 is a wide COLR glyph, a red block under a half-clear blue
    // one. On a double-height pair the helper draws it in the doubled cell,
    // 32 by 32 for two cells of 8 by 16. On a line of double width it is
    // that glyph made half as tall, 32 by 16, each pair of rows averaged:
    // the normal glyph at twice its width
    #[test]
    fn a_double_ask_draws_a_color_glyph_at_its_size() {
        let dir = scratch("doubled-color");
        let path = dir.join("TestMono.ttf");
        std::fs::write(&path, super::ttf::test_mono().build()).unwrap();
        let font = frit(&format!("source=outline:{}\n", path.display()));
        let emoji = ['\u{1f600}'];
        let ask = |doubled| ServeRequest {
            doubled,
            ..request(ASK, font.clone(), &emoji)
        };
        let (normal, _) = answered_face(&run(&[ask(None)]), "at the normal size");
        let (tall, cells) = answered_face(&run(&[ask(Some(Doubled::Height))]), "double height");
        assert_eq!(
            cells,
            [Cell {
                width: 16,
                height: 32
            }]
        );
        let (wide, cells) = answered_face(&run(&[ask(Some(Doubled::Width))]), "double width");
        assert_eq!(
            cells,
            [Cell {
                width: 16,
                height: 16
            }]
        );
        let n = normal.draw(&emoji, Style::default()).unwrap();
        let t = tall.draw(&emoji, Style::default()).unwrap();
        let w = wide.draw(&emoji, Style::default()).unwrap();
        assert_eq!((n.format(), n.width(), n.height()), (Format::Rgba, 16, 16));
        assert_eq!(
            (t.format(), t.cells(), t.width(), t.height()),
            (Format::Rgba, Width::Wide, 32, 32)
        );
        assert_eq!(
            (w.format(), w.cells(), w.width(), w.height()),
            (Format::Rgba, Width::Wide, 32, 16)
        );
        assert_eq!(Some(w.pixels().to_vec()), halved(&t));
        // the double-height glyph holds the normal one's red over at least
        // three times as many pixels, as a glyph drawn at twice the size
        // does; each fits its cells by its own rounding, so the ratio is
        // not exactly four
        let red = |g: &Glyph| {
            g.pixels()
                .chunks(4)
                .filter(|p| p[0] > 200 && p[2] == 0)
                .count()
        };
        assert!(red(&t) >= 3 * red(&n), "{} against {}", red(&t), red(&n));
    }

    // covers: text.line-size
    // issue #61: a cluster on a line of double size comes from the helper
    // at that size. E with a grave accent, decomposed, is shaped whole in
    // the doubled cell of a double-height pair, 18 by 32, and on a line of
    // double width it is that glyph made half as tall, 18 by 16. The tall
    // glyph's ink spans about twice the rows of the normal glyph's
    #[test]
    fn a_double_ask_shapes_a_cluster_at_its_size() {
        let marks = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/DejaVuSansMono-marks.ttf"
        );
        let font = frit(&format!("source=outline:{marks}\n"));
        let key = ['E', '\u{300}'];
        let ask = |doubled| ServeRequest {
            doubled,
            clusters: vec![key.iter().map(|&c| u32::from(c)).collect()],
            ..doubled_ask(font.clone(), &[], doubled)
        };
        let (normal, _) = answered_face(&run(&[ask(None)]), "at the normal size");
        let (tall, cells) = answered_face(&run(&[ask(Some(Doubled::Height))]), "double height");
        assert_eq!(
            cells,
            [Cell {
                width: 18,
                height: 32
            }]
        );
        let (wide, cells) = answered_face(&run(&[ask(Some(Doubled::Width))]), "double width");
        assert_eq!(
            cells,
            [Cell {
                width: 18,
                height: 16
            }]
        );
        let n = normal.cluster(&key, Style::default()).unwrap();
        let t = tall.cluster(&key, Style::default()).unwrap();
        let w = wide.cluster(&key, Style::default()).unwrap();
        assert_eq!((t.width(), t.height()), (18, 32));
        assert_eq!((w.width(), w.height()), (18, 16));
        assert_eq!(Some(w.pixels().to_vec()), halved(&t));
        let span = |g: &Glyph| {
            let rows = row_ink(g);
            let first = rows.iter().position(|&n| n > 0).unwrap();
            rows.iter().rposition(|&n| n > 0).unwrap() - first + 1
        };
        assert!(
            span(&t).abs_diff(2 * span(&n)) <= 2,
            "{} against {}",
            span(&t),
            span(&n)
        );
    }

    // halving a glyph averages each pair of rows, rounded to the nearest
    #[test]
    fn halving_a_glyph_averages_each_pair_of_rows() {
        let g =
            Glyph::from_pixels(2, 4, Format::Mask, vec![0, 255, 0, 254, 10, 20, 11, 20]).unwrap();
        assert_eq!(halved(&g), Some(vec![0, 255, 11, 20]));
        let odd = Glyph::from_pixels(1, 3, Format::Mask, vec![1, 2, 3]).unwrap();
        assert_eq!(halved(&odd), None);
    }

    #[test]
    fn ranges_join_runs() {
        assert_eq!(ranges(&[1, 2, 3, 5, 7, 8]), vec![(1, 3), (5, 5), (7, 8)]);
        assert_eq!(ranges(&[]), Vec::<Cover>::new());
    }
}
