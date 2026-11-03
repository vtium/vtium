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

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face, Format, Style};

use crate::serve::{
    ASK, FRAME_BYTES_MAX, FRAME_DONE, FRAME_FAILED, FRAME_GLYPHS, FRAME_NO_OWN_CELL,
    FRAME_OUTLINE_CELL, FRAME_OWN_CELL, FritText, OWN_CELL, ServeRequest,
};

/// The specimen's lines.
pub const LINES: [&str; 3] = [
    "The quick brown fox jumps",
    "over the lazy dog 0123456789",
    "┌──┬──┐ ├──┼──┤ └──┴──┘ ░▒▓█",
];

/// How long the helper has to answer.
pub const HELPER_WAIT: Duration = Duration::from_secs(5);

/// The specimen's ink and paper.
const INK: [u8; 3] = [0xe8, 0xe8, 0xe8];
const PAPER: [u8; 3] = [0x10, 0x10, 0x10];

/// A picture: its size in pixels and its pixels, RGBA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The pixel height `points` gives at `dpi` dots per inch, to the nearest
/// pixel: a point is 1/72 of an inch.
pub fn pixel_height(points: u32, dpi: u32) -> u32 {
    points.saturating_mul(dpi).saturating_add(36) / 72
}

/// One frame the helper sent.
type Frame = (u32, Vec<u8>);

fn frame(from: &mut impl Read) -> Result<Frame, String> {
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

/// The specimen's characters, each once, in order.
fn characters() -> Vec<char> {
    let mut seen = Vec::new();
    for c in LINES.iter().flat_map(|l| l.chars()) {
        if c != ' ' && !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen
}

/// The talk with the helper on its pipes: the cell at `height`, then the
/// glyph sets for the specimen's characters at that cell.
fn talk(
    stdin: &mut impl Write,
    stdout: &mut impl Read,
    frit: &FritText,
    height: u32,
) -> Result<(Vec<Set>, u32), String> {
    let request = |kind, cell, codepoints| ServeRequest {
        kind,
        frit: frit.clone(),
        cell,
        codepoints,
        clusters: Vec::new(),
        held: Vec::new(),
        console: None,
    };
    let send = |stdin: &mut dyn Write, r: ServeRequest| {
        stdin
            .write_all(&r.encode())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("the helper: {e}"))
    };
    send(
        stdin,
        request(OWN_CELL, Cell { width: 0, height }, Vec::new()),
    )?;
    let (cell, face_height) = loop {
        let (kind, payload) = frame(stdout)?;
        match kind {
            FRAME_OWN_CELL => {
                let (w, h) = pair(&payload).ok_or("the helper's cell is cut short")?;
                break (
                    Cell {
                        width: w,
                        height: h,
                    },
                    height,
                );
            }
            FRAME_OUTLINE_CELL => {
                let (w, h) = pair(&payload).ok_or("the helper's cell is cut short")?;
                break (
                    Cell {
                        width: w,
                        height: h,
                    },
                    h,
                );
            }
            FRAME_NO_OWN_CELL => {
                let cell = Cell {
                    width: (height / 2).max(1),
                    height,
                };
                break (cell, height);
            }
            FRAME_FAILED => return Err(text(&payload)),
            _ => {}
        }
    };
    let codepoints = characters().into_iter().map(u32::from).collect();
    send(stdin, request(ASK, cell, codepoints))?;
    let mut sets = Vec::new();
    loop {
        let (kind, payload) = frame(stdout)?;
        match kind {
            FRAME_GLYPHS => sets.push(Set::from_bytes(payload).map_err(|e| format!("{e:?}"))?),
            FRAME_DONE => break,
            FRAME_FAILED => return Err(text(&payload)),
            _ => {}
        }
    }
    Ok((sets, face_height))
}

/// The specimen of `frit` at `height` pixels, drawn by the helper at
/// `program`.
pub fn draw(program: &Path, frit: &FritText, height: u32) -> Result<Picture, String> {
    let mut child = Command::new(program)
        .arg(crate::serve::SERVE_ARG)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let (mut stdin, mut stdout) = match (child.stdin.take(), child.stdout.take()) {
        (Some(i), Some(o)) => (i, o),
        _ => return Err("the helper's pipes".to_owned()),
    };
    let (done, answer) = mpsc::channel();
    let frit_owned = frit.clone();
    std::thread::spawn(move || {
        let result = talk(&mut stdin, &mut stdout, &frit_owned, height);
        drop(stdin);
        let _ = done.send(result);
    });
    let result = answer
        .recv_timeout(HELPER_WAIT)
        .unwrap_or_else(|_| Err("the helper did not answer in time".to_owned()));
    let _ = child.kill();
    let _ = child.wait();
    let (sets, face_height) = result?;
    let faces: Vec<SetFace> = sets
        .into_iter()
        .filter_map(|s| SetFace::new(Arc::new(s), face_height))
        .collect();
    lay_out(&faces)
}

/// The specimen's lines drawn with `faces`, the first that holds a
/// character drawing it.
fn lay_out(faces: &[SetFace]) -> Result<Picture, String> {
    let cell = faces
        .first()
        .map(Face::cell)
        .ok_or("the helper drew no glyph")?;
    let cols = LINES.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let width = cell.width.saturating_mul(u32::try_from(cols).unwrap_or(0));
    let height = cell
        .height
        .saturating_mul(u32::try_from(LINES.len()).unwrap_or(0));
    let stride = usize::try_from(width).unwrap_or(0).saturating_mul(4);
    let mut rgba: Vec<u8> = std::iter::repeat_n([PAPER[0], PAPER[1], PAPER[2], 255], {
        usize::try_from(width.saturating_mul(height)).unwrap_or(0)
    })
    .flatten()
    .collect();
    for (row, line) in LINES.iter().enumerate() {
        let top = u32::try_from(row).unwrap_or(0).saturating_mul(cell.height);
        for (col, c) in line.chars().enumerate() {
            let left = u32::try_from(col).unwrap_or(0).saturating_mul(cell.width);
            let Some(glyph) = faces
                .iter()
                .find(|f| f.has_glyph(c))
                .and_then(|f| f.draw(&[c], Style::default()))
            else {
                continue;
            };
            for y in 0..glyph.height().min(cell.height) {
                for x in 0..glyph.width().min(cell.width) {
                    let Some(px) = glyph.pixel(x, y) else {
                        continue;
                    };
                    // coverage, or premultiplied color over the paper
                    let (alpha, color) = match glyph.format() {
                        Format::Mask => (px.first().copied().unwrap_or(0), INK),
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
                        let value = ink
                            .saturating_add(paper.saturating_mul(255u32.saturating_sub(a)) / 255);
                        if let Some(slot) = rgba.get_mut(at.saturating_add(i)) {
                            *slot = u8::try_from(value.min(255)).unwrap_or(255);
                        }
                    }
                }
            }
        }
    }
    Ok(Picture {
        width,
        height,
        rgba,
    })
}
