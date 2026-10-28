//! The helper's serve mode (ADR-1301 D2): `vtium-vitrify
//! serve`, which the console starts on its first need for glyphs and
//! which generates them from a frit at run time. It sets the parser's
//! sandbox on itself (no-new-privileges, the memory and file limits, no
//! children, refused as root), reads the frit and its fonts as the account
//! the console started it as, and answers each request with the frit's
//! glyphs at the cell asked for: the codepoints the request names first,
//! then the rest of the frit's coverage, a batch at a time, in the strike
//! chunk format the console already reads. It exits after
//! [`HELPER_IDLE_MS`] with no request, or when the console closes its end.
//!
//! ```text
//! request  magic "VTVSREQ1", frit path length u32, the path (UTF-8,
//!          absolute), cell width u32, cell height u32, first count u32,
//!          each codepoint u32, console font flag u32 (0 none, 1 one):
//!          width u32, height u32, glyph count u32, data length u32, the
//!          data, map count u32, each (codepoint u16, glyph u16)
//! answer   frames: kind u32, length u32, then the payload; kind 1 a
//!          vitrified set of one cell's glyphs, kind 2 the request done
//!          (no payload), kind 3 the request failed (UTF-8 reason)
//! ```
//!
//! Little-endian throughout. The console checks every frame before it
//! trusts it: the length against [`FRAME_BYTES_MAX`] before it allocates,
//! a set through vtium-font's reader, and every strike at the cell it
//! asked for. No codepoint is in two batches of one request.
//!
//! The kernel's font reaches a frit's `console:` source only as the bytes
//! the console read from its own VT at start and handed over
//! (vtium/vtium#145): the helper opens no VT, and every `console:VT` takes that font.
//!
//! The glyphs fill the console's cell, which the built-in font set: an
//! outline font is drawn at that cell's height and width, and a bitmap
//! font is scaled by the largest whole factor that fits, centred across
//! and on the cell's bottom row, as `create` places one; a bitmap font
//! whose cell does not fit gives nothing, and the console draws those
//! characters in the built-in font.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use rustix::process::{Resource, Rlimit, geteuid, setrlimit};
use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
use vtium_font::{CELL_SIDE_MAX, Cell, Style, Width};
use vtium_platform::vt::font::{ConsoleFont, FONT_GLYPHS_MAX, FONT_PITCH_TALL, UniPair};

use crate::build::Font;
use crate::child::PARSER_MEMORY;
use crate::frit::{Cover, Frit, Source};
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

pub const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQ1";
/// A frame of glyphs: a vitrified set.
pub const FRAME_GLYPHS: u32 = 1;
/// The request is answered whole.
pub const FRAME_DONE: u32 = 2;
/// The request failed: the reason, as UTF-8.
pub const FRAME_FAILED: u32 = 3;
/// The most bytes a frame's payload holds; a batch past it is cut in two
/// until it fits.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// The most bytes a failure's reason holds.
pub const REASON_BYTES_MAX: usize = 4096;
/// The most bytes a frit's path holds.
pub const PATH_BYTES_MAX: usize = 4096;
/// The most codepoints a request names first: a screen of 320 by 100
/// cells, each different.
pub const FIRST_MAX: usize = 32_000;
/// How many codepoints of the rest go in one batch.
pub const BATCH: usize = 1024;

/// What the console asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeRequest {
    /// The frit, by its absolute path.
    pub frit: PathBuf,
    pub cell: Cell,
    /// The codepoints drawn first: those on the screen.
    pub first: Vec<u32>,
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

    fn bytes(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut out = vec![0u8; n];
        self.0
            .read_exact(&mut out)
            .map_err(|_| "a request cut short".to_owned())?;
        Ok(out)
    }
}

impl ServeRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        let path = self.frit.to_string_lossy();
        put32(&mut out, len32(path.len()));
        out.extend_from_slice(path.as_bytes());
        put32(&mut out, self.cell.width);
        put32(&mut out, self.cell.height);
        put32(&mut out, len32(self.first.len()));
        for &c in &self.first {
            put32(&mut out, c);
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
    /// requests. Every field is checked: the path absolute UTF-8, the cell
    /// inside [`CELL_SIDE_MAX`], each codepoint a scalar value, the
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
        let n = f.count(PATH_BYTES_MAX, "a frit's path")?;
        let path = String::from_utf8(f.bytes(n)?).map_err(|_| "a path that is not UTF-8")?;
        if !path.starts_with('/') {
            return Err("a frit's path that is not absolute".to_owned());
        }
        let cell = Cell {
            width: f.u32()?,
            height: f.u32()?,
        };
        if !(1..=CELL_SIDE_MAX).contains(&cell.width) || !(1..=CELL_SIDE_MAX).contains(&cell.height)
        {
            return Err("a cell past CELL_SIDE_MAX".to_owned());
        }
        let n = f.count(FIRST_MAX, "the codepoints asked for first")?;
        let mut first = Vec::with_capacity(n);
        for _ in 0..n {
            let c = f.u32()?;
            if char::from_u32(c).is_none() {
                return Err("a codepoint that is not a Unicode scalar value".to_owned());
            }
            first.push(c);
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
            frit: PathBuf::from(path),
            cell,
            first,
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
    /// Reads the frit at `path` and each source it names, as this process's
    /// account; a source missing or unreadable is left out, and the frit
    /// fails only when none is left.
    pub fn load(
        path: &Path,
        paths: &Paths,
        console: Option<ConsoleFont>,
    ) -> Result<Engine, String> {
        let frit = Frit::load(path).map_err(|e| e.to_string())?;
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
                format!("{}: the frit names no source", path.display())
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
        })
    }

    /// Every codepoint the frit's sources give, sorted.
    pub fn coverage(&self) -> &[u32] {
        &self.coverage
    }

    /// Why no source gives a glyph at `cell`: each bitmap source whose own
    /// cell does not fit it.
    pub fn none_at(&self, cell: Cell) -> String {
        let unfit: Vec<String> = self
            .sources
            .iter()
            .filter_map(|(source, loaded)| match loaded {
                Loaded::Bitmap(f) if f.cell.width > cell.width || f.cell.height > cell.height => {
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
    /// of that one cell; `None` when no source gives any.
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
        for (source, loaded) in &self.sources {
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
                    let outline = rasterise(&request)?;
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

/// Answers `request` with `engine`'s glyphs: the codepoints it names
/// first, then the rest of the coverage a batch at a time, then done; or,
/// when no source gives a glyph at the cell, a failure naming why. A
/// batch whose set passes [`FRAME_BYTES_MAX`] is cut in two. Only a write
/// that fails is the error: the console is gone.
fn answer(engine: &Engine, request: &ServeRequest, out: &mut dyn Write) -> io::Result<()> {
    let mut first: Vec<u32> = request.first.clone();
    first.sort_unstable();
    first.dedup();
    let rest: Vec<u32> = engine
        .coverage()
        .iter()
        .copied()
        .filter(|c| first.binary_search(c).is_err())
        .collect();
    let mut queue: VecDeque<&[u32]> = std::iter::once(&first[..])
        .chain(rest.chunks(BATCH))
        .collect();
    let mut sent = false;
    while let Some(want) = queue.pop_front() {
        if want.is_empty() {
            continue;
        }
        match engine.draw(request.cell, want) {
            Ok(None) => {}
            Ok(Some(set)) if set.len() > FRAME_BYTES_MAX as usize && want.len() > 1 => {
                let (a, b) = want.split_at(want.len() / 2);
                queue.push_front(b);
                queue.push_front(a);
            }
            Ok(Some(set)) if set.len() > FRAME_BYTES_MAX as usize => {}
            Ok(Some(set)) => {
                write_frame(out, FRAME_GLYPHS, &set)?;
                sent = true;
            }
            Err(why) => return failed(out, &why),
        }
    }
    if !sent {
        return failed(out, &engine.none_at(request.cell));
    }
    write_frame(out, FRAME_DONE, &[])
}

fn failed(out: &mut dyn Write, why: &str) -> io::Result<()> {
    let mut why = why.to_owned();
    while why.len() > REASON_BYTES_MAX {
        why.pop();
    }
    write_frame(out, FRAME_FAILED, why.as_bytes())
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

/// Serves requests read from `input` until it ends, or until `idle` passes
/// with none, writing the answers to `out`. Returns the exit status: 0 at
/// the input's end or the idle exit, 1 on a malformed request or a write
/// that fails.
pub fn serve(
    input: impl Read + Send + 'static,
    out: &mut dyn Write,
    paths: &Paths,
    idle: Duration,
) -> u8 {
    answer_requests(&read_requests(input), out, paths, idle)
}

fn answer_requests(rx: &Requests, out: &mut dyn Write, paths: &Paths, idle: Duration) -> u8 {
    let mut engines: BTreeMap<PathBuf, Result<Engine, String>> = BTreeMap::new();
    let mut console: Option<ConsoleFont> = None;
    loop {
        let request = match rx.recv_timeout(idle) {
            Ok(Ok(Some(r))) => r,
            Ok(Ok(None)) | Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => {
                return 0;
            }
            Ok(Err(why)) => {
                let _ = failed(out, &format!("a malformed request: {why}"));
                return 1;
            }
        };
        if console.is_none() {
            console.clone_from(&request.console);
        }
        let engine = engines
            .entry(request.frit.clone())
            .or_insert_with(|| Engine::load(&request.frit, paths, console.clone()));
        let wrote = match engine {
            Ok(engine) => answer(engine, &request, out),
            Err(why) => failed(out, why),
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
    if geteuid().is_root() {
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

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let d = std::env::temp_dir().join(format!(
                "vtium-serve-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn frit(&self, text: &str) -> PathBuf {
            let p = self.0.join("t.vtfrit");
            std::fs::write(&p, text).unwrap();
            p
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn request(frit: PathBuf, first: &[char]) -> ServeRequest {
        ServeRequest {
            frit,
            cell: Cell {
                width: 8,
                height: 16,
            },
            first: first.iter().map(|&c| u32::from(c)).collect(),
            console: None,
        }
    }

    #[test]
    fn a_request_reads_back() {
        let r = ServeRequest {
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
            ..request(PathBuf::from("/etc/vtium/fonts/a.vtfrit"), &['A', '中'])
        };
        let bytes = r.encode();
        assert_eq!(ServeRequest::read(&mut &bytes[..]), Ok(Some(r)));
        assert_eq!(ServeRequest::read(&mut &b""[..]), Ok(None));
    }

    #[test]
    fn a_malformed_request_is_refused_before_allocation() {
        let good = request(PathBuf::from("/a.vtfrit"), &['A']).encode();
        // the path's length past its bound, then nothing: refused on the
        // count, before a buffer of that size is made
        let mut huge = REQUEST_MAGIC.to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        let e = ServeRequest::read(&mut &huge[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
        let mut many = REQUEST_MAGIC.to_vec();
        many.extend_from_slice(&9u32.to_le_bytes());
        many.extend_from_slice(b"/a.vtfrit");
        many.extend_from_slice(&8u32.to_le_bytes());
        many.extend_from_slice(&16u32.to_le_bytes());
        many.extend_from_slice(&u32::MAX.to_le_bytes());
        let e = ServeRequest::read(&mut &many[..]).unwrap_err();
        assert!(e.contains("past its bound"), "{e}");
        for (at, value) in [(21, 0u32), (25, CELL_SIDE_MAX + 1)] {
            let mut bad = good.clone();
            bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(ServeRequest::read(&mut &bad[..]).is_err(), "{at}");
        }
        let mut surrogate = good.clone();
        surrogate[33..37].copy_from_slice(&0xd800u32.to_le_bytes());
        assert!(ServeRequest::read(&mut &surrogate[..]).is_err());
        let relative = request(PathBuf::from("a.vtfrit"), &[]).encode();
        assert!(ServeRequest::read(&mut &relative[..]).is_err());
        assert!(ServeRequest::read(&mut &good[..good.len() - 1]).is_err());
    }

    #[test]
    fn the_first_codepoints_come_first_then_the_rest_once_then_done() {
        let d = Dir::new("order");
        let frit = d.frit("source=builtin:unscii-8\n");
        let mut out = Vec::new();
        let input = request(frit, &['Z', 'A']).encode();
        assert_eq!(
            serve(
                std::io::Cursor::new(input),
                &mut out,
                &Paths::default(),
                Duration::from_secs(5)
            ),
            0
        );
        let frames = frames(&out);
        assert_eq!(frames.last().map(|f| f.0), Some(FRAME_DONE));
        let sets: Vec<Arc<Set>> = frames
            .iter()
            .filter(|f| f.0 == FRAME_GLYPHS)
            .map(|f| Arc::new(Set::from_bytes(f.1.clone()).unwrap()))
            .collect();
        assert!(sets.len() > 1, "{}", sets.len());
        let faces: Vec<SetFace> = sets
            .iter()
            .map(|s| SetFace::new(Arc::clone(s), 16).unwrap())
            .collect();
        // the first holds what was asked for first and nothing else
        let has = |f: &SetFace, c| f.has_glyph(c);
        assert!(has(&faces[0], 'A') && has(&faces[0], 'Z') && !has(&faces[0], 'B'));
        // every set is at the cell asked for, and no codepoint is in two
        for s in &sets {
            assert_eq!(
                s.cells(),
                vec![Cell {
                    width: 8,
                    height: 16
                }]
            );
        }
        for c in ['A', 'B', 'é', '█'] {
            let n = faces.iter().filter(|f| has(f, c)).count();
            assert_eq!(n, 1, "{c}");
        }
        // Unscii-8's 8x8 sits on the bottom half of an 8x16 cell, scaled
        // by 1
        let a = faces[0].draw(&['A'], Style::default()).unwrap();
        assert_eq!((a.width(), a.height()), (8, 16));
        assert!((0..8).all(|y| (0..8).all(|x| a.pixel(x, y).unwrap()[0] == 0)));
    }

    #[test]
    fn a_frit_with_no_source_here_fails_the_request_and_the_helper_serves_on() {
        let d = Dir::new("missing");
        let frit = d.frit("source=file:/nonexistent/x.psf\n");
        let mut input = request(frit.clone(), &['A']).encode();
        input.extend(request(frit, &['B']).encode());
        let mut out = Vec::new();
        assert_eq!(
            serve(
                std::io::Cursor::new(input),
                &mut out,
                &Paths::default(),
                Duration::from_secs(5)
            ),
            0
        );
        let frames = frames(&out);
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().all(|f| f.0 == FRAME_FAILED));
        assert!(String::from_utf8_lossy(&frames[0].1).contains("not on this machine"));
    }

    #[test]
    fn a_request_no_source_can_fill_fails_naming_why() {
        // unifont's 8x16 does not fit a 4x8 cell: no glyph, and the reason
        let d = Dir::new("nofit");
        let frit = d.frit("source=builtin:unifont\n");
        let r = ServeRequest {
            cell: Cell {
                width: 4,
                height: 8,
            },
            ..request(frit, &['A'])
        };
        let mut out = Vec::new();
        assert_eq!(
            serve(
                std::io::Cursor::new(r.encode()),
                &mut out,
                &Paths::default(),
                Duration::from_secs(5)
            ),
            0
        );
        let frames = frames(&out);
        assert_eq!(
            frames.len(),
            1,
            "{:?}",
            frames.iter().map(|f| f.0).collect::<Vec<_>>()
        );
        assert_eq!(frames[0].0, FRAME_FAILED);
        let why = String::from_utf8_lossy(&frames[0].1);
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
    fn ranges_join_runs() {
        assert_eq!(ranges(&[1, 2, 3, 5, 7, 8]), vec![(1, 3), (5, 5), (7, 8)]);
        assert_eq!(ranges(&[]), Vec::<Cover>::new());
    }
}
