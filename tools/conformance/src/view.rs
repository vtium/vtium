//! The screen and frame views (INTERFACE.md, "Output"): the rows, their
//! runs and, in the frame view, the palette, the changed count and the
//! cursor. The head of each block (`size`, `cursor`, `flags`, the replies)
//! is the replay's.

use core::fmt::Write;

use vtium_term::{
    Age, Attrs, Color, CursorStyle, FrameCell, Glyph, Line, Palette, Rgb, Style, Terminal,
    UnderlineStyle, Width,
};

/// Which view the replay prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Screen,
    Frame,
}

/// The screen view's rows, each with its `attr` runs.
pub fn screen(term: &Terminal, out: &mut String) {
    let screen = term.screen();
    let palette = Palette::default();
    let frame = screen.frame(&palette, Age::NEVER);
    let reverse = screen.reverse_video();
    for row in 0..frame.rows() {
        let Some(line) = frame.line(row) else { break };
        let cells: Vec<(Option<String>, Width, String)> = (0..screen.cols())
            .map(|col| screen_cell(line, col, reverse, &palette))
            .collect();
        let _ = write!(out, "row {row} |");
        let last = cells
            .iter()
            .rposition(|(text, _, _)| text.is_some())
            .map_or(0, |i| i.saturating_add(1));
        let mut after_wide = false;
        for (text, width, _) in cells.iter().take(last) {
            if !after_wide {
                out.push_str(text.as_deref().unwrap_or(" "));
            }
            after_wide = !after_wide && *width == Width::Wide;
        }
        out.push_str("|\n");
        runs(
            &cells,
            |(_, _, desc)| desc.as_str(),
            |from, to, desc| {
                if !desc.is_empty() {
                    let _ = writeln!(out, "attr {row} {from}-{to}{desc}");
                }
            },
        );
    }
}

/// A screen-view cell: its text (`None` when it prints as empty), its
/// width, and its attributes as the `attr` line spells them.
fn screen_cell(
    line: &Line,
    col: usize,
    reverse: bool,
    palette: &Palette,
) -> (Option<String>, Width, String) {
    let cell = line.cell(col);
    let mut style = line.style(cell);
    // the C reference drew reverse video as every cell's inverse toggled
    if reverse && style.attrs.contains(Attrs::INVERSE) {
        style.attrs.remove(Attrs::INVERSE);
    } else if reverse {
        style.attrs.insert(Attrs::INVERSE);
    }
    let underlined = style.attrs.underline() != UnderlineStyle::None;
    let text = match line.glyph(cell) {
        Glyph::Empty => None,
        // the C reference drew a space with no underline as an empty cell
        Glyph::Char(' ') if !underlined => None,
        Glyph::Char(c) => Some(c.to_string()),
        Glyph::Cluster(s) => Some(s.to_owned()),
    };
    (text, cell.width(), attrs(&style, palette))
}

/// A style's keys as an `attr` line spells them, each only when it differs
/// from the default.
fn attrs(style: &Style, palette: &Palette) -> String {
    let mut out = String::new();
    let a = style.attrs;
    for (key, color) in [("fg", style.fg), ("bg", style.bg)] {
        if let Some(code) = color_code(color, palette) {
            let _ = write!(out, " {key}={code}");
        }
    }
    flag(&mut out, a.contains(Attrs::BOLD), "bold");
    flag(&mut out, a.contains(Attrs::ITALIC), "italic");
    match underline_name(a.underline()) {
        None => {}
        Some("single") => out.push_str(" underline"),
        Some(name) => {
            let _ = write!(out, " underline={name}");
        }
    }
    if let Some(code) = color_code(style.underline_color, palette) {
        let _ = write!(out, " ul={code}");
    }
    flag(&mut out, a.contains(Attrs::INVERSE), "inverse");
    flag(&mut out, a.contains(Attrs::PROTECTED), "protect");
    flag(&mut out, a.contains(Attrs::BLINK), "blink");
    flag(&mut out, a.contains(Attrs::DIM), "dim");
    flag(&mut out, a.contains(Attrs::STRIKE), "strike");
    out
}

/// A colour code: the index below 16, else `#rrggbb`; none for the default.
fn color_code(color: Color, palette: &Palette) -> Option<String> {
    match color {
        Color::Default => None,
        Color::Indexed(i) if i < 16 => Some(i.to_string()),
        Color::Indexed(i) => Some(hex(palette.color(i))),
        Color::Rgb(rgb) => Some(hex(rgb)),
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn flag(out: &mut String, on: bool, key: &str) {
    if on {
        out.push(' ');
        out.push_str(key);
    }
}

fn underline_name(u: UnderlineStyle) -> Option<&'static str> {
    match u {
        UnderlineStyle::None => None,
        UnderlineStyle::Single => Some("single"),
        UnderlineStyle::Double => Some("double"),
        UnderlineStyle::Curly => Some("curly"),
        UnderlineStyle::Dotted => Some("dotted"),
        UnderlineStyle::Dashed => Some("dashed"),
    }
}

/// Calls `emit` with each maximal run of equal descriptions, left to right.
fn runs<T>(cells: &[T], desc: impl Fn(&T) -> &str, mut emit: impl FnMut(usize, usize, &str)) {
    let mut from = 0;
    while let Some(first) = cells.get(from) {
        let d = desc(first);
        let len = cells
            .get(from..)
            .map_or(1, |rest| rest.iter().take_while(|c| desc(c) == d).count());
        let to = from.saturating_add(len.max(1));
        emit(from, to.saturating_sub(1), d);
        from = to;
    }
}

/// What the frame view keeps between blocks: the palette's name and the
/// age of the view's previous block.
#[derive(Debug)]
pub struct FrameState {
    pub palette: &'static str,
    since: Option<Age>,
}

impl FrameState {
    pub fn new(palette: &'static str) -> Self {
        Self {
            palette,
            since: None,
        }
    }
}

/// The cells changed for a display that drew at `since`, every cell when
/// it never drew, and the age it keeps after this frame.
pub fn changed(term: &mut Terminal, since: Option<Age>) -> (usize, Age) {
    let screen = term.screen();
    let frame = screen.frame(term.palette(), since.unwrap_or(Age::NEVER));
    let (rows, cols) = (frame.rows(), screen.cols());
    let n = match since {
        None => rows.saturating_mul(cols),
        Some(_) => (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (r, c)))
            .filter(|&(r, c)| frame.cell(r, c).is_some_and(|fc| fc.changed))
            .count(),
    };
    (n, term.screen_mut().checkpoint())
}

/// The frame view's lines after the block's head.
pub fn frame(term: &mut Terminal, state: &mut FrameState, out: &mut String) {
    let (changed, age) = changed(term, state.since);
    state.since = Some(age);
    let screen = term.screen();
    let palette = term.palette();
    let frame = screen.frame(palette, Age::NEVER);
    let reverse = screen.reverse_video();
    let _ = writeln!(
        out,
        "palette {} fg={} bg={}",
        state.palette,
        hex(palette.fg()),
        hex(palette.bg())
    );
    let _ = writeln!(out, "changed {changed}");
    let cols = screen.cols();
    let cell = |r: usize, c: usize| frame.cell(r, c);

    // the cursor: on the trailing half of a wide character it covers the
    // character from its leading cell; scrolled out of view it is hidden
    let (cx, cy) = screen.cursor();
    let x = cx.min(cols.saturating_sub(1));
    let view_row = cy.saturating_add(screen.scrolled_back());
    let visible = screen.cursor_visible() && view_row < frame.rows();
    let style = term.cursor_style();
    let _ = write!(out, "frame-cursor ");
    if visible {
        let tail = cell(view_row, x).is_some_and(|c| c.width == Width::WideTail);
        let x = if tail && x > 0 {
            x.saturating_sub(1)
        } else {
            x
        };
        let _ = write!(out, "{x},{view_row} {} visible", style_name(style));
        if let Some(c) = cell(view_row, x) {
            out.push_str(&cursor_cell(&frame_cell(&c, reverse), style));
        }
    } else {
        let _ = write!(out, "{x},{cy} {} hidden", style_name(style));
    }
    out.push('\n');

    let default = FCell::default_for(palette);
    for row in 0..frame.rows() {
        let cells: Vec<(FrameCell<'_>, String)> = (0..cols)
            .filter_map(|c| cell(row, c))
            .map(|c| {
                let desc = frame_cell(&c, reverse).describe();
                (c, desc)
            })
            .collect();
        let _ = write!(out, "row {row} |");
        let last = cells
            .iter()
            .rposition(|(c, _)| key(c).is_some_and(|k| k != Glyph::Char(' ')))
            .map_or(0, |i| i.saturating_add(1));
        for (c, _) in cells.iter().take(last) {
            if c.width == Width::WideTail {
                continue;
            }
            match key(c) {
                None => out.push(' '),
                Some(Glyph::Char(ch)) => out.push(ch),
                Some(Glyph::Cluster(s)) => out.push_str(s),
                Some(Glyph::Empty) => {}
            }
        }
        out.push_str("|\n");
        let default = default.describe();
        runs(
            &cells,
            |(_, d)| d.as_str(),
            |from, to, desc| {
                if desc != default {
                    let _ = writeln!(out, "cells {row} {from}-{to}{desc}");
                }
            },
        );
    }
}

/// A cell's glyph key: none for the trailing half of a wide character or
/// an empty cell.
fn key<'a>(c: &FrameCell<'a>) -> Option<Glyph<'a>> {
    match (c.width, c.glyph) {
        (Width::WideTail, _) | (_, Glyph::Empty) => None,
        (_, g) => Some(g),
    }
}

/// A frame cell as the frame view describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FCell {
    width: u8,
    fg: Rgb,
    bg: Rgb,
    ul: Rgb,
    ulset: bool,
    underline: UnderlineStyle,
    attrs: Attrs,
    inverse: bool,
}

impl FCell {
    fn default_for(palette: &Palette) -> Self {
        Self {
            width: 1,
            fg: palette.fg(),
            bg: palette.bg(),
            ul: palette.fg(),
            ulset: false,
            underline: UnderlineStyle::None,
            attrs: Attrs::empty(),
            inverse: false,
        }
    }

    fn describe(&self) -> String {
        let mut out = format!(
            " w={} fg={} bg={} ul={}",
            self.width,
            hex(self.fg),
            hex(self.bg),
            hex(self.ul)
        );
        flag(&mut out, self.ulset, "ulset");
        if let Some(name) = underline_name(self.underline) {
            let _ = write!(out, " underline={name}");
        }
        let a = self.attrs;
        flag(&mut out, a.contains(Attrs::BOLD), "bold");
        flag(&mut out, a.contains(Attrs::ITALIC), "italic");
        flag(&mut out, a.contains(Attrs::DIM), "dim");
        flag(&mut out, a.contains(Attrs::BLINK), "blink");
        flag(&mut out, self.inverse, "inverse");
        flag(&mut out, a.contains(Attrs::PROTECTED), "protect");
        flag(&mut out, a.contains(Attrs::STRIKE), "strike");
        out
    }
}

/// vtium-term's frame cell in the frame view's terms: the colours as the
/// frame resolved them, the underline colour following the foreground
/// unless SGR 58 set it, and inverse when reverse video, the cell's own
/// inverse and the selection leave it inverted.
fn frame_cell(c: &FrameCell<'_>, reverse: bool) -> FCell {
    FCell {
        width: match c.width {
            Width::Narrow | Width::Pad => 1,
            Width::Wide => 2,
            Width::WideTail => 0,
        },
        fg: c.fg,
        bg: c.bg,
        ul: c.underline_color.unwrap_or(c.fg),
        ulset: c.underline_color.is_some(),
        underline: c.attrs.underline(),
        attrs: c.attrs,
        inverse: (c.attrs.contains(Attrs::INVERSE) != reverse) != c.selected,
    }
}

/// The cell under the cursor as the cursor draws it: an underline cursor
/// toggles the underline, a bar leaves the cell, and a block swaps the
/// colours, the underline colour following the new foreground unless set.
fn cursor_cell(cell: &FCell, style: CursorStyle) -> String {
    let mut c = cell.clone();
    match style {
        CursorStyle::UnderlineBlink | CursorStyle::UnderlineSteady => {
            c.underline = match c.underline {
                UnderlineStyle::None => UnderlineStyle::Single,
                _ => UnderlineStyle::None,
            };
        }
        CursorStyle::BarBlink | CursorStyle::BarSteady => {}
        CursorStyle::Default | CursorStyle::BlockBlink | CursorStyle::BlockSteady => {
            core::mem::swap(&mut c.fg, &mut c.bg);
            if !c.ulset {
                c.ul = c.fg;
            }
        }
    }
    c.describe()
}

fn style_name(style: CursorStyle) -> &'static str {
    match style {
        CursorStyle::Default => "default",
        CursorStyle::BlockBlink => "block-blink",
        CursorStyle::BlockSteady => "block-steady",
        CursorStyle::UnderlineBlink => "underline-blink",
        CursorStyle::UnderlineSteady => "underline-steady",
        CursorStyle::BarBlink => "bar-blink",
        CursorStyle::BarSteady => "bar-steady",
    }
}
