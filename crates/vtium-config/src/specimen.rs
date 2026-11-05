//! A size's specimen (ADR-1201 D4): a few lines of Latin and box drawing
//! in a frit at the pixel height a size gives, as an RGBA picture.
//!
//! The glyph engine runs where the console runs it, in the helper the
//! console starts, `vtium-config serve`, under the helper's own sandbox:
//! [`draw`] starts it, asks it for the frit's cell at the height, then for
//! the specimen's characters at that cell, and lays out the glyphs it
//! answers with. The TUI's process parses no font, only the glyph chunk
//! format the console reads, through vtium-font's reader. The helper is
//! given [`HELPER_WAIT`] to answer, and is ended past it.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use vtium_font::set::{Set, SetFace, builtin, builtin_name};
use vtium_font::{Cell, Face, Format, Style};

use crate::serve::{
    ASK, FRAME_BYTES_MAX, FRAME_DONE, FRAME_DRAWS_NOTHING, FRAME_FAILED, FRAME_GLYPHS,
    FRAME_NO_OWN_CELL, FRAME_OUTLINE_CELL, FRAME_OWN_CELL, FRAME_SOURCES, FRAME_WARNING, FritText,
    LeftOut, OWN_CELL, SOURCES, ServeRequest,
};

/// The specimen's lines in every locale: Latin, digits and box drawing.
pub const LINES: [&str; 3] = [
    "The quick brown fox jumps",
    "over the lazy dog 0123456789",
    "┌──┬──┐ ├──┼──┤ └──┴──┘ ░▒▓█",
];

/// A line in the locale's script or letters, by the locale's language, as
/// `LANG` writes it before `_`, `.` or `@`. Each is a sentence that holds
/// the letters a reader of the language looks for. Only scripts whose
/// characters each take cells of their own are here: a script that joins
/// or stacks its letters, such as Arabic, Hebrew's points, Thai or
/// Devanagari, needs the specimen to shape clusters, which it does not
/// yet, so those locales get no line.
const LOCALE_LINES: [(&[&str], &str); 13] = [
    (
        &["ru", "uk", "be", "bg", "mk", "sr", "kk", "ky", "mn", "tg"],
        "Съешь же ещё этих мягких французских булок",
    ),
    (&["el"], "Ξεσκεπάζω την ψυχοφθόρα βδελυγμία"),
    (&["ja"], "いろはにほへと ちりぬるを 漢字"),
    (&["zh"], "我能吞下玻璃而不伤身体"),
    (&["ko"], "다람쥐 헌 쳇바퀴에 타고파"),
    (&["de"], "Zwölf Boxkämpfer jagen Viktor quer über den Deich"),
    (
        &["fr"],
        "Voix ambiguë d'un cœur qui au zéphyr préfère les jattes",
    ),
    (
        &["es"],
        "El pingüino Wenceslao hizo kilómetros bajo la lluvia",
    ),
    (&["pt"], "Luís argüia à Júlia que brações, fé, chá, óxido"),
    (&["pl"], "Pchnąć w tę łódź jeża lub ośm skrzyń fig"),
    (&["cs", "sk"], "Příliš žluťoučký kůň úpěl ďábelské ódy"),
    (&["tr"], "Pijamalı hasta yağız şoföre çabucak güvendi"),
    (&["vi"], "Tiếng Việt có dấu: đ ơ ư ă â ê ô"),
];

/// The specimen's lines for `locale`, as `LANG` or `LC_ALL` names it:
/// [`LINES`], then the locale's line where it has one.
pub fn lines(locale: &str) -> Vec<&'static str> {
    let language = locale.split(['_', '.', '@']).next().unwrap_or_default();
    let own = LOCALE_LINES
        .iter()
        .find(|(languages, _)| languages.contains(&language))
        .map(|(_, line)| *line);
    LINES.iter().copied().chain(own).collect()
}

/// The detailed specimen's lines (ADR-1201 D4, D9): what a font covers,
/// line by line: printable ASCII; Latin letters with accents, and
/// punctuation; box drawing and block elements; common symbols and
/// arrows; Powerline's symbols; then Greek, Cyrillic, CJK and emoji. Each
/// line fits in 40 cells.
pub const DETAIL: [&str; 13] = [
    " !\"#$%&'()*+,-./0123456789:;<=>?",
    "@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_",
    "`abcdefghijklmnopqrstuvwxyz{|}~",
    "ÀÁÂÄÅÆÇÈÉÊËÍÎÏÑÓÔÖØÚÜÝß àáâäåæçèéêëíîïñ",
    "óôöøúüýÿ ŁłŒœŠšŽžĞğİıŞş ¡¿«»“”‘’„–—…•",
    "┌─┬─┐ ╔═╦═╗ ╭─╮ ┏━┳━┓ ░▒▓█ ▀▄▌▐ ▖▗▘▝",
    "├─┼─┤ ╠═╬═╣ ╰─╯ ┗━┻━┛ ■□▪▫ ▲▼◆◇○●",
    "€£¥¢§¶©®™°±×÷µ ←↑→↓↔↕⇐⇒ ✓✗★☆♠♣♥♦",
    "\u{e0a0} \u{e0a1} \u{e0a2} \u{e0b0} \u{e0b1} \u{e0b2} \u{e0b3} \u{e0b4} \u{e0b6}",
    "Αα Ββ Γγ Δδ Λλ Ξξ Ππ Σσ Φφ Ψψ Ωω",
    "Аа Бб Вв Гг Дд Жж Ии Лл Фф Щщ Ыы Яя",
    "日本語 中文 한국어 ひらがな カタカナ",
    "😀 😂 👍 🎉 🚀 🌍 🔥 ✨ ❤",
];

/// Where a character of a specimen is drawn from (ADR-1201 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The frit's own sources.
    Font,
    /// The built-in font, which draws what the frit lacks.
    BuiltIn,
    /// Neither: the console draws nothing there.
    Missing,
}

/// The most pixels a picture takes, across and down: 3 MiB of RGBA, and
/// 4 MiB of kitty graphics' base64. A picture is cut to its area on the
/// screen first, and to this past it.
pub const PICTURE_MAX: (u32, u32) = (1024, 768);

/// How long the helper has to answer.
pub const HELPER_WAIT: Duration = Duration::from_secs(5);

/// The specimen's ink and paper.
const INK: [u8; 3] = [0xe8, 0xe8, 0xe8];
/// The ink of a character the built-in font draws in the detailed
/// specimen, and of the box that marks a missing one.
const MARK: [u8; 3] = [0x70, 0x70, 0x70];
const PAPER: [u8; 3] = [0x10, 0x10, 0x10];

/// A picture: its size in pixels and its pixels, RGBA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// How to start the glyph helper the console starts, `vtium-config serve`:
/// its program, and the options before `serve` that name where it finds
/// sources, as the command line named them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Helper {
    pub program: PathBuf,
    pub options: Vec<OsString>,
}

impl Helper {
    /// The helper at `program`, finding sources where the build says.
    pub fn at(program: impl Into<PathBuf>) -> Helper {
        Helper {
            program: program.into(),
            options: Vec::new(),
        }
    }
}

/// The pixel height `points` gives at `dpi` dots per inch, to the nearest
/// pixel: a point is 1/72 of an inch.
pub fn pixel_height(points: u32, dpi: u32) -> u32 {
    points.saturating_mul(dpi).saturating_add(36) / 72
}

/// One frame the helper sent.
type Frame = (u32, Vec<u8>);

fn frame(from: &mut dyn Read) -> Result<Frame, String> {
    let mut head = [0u8; 8];
    from.read_exact(&mut head)
        .map_err(|_| "the helper ended without an answer".to_owned())?;
    let (kind, len) = head.split_at(4);
    let kind = u32::from_le_bytes(kind.try_into().unwrap_or_default());
    let len = u32::from_le_bytes(len.try_into().unwrap_or_default());
    if len > FRAME_BYTES_MAX {
        return Err("the helper sent a frame past its bound".to_owned());
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    from.read_exact(&mut payload)
        .map_err(|_| "the helper's frame was cut short".to_owned())?;
    Ok((kind, payload))
}

fn pair(payload: &[u8]) -> Option<(u32, u32)> {
    let a = u32::from_le_bytes(payload.get(..4)?.try_into().ok()?);
    let b = u32::from_le_bytes(payload.get(4..8)?.try_into().ok()?);
    Some((a, b))
}

fn text(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload).into_owned()
}

/// The characters of `lines`, each once, in order.
fn characters(lines: &[&str]) -> Vec<char> {
    let mut seen = Vec::new();
    for c in lines.iter().flat_map(|l| l.chars()) {
        if c != ' ' && !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen
}

/// A request about `frit`.
fn request(kind: u32, frit: &FritText, cell: Cell, codepoints: Vec<u32>) -> ServeRequest {
    ServeRequest {
        kind,
        frit: frit.clone(),
        cell,
        style: vtium_font::Style::default(),
        codepoints,
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    }
}

fn send(stdin: &mut dyn Write, r: &ServeRequest) -> Result<(), String> {
    stdin
        .write_all(&r.encode())
        .and_then(|()| stdin.flush())
        .map_err(|e| format!("the helper: {e}"))
}

/// Sends `r`, as [`send`] does. A helper that fails as it starts sends its
/// failed frame and may close its end before the request is written, so
/// when the write fails, the helper's failed frame, read from `stdout`, is
/// the reason; the write's own error is the reason only when the helper
/// sent none.
fn send_or_failure(
    stdin: &mut dyn Write,
    stdout: &mut dyn Read,
    r: &ServeRequest,
) -> Result<(), String> {
    send(stdin, r).map_err(|why| failure_sent(stdout).unwrap_or(why))
}

/// The failure the helper sent before its frames end, if it sent one.
fn failure_sent(stdout: &mut dyn Read) -> Option<String> {
    loop {
        match frame(stdout) {
            Ok((FRAME_FAILED, payload)) => return Some(text(&payload)),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

/// Starts `helper` under its own sandbox, with an empty environment, `/`
/// as its directory and its three pipes: this process opens every
/// descriptor close-on-exec, the TUI's copies of the terminal included, so
/// the process that parses fonts holds none of the user's terminal, and the
/// helper sheds any other descriptor it is given as it starts
/// ([`crate::serve::SHED_VAR`]).
fn start(helper: &Helper) -> Result<(Child, ChildStdin, ChildStdout), String> {
    let mut child = Command::new(&helper.program)
        .args(&helper.options)
        .arg(crate::serve::SERVE_ARG)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", helper.program.display()))?;
    match (child.stdin.take(), child.stdout.take()) {
        (Some(i), Some(o)) => Ok((child, i, o)),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("the helper's pipes".to_owned())
        }
    }
}

/// Starts `helper` under its own sandbox, runs `talk` on its pipes, and
/// ends it; `talk`'s answer, or why there is none, within
/// [`HELPER_WAIT`].
fn with_helper<T: Send + 'static>(
    helper: &Helper,
    talk: impl FnOnce(&mut dyn Write, &mut dyn Read) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (mut child, mut stdin, mut stdout) = start(helper)?;
    let (done, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let result = talk(&mut stdin, &mut stdout);
        drop(stdin);
        let _ = done.send(result);
    });
    let result = answer
        .recv_timeout(HELPER_WAIT)
        .unwrap_or_else(|_| Err("the helper did not answer in time".to_owned()));
    let _ = child.kill();
    let _ = child.wait();
    result
}

/// What the helper said of one frit's sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sources {
    /// The helper read the frit: the sources it left out, which this
    /// machine lacks or refuses, none when it has them all.
    LeftOut(Vec<LeftOut>),
    /// The helper read the frit and found no source of it it can draw from,
    /// or the frit does not read: why.
    DrawsNothing(String),
    /// No answer came for the frit: the helper did not start, failed, or
    /// stalled before it answered; why. Nothing is known of its sources.
    Unknown(String),
}

/// The sources each of `frits` lacks or refuses, asked of one helper at
/// `helper` over one pipe, since the helper reads each frit once and
/// answers each sources query at once. Each answer is waited for at most
/// [`HELPER_WAIT`]; the answers that came are kept, and once one does not
/// come, the helper fails, ends, or cannot be started, every frit still
/// unanswered is [`Sources::Unknown`], with why. A failed frame is the
/// helper's own failure, as when it fails as it starts, and never a frit's
/// answer: a frit the helper draws nothing from has a frame of its own. A
/// warning is skipped, and a frame of any other kind is the helper's
/// failure too.
pub fn sources_of(helper: &Helper, frits: Vec<FritText>) -> Vec<Sources> {
    let count = frits.len();
    let mut answers = Vec::with_capacity(count);
    let session = start(helper).map(|(child, mut stdin, mut stdout)| {
        let (arrived, answer) = mpsc::channel();
        std::thread::spawn(move || {
            let empty = Cell {
                width: 0,
                height: 0,
            };
            // A helper that fails as it starts sends its failed frame and
            // may close its end before every request is written. The
            // answers it sent are still read, and its failed frame is the
            // reason; the write's own error is the reason only when the
            // helper sent none.
            let mut unsent = None;
            for frit in &frits {
                if let Err(why) = send(&mut stdin, &request(SOURCES, frit, empty, Vec::new())) {
                    unsent = Some(why);
                    break;
                }
            }
            for _ in 0..frits.len() {
                let found = loop {
                    match frame(&mut stdout) {
                        Ok((FRAME_SOURCES, payload)) => {
                            break Ok(Sources::LeftOut(
                                text(&payload).lines().filter_map(LeftOut::parse).collect(),
                            ));
                        }
                        Ok((FRAME_DRAWS_NOTHING, payload)) => {
                            break Ok(Sources::DrawsNothing(text(&payload)));
                        }
                        Ok((FRAME_FAILED, payload)) => {
                            break Err(format!("the helper failed: {}", text(&payload)));
                        }
                        // a warning a frit's text gave comes before its
                        // answer; any other kind answers no sources query,
                        // and taking the next frame as this frit's answer
                        // would give each later frit the one before's
                        Ok((FRAME_WARNING, _)) => {}
                        Ok((kind, _)) => {
                            break Err(format!(
                                "the helper sent a frame of kind {kind}, which answers no sources query"
                            ));
                        }
                        Err(why) => break Err(unsent.clone().unwrap_or(why)),
                    }
                };
                let failed = found.is_err();
                if arrived.send(found).is_err() || failed {
                    return;
                }
            }
        });
        (child, answer)
    });
    let why = match session {
        Ok((mut child, answer)) => {
            let why = loop {
                if answers.len() >= count {
                    break String::new();
                }
                match answer.recv_timeout(HELPER_WAIT) {
                    Ok(Ok(found)) => answers.push(found),
                    Ok(Err(why)) => break why,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        break "the helper did not answer in time".to_owned();
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break "the helper ended without an answer".to_owned();
                    }
                }
            };
            let _ = child.kill();
            let _ = child.wait();
            why
        }
        Err(why) => why,
    };
    while answers.len() < count {
        answers.push(Sources::Unknown(why.clone()));
    }
    answers
}

/// A frit's specimen as the helper drew it at one height: the glyphs it
/// sent at the frit's cell there, and what they are drawn with.
pub struct Sample {
    /// The specimen's lines.
    pub lines: Vec<&'static str>,
    /// The frit's cell at the height asked.
    pub cell: Cell,
    /// The height the faces are drawn at.
    face_height: u32,
    /// The frit's glyphs, each set the helper sent.
    sets: Vec<Arc<Set>>,
    /// The locale, which chooses the built-in font that draws what the
    /// frit lacks, as the console chooses it.
    locale: String,
}

/// What a size gives, for a terminal that draws no pictures (ADR-1201 D4):
/// the cell, the grid it gives on a display, and how many of the
/// specimen's characters the frit draws itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub cell: Cell,
    /// The columns and rows the cell gives on the display measured, where
    /// one was.
    pub grid: Option<(u32, u32)>,
    pub covered: usize,
    pub of: usize,
}

/// Asks the helper on `stdin` and `stdout` for `frit`'s cell at `height`
/// pixels: the cell, and the height its faces are drawn at, which is the
/// cell's height for an outline frit.
fn ask_cell(
    stdin: &mut dyn Write,
    stdout: &mut dyn Read,
    frit: &FritText,
    height: u32,
) -> Result<(Cell, u32), String> {
    send_or_failure(
        stdin,
        stdout,
        &request(OWN_CELL, frit, Cell { width: 0, height }, Vec::new()),
    )?;
    loop {
        let (kind, payload) = frame(stdout)?;
        let pair = || pair(&payload).ok_or("the helper's cell is cut short");
        match kind {
            FRAME_OWN_CELL => {
                let (w, h) = pair()?;
                return Ok((
                    Cell {
                        width: w,
                        height: h,
                    },
                    height,
                ));
            }
            FRAME_OUTLINE_CELL => {
                let (w, h) = pair()?;
                return Ok((
                    Cell {
                        width: w,
                        height: h,
                    },
                    h,
                ));
            }
            FRAME_NO_OWN_CELL => {
                let cell = Cell {
                    width: (height / 2).max(1),
                    height,
                };
                return Ok((cell, height));
            }
            FRAME_FAILED => return Err(text(&payload)),
            _ => {}
        }
    }
}

/// Asks the helper on `stdin` and `stdout` for the glyphs of `codepoints`
/// in `frit` at `cell`: each set it sends.
fn ask_glyphs(
    stdin: &mut dyn Write,
    stdout: &mut dyn Read,
    frit: &FritText,
    cell: Cell,
    codepoints: Vec<u32>,
) -> Result<Vec<Arc<Set>>, String> {
    send_or_failure(stdin, stdout, &request(ASK, frit, cell, codepoints))?;
    let mut sets = Vec::new();
    loop {
        let (kind, payload) = frame(stdout)?;
        match kind {
            FRAME_GLYPHS => {
                let set = Set::from_bytes(payload).map_err(|e| format!("{e:?}"))?;
                sets.push(Arc::new(set));
            }
            FRAME_DONE => return Ok(sets),
            FRAME_FAILED => return Err(text(&payload)),
            _ => {}
        }
    }
}

/// `frit`'s cell at `height` pixels, as the helper at `helper` gives it,
/// and the height its faces are drawn at.
pub fn own_cell(helper: &Helper, frit: &FritText, height: u32) -> Result<(Cell, u32), String> {
    let frit = frit.clone();
    with_helper(helper, move |stdin, stdout| {
        ask_cell(stdin, stdout, &frit, height)
    })
}

/// Asks the helper at `helper` for `frit`'s cell at `height` pixels and
/// then for the glyphs of `locale`'s specimen at that cell.
pub fn sample(
    helper: &Helper,
    frit: &FritText,
    height: u32,
    locale: &str,
) -> Result<Sample, String> {
    sample_of(helper, frit, height, locale, lines(locale))
}

/// The detailed specimen of `frit` at `height` pixels, [`DETAIL`] and the
/// locale's line where it has one, as the helper at `helper` draws it: what the TUI's Fonts view and `preview`
/// show of what a font covers (ADR-1201 D4, D9).
pub fn detail(
    helper: &Helper,
    frit: &FritText,
    height: u32,
    locale: &str,
) -> Result<Sample, String> {
    let own = lines(locale).into_iter().skip(LINES.len());
    sample_of(
        helper,
        frit,
        height,
        locale,
        DETAIL.into_iter().chain(own).collect(),
    )
}

/// `lines` of `frit` at `height` pixels, as the helper at `helper` draws
/// them: its cell, then the glyphs of the lines' characters at that cell.
fn sample_of(
    helper: &Helper,
    frit: &FritText,
    height: u32,
    locale: &str,
    lines: Vec<&'static str>,
) -> Result<Sample, String> {
    let codepoints: Vec<u32> = characters(&lines).into_iter().map(u32::from).collect();
    let frit = frit.clone();
    let (sets, cell, face_height) = with_helper(helper, move |stdin, stdout| {
        let (cell, face_height) = ask_cell(stdin, stdout, &frit, height)?;
        let sets = ask_glyphs(stdin, stdout, &frit, cell, codepoints)?;
        Ok((sets, cell, face_height))
    })?;
    Ok(Sample {
        lines,
        cell,
        face_height,
        sets,
        locale: locale.to_owned(),
    })
}

/// A picture of the console with a change applied (ADR-1201 D4): the TUI's
/// screen laid out at the grid the change gives on the display, drawn in
/// the change's font at its cell, and scaled down to fit `max`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    /// The font, a frit's name or a built-in font's.
    pub font: String,
    /// The pixel height the font is asked for.
    pub height: u32,
    /// The font's cell at that height, and the height its faces draw at.
    pub cell: Cell,
    pub face_height: u32,
    /// The display's size in pixels.
    pub display: (u32, u32),
    /// The columns and rows `cell` gives on `display`.
    pub grid: (usize, usize),
    /// The TUI's screen at `grid`.
    pub screen: Vec<crate::tui::canvas::Row>,
    /// The palette the draft's `palette` key names, and whether the TUI
    /// draws color at all.
    pub palette: String,
    pub color: bool,
    /// The most pixels the picture takes, across and down.
    pub max: (u32, u32),
}

/// The picture `shot` asks for, its glyphs drawn by the helper at
/// `helper` in `locale`.
pub fn screen(
    helper: &Helper,
    frit: &FritText,
    shot: &Shot,
    locale: &str,
) -> Result<Picture, String> {
    let mut chars: Vec<char> = Vec::new();
    for row in &shot.screen {
        for span in row {
            for c in shown_text(span).chars() {
                if c != ' ' && !c.is_control() && !chars.contains(&c) {
                    chars.push(c);
                }
            }
        }
    }
    let codepoints: Vec<u32> = chars.into_iter().map(u32::from).collect();
    let frit = frit.clone();
    let cell = shot.cell;
    let sets = with_helper(helper, move |stdin, stdout| {
        ask_glyphs(stdin, stdout, &frit, cell, codepoints)
    })?;
    let sample = Sample {
        lines: Vec::new(),
        cell,
        face_height: shot.face_height,
        sets,
        locale: locale.to_owned(),
    };
    sample.screen(shot)
}

/// A span's text as the screen shows it: a raw span without its escapes.
fn shown_text(span: &crate::tui::canvas::Span) -> String {
    if !span.raw {
        return span.text.clone();
    }
    let mut out = String::new();
    let mut chars = span.text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The specimen of `frit` at `height` pixels in `locale`, drawn by the
/// helper at `helper`, as many whole lines and columns of it as fit in
/// `max` pixels across and down, and at most [`PICTURE_MAX`].
pub fn draw(
    helper: &Helper,
    frit: &FritText,
    height: u32,
    max: (u32, u32),
    locale: &str,
) -> Result<Picture, String> {
    let max = (max.0.min(PICTURE_MAX.0), max.1.min(PICTURE_MAX.1));
    if height > max.1 || height == 0 {
        return Err(TOO_LARGE.to_owned());
    }
    sample(helper, frit, height, locale)?.picture(max)
}

impl Sample {
    /// The faces the specimen is drawn with: the frit's, then the built-in
    /// font, which draws every character the frit does not give, as the
    /// console draws it (ADR-1200 D7).
    fn faces(&self) -> Vec<SetFace> {
        let mut faces = self.frit_faces();
        let cell_height = faces.first().map_or(self.face_height, |f| f.cell().height);
        if let Some(face) = builtin(builtin_name(cell_height, &self.locale))
            .and_then(|set| SetFace::new(set, self.face_height))
        {
            faces.push(face);
        }
        faces
    }

    /// Where each character of the specimen is drawn from, each once, in
    /// order: the frit, the built-in font, or neither.
    pub fn origins(&self) -> Vec<(char, Origin)> {
        let faces = self.faces();
        let own = self.frit_faces().len();
        characters(&self.lines)
            .into_iter()
            .map(|c| (c, origin(&faces, own, c)))
            .collect()
    }

    /// The specimen as a picture that shows which characters the font has: a
    /// character the frit draws in the ink, one the built-in font draws in
    /// grey, and one neither draws as an empty grey box, so a gap shows
    /// (ADR-1201 D4).
    pub fn detailed(&self, max: (u32, u32)) -> Result<Picture, String> {
        let max = (max.0.min(PICTURE_MAX.0), max.1.min(PICTURE_MAX.1));
        let own = self.frit_faces().len();
        lay_out(&self.faces(), Some(own), &self.lines, max)
    }

    /// The faces of the frit's own glyphs.
    fn frit_faces(&self) -> Vec<SetFace> {
        self.sets
            .iter()
            .filter_map(|s| SetFace::new(Arc::clone(s), self.face_height))
            .collect()
    }

    /// The specimen as a picture, as many whole lines and columns of it as
    /// fit in `max` pixels, and at most [`PICTURE_MAX`].
    pub fn picture(&self, max: (u32, u32)) -> Result<Picture, String> {
        let max = (max.0.min(PICTURE_MAX.0), max.1.min(PICTURE_MAX.1));
        lay_out(&self.faces(), None, &self.lines, max)
    }

    /// The picture `shot` asks for: its screen laid out cell by cell in its
    /// cell on its display, each cell in its colors from the palette the
    /// shot names, the rest of the display in the palette's background,
    /// and the whole averaged down to fit the shot's `max`, keeping the
    /// display's shape (ADR-1201 D4).
    pub fn screen(&self, shot: &Shot) -> Result<Picture, String> {
        let faces = self.faces();
        let cell = shot.cell;
        let (dw, dh) = shot.display;
        if cell.width == 0 || cell.height == 0 || dw == 0 || dh == 0 {
            return Err(TOO_LARGE.to_owned());
        }
        let (ow, oh) = fit(shot.display, shot.max);
        if ow == 0 || oh == 0 {
            return Err(TOO_LARGE.to_owned());
        }
        let palette = vtium_term::Palette::named(&shot.palette).unwrap_or_default();
        let rgb = |c: vtium_term::Rgb| [c.r, c.g, c.b];
        let mut sums = Shrink::new((dw, dh), (ow, oh));
        let (grid_w, grid_h) = (
            u32::try_from(shot.grid.0)
                .unwrap_or(0)
                .saturating_mul(cell.width)
                .min(dw),
            u32::try_from(shot.grid.1)
                .unwrap_or(0)
                .saturating_mul(cell.height)
                .min(dh),
        );
        // the display past the grid's last column and row
        let paper = rgb(palette.bg());
        for y in 0..dh {
            let from = if y < grid_h { grid_w } else { 0 };
            for x in from..dw {
                sums.add(x, y, paper);
            }
        }
        let mut drawn: std::collections::BTreeMap<char, Option<vtium_font::Glyph>> =
            std::collections::BTreeMap::new();
        for (row, line) in shot.screen.iter().take(shot.grid.1).enumerate() {
            let top = u32::try_from(row)
                .unwrap_or(u32::MAX)
                .saturating_mul(cell.height);
            let mut col = 0usize;
            let mut cells: Vec<(char, [u8; 3], [u8; 3], usize)> = Vec::new();
            for span in line {
                let style = span.style;
                let pick = |c: Option<crate::tui::color::Color>, default: vtium_term::Rgb| match c {
                    Some(c) if shot.color => rgb(palette.color(c.index())),
                    _ => rgb(default),
                };
                let (mut ink, mut back) =
                    (pick(style.fg, palette.fg()), pick(style.bg, palette.bg()));
                if style.reverse {
                    std::mem::swap(&mut ink, &mut back);
                }
                for c in shown_text(span).chars() {
                    let wide = crate::tui::canvas::width(&c.to_string()).clamp(1, 2);
                    cells.push((c, ink, back, wide));
                }
            }
            for (c, ink, back, wide) in cells {
                if col >= shot.grid.0 {
                    break;
                }
                let span = cell.width.saturating_mul(u32::try_from(wide).unwrap_or(1));
                let left = u32::try_from(col)
                    .unwrap_or(u32::MAX)
                    .saturating_mul(cell.width);
                let glyph = drawn.entry(c).or_insert_with(|| {
                    if c == ' ' || c.is_control() {
                        return None;
                    }
                    faces
                        .iter()
                        .find(|f| f.has_glyph(c))
                        .and_then(|f| f.draw(&[c], Style::default()))
                });
                for y in 0..cell.height {
                    for x in 0..span {
                        let (px, py) = (left.saturating_add(x), top.saturating_add(y));
                        if px >= grid_w || py >= grid_h {
                            continue;
                        }
                        let color = glyph.as_ref().map_or(back, |g| over(g, x, y, ink, back));
                        sums.add(px, py, color);
                    }
                }
                col = col.saturating_add(wide);
            }
            // a row shorter than the grid has the background to its end
            let left = u32::try_from(col)
                .unwrap_or(u32::MAX)
                .saturating_mul(cell.width);
            for y in top..top.saturating_add(cell.height).min(grid_h) {
                for x in left.min(grid_w)..grid_w {
                    sums.add(x, y, paper);
                }
            }
        }
        // rows of the grid the screen has no line for
        let below = u32::try_from(shot.screen.len().min(shot.grid.1))
            .unwrap_or(u32::MAX)
            .saturating_mul(cell.height);
        for y in below.min(grid_h)..grid_h {
            for x in 0..grid_w {
                sums.add(x, y, paper);
            }
        }
        Ok(sums.picture())
    }

    /// The specimen as text for a terminal that draws no pictures: its
    /// cell, the grid it gives on a display of `display` pixels, and how
    /// many of its characters the frit draws itself.
    pub fn summary(&self, display: Option<(u32, u32)>) -> Summary {
        let chars = characters(&self.lines);
        let own = self.frit_faces();
        let covered = chars
            .iter()
            .filter(|&&c| own.iter().any(|f| f.has_glyph(c)))
            .count();
        let grid = display.and_then(|(w, h)| {
            Some((
                w.checked_div(self.cell.width)?,
                h.checked_div(self.cell.height)?,
            ))
        });
        Summary {
            cell: self.cell,
            grid,
            covered,
            of: chars.len(),
        }
    }
}

/// The size a picture of a display of `display` pixels takes to fit in
/// `max`, and at most [`PICTURE_MAX`], keeping the display's shape and
/// never larger than the display.
pub fn fit(display: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let (dw, dh) = (u64::from(display.0), u64::from(display.1));
    let mw = u64::from(max.0.min(PICTURE_MAX.0)).min(dw);
    let mh = u64::from(max.1.min(PICTURE_MAX.1)).min(dh);
    if dw == 0 || dh == 0 {
        return (0, 0);
    }
    // the side that binds: across when the display is wider than the room
    let (w, h) = if dw.saturating_mul(mh) > dh.saturating_mul(mw) {
        (mw, dh.saturating_mul(mw) / dw)
    } else {
        (dw.saturating_mul(mh) / dh, mh)
    };
    (u32::try_from(w).unwrap_or(0), u32::try_from(h).unwrap_or(0))
}

/// The color of pixel (`x`, `y`) of a cell that draws `glyph` in `ink` on
/// `back`: the glyph's coverage of `ink` over `back`, or its own color.
fn over(glyph: &vtium_font::Glyph, x: u32, y: u32, ink: [u8; 3], back: [u8; 3]) -> [u8; 3] {
    let Some(px) = glyph.pixel(x, y) else {
        return back;
    };
    let (alpha, color, premultiplied) = match glyph.format() {
        Format::Mask => (px.first().copied().unwrap_or(0), ink, false),
        Format::Rgba => (
            px.get(3).copied().unwrap_or(0),
            [
                px.first().copied().unwrap_or(0),
                px.get(1).copied().unwrap_or(0),
                px.get(2).copied().unwrap_or(0),
            ],
            true,
        ),
    };
    let a = u32::from(alpha);
    let mut out = [0u8; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let c = u32::from(color.get(i).copied().unwrap_or(0));
        let b = u32::from(back.get(i).copied().unwrap_or(0));
        let ink = if premultiplied {
            c
        } else {
            c.saturating_mul(a) / 255
        };
        let value = ink.saturating_add(b.saturating_mul(255u32.saturating_sub(a)) / 255);
        *slot = u8::try_from(value.min(255)).unwrap_or(255);
    }
    out
}

/// A picture of `to` pixels made from one of `from` pixels by averaging
/// each block of the larger picture that falls in a pixel of the smaller.
struct Shrink {
    from: (u32, u32),
    to: (u32, u32),
    /// Each pixel's sums of red, green and blue, and how many pixels.
    sums: Vec<[u32; 4]>,
}

impl Shrink {
    fn new(from: (u32, u32), to: (u32, u32)) -> Shrink {
        let n = usize::try_from(u64::from(to.0).saturating_mul(u64::from(to.1))).unwrap_or(0);
        Shrink {
            from,
            to,
            sums: vec![[0; 4]; n],
        }
    }

    /// Adds the larger picture's pixel (`x`, `y`) of `color`.
    fn add(&mut self, x: u32, y: u32, color: [u8; 3]) {
        let scale = |v: u32, from: u32, to: u32| {
            u32::try_from(u64::from(v).saturating_mul(u64::from(to)) / u64::from(from.max(1)))
                .unwrap_or(0)
                .min(to.saturating_sub(1))
        };
        let (tx, ty) = (
            scale(x, self.from.0, self.to.0),
            scale(y, self.from.1, self.to.1),
        );
        let at = usize::try_from(
            u64::from(ty)
                .saturating_mul(u64::from(self.to.0))
                .saturating_add(u64::from(tx)),
        )
        .unwrap_or(usize::MAX);
        if let Some(sum) = self.sums.get_mut(at) {
            let [r, g, b, n] = sum;
            for (s, c) in [r, g, b].into_iter().zip(color) {
                *s = s.saturating_add(u32::from(c));
            }
            *n = n.saturating_add(1);
        }
    }

    /// The smaller picture, each pixel the average of what was added to
    /// it.
    fn picture(self) -> Picture {
        let rgba = self
            .sums
            .iter()
            .flat_map(|[r, g, b, n]| {
                let n = (*n).max(1);
                let avg = |v: u32| u8::try_from(v / n).unwrap_or(255);
                [avg(*r), avg(*g), avg(*b), 255]
            })
            .collect();
        Picture {
            width: self.to.0,
            height: self.to.1,
            rgba,
        }
    }
}

/// Why a size has no picture when one line of it does not fit.
const TOO_LARGE: &str = "this size is larger than the preview area";

/// Where `c` is drawn from among `faces`, of which the first `own` are
/// the frit's.
fn origin(faces: &[SetFace], own: usize, c: char) -> Origin {
    match faces.iter().position(|f| f.has_glyph(c)) {
        Some(i) if i < own => Origin::Font,
        Some(_) => Origin::BuiltIn,
        None => Origin::Missing,
    }
}

/// `lines` drawn with `faces`, the first that holds a character drawing
/// it, in the first face's cell, as many whole lines and columns as fit in
/// `max` pixels. A glyph wider than the cell, as a CJK character, takes
/// two cells, as the console draws it. With `marked`, the number of the
/// frit's faces at the front of `faces`, each character shows where it
/// comes from: the built-in font's in grey, and a missing one as an empty
/// grey box as wide as the character.
fn lay_out(
    faces: &[SetFace],
    marked: Option<usize>,
    lines: &[&str],
    max: (u32, u32),
) -> Result<Picture, String> {
    let cell = faces
        .first()
        .map(Face::cell)
        .ok_or("the helper drew no glyph")?;
    let room = max.0.checked_div(cell.width).unwrap_or(0);
    // each line's glyphs and the cells each takes, as many as fit the room
    let laid: Vec<Vec<Placed>> = lines
        .iter()
        .map(|line| {
            let mut used = 0u32;
            line.chars()
                .map(|c| {
                    let glyph = faces
                        .iter()
                        .find(|f| f.has_glyph(c))
                        .and_then(|f| f.draw(&[c], Style::default()));
                    let wide = u32::try_from(crate::tui::canvas::width(&c.to_string()))
                        .unwrap_or(1)
                        .clamp(1, 2);
                    let cells = glyph
                        .as_ref()
                        .map_or(wide, |g| g.width().div_ceil(cell.width.max(1)).clamp(1, 2));
                    let (ink, boxed) = match marked.map(|own| origin(faces, own, c)) {
                        Some(Origin::BuiltIn) => (MARK, false),
                        Some(Origin::Missing) => (MARK, c != ' '),
                        _ => (INK, false),
                    };
                    Placed {
                        glyph,
                        cells,
                        ink,
                        boxed,
                    }
                })
                .take_while(|p| {
                    used = used.saturating_add(p.cells);
                    used <= room
                })
                .collect()
        })
        .collect();
    let cols = laid
        .iter()
        .map(|l| l.iter().map(|p| p.cells).sum::<u32>())
        .max()
        .unwrap_or(0);
    let shown = u32::try_from(laid.len())
        .unwrap_or(0)
        .min(max.1.checked_div(cell.height).unwrap_or(0));
    if cols == 0 || shown == 0 {
        return Err(TOO_LARGE.to_owned());
    }
    let width = cell.width.saturating_mul(cols);
    let height = cell.height.saturating_mul(shown);
    let stride = usize::try_from(width).unwrap_or(0).saturating_mul(4);
    let mut rgba: Vec<u8> = std::iter::repeat_n([PAPER[0], PAPER[1], PAPER[2], 255], {
        usize::try_from(width.saturating_mul(height)).unwrap_or(0)
    })
    .flatten()
    .collect();
    for (row, line) in laid
        .iter()
        .take(usize::try_from(shown).unwrap_or(0))
        .enumerate()
    {
        let top = u32::try_from(row).unwrap_or(0).saturating_mul(cell.height);
        let mut left = 0u32;
        for p in line {
            let span = cell.width.saturating_mul(p.cells);
            if let Some(glyph) = &p.glyph {
                paint(
                    &mut rgba,
                    stride,
                    glyph,
                    p.ink,
                    (left, top),
                    (span, cell.height),
                );
            } else if p.boxed {
                outline(&mut rgba, stride, p.ink, (left, top), (span, cell.height));
            }
            left = left.saturating_add(span);
        }
    }
    Ok(Picture {
        width,
        height,
        rgba,
    })
}

/// One character laid out: its glyph, the cells it takes, its ink, and
/// whether it is drawn as the empty box of a missing character.
struct Placed {
    glyph: Option<vtium_font::Glyph>,
    cells: u32,
    ink: [u8; 3],
    boxed: bool,
}

/// Draws an empty box of `ink` one pixel inside the cells at `at` of
/// `size` pixels: the mark of a missing character.
fn outline(rgba: &mut [u8], stride: usize, ink: [u8; 3], at: (u32, u32), size: (u32, u32)) {
    let (left, top) = at;
    let (w, h) = size;
    if w < 4 || h < 4 {
        return;
    }
    let (x0, y0, x1, y1) = (1, 1, w.saturating_sub(2), h.saturating_sub(2));
    let mut put = |x: u32, y: u32| {
        let at = usize::try_from(top.saturating_add(y))
            .unwrap_or(0)
            .saturating_mul(stride)
            .saturating_add(
                usize::try_from(left.saturating_add(x))
                    .unwrap_or(0)
                    .saturating_mul(4),
            );
        for (i, c) in ink.iter().enumerate() {
            if let Some(slot) = rgba.get_mut(at.saturating_add(i)) {
                *slot = *c;
            }
        }
    };
    for x in x0..=x1 {
        put(x, y0);
        put(x, y1);
    }
    for y in y0..=y1 {
        put(x0, y);
        put(x1, y);
    }
}

/// Draws `glyph` in `ink` over the paper in `rgba`, `stride` bytes a row,
/// with its top left at `at`, cut to `size` pixels.
fn paint(
    rgba: &mut [u8],
    stride: usize,
    glyph: &vtium_font::Glyph,
    ink: [u8; 3],
    at: (u32, u32),
    size: (u32, u32),
) {
    let (left, top) = at;
    for y in 0..glyph.height().min(size.1) {
        for x in 0..glyph.width().min(size.0) {
            let Some(px) = glyph.pixel(x, y) else {
                continue;
            };
            // coverage, or premultiplied color over the paper
            let (alpha, color) = match glyph.format() {
                Format::Mask => (px.first().copied().unwrap_or(0), ink),
                Format::Rgba => (
                    px.get(3).copied().unwrap_or(0),
                    [
                        px.first().copied().unwrap_or(0),
                        px.get(1).copied().unwrap_or(0),
                        px.get(2).copied().unwrap_or(0),
                    ],
                ),
            };
            let at = usize::try_from(top.saturating_add(y))
                .unwrap_or(0)
                .saturating_mul(stride)
                .saturating_add(
                    usize::try_from(left.saturating_add(x))
                        .unwrap_or(0)
                        .saturating_mul(4),
                );
            for (i, &c) in color.iter().enumerate() {
                let paper = u32::from(PAPER.get(i).copied().unwrap_or(0));
                let a = u32::from(alpha);
                let ink = match glyph.format() {
                    Format::Mask => u32::from(c).saturating_mul(a) / 255,
                    Format::Rgba => u32::from(c),
                };
                let value =
                    ink.saturating_add(paper.saturating_mul(255u32.saturating_sub(a)) / 255);
                if let Some(slot) = rgba.get_mut(at.saturating_add(i)) {
                    *slot = u8::try_from(value.min(255)).unwrap_or(255);
                }
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// A pipe whose reader is gone.
    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn failed_frame(why: &str) -> Vec<u8> {
        let mut bytes = FRAME_FAILED.to_le_bytes().to_vec();
        bytes.extend(u32::try_from(why.len()).unwrap_or(0).to_le_bytes());
        bytes.extend(why.as_bytes());
        bytes
    }

    fn any_request() -> ServeRequest {
        request(
            ASK,
            &FritText {
                name: "tiny".to_owned(),
                text: String::new(),
            },
            Cell {
                width: 8,
                height: 16,
            },
            Vec::new(),
        )
    }

    /// A sample of the built-in fonts alone, as a frit that sends no
    /// glyphs draws.
    fn builtin_only(cell: Cell) -> Sample {
        Sample {
            lines: Vec::new(),
            cell,
            face_height: cell.height,
            sets: Vec::new(),
            locale: "C".to_owned(),
        }
    }

    fn shot(
        display: (u32, u32),
        grid: (usize, usize),
        screen: Vec<crate::tui::canvas::Row>,
        max: (u32, u32),
    ) -> Shot {
        Shot {
            font: "unifont".to_owned(),
            height: 16,
            cell: Cell {
                width: 8,
                height: 16,
            },
            face_height: 16,
            display,
            grid,
            screen,
            palette: "vga".to_owned(),
            color: true,
            max,
        }
    }

    fn px(p: &Picture, x: u32, y: u32) -> [u8; 3] {
        let at = ((y * p.width + x) * 4) as usize;
        [p.rgba[at], p.rgba[at + 1], p.rgba[at + 2]]
    }

    /// Each cell is drawn in its colors from the palette the shot names,
    /// and the display past the grid in the palette's background.
    #[test]
    fn a_picture_of_the_console_draws_each_cell_in_the_palette_s_colors() {
        use crate::tui::canvas::Span;
        use crate::tui::color::{Color, Style};
        let cell = Cell {
            width: 8,
            height: 16,
        };
        let row = vec![
            Span {
                text: " ".to_owned(),
                style: Style::block(Color::Green),
                raw: false,
            },
            Span {
                text: "\u{2588}".to_owned(),
                style: Style::text(Color::Red),
                raw: false,
            },
        ];
        // a 24x20 display holds 3 by 1 cells of 8x16
        let p = builtin_only(cell)
            .screen(&shot((24, 20), (3, 1), vec![row], (24, 20)))
            .expect("a picture");
        assert_eq!((p.width, p.height), (24, 20));
        // vga's green, its red, then the background, and below the grid
        assert_eq!(px(&p, 4, 8), [0, 170, 0]);
        assert_eq!(px(&p, 12, 8), [170, 0, 0]);
        assert_eq!(px(&p, 20, 8), [0, 0, 0]);
        assert_eq!(px(&p, 4, 18), [0, 0, 0]);
        // with no color, the block is the default colors
        let mut plain = shot((24, 20), (3, 1), Vec::new(), (24, 20));
        plain.color = false;
        plain.screen = vec![vec![Span {
            text: " ".to_owned(),
            style: Style::block(Color::Green),
            raw: false,
        }]];
        let p = builtin_only(cell).screen(&plain).expect("a picture");
        assert_eq!(px(&p, 4, 8), [0, 0, 0]);
    }

    /// The detailed specimen holds every printable ASCII character, Latin
    /// letters with accents, box drawing and blocks, symbols and arrows,
    /// and Greek, Cyrillic, CJK and emoji, each line at most 40 cells.
    #[test]
    fn the_detailed_specimen_holds_each_range() {
        let all: String = DETAIL.concat();
        for c in ' '..='~' {
            assert!(all.contains(c), "{c:?}");
        }
        for c in [
            'é', 'ß', 'Ł', '“', '─', '╬', '█', '▖', '€', '→', '✓', 'Ω', 'Я', '日', '한', '😀',
        ] {
            assert!(all.contains(c), "{c:?}");
        }
        for line in DETAIL {
            assert!(crate::tui::canvas::width(line) <= 40, "{line}");
        }
    }

    /// Each character shows where it comes from: one the built-in font
    /// draws is grey, and one nothing draws is an empty grey box.
    #[test]
    fn a_missing_character_is_an_empty_box_and_a_built_in_one_is_grey() {
        let cell = Cell {
            width: 8,
            height: 16,
        };
        let mut sample = builtin_only(cell);
        sample.lines = vec!["\u{2588}\u{10fffd}"];
        assert_eq!(
            sample.origins(),
            [
                ('\u{2588}', Origin::BuiltIn),
                ('\u{10fffd}', Origin::Missing)
            ]
        );
        let p = sample.detailed((64, 16)).expect("a picture");
        assert_eq!((p.width, p.height), (16, 16));
        // the full block in grey; the missing character's box: its edge one
        // pixel in, and paper inside it
        assert_eq!(px(&p, 4, 8), MARK);
        assert_eq!(px(&p, 9, 8), MARK);
        assert_eq!(px(&p, 12, 1), MARK);
        assert_eq!(px(&p, 12, 8), PAPER);
        assert_eq!(px(&p, 8, 8), PAPER);
        // the plain specimen draws the built-in font's glyph in the ink and
        // no box
        let p = sample.picture((64, 16)).expect("a picture");
        assert_eq!(px(&p, 4, 8), INK);
        assert_eq!(px(&p, 9, 8), PAPER);
    }

    /// The picture keeps the display's shape inside its room, and each of
    /// its pixels is the average of the display's pixels it covers.
    #[test]
    fn a_picture_of_the_console_is_the_display_averaged_down() {
        use crate::tui::canvas::Span;
        use crate::tui::color::{Color, Style};
        assert_eq!(fit((1920, 1080), (480, 480)), (480, 270));
        assert_eq!(fit((640, 480), (1000, 100)), (133, 100));
        assert_eq!(fit((640, 480), (4000, 4000)), (640, 480), "never larger");
        assert_eq!(fit((4000, 3000), (4000, 4000)), PICTURE_MAX);
        let cell = Cell {
            width: 8,
            height: 16,
        };
        // one white cell and one black, averaged into one pixel: grey
        let row = vec![
            Span {
                text: " ".to_owned(),
                style: Style::block(Color::BrightWhite),
                raw: false,
            },
            Span {
                text: " ".to_owned(),
                style: Style::PLAIN,
                raw: false,
            },
        ];
        let p = builtin_only(cell)
            .screen(&shot((16, 16), (2, 1), vec![row], (1, 1)))
            .expect("a picture");
        assert_eq!((p.width, p.height), (1, 1));
        assert_eq!(px(&p, 0, 0), [127, 127, 127]);
    }

    #[test]
    fn a_request_the_helper_closed_on_gives_the_helpers_failure() {
        let mut stdout = std::io::Cursor::new(failed_frame("boom"));
        let sent = send_or_failure(&mut Closed, &mut stdout, &any_request());
        assert_eq!(sent, Err("boom".to_owned()));
    }

    #[test]
    fn a_request_the_helper_closed_on_without_a_failure_gives_the_writes_error() {
        let mut stdout = std::io::Cursor::new(Vec::new());
        let sent = send_or_failure(&mut Closed, &mut stdout, &any_request());
        assert!(
            matches!(&sent, Err(why) if why.starts_with("the helper: ")),
            "{sent:?}"
        );
    }
}
