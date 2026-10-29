//! The helper's serve mode (ADR-1301 D2): `vtium-vitrify serve`, which
//! the console starts on its first need for glyphs and which generates
//! them from a frit at run time. It sets the parser's sandbox on itself
//! (no-new-privileges, the memory and file limits, no children, refused as
//! root), takes the frit's name and text from the console, so a frit made
//! on the fly needs no file, reads the frit's fonts as the account the
//! console started it as, and answers in the strike chunk format the
//! console already reads.
//!
//! The console sends four requests (the operator's fill model,
//! 2026-10-28). An ask names codepoints at a cell, those on the screen or
//! newly needed, and is answered whole, ahead of any fill. A fill fills a
//! cell's coverage in order: the codepoints it names (the screen's), then
//! printable Latin and box drawing ([`LATIN_AND_BOX`]), then the rest of
//! the frit's coverage, skipping what the console holds, one [`BATCH`] at
//! a time; between batches the helper takes any waiting request, so a
//! request waits at most one batch. A stop ends a cell's fill, as when the
//! console's store is full or no display draws at the cell. A cell query
//! asks for the frit's own cell before the console's first frame, for a
//! frit of bitmap sources that states no cell (the operator, 2026-10-28):
//! the helper answers its first source's cell when every other source's
//! fits in it, and otherwise, an outline source among them, says why it
//! has none ([`Engine::own_cell`]). The helper
//! keeps what it sent only for the request that sent it, so it sends a
//! glyph once within one ask or one fill; the console's store is the one
//! record of what the console holds (ADR-1200 D18). The helper exits once
//! it has no request and no fill for [`HELPER_IDLE_MS`], or when the
//! console closes its end and its work is done.
//!
//! ```text
//! request  magic "VTVSREQ2", kind u32 (1 ask, 2 fill, 3 stop, 4 cell
//!          query), frit name length u32, the name (ADR-1200 D1's
//!          characters), frit text length u32, the text (UTF-8), cell width
//!          u32, cell height u32 (both 0 for a cell query), capabilities length u32, the capabilities (reserved for the
//!          display's, ADR-1301 D1 and D3: always 0 bytes, and refused
//!          otherwise), codepoint count u32, each codepoint u32 (an ask's
//!          wanted, a fill's first; none for a cell query), held range
//!          count u32, each range's first and last codepoint u32 (what the
//!          console holds at the cell, for a fill; none for a cell query),
//!          console font flag u32 (0 none, 1 one):
//!          width u32, height u32, glyph count u32, data length u32, the
//!          data, map count u32, each (codepoint u16, glyph u16)
//! answer   frames: kind u32, length u32, then the payload; kind 1 an
//!          ask's glyphs and 4 a fill's, each a vitrified set of one cell's
//!          glyphs; kind 2 the oldest ask answered (no payload); kind 5 a
//!          cell's fill done (its width u32 and height u32); kind 6 the
//!          frit's own cell (its width u32 and height u32); kind 7 no own
//!          cell (UTF-8 reason); kind 3 a failure (UTF-8 reason)
//! ```
//!
//! Little-endian throughout. The console checks every frame before it
//! trusts it: the length against [`FRAME_BYTES_MAX`] before it allocates,
//! a set through vtium-font's reader, every strike at the cell asked for,
//! an ask's glyphs among those it asked for, and a fill's among those it
//! does not hold.
//!
//! The kernel's font reaches a frit's `console:` source only as the bytes
//! the console read from its own VT at start and handed over
//! (vtium/vtium#145): the helper opens no VT, and every `console:VT` takes that font.
//!
//! The glyphs are drawn at the cell a request names, the frit's, and
//! never another (#145): an outline font at that cell's height and width,
//! and a bitmap font scaled by the largest whole factor that fits, centred
//! across and on the cell's bottom row, as `create` places one; a bitmap
//! font whose cell does not fit gives nothing, and the console draws those
//! characters in the built-in font.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use rustix::process::{Resource, Rlimit, Uid, geteuid, setrlimit};
use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
use vtium_font::{CELL_SIDE_MAX, Cell, Style, Width};
use vtium_platform::vt::font::{ConsoleFont, FONT_GLYPHS_MAX, FONT_PITCH_TALL, UniPair};

use crate::build::Font;
use crate::child::PARSER_MEMORY;
use crate::frit::{Cover, FRIT_BYTES_MAX, Frit, Source, valid_name};
use crate::outline::{OutlineRequest, family_key, rasterise};
use crate::sources::{Consoles, FONT_DIRS, Host, Input, KBD_DIRS, resolve};

/// The command that makes the binary the helper.
pub const SERVE_ARG: &str = "serve";
/// How long the helper waits for a request before it exits (the operator,
/// 2026-10-28; ADR-1301 D2).
pub const HELPER_IDLE_MS: u64 = 60_000;
/// The helper's CPU time over its life, in seconds: the kernel ends it
/// past this, and the console's next request starts another, a bounded
/// number of times a boot (the console's `RESTARTS_MAX`).
pub const HELPER_SECONDS: u64 = 300;

pub const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQ2";
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
/// The frit's own cell, a cell query's answer: its width and height.
pub const FRAME_OWN_CELL: u32 = 6;
/// The frit has no own cell, a cell query's answer: the reason, as UTF-8.
pub const FRAME_NO_OWN_CELL: u32 = 7;
/// An ask: the codepoints named, answered whole.
pub const ASK: u32 = 1;
/// A fill: the cell's coverage, in order, around the asks.
pub const FILL: u32 = 2;
/// The end of a cell's fill.
pub const STOP: u32 = 3;
/// A cell query: the frit's own cell, at no cell and naming nothing.
pub const OWN_CELL: u32 = 4;
/// What a fill draws after the screen's codepoints: printable Latin
/// (Basic Latin, Latin-1 and Latin Extended-A), then box drawing, block
/// elements and shades.
pub const LATIN_AND_BOX: [Cover; 3] = [(0x21, 0x7e), (0xa1, 0x17f), (0x2500, 0x259f)];
/// The most ranges a fill's held codepoints take.
pub const HELD_RANGES_MAX: usize = 65_536;
/// The most bytes a frame's payload holds; a batch past it is cut in two
/// until it fits.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
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
    /// [`ASK`], [`FILL`], [`STOP`] or [`OWN_CELL`].
    pub kind: u32,
    pub frit: FritText,
    pub cell: Cell,
    /// An ask's codepoints, or the codepoints a fill draws first: those on
    /// the screen.
    pub codepoints: Vec<u32>,
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
        // the display's capabilities, reserved
        put32(&mut out, 0);
        put32(&mut out, len32(self.codepoints.len()));
        for &c in &self.codepoints {
            put32(&mut out, c);
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
    /// requests. Every field is checked: the kind one of four, the frit's
    /// name a frit name (ADR-1200 D1), checked before it is allocated, and
    /// its text UTF-8, each inside its bound, the cell inside
    /// [`CELL_SIDE_MAX`], a cell query's at no cell and naming no
    /// codepoint and no range, the reserved capabilities empty, each codepoint a
    /// scalar value, each held range in order and inside Unicode, the
    /// kernel's font inside the kernel's own bounds with its data exactly
    /// as long as its shape.
    pub fn read(r: &mut impl Read) -> Result<Option<ServeRequest>, String> {
        let mut magic = [0u8; 8];
        let (head, rest) = magic.split_at_mut(1);
        match r.read(head) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(e) => return Err(format!("cannot read a request: {e}")),
        }
        r.read_exact(rest)
            .map_err(|_| "a request cut short".to_owned())?;
        if &magic != REQUEST_MAGIC {
            return Err("not a serve request".to_owned());
        }
        let mut f = Fields(r);
        let kind = f.u32()?;
        if !matches!(kind, ASK | FILL | STOP | OWN_CELL) {
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
        let query = kind == OWN_CELL;
        if query && (cell.width, cell.height) != (0, 0) {
            return Err("a cell query at a cell".to_owned());
        }
        if !query
            && (!(1..=CELL_SIDE_MAX).contains(&cell.width)
                || !(1..=CELL_SIDE_MAX).contains(&cell.height))
        {
            return Err("a cell past CELL_SIDE_MAX".to_owned());
        }
        if f.u32()? != 0 {
            return Err("display capabilities this helper does not read".to_owned());
        }
        let n = f.count(
            if query { 0 } else { FIRST_MAX },
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
        Ok(Some(ServeRequest {
            kind,
            frit: FritText { name, text },
            cell,
            codepoints,
            held,
            console,
        }))
    }
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

/// A frit's sources, ready to draw at any cell.
pub struct Engine {
    name: String,
    sources: Vec<(Source, Loaded)>,
    /// Every codepoint some source gives, sorted.
    coverage: Vec<u32>,
    /// The sources that failed as they drew, by their place in `sources`,
    /// and why: left out for the engine's life (ADR-1200 D14).
    failed: RefCell<BTreeMap<usize, String>>,
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

/// A 1-bit glyph `width` cells wide of a font of cell `from`, scaled by
/// `scale` into `cell`, centred across and on its bottom row.
fn place(glyph: &[u8], width: Width, from: Cell, cell: Cell, scale: u32) -> Vec<u8> {
    let cells = width.cells();
    let (src_w, dst_w) = (from.width * cells, cell.width * cells);
    let (src_row, dst_row) = (src_w.div_ceil(8) as usize, dst_w.div_ceil(8) as usize);
    let x0 = (dst_w - src_w * scale) / 2;
    let y0 = cell.height - from.height * scale;
    let mut out = vec![0u8; dst_row * cell.height as usize];
    for y in 0..from.height * scale {
        for x in 0..src_w * scale {
            if bit(glyph, src_row, x / scale, y / scale) {
                set_bit(&mut out, dst_row, x0 + x, y0 + y);
            }
        }
    }
    out
}

impl Engine {
    /// Parses `frit` and reads each source it names, as this process's
    /// account; a source missing or unreadable is left out, and the frit
    /// fails only when none is left. Its messages name the frit by its
    /// name.
    pub fn load(
        frit: &FritText,
        paths: &Paths,
        console: Option<ConsoleFont>,
    ) -> Result<Engine, String> {
        let file = Path::new(&frit.name);
        let frit =
            Frit::parse(frit.text.as_bytes(), file, &frit.name).map_err(|e| e.to_string())?;
        let handed = Handed(console);
        let host = Host {
            kbd_dirs: paths.kbd_dirs.clone(),
            font_dirs: paths.font_dirs.clone(),
            vconsole_conf: paths.vconsole_conf.clone(),
            kernel_cmdline: paths.kernel_cmdline.clone(),
            consoles: &handed,
        };
        let mut sources = Vec::new();
        let mut why = Vec::new();
        for source in &frit.sources {
            let loaded = resolve(source, &host)
                .map_err(|e| e.to_string())
                .and_then(|r| match r.input {
                    Input::Builtin(font) => Ok(Loaded::Bitmap(font)),
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
                });
            match loaded {
                Ok(l) => sources.push((source.clone(), l)),
                Err(e) => why.push(format!("source={}: {e}", source.spelling)),
            }
        }
        if sources.is_empty() {
            return Err(if why.is_empty() {
                format!("{}: the frit names no source", file.display())
            } else {
                why.join("; ")
            });
        }
        let mut coverage: Vec<u32> = sources
            .iter()
            .flat_map(|(source, l)| {
                let cs: Vec<u32> = match l {
                    Loaded::Bitmap(f) => f.map.keys().copied().collect(),
                    Loaded::Outline { mapped, .. } => mapped.clone(),
                };
                cs.into_iter().filter(|&c| source.covers(c))
            })
            .collect();
        coverage.sort_unstable();
        coverage.dedup();
        let name = if crate::frit::valid_name(&frit.name) {
            frit.name
        } else {
            "generated".to_owned()
        };
        Ok(Engine {
            name,
            sources,
            coverage,
            failed: RefCell::default(),
        })
    }

    /// Every codepoint the frit's sources give, sorted.
    pub fn coverage(&self) -> &[u32] {
        &self.coverage
    }

    /// The frit's own cell (the operator, 2026-10-28): its first source's,
    /// when every source is a bitmap font whose cell fits in it, so each
    /// draws at a whole factor; else why it has none, an outline source
    /// among its sources or a bitmap one whose cell does not fit.
    pub fn own_cell(&self) -> Result<Cell, String> {
        let mut own: Option<(&Source, Cell)> = None;
        for (source, loaded) in &self.sources {
            let Loaded::Bitmap(font) = loaded else {
                return Err(format!(
                    "source={} is an outline font, which has no cell of its own",
                    source.spelling
                ));
            };
            match own {
                None => own = Some((source, font.cell)),
                Some((first, cell))
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
                Some(_) => {}
            }
        }
        own.map(|(_, cell)| cell)
            .ok_or_else(|| "the frit names no source".to_owned())
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

    /// The glyphs at `cell` for `want`, sorted codepoints: each from the
    /// first source in the frit's order that gives it, as a vitrified set
    /// of that one cell; `None` when no source gives any. A source that
    /// fails as it draws is left out, and the next one gives its glyphs.
    pub fn draw(&self, cell: Cell, want: &[u32]) -> Result<Option<Vec<u8>>, String> {
        let strike = |format| {
            StrikeBuilder::new(format, cell, Style::default())
                .baseline(cell.height)
                .compress(false)
        };
        let (mut bits, mut r8, mut rgba) = (
            strike(PixelFormat::Bits),
            strike(PixelFormat::R8),
            strike(PixelFormat::Rgba),
        );
        let mut mapped: BTreeSet<u32> = BTreeSet::new();
        for (i, (source, loaded)) in self.sources.iter().enumerate() {
            if self.failed.borrow().contains_key(&i) {
                continue;
            }
            let left: Vec<u32> = want
                .iter()
                .copied()
                .filter(|&c| !mapped.contains(&c) && source.covers(c))
                .collect();
            if left.is_empty() {
                continue;
            }
            match loaded {
                Loaded::Bitmap(font) => {
                    let scale = (cell.width / font.cell.width.max(1))
                        .min(cell.height / font.cell.height.max(1));
                    if scale == 0 {
                        continue;
                    }
                    let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
                    for c in left {
                        let (Some(&g), Some(ch)) = (font.map.get(&c), char::from_u32(c)) else {
                            continue;
                        };
                        let index = match placed.get(&g) {
                            Some(&i) => i,
                            None => {
                                let Some((width, rows)) = font.glyphs.get(g as usize) else {
                                    continue;
                                };
                                let i =
                                    bits.glyph(*width, place(rows, *width, font.cell, cell, scale));
                                placed.insert(g, i);
                                i
                            }
                        };
                        bits.map(ch, index);
                        mapped.insert(c);
                    }
                }
                Loaded::Outline { font, family, .. } => {
                    let request = OutlineRequest {
                        font: Arc::clone(font),
                        family: family.clone(),
                        cells: vec![(cell.width, cell.height)],
                        cover: ranges(&left),
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
                    let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
                    for &(c, g) in &s.map {
                        let (Some(glyph), Some(ch)) = (s.glyphs.get(g as usize), char::from_u32(c))
                        else {
                            continue;
                        };
                        if mapped.contains(&c) {
                            continue;
                        }
                        let target = if glyph.color { &mut rgba } else { &mut r8 };
                        let index = *placed.entry(g).or_insert_with(|| {
                            let width = if glyph.wide {
                                Width::Wide
                            } else {
                                Width::Narrow
                            };
                            target.glyph(width, glyph.pixels.clone())
                        });
                        target.map(ch, index);
                        mapped.insert(c);
                    }
                }
            }
        }
        if mapped.is_empty() {
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

/// Draws `want` at `cell` and writes it as frames of `kind`, a set that
/// passes [`FRAME_BYTES_MAX`] cut in two. Returns whether any glyph went, or the engine's
/// failure; only a write that fails is the outer error.
fn send_glyphs(
    engine: &Engine,
    cell: Cell,
    want: &[u32],
    kind: u32,
    out: &mut dyn Write,
) -> io::Result<Result<bool, String>> {
    let mut queue: VecDeque<&[u32]> = VecDeque::from([want]);
    let mut any = false;
    while let Some(want) = queue.pop_front() {
        if want.is_empty() {
            continue;
        }
        match engine.draw(cell, want) {
            Ok(None) => {}
            Ok(Some(set)) if set.len() > FRAME_BYTES_MAX as usize && want.len() > 1 => {
                let (a, b) = want.split_at(want.len() / 2);
                queue.push_front(b);
                queue.push_front(a);
            }
            Ok(Some(set)) if set.len() > FRAME_BYTES_MAX as usize => {}
            Ok(Some(set)) => {
                write_frame(out, kind, &set)?;
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
            order,
            next: 0,
            any: !request.held.is_empty(),
        }
    }
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
type Requests = mpsc::Receiver<Result<Option<ServeRequest>, String>>;

fn read_requests(input: impl Read + Send + 'static) -> Requests {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut input = input;
        loop {
            let r = ServeRequest::read(&mut input);
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
    answer_requests(&read_requests(input), out, paths, idle)
}

/// The helper's work: the asks waiting, in order, and the fills.
struct Work<'p> {
    paths: &'p Paths,
    /// Each frit's engine, by its name and text, so a frit whose text
    /// changes is read again.
    engines: BTreeMap<FritText, Result<Engine, String>>,
    console: Option<ConsoleFont>,
    asks: VecDeque<ServeRequest>,
    fills: VecDeque<Fill>,
}

impl Work<'_> {
    fn engine(&mut self, frit: &FritText) -> &Result<Engine, String> {
        let (paths, console) = (self.paths, self.console.clone());
        self.engines
            .entry(frit.clone())
            .or_insert_with(|| Engine::load(frit, paths, console))
    }

    /// Takes a request in: an ask waits its turn, a fill replaces the
    /// cell's, a stop ends it, and a cell query is answered at once.
    fn take(&mut self, request: ServeRequest, out: &mut dyn Write) -> io::Result<()> {
        if self.console.is_none() {
            self.console.clone_from(&request.console);
        }
        let same = |f: &Fill| f.frit == request.frit && f.cell == request.cell;
        match request.kind {
            ASK => self.asks.push_back(request),
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
                    Ok(engine) => engine.own_cell(),
                    Err(why) => Err(why.clone()),
                };
                return match own {
                    Ok(cell) => {
                        let mut payload = Vec::with_capacity(8);
                        put32(&mut payload, cell.width);
                        put32(&mut payload, cell.height);
                        write_frame(out, FRAME_OWN_CELL, &payload)
                    }
                    Err(why) => reason(out, FRAME_NO_OWN_CELL, &why),
                };
            }
            _ => self.fills.retain(|f| !same(f)),
        }
        Ok(())
    }

    /// Answers the oldest ask whole, then done; or the engine's failure.
    fn ask(&mut self, request: &ServeRequest, out: &mut dyn Write) -> io::Result<()> {
        let mut want = request.codepoints.clone();
        want.sort_unstable();
        want.dedup();
        let cell = request.cell;
        let mut result = Ok(());
        match self.engine(&request.frit) {
            Ok(engine) => {
                for w in want.chunks(BATCH) {
                    if let Err(why) = send_glyphs(engine, cell, w, FRAME_GLYPHS, out)? {
                        result = Err(why);
                        break;
                    }
                }
            }
            Err(why) => result = Err(why.clone()),
        }
        match result {
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
            Ok(engine) => send_glyphs(engine, fill.cell, &batch, FRAME_FILL, out)?,
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
        let mut cell = Vec::with_capacity(8);
        put32(&mut cell, fill.cell.width);
        put32(&mut cell, fill.cell.height);
        write_frame(out, FRAME_FILLED, &cell)
    }
}

fn answer_requests(rx: &Requests, out: &mut dyn Write, paths: &Paths, idle: Duration) -> u8 {
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
                    if work.take(r, out).is_err() {
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
            Some(ask) => work.ask(&ask, out),
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
    refuse_root(geteuid())
}

/// The helper never runs as root (ADR-1200 D15): `euid` root is refused.
fn refuse_root(euid: Uid) -> Result<(), String> {
    if euid.is_root() {
        return Err("the helper refuses to run as root".to_owned());
    }
    Ok(())
}

/// `vtium-vitrify serve`: no-new-privileges, the requests' reader, the
/// limits, then the requests on stdin answered on stdout until
/// [`HELPER_IDLE_MS`] passes with none. A failure to sandbox is written as
/// a failed frame, so the console logs it.
pub fn command(paths: &Paths) -> u8 {
    let mut out = io::stdout().lock();
    if let Err(e) = rustix::thread::set_no_new_privs(true) {
        let _ = failed(
            &mut out,
            &format!("the helper cannot set no-new-privileges: {e}"),
        );
        return 1;
    }
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
    )
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::time::Instant;
    use vtium_font::Face;
    use vtium_font::set::{Set, SetFace};

    /// The frames in `bytes`, kind and payload.
    fn frames(mut bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let kind = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            out.push((kind, bytes[8..8 + len].to_vec()));
            bytes = &bytes[8 + len..];
        }
        out
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
            kind,
            frit,
            cell: CELL,
            codepoints: codepoints.iter().map(|&c| u32::from(c)).collect(),
            held: Vec::new(),
            console: None,
        }
    }

    fn run(input: &[ServeRequest]) -> Vec<(u32, Vec<u8>)> {
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
        frames(&out)
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

    #[test]
    fn a_malformed_request_is_refused_before_allocation() {
        // the frit `a` with no text: its cell at 21, its capabilities at
        // 29, its codepoint at 37
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
        for v in [0u32, 8, 16, 0, u32::MAX] {
            many.extend_from_slice(&v.to_le_bytes());
        }
        let e = ServeRequest::read(&mut &many[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
        // the kind, the cell's width and height, capabilities not yet
        // read, the codepoint, the held count
        for (at, value) in [
            (8, 9u32),
            (21, 0),
            (25, CELL_SIDE_MAX + 1),
            (29, 1),
            (37, 0xd800),
            (41, u32::MAX),
        ] {
            let mut bad = good.clone();
            bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(ServeRequest::read(&mut &bad[..]).is_err(), "{at}");
        }
        let backwards = ServeRequest {
            held: vec![(0x42, 0x41)],
            ..request(FILL, a, &[])
        };
        assert!(ServeRequest::read(&mut &backwards.encode()[..]).is_err());
        for name in ["", "a\nb"] {
            let named = ServeRequest {
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

    /// A cell query for `frit`: at no cell, naming nothing.
    fn query(frit: FritText) -> ServeRequest {
        ServeRequest {
            cell: Cell {
                width: 0,
                height: 0,
            },
            ..request(OWN_CELL, frit, &[])
        }
    }

    /// The frit `t` whose sources are `sources`, each a PSF2 font written
    /// to a file of its own as `(width, height)`, or a source's spelling.
    fn psf_frit(name: &str, sources: &[Result<(u32, u32), &str>]) -> (FritText, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vtium-serve-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
        let (psf, dir) = psf_frit("own-psf", &[Ok((8, 16))]);
        let (mixed, mixed_dir) = psf_frit("own-mixed", &[Ok((8, 16)), Err("builtin:unscii-8")]);
        let frames = run(&[query(psf), query(mixed)]);
        let _ = (
            std::fs::remove_dir_all(dir),
            std::fs::remove_dir_all(mixed_dir),
        );
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
        let (frit, dir) = psf_frit("own-unfit", &[Ok((8, 8)), Err("builtin:unifont")]);
        let frames = run(&[
            query(frit),
            query(self::frit("source=file:/nonexistent/x.psf\n")),
        ]);
        let _ = std::fs::remove_dir_all(dir);
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

    // a cell query names no cell, no codepoint and no range
    #[test]
    fn a_cell_query_at_a_cell_or_naming_codepoints_is_refused() {
        let good = query(frit("")).encode();
        assert!(ServeRequest::read(&mut &good[..]).unwrap().is_some());
        for bad in [
            ServeRequest {
                cell: CELL,
                ..query(frit(""))
            },
            ServeRequest {
                codepoints: vec![0x41],
                ..query(frit(""))
            },
            ServeRequest {
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
            held: vec![(0x42, 0x42)],
            ..request(FILL, frit, &['α'])
        };
        let frames = run(&[fill]);
        assert_eq!(frames.last().map(|f| f.0), Some(FRAME_FILLED));
        assert_eq!(frames.last().unwrap().1, [8, 0, 0, 0, 16, 0, 0, 0]);
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
        work.take(request(STOP, frit, &[]), &mut out).unwrap();
        assert!(work.fills.is_empty());
        assert!(out.is_empty());
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
                cell: small,
                ..request(ASK, frit.clone(), &['A'])
            },
            ServeRequest {
                cell: small,
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

    #[test]
    fn the_helper_refuses_to_run_as_root() {
        let why = refuse_root(Uid::ROOT).unwrap_err();
        assert!(why.contains("refuses to run as root"), "{why}");
        assert_eq!(refuse_root(Uid::from_raw(65534)), Ok(()));
    }

    #[test]
    fn ranges_join_runs() {
        assert_eq!(ranges(&[1, 2, 3, 5, 7, 8]), vec![(1, 3), (5, 5), (7, 8)]);
        assert_eq!(ranges(&[]), Vec::<Cover>::new());
    }
}
