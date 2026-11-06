//! The screen and frame views (INTERFACE.md, "Output"): the rows, their
//! runs and, in the frame view, the palette, the changed count and the
//! cursor; and, in a build with the bidi feature, the display view
//! (INTERFACE-vtium.md, "Right-to-left text"): each row in the order it is
//! drawn. The head of each block (`size`, `cursor`, `flags`, the replies)
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
    /// Each row in the order it is drawn, in a build with the bidi
    /// feature.
    #[cfg(feature = "bidi")]
    Display,
}

/// The display view: where the cursor is drawn and the side of its cell a
/// bar stands on, then each row as drawn from the left, a mirrored
/// character as its counterpart, an empty cell as a space and the second
/// cell of a wide character as nothing, the empty cells at its right end
/// trimmed (ADR-1004 D14).
#[cfg(feature = "bidi")]
pub fn display(term: &Terminal, out: &mut String) {
    let palette = Palette::default();
    let frame = term.screen().frame(&palette, Age::NEVER);
    let cursor = frame.cursor();
    let side = if cursor.rtl { "right" } else { "left" };
    let _ = writeln!(out, "display-cursor {},{} {side}", cursor.col, cursor.row);
    for row in 0..frame.rows() {
        let cells: Vec<Option<String>> = frame
            .cells(row)
            .map(|c| match c.glyph {
                Glyph::Char(ch) => Some(ch.to_string()),
                // a mirrored cluster draws its first character's
                // counterpart with the cluster's marks on it
                Glyph::Cluster(s) => Some(match c.mirror {
                    Some(m) => core::iter::once(m).chain(s.chars().skip(1)).collect(),
                    None => s.to_owned(),
                }),
                Glyph::Empty if c.width == Width::WideTail => Some(String::new()),
                Glyph::Empty => None,
            })
            .collect();
        let last = cells
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i.saturating_add(1));
        let _ = write!(out, "row {row} |");
        for cell in cells.iter().take(last) {
            out.push_str(cell.as_deref().unwrap_or(" "));
        }
        out.push_str("|\n");
    }
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
    #[cfg(feature = "images")]
    images(term, None, out);
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
    // reverse video draws as every cell's inverse toggled
    if reverse && style.attrs.contains(Attrs::INVERSE) {
        style.attrs.remove(Attrs::INVERSE);
    } else if reverse {
        style.attrs.insert(Attrs::INVERSE);
    }
    let underlined = style.attrs.underline() != UnderlineStyle::None;
    let text = match line.glyph(cell) {
        Glyph::Empty => None,
        // a space with no underline draws as an empty cell
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

/// A color code: the index below 16, else `#rrggbb`; none for the default.
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
    /// The image store's revision at the view's previous block, which a
    /// placement's `changed` flag is judged from.
    #[cfg(feature = "images")]
    revision: Option<u64>,
}

impl FrameState {
    pub fn new(palette: &'static str) -> Self {
        Self {
            palette,
            since: None,
            #[cfg(feature = "images")]
            revision: None,
        }
    }
}

/// The `placement` lines of the view's placements, in draw order, then an
/// `image` line for each image they show, numbered as placements first
/// show them. With `since`, the frame view's: each placement carries its
/// `changed` flag when it changed after the store's revision `since`
/// names, every one in the first block.
#[cfg(feature = "images")]
fn images(term: &Terminal, since: Option<Option<u64>>, out: &mut String) {
    let placements = term.screen().placements();
    let mut shown: Vec<&vtium_term::images::FramePlacement> = Vec::new();
    for (i, p) in placements.iter().enumerate() {
        let n = match shown.iter().position(|q| q.image == p.image) {
            Some(n) => n,
            None => {
                shown.push(p);
                shown.len().saturating_sub(1)
            }
        };
        let _ = write!(
            out,
            "placement {i} row {} col {} box {}x{} at {},{} size {}x{} src {},{} {}x{} cell {}x{} z {} {} {} image {n}",
            p.row,
            p.col,
            p.cols,
            p.rows,
            p.x,
            p.y,
            p.width,
            p.height,
            p.src_x,
            p.src_y,
            p.src_width,
            p.src_height,
            p.cell_width,
            p.cell_height,
            p.z,
            p.layer.name(),
            p.kind,
        );
        if p.opaque {
            out.push_str(" opaque");
        }
        if let Some(since) = since
            && since.is_none_or(|r| p.revision > r)
        {
            out.push_str(" changed");
        }
        out.push('\n');
    }
    let store = term.screen().images();
    for (n, p) in shown.iter().enumerate() {
        let _ = writeln!(
            out,
            "image {n} {}x{} digest {:08x}",
            p.image_width,
            p.image_height,
            digest(p.pixels.as_slice())
        );
        animation(store, n, p.image, out);
    }
}

/// The `anim` lines of image `n` (INTERFACE-vtium.md, "Kitty animation"),
/// none for a picture no animation command named: what the store records
/// of its animation, then each frame's gap and the digest of its whole
/// pixels as the store composes them.
#[cfg(feature = "images")]
fn animation(
    store: &vtium_term::images::Store,
    n: usize,
    image: vtium_term::images::ImageKey,
    out: &mut String,
) {
    use vtium_term::images::Run;
    let Some(a) = store.animation(image) else {
        return;
    };
    let run = match a.run {
        Run::Stopped => "stopped",
        Run::Loading => "loading",
        Run::Looping => "looping",
    };
    let _ = writeln!(
        out,
        "anim {n} frames {} current {} run {run} loops {} stamps {} {}",
        a.frames, a.current, a.loops, a.frame_stamp, a.run_stamp
    );
    for frame in 1..=a.frames {
        let gap = store.frame_gap(image, frame);
        let pixels = store.frame_pixels(image, frame);
        let (Some(gap), Some(pixels)) = (gap, pixels) else {
            // the store counts a frame it cannot give: print the hole, so
            // it shows as drift
            let _ = writeln!(out, "anim {n} frame {frame} missing");
            continue;
        };
        let _ = writeln!(
            out,
            "anim {n} frame {frame} gap {gap} digest {:08x}",
            digest(pixels.pixels.as_slice())
        );
    }
}

/// FNV-1a over each pixel's A, R, G, B bytes, rows top to bottom.
#[cfg(feature = "images")]
fn digest(pixels: &[u32]) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for &px in pixels {
        for byte in px.to_be_bytes() {
            h = (h ^ u32::from(byte)).wrapping_mul(16_777_619);
        }
    }
    h
}

/// The cells changed for a display that drew at `since`, every cell when
/// it never drew, and the age it keeps after this frame.
pub fn changed(term: &mut Terminal, since: Option<Age>) -> (usize, Age) {
    let screen = term.screen();
    let frame = written(screen.frame(term.palette(), since.unwrap_or(Age::NEVER)));
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

/// `frame` with every row in the order it was written, its cells' damage
/// as the screen keeps it: the frame view prints the cells as the program
/// wrote them, as interface 1.0's goldens hold, and the display view prints
/// the order a build with the bidi feature draws.
fn written(frame: vtium_term::Frame<'_>) -> vtium_term::Frame<'_> {
    #[cfg(feature = "bidi")]
    let frame = frame.with_order(vtium_term::Order::default());
    frame
}

/// The frame view's lines after the block's head.
pub fn frame(term: &mut Terminal, state: &mut FrameState, out: &mut String) {
    let (changed, age) = changed(term, state.since);
    state.since = Some(age);
    let screen = term.screen();
    let palette = term.palette();
    let frame = written(screen.frame(palette, Age::NEVER));
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
    #[cfg(feature = "images")]
    {
        images(term, Some(state.revision), out);
        state.revision = Some(term.screen().images().revision());
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

/// vtium-term's frame cell in the frame view's terms: the colors as the
/// frame resolved them, the underline color following the foreground
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
/// colors, the underline color following the new foreground unless set.
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
