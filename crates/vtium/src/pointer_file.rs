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
//! The console never parses a pointer file. It reads each file once, under
//! the ownership check that a font set passes, and sends the bytes to the
//! font helper in a pointer ask. The helper draws the pictures in its
//! sandbox and answers with pixels and a hotspot, which the console checks
//! before it trusts them ([`Arrow::from_rgba`]). For a cursor theme, the
//! console tries each shape's file names in order and takes the first file
//! the helper can draw ([`theme_names`]). For an SVG file, it keeps the
//! bytes and asks again for each new cell height a display draws at,
//! because each height needs a new drawing.
//!
//! The work runs on a thread of its own, with a helper process of its own
//! for each round of asks, so neither a slow helper nor a large file holds
//! up the console's loop. The pictures go into one table that every
//! display's pointer reads before each draw ([`current`]), and vtium draws
//! its compiled-in pictures until the table holds the file's.
//!
//! References:
//! - ADR-1309 D2: what the path names.
//! - ADR-1309 D3: which file of a cursor theme serves each shape.
//! - ADR-1309 D5: the console reads, the helper draws, and the console checks each picture.
//! - ADR-1309 D9: what vtium logs.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use vtium_render::{
    Arrow, POINTER_SIDE_MAX, POINTER_SVG_BYTES_MAX, PictureSizes, PointerPictures, XCURSOR_FILE_MAX,
};
use vtium_settings::Trust;
use vtium_term::PointerShape;

use crate::helper::Launcher;

const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQA";
/// The pointer ask, the font helper's request of kind 9.
const POINTER: u32 = 9;
const POINTER_SVG: u32 = 1;
const POINTER_XCURSOR: u32 = 2;
const FRAME_DONE: u32 = 2;
const FRAME_FAILED: u32 = 3;
const FRAME_SANDBOX: u32 = 14;
const FRAME_POINTER: u32 = 16;
const FRAME_NO_POINTER: u32 = 17;
const NO_POINTER_MISSING: u32 = 0;
const REASON_BYTES_MAX: u32 = 4096;
/// The most cell heights one pointer ask names, the helper's
/// `POINTER_HEIGHTS_MAX`.
const HEIGHTS_MAX: usize = 16;
/// The most bytes a picture frame holds: six numbers and 64 × 64 pixels.
const PICTURE_BYTES_MAX: u32 = 24 + POINTER_SIDE_MAX * POINTER_SIDE_MAX * 4;
/// How long one round of asks may take before vtium ends the helper and
/// keeps its own pictures for the shapes not yet answered.
const ROUND_MS: u64 = 10_000;

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
                format!("{}.", std::io::Error::from(*kind))
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
        let mut out = REQUEST_MAGIC.to_vec();
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
    /// The helper cannot draw the file's picture, or the console refused a
    /// picture it sent: the words finish "because ...".
    Refused(String),
}

/// The 32-bit number at word `i` of `b`.
fn word(b: &[u8], i: usize) -> Option<u32> {
    let at = i.checked_mul(4)?;
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// Reads one frame from `r`: its kind and payload, its length checked
/// against its kind's bound before anything is allocated.
fn frame(r: &mut impl Read) -> Result<(u32, Vec<u8>), String> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head)
        .map_err(|_| "the font helper ended before it answered".to_owned())?;
    let (kind, len) = (
        word(&head, 0).unwrap_or(0),
        word(&head, 1).unwrap_or(u32::MAX),
    );
    let max = match kind {
        FRAME_DONE => 0,
        FRAME_SANDBOX => 4,
        FRAME_POINTER => PICTURE_BYTES_MAX,
        FRAME_NO_POINTER => 8_u32.saturating_add(REASON_BYTES_MAX),
        FRAME_FAILED => REASON_BYTES_MAX,
        other => return Err(format!("the font helper sent a frame of kind {other}")),
    };
    if len > max {
        return Err(format!(
            "the font helper sent a frame of {len} bytes, past its kind's {max}"
        ));
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    r.read_exact(&mut payload)
        .map_err(|_| "the font helper's answer was cut short".to_owned())?;
    Ok((kind, payload))
}

/// The text of a reason the helper sent, its control characters escaped so
/// that the helper cannot steer what shows the log.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .flat_map(|c| {
            let escaped = c.is_control().then(|| c.escape_default());
            escaped
                .into_iter()
                .flatten()
                .chain((!c.is_control()).then_some(c))
        })
        .collect()
}

/// Checks one picture frame's payload for `ask`: its shape the one asked,
/// its size among the heights asked for an SVG file and not one already
/// sent, and the picture itself ([`Arrow::from_rgba`]).
fn picture(payload: &[u8], ask: &Ask, sent: &[(u32, Arrow)]) -> Result<(u32, Arrow), String> {
    let field = |i| word(payload, i).ok_or("a picture without its fields");
    let (shape, size, width, height) = (field(0)?, field(1)?, field(2)?, field(3)?);
    let hot = (field(4)?, field(5)?);
    if shape != shape_code(ask.shape) {
        return Err("a picture of a shape it was not asked for".to_owned());
    }
    if ask.svg && !ask.heights.contains(&size) {
        return Err(format!(
            "a picture at {size} pixels, which was not asked for"
        ));
    }
    if size == 0 || sent.iter().any(|(s, _)| *s == size) {
        return Err(format!(
            "a second picture at {size} pixels, or one of no size"
        ));
    }
    let rgba = payload.get(24..).unwrap_or_default();
    Arrow::from_rgba(width, height, hot, rgba).map(|a| (size, a))
}

/// Reads the helper's answers to `asks`, in order, from `r`. Each frame is
/// checked before it is trusted: a picture that fails its checks refuses
/// that ask's file. The error is why the whole answer is refused: a frame
/// of a kind or length the console does not take, a failure the helper
/// reports, or an answer cut short.
pub fn answers(r: &mut impl Read, asks: &[Ask]) -> Result<Vec<Answer>, String> {
    let mut out = Vec::with_capacity(asks.len());
    for ask in asks {
        let mut pictures: Vec<(u32, Arrow)> = Vec::new();
        let mut verdict: Option<Answer> = None;
        loop {
            let (kind, payload) = frame(r)?;
            match kind {
                FRAME_SANDBOX => {}
                FRAME_FAILED => return Err(format!("the font helper failed: {}", text(&payload))),
                FRAME_DONE => break,
                FRAME_NO_POINTER => {
                    if word(&payload, 0) != Some(shape_code(ask.shape)) {
                        return Err(
                            "the font helper answered a shape it was not asked for".to_owned()
                        );
                    }
                    verdict.get_or_insert(match word(&payload, 1) {
                        Some(NO_POINTER_MISSING) => Answer::Missing,
                        _ => Answer::Refused(text(payload.get(8..).unwrap_or_default())),
                    });
                }
                _ => match picture(&payload, ask, &pictures) {
                    Ok(p) => pictures.push(p),
                    Err(why) => {
                        verdict.get_or_insert(Answer::Refused(format!(
                            "vtium refused a picture it sent: {why}"
                        )));
                    }
                },
            }
        }
        out.push(match verdict {
            Some(v) => v,
            None if pictures.is_empty() => Answer::Missing,
            None => Answer::Pictures(pictures),
        });
    }
    Ok(out)
}

/// Runs one round of asks through a font helper of its own, which
/// `launcher` starts, and reads its answers. The asks are written on a
/// thread, since a cursor file can be megabytes long and the helper
/// answers while it reads. A helper that gives no whole answer in
/// [`ROUND_MS`] is ended.
pub fn exchange(launcher: &Launcher, asks: &[Ask]) -> Result<Vec<Answer>, String> {
    let mut child = launcher.spawn_piped()?;
    let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("the font helper has no pipes".to_owned());
    };
    let bytes: Vec<Vec<u8>> = asks.iter().map(Ask::encode).collect();
    let writer = std::thread::Builder::new()
        .name("pointer-asks".to_owned())
        .spawn(move || {
            for b in bytes {
                if stdin.write_all(&b).is_err() {
                    break;
                }
            }
            // the helper ends once its input ends and its work is done
        })
        .map_err(|e| format!("cannot start the pointer asks' writer: {e}"))?;
    let (tx, rx) = mpsc::channel();
    let owned = asks.to_vec();
    let reader = std::thread::Builder::new()
        .name("pointer-answers".to_owned())
        .spawn(move || {
            let _ = tx.send(answers(&mut stdout, &owned));
        })
        .map_err(|e| format!("cannot start the pointer answers' reader: {e}"))?;
    let result = rx
        .recv_timeout(Duration::from_millis(ROUND_MS))
        .unwrap_or_else(|_| Err(format!("the font helper gave no answer in {ROUND_MS} ms")));
    let _ = child.kill();
    let _ = child.wait();
    let _ = writer.join();
    let _ = reader.join();
    result
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

/// The line vtium logs for a picture the helper cannot draw.
fn cannot_draw(shape: PointerShape, path: &Path, why: &str) -> String {
    format!(
        "The font helper cannot draw the {} shape from {}, because {why}. vtium draws its own picture for that shape.",
        shape.name(),
        path.display()
    )
}

/// What a load logs: each line at its level, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

/// One round of asks and its answers: [`exchange`] in the console, the
/// real helper's code in process in a test.
pub type Round<'a> = dyn FnMut(&[Ask]) -> Result<Vec<Answer>, String> + 'a;

/// The pictures of the cursor theme at `dir`: for each shape, the first of
/// its file names whose file vtium reads and the helper draws. A file that
/// is missing, that vtium does not read, or that the helper cannot draw
/// counts as missing, and the next name is tried in the next round.
pub fn load_theme(
    dir: &Path,
    trust: &Trust,
    size: Option<u32>,
    round: &mut Round<'_>,
) -> (PointerPictures, Log) {
    let mut table = PointerPictures::new(PictureSizes::Theme { size });
    let mut log = Log::default();
    // each shape still looking for a file, with the next name to try
    let mut looking: Vec<(PointerShape, usize)> =
        PointerShape::ALL.iter().map(|&s| (s, 0)).collect();
    let mut lacking: Vec<&str> = Vec::new();
    while !looking.is_empty() {
        let mut asks = Vec::new();
        let mut asked = Vec::new();
        for (shape, next) in std::mem::take(&mut looking) {
            let names = theme_names(shape);
            let mut at = next;
            let found = loop {
                let Some(name) = names.get(at) else {
                    break None;
                };
                at = at.saturating_add(1);
                let path = dir.join("cursors").join(name);
                match read(&path, trust, XCURSOR_FILE_MAX) {
                    Ok(Some(bytes)) => break Some((path, bytes)),
                    Ok(None) => {}
                    Err(line) => log.warnings.push(line),
                }
            };
            match found {
                Some((path, bytes)) => {
                    asks.push(Ask {
                        svg: false,
                        shape,
                        size,
                        heights: Vec::new(),
                        bytes: Arc::new(bytes),
                    });
                    asked.push((shape, at, path));
                }
                None => lacking.push(shape.name()),
            }
        }
        if asks.is_empty() {
            break;
        }
        let answered = match round(&asks) {
            Ok(a) => a,
            Err(why) => {
                log.warnings.push(format!(
                    "The font helper cannot draw the pointer file {}, because {why}. vtium draws its own pointer pictures.",
                    dir.display()
                ));
                return (table, log);
            }
        };
        for ((shape, at, path), answer) in asked.into_iter().zip(answered) {
            match answer {
                Answer::Pictures(pictures) => {
                    for (s, a) in pictures {
                        table.insert(shape, s, a);
                    }
                }
                Answer::Missing => looking.push((shape, at)),
                Answer::Refused(why) => {
                    log.warnings.push(format!(
                        "The font helper cannot draw the {} shape from {}, because {why}.",
                        shape.name(),
                        path.display()
                    ));
                    looking.push((shape, at));
                }
            }
        }
    }
    lacking.sort_by_key(|n| PointerShape::ALL.iter().position(|s| s.name() == *n));
    if !lacking.is_empty() {
        log.infos.push(lacks_line(dir, &lacking));
    }
    (table, log)
}

/// An SVG pointer file's pictures as they come in: the file's bytes, read
/// once, the table so far, and what each shape's first answer said.
#[derive(Debug)]
pub struct SvgFile {
    path: PathBuf,
    bytes: Arc<Vec<u8>>,
    table: PointerPictures,
    /// The shapes whose symbol the helper drew nothing for, missing or
    /// refused, which are never asked for again.
    given_up: BTreeSet<&'static str>,
    /// Whether the line about the shapes the file lacks was logged.
    lacks_logged: bool,
}

impl SvgFile {
    /// The SVG file at `path`, read under `trust`, or the line vtium logs
    /// when it does not read it.
    pub fn read(path: &Path, trust: &Trust) -> Result<SvgFile, String> {
        let bytes = read(path, trust, POINTER_SVG_BYTES_MAX)?.ok_or_else(|| {
            format!(
                "vtium does not read the pointer file {}, because it is missing.",
                path.display()
            )
        })?;
        Ok(SvgFile {
            path: path.to_owned(),
            bytes: Arc::new(bytes),
            table: PointerPictures::new(PictureSizes::Svg),
            given_up: BTreeSet::new(),
            lacks_logged: false,
        })
    }

    pub fn table(&self) -> &PointerPictures {
        &self.table
    }

    /// Asks for every shape at `heights` in one round, and takes the
    /// answers into the table. The first round logs the shapes the file
    /// lacks; each shape the helper cannot draw is logged once.
    pub fn draw(&mut self, heights: &[u32], round: &mut Round<'_>) -> Log {
        let mut log = Log::default();
        let asks: Vec<Ask> = PointerShape::ALL
            .iter()
            .filter(|s| !self.given_up.contains(s.name()))
            .map(|&shape| Ask {
                svg: true,
                shape,
                size: None,
                heights: heights.to_vec(),
                bytes: Arc::clone(&self.bytes),
            })
            .collect();
        if asks.is_empty() {
            return log;
        }
        let answered = match round(&asks) {
            Ok(a) => a,
            Err(why) => {
                log.warnings.push(format!(
                    "The font helper cannot draw the pointer file {}, because {why}. vtium draws its own pointer pictures.",
                    self.path.display()
                ));
                return log;
            }
        };
        let mut lacking = Vec::new();
        for (ask, answer) in asks.iter().zip(answered) {
            match answer {
                Answer::Pictures(pictures) => {
                    for (s, a) in pictures {
                        self.table.insert(ask.shape, s, a);
                    }
                }
                Answer::Missing => {
                    self.given_up.insert(ask.shape.name());
                    lacking.push(ask.shape.name());
                }
                Answer::Refused(why) => {
                    self.given_up.insert(ask.shape.name());
                    log.warnings.push(cannot_draw(ask.shape, &self.path, &why));
                }
            }
        }
        if !self.lacks_logged && !lacking.is_empty() {
            log.infos.push(lacks_line(&self.path, &lacking));
        }
        self.lacks_logged = true;
        log
    }
}

/// The line vtium logs for a path that is neither an SVG file nor a cursor
/// theme's folder.
pub fn unusable_line(path: &Path) -> String {
    format!(
        "The pointer file {} is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures.",
        path.display()
    )
}

/// The table every display's pointer reads, and the heights an SVG file
/// still has to be drawn at.
struct Store {
    pictures: Option<Arc<PointerPictures>>,
    /// The heights asked for an SVG file, so each is asked once.
    asked: BTreeSet<u32>,
    /// Where a new height goes, while an SVG file's loader runs.
    heights: Option<mpsc::Sender<u32>>,
    /// Whether the table changed since the loop last looked.
    changed: bool,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    pictures: None,
    asked: BTreeSet::new(),
    heights: None,
    changed: false,
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

/// A display draws its pointer at cells `cell_height` tall: an SVG file is
/// drawn at that height, once, when it was not yet.
pub fn want(cell_height: u32) {
    let mut s = store();
    if s.heights.is_some() && s.asked.insert(cell_height) {
        let sent = s.heights.as_ref().map(|tx| tx.send(cell_height).is_ok());
        if sent == Some(false) {
            s.heights = None;
        }
    }
}

/// Whether the table changed since the last call: the loop then draws each
/// display, so that its pointer takes the new pictures.
pub fn take_changed() -> bool {
    std::mem::take(&mut store().changed)
}

fn publish(table: &PointerPictures, wake: &(dyn Fn() + Send + Sync)) {
    {
        let mut s = store();
        s.pictures = Some(Arc::new(table.clone()));
        s.changed = true;
    }
    wake();
}

fn emit(log: &Log) {
    for line in &log.warnings {
        log::warn!("{line}");
    }
    for line in &log.infos {
        log::info!("{line}");
    }
}

/// Starts loading the pointer file at `path`, on a thread of its own,
/// whose helpers `launcher` starts; `wake` turns the loop once the table
/// changes. `size` is `pointer-size`. Each file is read under `trust`.
pub fn start(
    path: &Path,
    size: Option<u32>,
    trust: Trust,
    launcher: Launcher,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let located = locate(path);
    let mut round = move |asks: &[Ask]| exchange(&launcher, asks);
    match located {
        Located::Unusable(path) => log::warn!("{}", unusable_line(&path)),
        Located::Theme(dir) => {
            let spawned = std::thread::Builder::new()
                .name("pointer-file".to_owned())
                .spawn(move || {
                    let (table, log) = load_theme(&dir, &trust, size, &mut round);
                    emit(&log);
                    publish(&table, wake.as_ref());
                });
            if let Err(e) = spawned {
                log::warn!("vtium cannot start reading the pointer file: {e}");
            }
        }
        Located::Svg(file) => {
            let mut svg = match SvgFile::read(&file, &trust) {
                Ok(svg) => svg,
                Err(line) => return log::warn!("{line}"),
            };
            let (tx, rx) = mpsc::channel::<u32>();
            store().heights = Some(tx);
            let spawned = std::thread::Builder::new()
                .name("pointer-file".to_owned())
                .spawn(move || {
                    while let Ok(first) = rx.recv() {
                        // every height waiting goes in one round
                        let mut heights = vec![first];
                        heights.extend(rx.try_iter());
                        heights.truncate(HEIGHTS_MAX);
                        let log = svg.draw(&heights, &mut round);
                        emit(&log);
                        publish(svg.table(), wake.as_ref());
                    }
                });
            if let Err(e) = spawned {
                log::warn!("vtium cannot start reading the pointer file: {e}");
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests;
