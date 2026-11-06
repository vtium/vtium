//! `conformance scenarios --play`: a behaviour scenario played in the
//! reviewer's terminal one command at a time (ADR-1504 D6). Each step
//! shows the command and prints its side effects; a dump draws the screen
//! with its colors and attributes, from the frame view's cells; then the
//! player waits for a key.

use std::io::Write;

use super::Scenario;
use super::run::Runner;
use super::transcript::{self, Block};
use crate::replay;
use crate::view::View;

/// One cell's look, as a `cells` line or the cursor's cell describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Look {
    wide: u8,
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
    ul: (u8, u8, u8),
    underline: Option<&'static str>,
    bold: bool,
    italic: bool,
    blink: bool,
    strike: bool,
}

fn rgb(text: &str) -> Option<(u8, u8, u8)> {
    let h = text.strip_prefix('#')?;
    let v = |i: usize| u8::from_str_radix(h.get(i..i.saturating_add(2))?, 16).ok();
    Some((v(0)?, v(2)?, v(4)?))
}

/// A cell description, `w= fg= bg= ul=` and its keys, read over `base`.
fn look(words: &[&str], base: &Look) -> Look {
    let mut l = base.clone();
    l.wide = 1;
    for w in words {
        let (k, v) = w.split_once('=').unwrap_or((w, ""));
        match k {
            "w" => l.wide = v.parse().unwrap_or(1),
            "fg" => l.fg = rgb(v).unwrap_or(l.fg),
            "bg" => l.bg = rgb(v).unwrap_or(l.bg),
            "ul" => l.ul = rgb(v).unwrap_or(l.ul),
            "underline" => {
                l.underline = Some(match v {
                    "double" => "4:2",
                    "curly" => "4:3",
                    "dotted" => "4:4",
                    "dashed" => "4:5",
                    _ => "4",
                });
            }
            "bold" => l.bold = true,
            "italic" => l.italic = true,
            "blink" => l.blink = true,
            "strike" => l.strike = true,
            _ => {}
        }
    }
    l
}

/// The SGR that draws `l` in a terminal with direct color.
fn sgr(l: &Look) -> String {
    let (f, b, u) = (l.fg, l.bg, l.ul);
    let mut s = format!(
        "\x1b[0;38;2;{};{};{};48;2;{};{};{}",
        f.0, f.1, f.2, b.0, b.1, b.2
    );
    if let Some(ul) = l.underline {
        s.push_str(&format!(";{ul};58;2;{};{};{}", u.0, u.1, u.2));
    }
    for (on, code) in [
        (l.bold, ";1"),
        (l.italic, ";3"),
        (l.blink, ";5"),
        (l.strike, ";9"),
    ] {
        if on {
            s.push_str(code);
        }
    }
    s.push('m');
    s
}

/// Whether `c` joins the character before it in its cell: a combining
/// mark, a variation selector or a joiner. An approximation, which is
/// enough to draw a screen for a reviewer.
fn joins(c: char) -> bool {
    matches!(u32::from(c),
        0x0300..=0x036f | 0x0483..=0x0489 | 0x0591..=0x05bd | 0x0610..=0x061a
        | 0x064b..=0x065f | 0x0e31 | 0x0e34..=0x0e3a | 0x1ab0..=0x1aff
        | 0x1dc0..=0x1dff | 0x200c..=0x200d | 0x20d0..=0x20ff
        | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f)
}

/// Draws a frame view's block: a box around the screen, each cell in its
/// colors and attributes, the cursor's cell as the cursor draws it.
fn draw(body: &[String], out: &mut String) {
    let mut cols = 0usize;
    let mut base = Look::default();
    let mut cursor: Option<(usize, usize, Look)> = None;
    let mut rows: Vec<(String, Vec<Look>)> = Vec::new();
    for line in body {
        let words: Vec<&str> = line.split(' ').collect();
        match words.first().copied() {
            Some("size") => {
                cols = words
                    .get(1)
                    .and_then(|s| s.split_once('x'))
                    .and_then(|(c, _)| c.parse().ok())
                    .unwrap_or(0);
            }
            Some("palette") => base = look(words.get(2..).unwrap_or_default(), &base),
            Some("frame-cursor") if words.get(3) == Some(&"visible") => {
                let at = words.get(1).and_then(|p| p.split_once(','));
                if let Some((x, y)) = at.and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
                {
                    cursor = Some((x, y, look(words.get(4..).unwrap_or_default(), &base)));
                }
            }
            Some("row") => {
                let text = line
                    .split_once('|')
                    .and_then(|(_, t)| t.strip_suffix('|'))
                    .unwrap_or_default();
                rows.push((text.to_owned(), vec![look(&[], &base); cols]));
            }
            Some("cells") => {
                let span = words.get(2).and_then(|s| s.split_once('-'));
                let (Some((a, b)), Some((_, cells))) = (span, rows.last_mut()) else {
                    continue;
                };
                let l = look(words.get(3..).unwrap_or_default(), &base);
                let (a, b): (usize, usize) = (a.parse().unwrap_or(0), b.parse().unwrap_or(0));
                for cell in cells.iter_mut().take(b.saturating_add(1)).skip(a) {
                    cell.clone_from(&l);
                }
            }
            _ => {}
        }
    }
    let bar = "─".repeat(cols);
    out.push_str(&format!("┌{bar}┐\n"));
    for (y, (text, looks)) in rows.iter().enumerate() {
        out.push('│');
        let mut chars = text.chars().peekable();
        let mut x = 0;
        while x < cols {
            let mut l = looks.get(x).cloned().unwrap_or_default();
            if l.wide == 0 {
                x = x.saturating_add(1);
                continue;
            }
            let mut glyph = String::new();
            if let Some(c) = chars.next() {
                glyph.push(c);
                while let Some(&m) = chars.peek() {
                    if !joins(m) {
                        break;
                    }
                    glyph.push(m);
                    chars.next();
                }
            } else {
                glyph.push(' ');
            }
            if let Some((cx, cy, cl)) = &cursor
                && *cx == x
                && *cy == y
            {
                l.clone_from(cl);
            }
            out.push_str(&sgr(&l));
            out.push_str(&glyph);
            x = x.saturating_add(usize::from(l.wide.max(1)));
        }
        out.push_str("\x1b[0m│\n");
    }
    out.push_str(&format!("└{bar}┘\n"));
}

/// What a reviewer's key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Quit,
}

/// Plays `s` for a reviewer: `on_file` is its transcript on file, if any,
/// `provisional` the reason the provisional list gives, and `wait` waits
/// for the reviewer's key after each command. Returns whether the
/// reviewer played it to its end.
pub fn play(
    runner: &Runner,
    s: &Scenario,
    on_file: Option<&str>,
    provisional: Option<&str>,
    out: &mut dyn Write,
    wait: &mut dyn FnMut() -> Step,
) -> Result<bool, String> {
    let played = runner.play(s, &[])?;
    let (input, tags) = runner.input(s, View::Frame, &[]);
    let frame =
        replay::run(&input).map_err(|f| format!("the replay exited {}: {}", f.code, f.message))?;
    let blocks: Vec<Block> = transcript::blocks(&frame);
    let mut at = Vec::new();
    for (j, tag) in tags.iter().enumerate() {
        if tag.is_some() {
            at.push(j);
        }
    }
    let mut text = format!("\x1b[1mscenario {}\x1b[0m  {}\n", s.name, s.file);
    text.push_str(&match provisional {
        Some(reason) => format!("provisional: {reason}\n"),
        None => "reviewed: its transcript judges\n".to_owned(),
    });
    for line in &s.header {
        text.push_str(&format!("\x1b[2m{line}\x1b[0m\n"));
    }
    text.push_str(
        &match on_file.map(|f| transcript::differs(f, &played.transcript)) {
            None => "This scenario has no transcript on file yet.\n".to_owned(),
            Some(None) => "The transcript on file matches this run.\n".to_owned(),
            Some(Some(why)) => format!("The transcript on file differs from this run. {why}\n"),
        },
    );
    let emit = |text: &mut String, out: &mut dyn Write| -> Result<(), String> {
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        text.clear();
        Ok(())
    };
    let commands: Vec<_> = s.commands().collect();
    let sections: Vec<Vec<&str>> = {
        let mut v: Vec<Vec<&str>> = Vec::new();
        for line in played.transcript.lines() {
            if line.starts_with("> ") {
                v.push(Vec::new());
            }
            if let Some(last) = v.last_mut() {
                last.push(line);
            }
        }
        v
    };
    for (i, section) in sections.iter().enumerate() {
        let c = commands.get(i);
        text.push_str(&format!(
            "\n\x1b[1m{}\x1b[0m\n",
            section.first().copied().unwrap_or_default()
        ));
        let mut dump = Vec::new();
        for line in section.iter().skip(1) {
            if line.starts_with("< ") {
                text.push_str(&format!("\x1b[36m{line}\x1b[0m\n"));
            } else {
                dump.push(*line);
            }
        }
        if c.is_some_and(|c| !c.dump.is_empty()) {
            if let Some(b) = at.get(i).and_then(|&j| blocks.get(j)) {
                draw(&b.body, &mut text);
            }
            for line in dump.iter().filter(|l| {
                ["cursor ", "flags ", "test state"]
                    .iter()
                    .any(|p| l.starts_with(p))
            }) {
                text.push_str(&format!("{line}\n"));
            }
        }
        text.push_str("\x1b[2m(any key: next command, q: stop)\x1b[0m");
        emit(&mut text, out)?;
        let step = wait();
        text.push_str("\r\x1b[2K");
        if step == Step::Quit {
            text.push('\n');
            emit(&mut text, out)?;
            return Ok(false);
        }
    }
    text.push_str(&format!(
        "\nThe scenario {} ends here. Its assertions: {}\n",
        s.name,
        match &played.assertions {
            crate::probes::Outcome::Pass => "pass".to_owned(),
            crate::probes::Outcome::Fail(why) | crate::probes::Outcome::Unjudged(why) =>
                format!("FAIL, {why}"),
        }
    ));
    emit(&mut text, out)?;
    Ok(true)
}
