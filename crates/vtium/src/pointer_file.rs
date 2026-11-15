//! The pointer file `pointer-theme` names: an SVG file or a cursor theme's
//! folder, whose pictures replace vtium's own pointer pictures.
//!
//! vtium tells the two apart by what is at the path when the console
//! starts: a regular file is an SVG file, and a folder that holds a
//! `cursors` folder is a cursor theme. Anything else, a missing path
//! included, keeps vtium's own pictures, with one log line. vtium searches
//! no icon folder and follows no `Inherits=` line, so the path names
//! exactly the files vtium reads.
//!
//! The console never parses a pointer file. It reads each file under the
//! ownership check that a font set passes, and sends the bytes to the font
//! helper in a pointer ask. The helper draws the pictures in its sandbox
//! and answers with pixels and a hotspot. The console checks each answer
//! frame by frame before it trusts it ([`Reply`]), and refuses an answer
//! that brings more pictures or pixels than the ask can bring.
//!
//! The [`Loader`] runs in the console's main loop and sends one ask at a
//! time through a [`crate::helper::Helper`], the same client that asks the
//! glyph helper for glyphs, so the pointer asks share its restart limit and
//! its timeouts. For a cursor theme, the loader reads one shape's file at a
//! time, trying the shape's file names in order until the helper draws one
//! ([`theme_names`]), and then the console no longer needs the theme. For
//! an SVG file, the loader keeps the bytes and asks for every shape again
//! at each new cell height a display draws at, at most [`HEIGHTS_MAX`]
//! heights to an ask. It keeps pictures at [`SVG_HEIGHTS_KEPT`] heights at
//! most, and drops the height drawn longest ago to make room for a new one.
//! When the helper fails during an ask, the loader gives the file up: it
//! logs one line, keeps the pictures it has, and asks for nothing more.
//!
//! The pictures go into one table that every display's pointer reads
//! before each draw ([`current`]), and vtium draws its compiled-in pictures
//! for every shape and height the table lacks.
//!
//! References:
//! - ADR-1309 D2: what the path names.
//! - ADR-1309 D3: which file of a cursor theme serves each shape.
//! - ADR-1309 D5: the console reads, the helper draws as it does for glyphs, and the console checks each picture.
//! - ADR-1309 D6: the pixels kept of one shape of a cursor theme.
//! - ADR-1309 D9: what vtium logs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use vtium_render::{
    Arrow, POINTER_SIDE_MAX, POINTER_SVG_BYTES_MAX, POINTER_THEME_PIXELS_MAX, PictureSizes,
    PointerPictures, XCURSOR_FILE_MAX,
};
use vtium_settings::Trust;
use vtium_term::PointerShape;

const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQA";
/// The pointer ask, the font helper's request of kind 9.
const POINTER: u32 = 9;
const POINTER_SVG: u32 = 1;
const POINTER_XCURSOR: u32 = 2;
/// A pointer picture, an answer frame to a pointer ask.
pub(crate) const FRAME_POINTER: u32 = 16;
/// A pointer shape the helper draws nothing for, an answer frame to a
/// pointer ask.
pub(crate) const FRAME_NO_POINTER: u32 = 17;
const NO_POINTER_MISSING: u32 = 0;
/// The most cell heights one pointer ask names, the helper's
/// `POINTER_HEIGHTS_MAX`.
pub const HEIGHTS_MAX: usize = 16;
/// The most bytes a picture frame holds: six numbers and 64 × 64 pixels.
pub(crate) const PICTURE_BYTES_MAX: u32 = 24 + POINTER_SIDE_MAX * POINTER_SIDE_MAX * 4;
/// The most cell heights an SVG file's pictures are kept at. A display at
/// a new height past it makes the console drop the pictures at the height
/// asked for longest ago, so the table holds at most this many heights of
/// ten pictures of 64 × 64 pixels, 2.5 MiB.
pub const SVG_HEIGHTS_KEPT: usize = 16;

/// What a pointer file's path names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Located {
    /// A regular file: an SVG pointer file.
    Svg(PathBuf),
    /// A folder that holds a `cursors` folder: a cursor theme.
    Theme(PathBuf),
    /// Anything else, a missing path included.
    Unusable(PathBuf),
}

/// What the path `path` names, by the file system's types alone: the
/// console reads no file to decide it.
pub fn locate(path: &Path) -> Located {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Located::Svg(path.to_owned()),
        Ok(m) if m.is_dir() && path.join("cursors").is_dir() => Located::Theme(path.to_owned()),
        _ => Located::Unusable(path.to_owned()),
    }
}

/// The file names of a cursor theme that may serve `shape`, in the order
/// vtium tries them: the cursor specification's name first, then the X
/// cursor font's. `move` takes the four-headed `all-scroll`, since the
/// specification names no move cursor, and `hand1` is left out, since
/// themes link it to an open hand.
pub fn theme_names(shape: PointerShape) -> &'static [&'static str] {
    match shape {
        PointerShape::Default => &["default", "left_ptr"],
        PointerShape::Text => &["text", "xterm"],
        PointerShape::Pointer => &["pointer", "hand2"],
        PointerShape::Crosshair => &["crosshair", "cross"],
        PointerShape::Wait => &["wait", "watch"],
        PointerShape::Progress => &["progress", "left_ptr_watch"],
        PointerShape::NotAllowed => &["not-allowed", "crossed_circle"],
        PointerShape::Move => &["all-scroll", "fleur"],
        PointerShape::EwResize => &["ew-resize", "sb_h_double_arrow", "col-resize"],
        PointerShape::NsResize => &["ns-resize", "sb_v_double_arrow", "row-resize"],
    }
}

/// The bytes of the pointer file at `path`, read under the ownership check
/// `trust` makes, at most `limit` bytes: `Ok(None)` when it does not exist,
/// and otherwise the sentence vtium logs when it does not read it.
pub fn read(path: &Path, trust: &Trust, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let refused = |because: String| {
        format!(
            "vtium does not read the pointer file {}, because {because}",
            path.display()
        )
    };
    match vtium_settings::read_limited(path, trust, limit) {
        Ok(bytes) => Ok(bytes),
        Err(rejected) => Err(refused(match rejected.errors.first().map(|e| &e.problem) {
            Some(vtium_settings::Problem::TooLarge) => {
                format!("it is longer than {} MiB.", (limit / (1024 * 1024)).max(1))
            }
            Some(vtium_settings::Problem::Untrusted(why)) => why.reason().to_string(),
            Some(vtium_settings::Problem::NotAFile) => "it is not a regular file.".to_owned(),
            Some(vtium_settings::Problem::Unreadable(kind)) => {
                format!("reading it failed ({}).", std::io::Error::from(*kind))
            }
            _ => "it cannot be read.".to_owned(),
        })),
    }
}

/// One pointer ask: a file's bytes and the shape to draw from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    /// Whether the bytes are an SVG file; otherwise a cursor theme's file.
    pub svg: bool,
    pub shape: PointerShape,
    /// `pointer-size`, for a cursor theme's file.
    pub size: Option<u32>,
    /// The cell heights an SVG file's symbol is drawn at.
    pub heights: Vec<u32>,
    pub bytes: Arc<Vec<u8>>,
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A shape's place in [`PointerShape::ALL`], as the ask carries it.
fn shape_code(shape: PointerShape) -> u32 {
    PointerShape::ALL
        .iter()
        .position(|&s| s == shape)
        .map_or(0, len32)
}

impl Ask {
    /// The ask's bytes, the layout vtium-config's serve mode reads.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.bytes.len().saturating_add(100));
        out.extend_from_slice(REQUEST_MAGIC);
        put32(&mut out, POINTER);
        put32(
            &mut out,
            if self.svg {
                POINTER_SVG
            } else {
                POINTER_XCURSOR
            },
        );
        put32(&mut out, shape_code(self.shape));
        put32(&mut out, self.size.unwrap_or(0));
        let heights = self.heights.get(..HEIGHTS_MAX).unwrap_or(&self.heights);
        put32(&mut out, len32(heights.len()));
        for &h in heights {
            put32(&mut out, h);
        }
        put32(&mut out, len32(self.bytes.len()));
        out.extend_from_slice(&self.bytes);
        out
    }
}

/// The helper's answer to one ask, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The pictures drawn, each with its size: the cell height asked for
    /// an SVG file, the nominal size for a cursor theme's file.
    Pictures(Vec<(u32, Arrow)>),
    /// The file has no picture of the shape.
    Missing,
    /// The helper cannot draw the file's picture: its reason, which
    /// finishes the sentence "because ...".
    Refused(String),
    /// The console refused a picture the helper sent: why, which finishes
    /// the sentence "because ...".
    Bad(String),
}

/// The 32-bit number at word `i` of `b`.
fn word(b: &[u8], i: usize) -> Option<u32> {
    let at = i.checked_mul(4)?;
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The helper's answer to one ask as its frames come in. The helper's
/// frame reader has already checked each frame's length against its
/// kind's bound, and [`Reply::frame`] checks the rest.
#[derive(Debug)]
pub struct Reply {
    /// The ask, without the file's bytes.
    ask: Ask,
    pictures: Vec<(u32, Arrow)>,
    /// The picture frames taken so far, refused ones included.
    frames: usize,
    /// The pixels of the pictures taken so far.
    pixels: usize,
    verdict: Option<Answer>,
}

impl Reply {
    /// The answer to `ask`, before its first frame.
    pub fn new(ask: &Ask) -> Reply {
        Reply {
            ask: Ask {
                svg: ask.svg,
                shape: ask.shape,
                size: ask.size,
                heights: ask.heights.clone(),
                bytes: Arc::default(),
            },
            pictures: Vec::new(),
            frames: 0,
            pixels: 0,
            verdict: None,
        }
    }

    /// The most picture frames the ask can bring: one for each height
    /// asked of an SVG file, one when `pointer-size` names a size, and
    /// otherwise one for each pixel of the budget, since each picture has
    /// a pixel at least.
    fn frames_max(&self) -> usize {
        match (self.ask.svg, self.ask.size) {
            (true, _) => self.ask.heights.len(),
            (false, Some(_)) => 1,
            (false, None) => POINTER_THEME_PIXELS_MAX,
        }
    }

    /// Takes one frame of the answer, a `FRAME_POINTER` or a
    /// `FRAME_NO_POINTER`. A picture that fails its checks refuses the
    /// ask's file, and the frames after it are read and dropped. The error
    /// refuses the whole answer, and the console then ends the helper: a
    /// frame about a shape not asked for, or more picture frames than the
    /// ask can bring.
    pub fn frame(&mut self, kind: u32, payload: &[u8]) -> Result<(), String> {
        if kind == FRAME_NO_POINTER {
            if word(payload, 0) != Some(shape_code(self.ask.shape)) {
                return Err(
                    "the font helper answered a shape that vtium did not ask for".to_owned(),
                );
            }
            let reason = || {
                crate::helper::escaped(&String::from_utf8_lossy(
                    payload.get(8..).unwrap_or_default(),
                ))
            };
            self.verdict.get_or_insert_with(|| match word(payload, 1) {
                Some(NO_POINTER_MISSING) => Answer::Missing,
                _ => Answer::Refused(reason()),
            });
            return Ok(());
        }
        self.frames = self.frames.saturating_add(1);
        if self.frames > self.frames_max() {
            return Err(format!(
                "the font helper sent more than {} pictures for one shape",
                self.frames_max()
            ));
        }
        match self.picture(payload) {
            Ok((size, arrow)) => {
                let n = arrow.pixels().len();
                self.pixels = self.pixels.saturating_add(n);
                self.pictures.push((size, arrow));
            }
            Err(why) => {
                self.verdict.get_or_insert(Answer::Bad(why));
            }
        }
        Ok(())
    }

    /// Checks one picture frame's payload: its shape the one asked, its
    /// size among the heights asked for an SVG file and not one already
    /// taken, the picture itself ([`Arrow::from_rgba`]), a pixel in it that
    /// shows, and for a cursor theme the pixels of the shape's pictures
    /// within [`POINTER_THEME_PIXELS_MAX`].
    fn picture(&self, payload: &[u8]) -> Result<(u32, Arrow), String> {
        let field = |i| word(payload, i).ok_or("the font helper sent a picture without its fields");
        let (shape, size, width, height) = (field(0)?, field(1)?, field(2)?, field(3)?);
        let hot = (field(4)?, field(5)?);
        if shape != shape_code(self.ask.shape) {
            return Err(
                "the font helper sent a picture of a shape that vtium did not ask for".to_owned(),
            );
        }
        if self.ask.svg && !self.ask.heights.contains(&size) {
            return Err(format!(
                "the font helper sent a picture for cells {size} pixels tall, which vtium did not ask for"
            ));
        }
        if size == 0 {
            return Err("the font helper sent a picture of size 0".to_owned());
        }
        if self.pictures.iter().any(|(s, _)| *s == size) {
            return Err(format!("the font helper sent two pictures of size {size}"));
        }
        let rgba = payload.get(24..).unwrap_or_default();
        let arrow = Arrow::from_rgba(width, height, hot, rgba)
            .map_err(|why| format!("the font helper sent {why}"))?;
        if !arrow.pixels().iter().any(|p| p >> 24 != 0) {
            return Err("the font helper sent a picture in which no pixel shows".to_owned());
        }
        if !self.ask.svg
            && self.pixels.saturating_add(arrow.pixels().len()) > POINTER_THEME_PIXELS_MAX
        {
            return Err(format!(
                "the font helper sent pictures of more than {POINTER_THEME_PIXELS_MAX} pixels in all"
            ));
        }
        Ok((size, arrow))
    }

    /// The answer once the helper ends it: the first refusal, or the
    /// pictures, or missing when there are none.
    pub fn finish(self) -> Answer {
        match self.verdict {
            Some(v) => v,
            None if self.pictures.is_empty() => Answer::Missing,
            None => Answer::Pictures(self.pictures),
        }
    }
}

/// `items` as an English list: "a", "a and b", "a, b and c".
fn listed(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The line vtium logs for the shapes a pointer file has no picture for.
fn lacks_line(path: &Path, lacking: &[&str]) -> String {
    let (shapes, them) = if lacking.len() == 1 {
        ("shape", "it")
    } else {
        ("shapes", "them")
    };
    format!(
        "The pointer file {} has no picture for the {} {shapes}, so vtium draws its own pictures for {them}.",
        path.display(),
        listed(lacking)
    )
}

/// The line vtium logs for a picture of `shape` from the file at `path`
/// that it does not use: the helper cannot draw it, or the console refused
/// what the helper sent.
fn unused_line(shape: PointerShape, path: &Path, answer: &Answer) -> Option<String> {
    let (shape, path) = (shape.name(), path.display());
    match answer {
        Answer::Refused(why) => Some(format!(
            "The font helper cannot draw the {shape} shape from {path}, because {why}. vtium draws its own picture for that shape."
        )),
        Answer::Bad(why) => Some(format!(
            "vtium does not use the {shape} picture from {path}, because {why}. It draws its own picture for that shape."
        )),
        _ => None,
    }
}

/// The line vtium logs when the font helper fails while it draws the
/// pointer file at `path`, or cannot be started for it, `why` being the
/// reason the font helper's client gives.
pub fn lost_line(path: &Path, why: &str) -> String {
    format!(
        "The font helper cannot draw the pointer file {}, so vtium draws its own pictures for the shapes it has no picture for yet. The reason: {}.",
        path.display(),
        why.trim_end_matches('.')
    )
}

/// The line vtium logs for a path that is neither an SVG file nor a cursor
/// theme's folder.
pub fn unusable_line(path: &Path) -> String {
    format!(
        "The pointer file {} is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures.",
        path.display()
    )
}

/// What a load logs: each line at its level, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

/// What the loader still has to ask, by the kind of file.
#[derive(Debug)]
enum File {
    /// A cursor theme: `pointer-size`, the shape looked for, as its place
    /// in [`PointerShape::ALL`], and the place of the next of its names to
    /// try.
    Theme {
        size: Option<u32>,
        shape: usize,
        name: usize,
    },
    /// An SVG file: its bytes, the heights of the round of asks under way,
    /// and the shapes this round has yet to ask for, the last first.
    Svg {
        bytes: Arc<Vec<u8>>,
        heights: Vec<u32>,
        left: Vec<PointerShape>,
        /// The shapes whose symbol the helper drew nothing for, missing or
        /// refused, which are never asked for again.
        given_up: Vec<PointerShape>,
        /// Whether a round has ended, after which the line about the
        /// shapes the file lacks is never logged again.
        rounds: bool,
    },
}

/// The pointer file's pictures as they come in: what the file is, the
/// table so far, and what is left to ask. The console's loop takes the
/// asks one at a time ([`Loader::next_ask`]) and hands back each answer
/// ([`Loader::take`]).
#[derive(Debug)]
pub struct Loader {
    path: PathBuf,
    trust: Trust,
    file: File,
    table: PointerPictures,
    /// The shape of the ask under way, and the file its bytes came from.
    asked: Option<(PointerShape, PathBuf)>,
    /// The shapes the file has no picture for, in [`PointerShape::ALL`]'s
    /// order.
    lacking: Vec<&'static str>,
    log: Log,
    /// Whether the table changed since it was last published.
    changed: bool,
    /// Whether the loader asks for nothing more: a theme's every shape is
    /// answered, or the helper failed.
    ended: bool,
}

impl Loader {
    /// A loader for the pointer file at `path`, each of whose files is read
    /// under `trust`, with `pointer-size` `size`; `None`, and the line
    /// logged, when the path names nothing vtium uses. An SVG file is read
    /// now, and a theme's files as each shape is asked for.
    pub fn start(path: &Path, size: Option<u32>, trust: Trust) -> Option<Loader> {
        let (file, sizes) = match locate(path) {
            Located::Unusable(path) => {
                log::warn!("{}", unusable_line(&path));
                return None;
            }
            Located::Theme(_) => (
                File::Theme {
                    size,
                    shape: 0,
                    name: 0,
                },
                PictureSizes::Theme { size },
            ),
            Located::Svg(_) => {
                let bytes = match read(path, &trust, POINTER_SVG_BYTES_MAX) {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => {
                        log::warn!(
                            "vtium does not read the pointer file {}, because it is missing.",
                            path.display()
                        );
                        return None;
                    }
                    Err(line) => {
                        log::warn!("{line}");
                        return None;
                    }
                };
                store().svg = true;
                (
                    File::Svg {
                        bytes: Arc::new(bytes),
                        heights: Vec::new(),
                        left: Vec::new(),
                        given_up: Vec::new(),
                        rounds: false,
                    },
                    PictureSizes::Svg,
                )
            }
        };
        Some(Loader {
            path: path.to_owned(),
            trust,
            file,
            table: PointerPictures::new(sizes),
            asked: None,
            lacking: Vec::new(),
            log: Log::default(),
            changed: false,
            ended: false,
        })
    }

    pub fn table(&self) -> &PointerPictures {
        &self.table
    }

    /// Whether the loader will ask for nothing more.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// The next ask to send, or `None` when there is nothing to ask now.
    /// A theme's next ask reads the next file to try; an SVG file's first
    /// ask of a round takes the heights the displays wait for
    /// ([`want`]).
    pub fn next_ask(&mut self) -> Option<Ask> {
        if self.ended {
            return None;
        }
        match &mut self.file {
            File::Theme { size, shape, name } => loop {
                let Some(&s) = PointerShape::ALL.get(*shape) else {
                    if !self.lacking.is_empty() {
                        self.log.infos.push(lacks_line(&self.path, &self.lacking));
                    }
                    self.ended = true;
                    return None;
                };
                let Some(file) = theme_names(s).get(*name) else {
                    self.lacking.push(s.name());
                    *shape = shape.saturating_add(1);
                    *name = 0;
                    continue;
                };
                *name = name.saturating_add(1);
                let path = self.path.join("cursors").join(file);
                match read(&path, &self.trust, XCURSOR_FILE_MAX) {
                    Ok(Some(bytes)) => {
                        self.asked = Some((s, path));
                        return Some(Ask {
                            svg: false,
                            shape: s,
                            size: *size,
                            heights: Vec::new(),
                            bytes: Arc::new(bytes),
                        });
                    }
                    Ok(None) => {}
                    Err(line) => self.log.warnings.push(line),
                }
            },
            File::Svg {
                bytes,
                heights,
                left,
                given_up,
                rounds,
            } => {
                let mut s = store();
                for h in std::mem::take(&mut s.forget) {
                    self.table.forget(h);
                    self.changed = true;
                }
                if left.is_empty() {
                    if !heights.is_empty() && !*rounds {
                        *rounds = true;
                        if !self.lacking.is_empty() {
                            self.log.infos.push(lacks_line(&self.path, &self.lacking));
                        }
                    }
                    let n = s.waiting.len().min(HEIGHTS_MAX);
                    *heights = s.waiting.drain(..n).collect();
                    if heights.is_empty() {
                        return None;
                    }
                    *left = PointerShape::ALL
                        .iter()
                        .rev()
                        .filter(|shape| !given_up.contains(shape))
                        .copied()
                        .collect();
                }
                let shape = left.pop()?;
                self.asked = Some((shape, self.path.clone()));
                Some(Ask {
                    svg: true,
                    shape,
                    size: None,
                    heights: heights.clone(),
                    bytes: Arc::clone(bytes),
                })
            }
        }
    }

    /// Takes the answer to the ask [`Loader::next_ask`] gave last, or why the
    /// font helper gave none. Pictures go into the table. A theme's shape
    /// that the file has no picture of, or whose picture is refused, tries
    /// its next file name; an SVG file's is never asked for again. A
    /// helper that failed gives the file up.
    pub fn take(&mut self, answer: Result<Answer, String>) {
        let Some((shape, path)) = self.asked.take() else {
            return;
        };
        let answer = match answer {
            Ok(answer) => answer,
            Err(why) => {
                self.log.warnings.push(lost_line(&self.path, &why));
                self.ended = true;
                store().svg = false;
                return;
            }
        };
        self.log.warnings.extend(unused_line(shape, &path, &answer));
        let missing = matches!(answer, Answer::Missing);
        let Answer::Pictures(pictures) = answer else {
            // a theme's shape tries its next file name, and an SVG file's
            // shape is not asked for again
            if let File::Svg { given_up, .. } = &mut self.file {
                given_up.push(shape);
                if missing {
                    self.lacking.push(shape.name());
                }
            }
            return;
        };
        if let File::Theme {
            shape: next, name, ..
        } = &mut self.file
        {
            *next = next.saturating_add(1);
            *name = 0;
        }
        for (size, arrow) in pictures {
            self.table.insert(shape, size, arrow);
        }
        self.changed = true;
    }

    /// Logs what the load has to log since the last call.
    pub fn emit(&mut self) {
        let log = std::mem::take(&mut self.log);
        for line in &log.warnings {
            log::warn!("{line}");
        }
        for line in &log.infos {
            log::info!("{line}");
        }
    }

    /// What the load has to log since the last call, taken: for a test.
    #[cfg(test)]
    fn take_log(&mut self) -> Log {
        std::mem::take(&mut self.log)
    }

    /// Hands the table to every display's pointer when it changed since
    /// the last call, and says whether it did.
    pub fn publish(&mut self) -> bool {
        if !std::mem::take(&mut self.changed) {
            return false;
        }
        store().pictures = Some(Arc::new(self.table.clone()));
        true
    }
}

/// The table every display's pointer reads, and the heights an SVG file
/// still has to be drawn at.
struct Store {
    pictures: Option<Arc<PointerPictures>>,
    /// Whether an SVG file is loading, and so asks for heights.
    svg: bool,
    /// The heights asked for an SVG file and kept, the oldest first, so
    /// each is asked once while it is kept.
    asked: Vec<u32>,
    /// The heights asked and not yet sent to the helper.
    waiting: Vec<u32>,
    /// The heights dropped to make room, whose pictures the loader drops.
    forget: Vec<u32>,
    /// Turns the console's loop, so that a new height is sent.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    pictures: None,
    svg: false,
    asked: Vec::new(),
    waiting: Vec::new(),
    forget: Vec::new(),
    wake: None,
});

fn store() -> std::sync::MutexGuard<'static, Store> {
    STORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The pointer file's pictures, once the helper has answered; `None`
/// before, and when no pointer file is set.
pub fn current() -> Option<Arc<PointerPictures>> {
    store().pictures.clone()
}

/// Sets what turns the console's loop when a display wants an SVG file
/// drawn at a new height.
pub fn set_wake(wake: Arc<dyn Fn() + Send + Sync>) {
    store().wake = Some(wake);
}

/// A display draws its pointer at cells `cell_height` tall: an SVG file is
/// drawn at that height when it is not kept, and the height kept longest
/// is dropped to make room past [`SVG_HEIGHTS_KEPT`].
pub fn want(cell_height: u32) {
    let wake = {
        let mut s = store();
        if !s.svg || s.asked.contains(&cell_height) {
            return;
        }
        if s.asked.len() >= SVG_HEIGHTS_KEPT {
            let old = s.asked.remove(0);
            s.waiting.retain(|&h| h != old);
            s.forget.push(old);
        }
        s.asked.push(cell_height);
        s.waiting.push(cell_height);
        s.wake.clone()
    };
    if let Some(wake) = wake {
        wake();
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
pub(crate) mod tests;
