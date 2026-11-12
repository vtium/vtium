//! `preview FONT...` (ADR-1201 D9): each font's detailed specimen, the
//! one the TUI's Fonts view shows (ADR-1201 D4), drawn by the glyph helper
//! under its own sandbox, and printed as a picture through kitty graphics'
//! raw pixels where the terminal answers the graphics query, or as a text
//! summary otherwise. The picture marks what the font itself draws, what
//! the built-in font draws for it, and what neither draws. It writes
//! nothing but its output.
//!
//! A FONT is a frit's name, a built-in font's, or any source a frit's
//! `source=` takes, a family alone being `font:FAMILY`, as `new` takes it.
//! Each is drawn at `--size POINTS`, else vtium.conf's `font-size` where
//! it is set and not automatic, else [`POINTS`], at the density
//! [`crate::tui::density`] gives.

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::serve::FritText;
use crate::specimen::{PICTURE_MAX, Sample, Summary, pixel_height};
use crate::tui::Paths;
use crate::tui::input::Reply;

/// The size a font is previewed at when nothing names one.
pub const POINTS: u32 = 12;

/// The first image id `preview` gives its pictures, one id a font, so
/// each picture stays on the screen beside the one before it.
const IMAGE_BASE: u32 = 12_010;

/// The frit `font` names: a frit or a built-in font in `paths`' frits, or
/// a one-source frit of a source as `new` takes it.
/// A name a frit could bear is looked for in the frits' directory, so it
/// fails when there is none.
fn frit_for(paths: &Paths, font: &str) -> Result<FritText, String> {
    if crate::frit::RESERVED.contains(&font) {
        return crate::tui::frit_text(Path::new(""), font);
    }
    if crate::frit::valid_name(font) {
        let frits = paths.frits.as_ref().map_err(Clone::clone)?;
        if frits.join(format!("{font}.vtfrit")).is_file() {
            return crate::tui::frit_text(frits, font);
        }
    }
    let name = match crate::commands::name_from(font) {
        n if n.is_empty() => "preview".to_owned(),
        n => n,
    };
    Ok(FritText {
        text: crate::commands::frit_from(&name, &[font.to_owned()], None, None),
        name,
    })
}

/// Whether the controlling terminal answers kitty graphics' query before
/// DA1, asked in raw mode within [`crate::switch::WAIT`].
fn draws_pictures() -> bool {
    let Ok(tty) = OpenOptions::new().read(true).write(true).open("/dev/tty") else {
        return false;
    };
    let query = format!(
        "\x1b_Gi={},s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\",
        crate::tui::model::QUERY_ID
    );
    crate::switch::Raw::new(&tty)
        .and_then(|_raw| {
            let deadline = crate::switch::deadline();
            crate::switch::ask(&mut &tty, query.as_bytes(), deadline)
        })
        .is_ok_and(|replies| replies.contains(&Reply::Graphics(true)))
}

/// A summary's words: the cell, the grid where a display was measured,
/// how much of the specimen the font draws itself, and how it makes its
/// superscripts and subscripts (ADR-1202 D14), each field after a tab.
pub fn summary_text(s: &Summary) -> String {
    let grid = s
        .grid
        .map(|(c, r)| format!(", console {c}x{r} characters"))
        .unwrap_or_default();
    let script = s
        .script
        .as_ref()
        .map(|made| format!("\t{}", crate::tui::words::script_caption(made)))
        .unwrap_or_default();
    format!(
        "character cell {}x{}{grid}\tcan display {} of the preview's {} characters{script}",
        s.cell.width, s.cell.height, s.covered, s.of
    )
}

/// `preview` with its words, writing to `out` and `err`; returns the exit
/// status: 0 when every font drew, 1 when one did not.
pub fn command(words: &[String], paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut fonts = Vec::new();
    let mut size = None;
    let mut it = words.iter();
    while let Some(word) = it.next() {
        match word.as_str() {
            "--size" => match it.next().and_then(|s| s.parse::<u32>().ok()) {
                Some(points @ 1..=96) => size = Some(points),
                _ => {
                    let _ = writeln!(
                        err,
                        "vtium-config: preview's --size takes 1 to 96 points\nTry vtium-config --help."
                    );
                    return 2;
                }
            },
            w if w.starts_with("--") => {
                let _ = writeln!(
                    err,
                    "vtium-config: preview has no option {w}\nTry vtium-config --help."
                );
                return 2;
            }
            w => fonts.push(w.to_owned()),
        }
    }
    if fonts.is_empty() {
        let _ = writeln!(
            err,
            "vtium-config: preview needs at least one font\nTry vtium-config --help."
        );
        return 2;
    }
    let conf = match crate::tui::read_conf(&paths.configdir.join("vtium.conf")) {
        Ok(conf) => conf,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}");
            return 1;
        }
    };
    let configured = conf
        .as_deref()
        .and_then(|c| vtium_settings::setting(c, "font-size"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|s| *s > 0);
    let points = size.or(configured).unwrap_or(POINTS);
    let displays = crate::display::displays(&paths.sys);
    let (dpi, _) = crate::tui::density(conf.as_deref(), &displays);
    let display = displays.first().map(|d| d.pixels);
    let height = pixel_height(points, dpi);
    let pictures = std::io::stdout().is_terminal() && draws_pictures();
    let locale = crate::tui::locale();
    let mut status = 0;
    for (n, font) in fonts.iter().enumerate() {
        let sample = frit_for(paths, font)
            .and_then(|frit| crate::specimen::detail(&paths.helper, &frit, height, &locale));
        let sample: Sample = match sample {
            Ok(sample) => sample,
            Err(why) => {
                let _ = writeln!(out, "failed\t{font}\t{why}");
                status = 1;
                continue;
            }
        };
        let summary = sample.summary(display);
        let _ = writeln!(out, "{font}\t{points} pt\t{}", summary_text(&summary));
        if pictures {
            match sample.detailed(PICTURE_MAX) {
                Ok(p) => {
                    let id = IMAGE_BASE.saturating_add(u32::try_from(n).unwrap_or(0));
                    let _ = out.write_all(&crate::kitty::print(id, p.width, p.height, &p.rgba));
                    let _ = out.write_all(b"\r\n");
                }
                Err(why) => {
                    let _ = writeln!(out, "failed\t{font}\t{why}");
                    status = 1;
                }
            }
        }
    }
    status
}
