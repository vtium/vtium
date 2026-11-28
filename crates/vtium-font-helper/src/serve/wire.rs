//! The font helper's wire format: the requests the console writes and the
//! helper reads, with their magic, their kinds and their bounds, the codes
//! their fields carry, and the frames the helper writes back, its sandbox
//! report among them.

use std::io::{self, Read, Write};
use std::path::PathBuf;

use vtium_font::spacing::Band;
use vtium_font::{CELL_SIDE_MAX, CLUSTER_SCALARS_MAX, Cell, Doubled, Script, Style};
use vtium_platform::vt::font::{ConsoleFont, FONT_GLYPHS_MAX, FONT_PITCH_TALL, UniPair};

use crate::frit::{Cover, FRIT_BYTES_MAX, FRIT_SOURCES_MAX, valid_name};
#[cfg(doc)]
use crate::made::Made;
use crate::seccomp::Missing;
use crate::sources::Gate;
use vtium_render::{POINTER_SIDE_MAX, POINTER_SVG_BYTES_MAX, XCURSOR_FILE_MAX};
use vtium_settings::batch::{Batch, Entry};
use vtium_settings::cullet::{self, Rule};
use vtium_term::{ClusterWidth, PointerShape};

#[cfg(doc)]
use super::engine::DrawsNothing;
use super::sandbox::NoDomain;

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
/// A sources query's answer: each source left out, one a line, and each
/// font that differs from what its source needs. For a frit that reads and
/// has no source left, a first line of the kind `nothing` says why
/// ([`DrawsNothing::line_text`]).
pub const FRAME_SOURCES: u32 = 11;
/// The answer to a sources query for a frit that does not read, and to a
/// coverage query for a frit the helper draws nothing from: the reason, as
/// UTF-8. It is a kind of its own so that [`FRAME_FAILED`],
/// which the helper also sends when it fails as it starts, is never taken
/// as a frit's answer.
pub const FRAME_DRAWS_NOTHING: u32 = 12;
/// How an [`ASK_MADE`] made the superscript and subscript glyphs it drew
/// ([`Made::encode_all`]), sent once before its [`FRAME_DONE`].
pub const FRAME_MADE: u32 = 13;
/// The helper's report of its sandbox, sent once before its first answer,
/// which the console logs once a boot when a part is not in force: a u32
/// of [`SANDBOX_DOMAIN`], [`SANDBOX_FALLBACK`] and [`SANDBOX_SOCKETS`], with
/// why the seccomp filters are not in force in the byte at
/// [`SANDBOX_SECCOMP_SHIFT`] and why the landlock domain is not in the
/// byte at [`SANDBOX_DOMAIN_SHIFT`] ([`Sandboxed::word`]). A console of
/// another build never meets it, since its request's magic is refused
/// first.
///
/// References:
/// - ADR-1200 D15: the report and the reasons it carries.
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
/// The active fonts ([`ActiveAsk`]), read with a layout of its own.
pub const ACTIVE: u32 = 10;
/// A description of one font file ([`DescribeAsk`]), which only
/// vtium-config sends.
pub const DESCRIBE: u32 = 11;
/// The sources of a frit that the helper leaves out, as one `u32` with a
/// bit for each `source=` line, the first line the lowest bit
/// ([`Request::LeaveOut`]).
pub const LEAVE_OUT: u32 = 12;
/// The answer to a [`DescribeAsk`]: the font file's
/// [`crate::describe::Description`].
pub const FRAME_DESCRIPTION: u32 = 18;
/// The answer to a [`DescribeAsk`] when the helper has no description:
/// why, as text. The file is missing, the helper's gate refuses it, or it
/// is not a font that vtium reads.
pub const FRAME_NO_DESCRIPTION: u32 = 19;
/// The most bytes the paths of an [`ActiveAsk`] hold together. The console
/// sends every path of the merged batch, batch.conf's and the packages'
/// lists', so the bound is the merged batch's: a smaller one would refuse
/// the request of a machine whose lists pass batch.conf's own bound, and
/// every font helper would fail. The bound keeps the memory the helper
/// spends reading the request in check, and it is checked before each
/// path is allocated.
///
/// References:
/// - ADR-1205 D4, as amended on 2026-11-27: the merged batch's bound of 1 MiB, which bounds the request of active fonts.
pub const ACTIVE_BYTES_MAX: usize = vtium_settings::batch::MERGED_BATCH_MAX;
/// The longest path an [`ActiveAsk`] names: `PATH_MAX`, the longest path
/// the kernel takes, with its terminating zero.
pub const ACTIVE_PATH_MAX: usize = 4096;
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
/// What a fill draws after the screen's codepoints, in codepoint order:
/// printable Latin (Basic Latin, Latin-1 and Latin Extended-A), General
/// Punctuation, Arrows, box drawing, block elements and shades, and
/// Geometric Shapes. A program's interface draws its dashes, ellipses,
/// arrows, bullets and triangles from these, so they arrive with its text.
///
/// References:
/// - ADR-1200 D18: the fill's order, and its second tier, which the note
///   of 2026-11-25 widens with General Punctuation, Arrows and Geometric
///   Shapes.
pub const LATIN_AND_BOX: [Cover; 7] = [
    (0x21, 0x7e),
    (0xa1, 0x17f),
    (0x2010, 0x2027),
    (0x2030, 0x205e),
    (0x2190, 0x21ff),
    (0x2500, 0x259f),
    (0x25a0, 0x25ff),
];
/// The most ranges a fill's held codepoints take.
pub const HELD_RANGES_MAX: usize = 65_536;
/// The most bytes a frame's payload holds; a batch past it is cut in two
/// until it fits.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// [`FRAME_BYTES_MAX`] as a length.
const FRAME: usize = FRAME_BYTES_MAX as usize;
/// The most bytes a set in a glyph frame holds: the frame's, less the
/// band in front of it.
pub(super) const FRAME_SET: usize = FRAME - 8;
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

pub(super) fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn len32(n: usize) -> u32 {
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

/// Any request the helper reads: one of the glyph requests, a pointer
/// ask, the active fonts, a font file's description, or the sources left
/// out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Serve(ServeRequest),
    Pointer(PointerAsk),
    Active(ActiveAsk),
    Describe(DescribeAsk),
    /// The sources of each frit that the helper reads from now on leaves
    /// out, a bit for each `source=` line in the order the frit's text
    /// gives them, the first line the lowest bit. A frit read already is
    /// read again at its next request. The console sends it first to a
    /// helper it starts after a source made an earlier helper crash, hang
    /// or reach its CPU limit, and with a source left out on purpose while
    /// it looks for the source that did. A helper serves one frit for the
    /// console, so the bits name that frit's sources.
    ///
    /// References:
    /// - ADR-1200 D14: a crash, a fault or a timeout fails one source and
    ///   nothing else.
    LeaveOut(u32),
}

/// The bytes of a [`Request::LeaveOut`] that leaves out the sources whose
/// bits `mask` sets.
pub fn leave_out_request(mask: u32) -> Vec<u8> {
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, LEAVE_OUT);
    put32(&mut out, mask);
    out
}

/// A description of one font file, for vtium-config's Fonts tab and its
/// listing of the fonts found: the helper opens the file at `path` through
/// its gate, reads it in its sandbox, and answers at once with a
/// [`FRAME_DESCRIPTION`] or a [`FRAME_NO_DESCRIPTION`]. The console never
/// sends it. A helper that vtium-config starts holds the open gate, so it
/// describes a font in a user's folder too, which vtium-config shows but
/// never activates. A helper that holds the active fonts describes only
/// those.
///
/// References:
/// - ADR-1205 D3: vtium-config reads a font's family, coverage and sizes
///   only through the sandboxed helper, and may preview a user's font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescribeAsk {
    /// The font file, an absolute path.
    pub path: PathBuf,
}

impl DescribeAsk {
    /// The request's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        put32(&mut out, DESCRIBE);
        let bytes = self.path.as_os_str().as_encoded_bytes();
        put32(&mut out, len32(bytes.len()));
        out.extend_from_slice(bytes);
        out
    }

    /// Reads the path that follows the kind: 1 to [`ACTIVE_PATH_MAX`]
    /// bytes, UTF-8, absolute and with no NUL, checked before it is
    /// allocated.
    fn read_body<R: Read>(f: &mut Fields<'_, R>) -> Result<DescribeAsk, String> {
        let len = f.count(ACTIVE_PATH_MAX, "a font file's path")?;
        let path = String::from_utf8(f.bytes(len)?)
            .ok()
            .filter(|p| p.starts_with('/') && !p.contains('\0'))
            .ok_or("a font file's path that is not an absolute UTF-8 path")?;
        Ok(DescribeAsk {
            path: PathBuf::from(path),
        })
    }

    /// Opens the file through `gate`, reads at most
    /// [`crate::outline::OUTLINE_BYTES_MAX`] bytes of it, and writes its description, or
    /// why there is none.
    pub(super) fn answer(&self, gate: &Gate, out: &mut dyn Write) -> io::Result<()> {
        let described = gate
            .read(&self.path, crate::outline::OUTLINE_BYTES_MAX)
            .map_err(|e| e.to_string())
            .and_then(|bytes| crate::describe::describe(&bytes, &self.path.to_string_lossy()));
        match described {
            Ok(d) => write_frame(out, FRAME_DESCRIPTION, &d.encode()),
            Err(why) => reason(out, FRAME_NO_DESCRIPTION, &why),
        }
    }
}

/// The active fonts, which the console sends first to each helper it
/// starts: the paths the batch lists, and the account whose files pass the
/// trust rule beside root's. The console reads the batch, because it runs
/// as root and reads root's configuration, and it knows whose files count,
/// which a helper dropped to `nobody` cannot tell.
///
/// References:
/// - ADR-1205 D4: vtium draws only from the active resources, under the
///   trust rule, and accepts its own account's files when it runs
///   unprivileged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveAsk {
    /// The account whose files pass the trust rule beside root's: 0 when
    /// the console runs as root, else the console's own account, which
    /// the helper then runs as too.
    pub owner: u32,
    /// The paths the batch lists, each absolute.
    pub paths: Vec<PathBuf>,
}

impl ActiveAsk {
    /// The active fonts that `batch` lists, whose files pass the trust
    /// rule when root or `owner` owns them. A path that the helper would
    /// refuse, one that is not absolute UTF-8, holds a NUL byte or is
    /// longer than [`ACTIVE_PATH_MAX`], is left out, so the other paths
    /// still reach the helper, and the paths stop before they would pass
    /// [`ACTIVE_BYTES_MAX`], as the console's request does.
    pub fn of_batch(owner: u32, batch: &vtium_settings::batch::Batch) -> ActiveAsk {
        let mut paths = Vec::new();
        let mut total = 0usize;
        for vtium_settings::batch::Entry::Path(path) in batch.entries() {
            let Some(text) = path
                .to_str()
                .filter(|t| t.starts_with('/') && !t.contains('\0') && t.len() <= ACTIVE_PATH_MAX)
            else {
                continue;
            };
            total = total.saturating_add(text.len());
            if total > ACTIVE_BYTES_MAX {
                break;
            }
            paths.push(path.clone());
        }
        ActiveAsk { owner, paths }
    }

    /// The request's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        put32(&mut out, ACTIVE);
        put32(&mut out, self.owner);
        put32(&mut out, len32(self.paths.len()));
        for path in &self.paths {
            let bytes = path.as_os_str().as_encoded_bytes();
            put32(&mut out, len32(bytes.len()));
            out.extend_from_slice(bytes);
        }
        out
    }

    /// Reads the fields that follow the kind. Each path is checked: 1 to
    /// [`ACTIVE_PATH_MAX`] bytes, UTF-8, absolute and with no NUL, and the
    /// paths together within [`ACTIVE_BYTES_MAX`], checked before each is
    /// allocated.
    fn read_body<R: Read>(f: &mut Fields<'_, R>) -> Result<ActiveAsk, String> {
        let owner = f.u32()?;
        let n = f.count(ACTIVE_BYTES_MAX, "the active fonts")?;
        let mut paths = Vec::with_capacity(n.min(1024));
        let mut total = 0usize;
        for _ in 0..n {
            let len = f.count(ACTIVE_PATH_MAX, "an active font's path")?;
            total = total.saturating_add(len);
            if total > ACTIVE_BYTES_MAX {
                return Err(format!(
                    "active fonts' paths past their bound of {ACTIVE_BYTES_MAX} bytes"
                ));
            }
            let path = String::from_utf8(f.bytes(len)?)
                .ok()
                .filter(|p| p.starts_with('/') && !p.contains('\0'))
                .ok_or("an active font's path that is not an absolute UTF-8 path")?;
            paths.push(PathBuf::from(path));
        }
        Ok(ActiveAsk { owner, paths })
    }

    /// The gate this request opens in a helper that runs as `euid`: the
    /// batch of its paths, under the rule of root's files and the owner's.
    /// An owner other than root or the helper's own account is refused,
    /// since the console starts the helper either as `nobody`, from root,
    /// or as its own account.
    pub fn gate(&self, euid: u32) -> Result<Gate, String> {
        if self.owner != 0 && self.owner != euid {
            return Err(format!(
                "active fonts whose files belong to uid {}, which is neither root nor the font helper's own account",
                self.owner
            ));
        }
        Ok(Gate::Active(Box::new(cullet::Gate::new(
            Rule::running_as(self.owner),
            None,
            Batch::new(self.paths.iter().cloned().map(Entry::Path).collect()),
        ))))
    }
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
    } else if kind == ACTIVE {
        ActiveAsk::read_body(&mut f).map(|a| Some(Request::Active(a)))
    } else if kind == DESCRIBE {
        DescribeAsk::read_body(&mut f).map(|a| Some(Request::Describe(a)))
    } else if kind == LEAVE_OUT {
        // a bit past the most sources a frit holds names no source
        let mask = f.u32()?;
        if u32::try_from(FRIT_SOURCES_MAX)
            .ok()
            .and_then(|n| mask.checked_shr(n))
            .is_none_or(|past| past != 0)
        {
            return Err(format!(
                "sources left out past the {FRIT_SOURCES_MAX} a frit holds"
            ));
        }
        Ok(Some(Request::LeaveOut(mask)))
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
            Some(
                Request::Serve(_)
                | Request::Active(_)
                | Request::Describe(_)
                | Request::LeaveOut(_),
            ) => Err("a glyph request where a pointer ask was expected".to_owned()),
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

/// A frame's payload of `cell`: its width and height.
pub(super) fn cell_payload(cell: Cell) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8);
    put32(&mut payload, cell.width);
    put32(&mut payload, cell.height);
    payload
}

/// A frame's payload of `cell` whose line spacing adds `band`: its width,
/// its height, and the rows added above and below (ADR-1302 D13). Two
/// heads can draw one cell size with different bands, so a fill's end and
/// a stop name both.
pub(super) fn spaced_payload(cell: Cell, band: Band) -> Vec<u8> {
    let mut payload = cell_payload(cell);
    put32(&mut payload, band.above);
    put32(&mut payload, band.below);
    payload
}

/// A frame of glyphs: the rows line spacing adds to their cell, then
/// `set`, so the console holds them by the cell and its band.
pub(super) fn glyph_payload(band: Band, set: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(set.len().saturating_add(8));
    put32(&mut payload, band.above);
    put32(&mut payload, band.below);
    payload.extend_from_slice(set);
    payload
}

pub(super) fn failed(out: &mut dyn Write, why: &str) -> io::Result<()> {
    reason(out, FRAME_FAILED, why)
}

/// Writes `why`, cut at [`REASON_BYTES_MAX`], as a frame of `kind`.
pub(super) fn reason(out: &mut dyn Write, kind: u32, why: &str) -> io::Result<()> {
    let mut why = why.to_owned();
    while why.len() > REASON_BYTES_MAX {
        why.pop();
    }
    write_frame(out, kind, why.as_bytes())
}

/// The parts of the helper's sandbox in force, as the low bits of its
/// [`FRAME_SANDBOX`]: its landlock domain, its fallback filter, which
/// stands in for the domain on a kernel without landlock, and its socket
/// filter.
pub const SANDBOX_DOMAIN: u32 = 1;
pub const SANDBOX_FALLBACK: u32 = 2;
pub const SANDBOX_SOCKETS: u32 = 4;
/// Where [`FRAME_SANDBOX`] carries why the seccomp filters are not in
/// force, a [`Missing`], and why the landlock domain is not, a
/// [`NoDomain`]: each a byte, 0 when the part is in force.
pub const SANDBOX_SECCOMP_SHIFT: u32 = 8;
pub const SANDBOX_DOMAIN_SHIFT: u32 = 16;

/// The helper's report of its sandbox, which it sends as one u32 in its
/// [`FRAME_SANDBOX`]: the parts in force, why its seccomp filters are not,
/// and why its landlock domain is not. A console that reads only the low
/// bits still reads the parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sandboxed {
    /// [`SANDBOX_DOMAIN`], [`SANDBOX_FALLBACK`] and [`SANDBOX_SOCKETS`].
    pub parts: u32,
    /// Why the seccomp filters are not in force, `None` when they are.
    pub seccomp: Option<Missing>,
    /// Why the landlock domain is not in force, `None` when it is.
    pub domain: Option<NoDomain>,
}

impl Sandboxed {
    /// The report as [`FRAME_SANDBOX`] carries it.
    pub fn word(self) -> u32 {
        let seccomp = self.seccomp.map_or(0, |m| m as u32);
        let domain = self.domain.map_or(0, |d| d as u32);
        self.parts | seccomp << SANDBOX_SECCOMP_SHIFT | domain << SANDBOX_DOMAIN_SHIFT
    }
}
