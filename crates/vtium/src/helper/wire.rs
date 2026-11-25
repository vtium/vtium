//! The requests the console writes to the font helper and the frames it
//! reads back.

use std::io::Read;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Instant;
use vtium_term::ClusterWidth;

use vtium_font::set::Set;
use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Doubled, Script, Variant};
use vtium_platform::vt::font::ConsoleFont;

use crate::deferred::Text;
use crate::pointer_file::{FRAME_NO_POINTER, FRAME_POINTER};

use super::{FIRST_MAX, FRAME_BYTES_MAX, Helper, REASON_BYTES_MAX};

pub(super) const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQA";
/// Why the console gives the helper up when the two read different layouts
/// of their format: a helper of an older build refuses this console's
/// request as "not a serve request", and an answer of an older layout does
/// not read. The console and the font helper are installed together, so
/// reinstalling vtium mends it.
pub(super) const OTHER_BUILD: &str = "the console and the font helper come from different builds of vtium, so they cannot read each other's messages; reinstall vtium so that both come from one build";

/// An ask. A console built with right-to-left text asks with the kind that
/// also takes the part of a cluster the font draws only without some of
/// its marks (ADR-1200 D22 step 3), which its lookup lays the missing marks
/// over; the helper sends a console of the minspec build none.
#[cfg(not(feature = "bidi"))]
pub(super) const ASK: u32 = 1;
#[cfg(feature = "bidi")]
pub(super) const ASK: u32 = 6;
pub(super) const FILL: u32 = 2;
pub(super) const STOP: u32 = 3;
pub(super) const OWN_CELL: u32 = 4;
pub(super) const SOURCES: u32 = 5;
pub(super) const COVERAGE: u32 = 8;
/// The active fonts, which the console sends first to each helper it
/// starts ([`Launcher::with_active`]).
pub(super) const ACTIVE: u32 = 10;
/// The sources the helper leaves out, a bit for each `source=` line.
pub(super) const LEAVE_OUT: u32 = 12;
/// The most bytes the paths of the active fonts hold together, the
/// helper's bound: the batch file's own.
pub(super) const ACTIVE_BYTES_MAX: usize = vtium_settings::batch::BATCH_MAX;
/// The longest path the active fonts name, the helper's bound.
pub(super) const ACTIVE_PATH_MAX: usize = 4096;
pub(super) const FRAME_GLYPHS: u32 = 1;
pub(super) const FRAME_DONE: u32 = 2;
pub(super) const FRAME_FAILED: u32 = 3;
pub(super) const FRAME_FILL: u32 = 4;
pub(super) const FRAME_FILLED: u32 = 5;
pub(super) const FRAME_OWN_CELL: u32 = 6;
pub(super) const FRAME_NO_OWN_CELL: u32 = 7;
pub(super) const FRAME_STOPPED: u32 = 8;
pub(super) const FRAME_WARNING: u32 = 9;
pub(super) const FRAME_OUTLINE_CELL: u32 = 10;
pub(super) const FRAME_SOURCES: u32 = 11;
pub(super) const FRAME_DRAWS_NOTHING: u32 = 12;
pub(super) const FRAME_SANDBOX: u32 = 14;
pub(super) const FRAME_COVERAGE: u32 = 15;
/// What glyphs a request is about besides their cell: their variant, a
/// style and a script, and their double size, `None` for the normal size.
pub(super) type Kind = (Variant, Option<Doubled>);

pub(super) fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A frit as the console hands it to the helper: its name and its text,
/// read from its file or made on the fly (ADR-1200 D9), so the helper
/// needs no file for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FritText {
    pub name: String,
    pub text: String,
}

impl FritText {
    /// The frit at `path` in the frits' directory `frits`, named for its
    /// file less `.vtfrit`, read once the ownership check `trust` makes
    /// passes ([`crate::font::frit_text`]), with the layers of the frits'
    /// directory in place ([`crate::font::with_layers`]). Each sentence
    /// about a `layer=` line that adds nothing is logged once a boot.
    pub fn read(
        path: &Path,
        frits: &Path,
        trust: &vtium_settings::Trust,
    ) -> Result<FritText, String> {
        let (frit, notes) = FritText::read_noting(path, frits, trust)?;
        for note in &notes {
            crate::font::log_layer_note(note);
        }
        Ok(frit)
    }

    /// The frit at `path`, as [`FritText::read`] reads it, and the
    /// sentences about its `layer=` lines that add nothing, which the
    /// caller reports.
    pub fn read_noting(
        path: &Path,
        frits: &Path,
        trust: &vtium_settings::Trust,
    ) -> Result<(FritText, Vec<String>), String> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n))
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("there is no font set named {}", path.display()))?;
        let text = crate::font::frit_text(path, trust)
            .map_err(|why| crate::font::refusal(name, path, &why))?;
        let (text, notes) = crate::font::with_layers(name, frits, &text, trust);
        Ok((
            FritText {
                name: name.to_owned(),
                text,
            },
            notes,
        ))
    }
}

/// One request's bytes: `kind` at `cell`, whose line spacing adds `band`
/// (ADR-1302 D13), in `variant`, a style and a script, at the double size
/// `doubled` or the normal size, for `frit`, naming
/// `codepoints` and
/// `clusters` (each cut at [`FIRST_MAX`]) and `held`, with the kernel's
/// font. The display's capabilities are reserved, and sent empty.
pub(super) fn encode(
    frit: &FritText,
    console: Option<&ConsoleFont>,
    (kind, cell, band, (variant, doubled), rule): (u32, Cell, Band, Kind, ClusterWidth),
    (codepoints, clusters): (&[u32], &[Text]),
    held: &[(u32, u32)],
) -> Vec<u8> {
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, kind);
    for field in [&frit.name, &frit.text] {
        put32(&mut out, len32(field.len()));
        out.extend_from_slice(field.as_bytes());
    }
    put32(&mut out, cell.width);
    put32(&mut out, cell.height);
    put32(&mut out, band.above);
    put32(&mut out, band.below);
    let style = variant.style;
    put32(
        &mut out,
        u32::from(style.bold) | u32::from(style.italic) << 1,
    );
    put32(
        &mut out,
        match variant.script {
            Script::Baseline => 0,
            Script::Super => 1,
            Script::Sub => 2,
        },
    );
    put32(
        &mut out,
        match rule {
            ClusterWidth::Double => 0,
            ClusterWidth::Single => 1,
        },
    );
    put32(
        &mut out,
        match doubled {
            None => 0,
            Some(Doubled::Width) => 1,
            Some(Doubled::Height) => 2,
        },
    );
    put32(&mut out, 0);
    let codepoints = codepoints.get(..FIRST_MAX).unwrap_or(codepoints);
    put32(&mut out, len32(codepoints.len()));
    for &c in codepoints {
        put32(&mut out, c);
    }
    let clusters = clusters.get(..FIRST_MAX).unwrap_or(clusters);
    put32(&mut out, len32(clusters.len()));
    for cluster in clusters {
        put32(&mut out, len32(cluster.len()));
        for &c in cluster {
            put32(&mut out, u32::from(c));
        }
    }
    put32(&mut out, len32(held.len()));
    for &(a, b) in held {
        put32(&mut out, a);
        put32(&mut out, b);
    }
    match console {
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

/// The cell a frit's first source gives, a cell query's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnCell {
    /// A bitmap source's own cell, which the console takes at the largest
    /// whole multiple not over the height it draws at.
    Bitmap(Cell),
    /// An outline source's cell at the height asked, taken as it is.
    Outline(Cell),
}

/// Why a cell query has no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoCell {
    pub why: String,
    /// No answer came in time, as opposed to an answer that says there is
    /// none, which the frit's text decides and asking again cannot change.
    pub late: bool,
    /// The helper answered that the frit's fonts give no cell at the
    /// height asked, as below the shortest strike of a bitmap font with
    /// strikes. False when the helper could not answer: it failed, ended,
    /// is not running, sent a malformed answer, or answered too late.
    pub from_frit: bool,
}

impl NoCell {
    /// The helper could not answer, for `why`.
    pub(super) fn refused(why: String) -> NoCell {
        NoCell {
            why,
            late: false,
            from_frit: false,
        }
    }

    /// The helper answered that there is no cell, for `why`.
    fn answered(why: String) -> NoCell {
        NoCell {
            why,
            late: false,
            from_frit: true,
        }
    }
}

impl std::fmt::Display for NoCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

/// What the frame `answer` says to a cell query asked at `height`: the
/// cell, or the helper's refusal. The error is why the frame is malformed,
/// an outline cell of another height. `answer` is a cell frame's event.
pub(super) fn cell_answer(answer: Event, height: u32) -> Result<Result<OwnCell, NoCell>, String> {
    match answer {
        Event::OwnCell(cell) => Ok(Ok(OwnCell::Bitmap(cell))),
        Event::OutlineCell(cell) if cell.height == height => Ok(Ok(OwnCell::Outline(cell))),
        Event::OutlineCell(cell) => Err(format!(
            "an outline cell {} pixels high, where {height} was asked",
            cell.height
        )),
        Event::NoOwnCell(why) => Ok(Err(NoCell::answered(why.trim().to_owned()))),
        _ => Err("a frame that answers no cell query".to_owned()),
    }
}

/// What the reader thread saw.
#[derive(Debug)]
pub(crate) enum Event {
    /// A set, an ask's or a fill's, the codepoints it maps as `(first,
    /// count)` runs, and the bytes its frame took.
    Glyphs {
        fill: bool,
        /// The rows line spacing adds to the set's cell (ADR-1302 D13).
        band: Band,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    },
    /// The oldest ask is answered.
    Done,
    /// A cell's fill is done, at its band.
    Filled(SpacedCell),
    /// A stop at a cell and band is taken: the frames of the fill it ended
    /// are all sent.
    Stopped(SpacedCell),
    /// A bitmap frit's own cell, a cell query's answer.
    OwnCell(Cell),
    /// An outline frit's cell at the height asked, a cell query's answer.
    OutlineCell(Cell),
    /// The frit has no own cell: why.
    NoOwnCell(String),
    /// A warning the frit's text gave as the helper read it.
    Warning(String),
    /// The parts of the helper's sandbox in force, as [`FRAME_SANDBOX`]'s
    /// bits.
    Sandbox(u32),
    /// A coverage query's answer: the letters asked that the frit covers,
    /// and an installed font that covers the most of the rest, empty when
    /// none does.
    Coverage(Vec<u32>, String),
    /// A sources query's answer: one line for each source the helper left
    /// out and for each font that differs from what its source needs.
    Sources(String),
    /// The frit draws nothing, a coverage or sources query's answer: why.
    DrawsNothing(String),
    /// A frame that answers a pointer ask: its kind, a picture or a shape
    /// drawn nothing for, and its payload, which
    /// [`crate::pointer_file::Reply`] checks.
    Pointer(u32, Vec<u8>),
    Failed(String),
    /// Bytes that are not a frame, or a set its reader refused.
    Malformed(String),
    /// The helper closed its end and was reaped: how it exited.
    Closed(Option<ExitStatus>),
}

/// Reads one frame's head: its kind and length, or `None` at the end of
/// the stream. An end inside the head is the end of the stream too: the
/// helper has closed its output, as when the kernel ended it while it
/// wrote, and how it exited says why.
fn head(r: &mut impl Read) -> Result<Option<(u32, u32)>, String> {
    let mut b = [0u8; 8];
    let mut got = 0;
    while got < b.len() {
        let rest = b.get_mut(got..).unwrap_or_default();
        match r.read(rest) {
            Ok(0) => return Ok(None),
            Ok(n) => got = got.saturating_add(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("cannot read the helper's answer: {e}")),
        }
    }
    let [k0, k1, k2, k3, n0, n1, n2, n3] = b;
    Ok(Some((
        u32::from_le_bytes([k0, k1, k2, k3]),
        u32::from_le_bytes([n0, n1, n2, n3]),
    )))
}

/// The codepoints a set maps, as `(first, count)` runs, read from its
/// `CMAP` section as vtium-font's format lays it out: every offset and
/// count is checked against the bytes, so a malformed set is an error,
/// and the set's reader checks the rest.
fn mapped_runs(bytes: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let bad = || "glyphs whose map vtium cannot read".to_owned();
    let field = |at: usize, n: usize| bytes.get(at..at.checked_add(n)?);
    let u32_at = |at: usize| {
        field(at, 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let u64_at = |at: usize| {
        field(at, 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
    };
    let usize_of = |v: u64| usize::try_from(v).ok();
    let sections = u32_at(12).ok_or_else(bad)?;
    let (mut off, mut len) = (None, None);
    for i in 0..sections {
        let entry = usize_of(u64::from(i))
            .and_then(|i| i.checked_mul(20))
            .and_then(|e| e.checked_add(16))
            .ok_or_else(bad)?;
        if field(entry, 4).ok_or_else(bad)? == b"CMAP" {
            off = entry.checked_add(4).and_then(u64_at).and_then(usize_of);
            len = entry.checked_add(12).and_then(u64_at).and_then(usize_of);
            break;
        }
    }
    let (off, len) = off.zip(len).ok_or_else(bad)?;
    let cmap = bytes
        .get(off..off.checked_add(len).ok_or_else(bad)?)
        .ok_or_else(bad)?;
    let at32 = |at: usize| {
        cmap.get(at..at.checked_add(4)?)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let strikes = at32(0).ok_or_else(bad)?;
    let record = usize_of(u64::from(at32(4).ok_or_else(bad)?)).ok_or_else(bad)?;
    if record < 16 {
        return Err(bad());
    }
    let mut at = 8usize;
    let mut runs = Vec::new();
    for _ in 0..strikes {
        let count = at32(at).ok_or_else(bad)?;
        at = at.checked_add(4).ok_or_else(bad)?;
        for _ in 0..count {
            let first = at32(at).ok_or_else(bad)?;
            let n = at.checked_add(4).and_then(at32).ok_or_else(bad)?;
            runs.push((first, n));
            at = at.checked_add(record).ok_or_else(bad)?;
        }
    }
    Ok(runs)
}

/// Reads one frame into an event, or `None` at the end of the stream: its
/// length checked against its kind's bound before its payload is
/// allocated. A stream that ends inside a frame has ended, and the
/// helper's exit status, which follows, says why.
pub(crate) fn frame(r: &mut impl Read) -> Option<Event> {
    let (kind, len) = match head(r) {
        Ok(Some(h)) => h,
        Ok(None) => return None,
        Err(why) => return Some(Event::Malformed(why)),
    };
    let max = match kind {
        FRAME_GLYPHS | FRAME_FILL => FRAME_BYTES_MAX,
        FRAME_DONE => 0,
        FRAME_FILLED | FRAME_STOPPED => 16,
        FRAME_OWN_CELL | FRAME_OUTLINE_CELL => 8,
        FRAME_SANDBOX => 4,
        FRAME_COVERAGE | FRAME_SOURCES => FRAME_BYTES_MAX,
        FRAME_POINTER => crate::pointer_file::PICTURE_BYTES_MAX,
        FRAME_NO_POINTER => REASON_BYTES_MAX.saturating_add(8),
        FRAME_FAILED | FRAME_NO_OWN_CELL | FRAME_WARNING | FRAME_DRAWS_NOTHING => REASON_BYTES_MAX,
        other => return Some(Event::Malformed(format!("a frame of unknown kind {other}"))),
    };
    if len > max {
        return Some(Event::Malformed(format!(
            "a frame of {len} bytes, past its kind's {max}"
        )));
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    match r.read_exact(&mut payload) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
        Err(e) => {
            return Some(Event::Malformed(format!(
                "cannot read the helper's answer: {e}"
            )));
        }
    }
    let word = |b: &[u8], i: usize| -> Option<u32> {
        let at = i.checked_mul(4)?;
        Some(u32::from_le_bytes(
            b.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    Some(match kind {
        FRAME_GLYPHS | FRAME_FILL => {
            // the rows line spacing adds to the set's cell, then the set
            let (Some(above), Some(below)) = (word(&payload, 0), word(&payload, 1)) else {
                return Some(Event::Malformed("glyphs without their band".to_owned()));
            };
            let band = Band { above, below };
            let payload = payload.split_off(8);
            // the store counts the set's bytes, without the band
            let len = u64::try_from(payload.len()).unwrap_or(u64::MAX);
            match mapped_runs(&payload) {
                Err(why) => Event::Malformed(why),
                Ok(runs) => match Set::from_bytes(payload) {
                    Ok(set) => Event::Glyphs {
                        fill: kind == FRAME_FILL,
                        band,
                        set: Arc::new(set),
                        runs,
                        bytes: len,
                    },
                    Err(e) => Event::Malformed(format!("glyphs vtium cannot read: {e}")),
                },
            }
        }
        FRAME_DONE => Event::Done,
        FRAME_FILLED | FRAME_STOPPED => match (0..4).map(|i| word(&payload, i)).collect() {
            Some::<Vec<u32>>(w) if w.len() == 4 && payload.len() == 16 => {
                let at = SpacedCell {
                    cell: Cell {
                        width: w.first().copied().unwrap_or(0),
                        height: w.get(1).copied().unwrap_or(0),
                    },
                    band: Band {
                        above: w.get(2).copied().unwrap_or(0),
                        below: w.get(3).copied().unwrap_or(0),
                    },
                };
                if kind == FRAME_FILLED {
                    Event::Filled(at)
                } else {
                    Event::Stopped(at)
                }
            }
            // a fill's end of the layout before the band
            _ if payload.len() == 8 => Event::Malformed(OTHER_BUILD.to_owned()),
            _ => Event::Malformed("a fill's end without its cell".to_owned()),
        },
        FRAME_OWN_CELL | FRAME_OUTLINE_CELL => match *payload.as_slice() {
            [w0, w1, w2, w3, h0, h1, h2, h3] => {
                let cell = Cell {
                    width: u32::from_le_bytes([w0, w1, w2, w3]),
                    height: u32::from_le_bytes([h0, h1, h2, h3]),
                };
                let side = 1..=vtium_font::CELL_SIDE_MAX;
                if !(side.contains(&cell.width) && side.contains(&cell.height)) {
                    Event::Malformed("an own cell past CELL_SIDE_MAX".to_owned())
                } else if kind == FRAME_OWN_CELL {
                    Event::OwnCell(cell)
                } else {
                    Event::OutlineCell(cell)
                }
            }
            _ => Event::Malformed("an own cell without its size".to_owned()),
        },
        FRAME_NO_OWN_CELL => Event::NoOwnCell(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_WARNING => Event::Warning(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_SANDBOX => match word(&payload, 0) {
            Some(parts) => Event::Sandbox(parts),
            _ => Event::Malformed("a sandbox report without its parts".to_owned()),
        },
        FRAME_COVERAGE => coverage_answer(&payload),
        FRAME_SOURCES => Event::Sources(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_DRAWS_NOTHING => Event::DrawsNothing(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_POINTER | FRAME_NO_POINTER => Event::Pointer(kind, payload),
        _ => Event::Failed(String::from_utf8_lossy(&payload).into_owned()),
    })
}

/// A coverage query's answer, [`FRAME_COVERAGE`]'s payload: the count and
/// each codepoint the frit covers, each a scalar value, then a family name
/// in UTF-8.
fn coverage_answer(payload: &[u8]) -> Event {
    let word = |i: usize| -> Option<u32> {
        let at = i.checked_mul(4)?;
        Some(u32::from_le_bytes(
            payload.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    let Some(n) = word(0).and_then(|n| usize::try_from(n).ok()) else {
        return Event::Malformed("a coverage answer without its count".to_owned());
    };
    let covered: Option<Vec<u32>> = (1..=n)
        .map(|i| word(i).filter(|&c| char::from_u32(c).is_some()))
        .collect();
    let rest = n
        .checked_add(1)
        .and_then(|k| k.checked_mul(4))
        .and_then(|at| payload.get(at..));
    match (covered, rest.map(std::str::from_utf8)) {
        (Some(covered), Some(Ok(font))) => Event::Coverage(covered, font.to_owned()),
        _ => Event::Malformed("a coverage answer cut short or not UTF-8".to_owned()),
    }
}

impl Helper {
    /// Sends one request of `kind` at a cell and the rows line spacing
    /// adds to it, in `variant`, starting the helper first when it is not
    /// running.
    pub(super) fn send(
        &mut self,
        what: (u32, SpacedCell, Kind, ClusterWidth),
        named: (&[u32], &[Text]),
        held: &[(u32, u32)],
        now: Instant,
    ) {
        let bytes = self.request(what, named, held);
        self.submit(bytes, now);
    }

    /// One request's bytes, of `kind` at a cell and the rows line spacing
    /// adds to it, in `variant`.
    pub(super) fn request(
        &self,
        (kind, at, of, rule): (u32, SpacedCell, Kind, ClusterWidth),
        named: (&[u32], &[Text]),
        held: &[(u32, u32)],
    ) -> Vec<u8> {
        encode(
            &self.frit,
            self.console.as_ref(),
            (kind, at.cell, at.band, of, rule),
            named,
            held,
        )
    }
}
