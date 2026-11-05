//! The Flow view drawn (ADR-1403 D7): six columns, each in its own color,
//! displays, heads, looks and input devices as blocks of color, rules as
//! colored text, and lines between them; the card Enter opens below the
//! columns, and the route card `?` opens in their place.
//!
//! Each column stacks its items in rows of its own. The heads and their
//! looks share their rows, so each head sits beside its look. A column
//! taller than the room scrolls so the selected item shows. The lines run
//! in the gaps between the columns: down or up the gap's first cell from
//! the line's source, then across into an arrow at its target.

use std::collections::BTreeMap;

use super::canvas::{Canvas, mid, pick, wrap};
use super::color::{Style, palette};
use super::flow::{Flow, Item, Kind, New, Route, RuleState, Special, Target, general};
use super::words;

/// Where each column starts and how wide it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub x: [usize; 6],
    pub w: [usize; 6],
}

/// Each column's least and wanted width, as the mockup shares the room.
const LEAST: [usize; 6] = [11, 13, 11, 11, 13, 12];
const WANTED: [usize; 6] = [20, 28, 13, 20, 28, 30];
/// The order the columns take the cells left after the sharing.
const SPARE: [usize; 6] = [4, 1, 5, 0, 3, 2];

/// The columns at a screen `cols` wide: gaps of 2 cells, 3 from 100
/// columns, with 1 between a head and its look; each column takes its least
/// width and a share of the rest by how much more it wants.
pub fn geometry(cols: usize) -> Geometry {
    let g = if cols >= 100 { 3 } else { 2 };
    let gaps = [g, g, 1, g, g];
    let room = cols.saturating_sub(gaps.iter().sum());
    let least: usize = LEAST.iter().sum();
    let wanted: usize = WANTED.iter().sum();
    let extra = room.saturating_sub(least).min(wanted.saturating_sub(least));
    let span = wanted.saturating_sub(least).max(1);
    let mut w = [0usize; 6];
    for (i, slot) in w.iter_mut().enumerate() {
        let (l, p) = (
            LEAST.get(i).copied().unwrap_or(0),
            WANTED.get(i).copied().unwrap_or(0),
        );
        *slot = l.saturating_add(p.saturating_sub(l).saturating_mul(extra) / span);
    }
    let mut left = room.saturating_sub(w.iter().sum());
    let mut i = 0usize;
    while left > 0 {
        if let Some(slot) = SPARE.get(i % 6).and_then(|c| w.get_mut(*c)) {
            *slot = slot.saturating_add(1);
        }
        left -= 1;
        i = i.saturating_add(1);
    }
    let mut x = [0usize; 6];
    let mut at = 0usize;
    for i in 0..6 {
        if let Some(slot) = x.get_mut(i) {
            *slot = at;
        }
        at = at
            .saturating_add(w.get(i).copied().unwrap_or(0))
            .saturating_add(gaps.get(i).copied().unwrap_or(0));
    }
    Geometry { x, w }
}

/// One row of an item: its text and style, and whether its style fills the
/// column's width, as a block's does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub style: Style,
    pub block: bool,
}

fn text_line(text: impl Into<String>, style: Style) -> Line {
    Line {
        text: text.into(),
        style,
        block: false,
    }
}

fn block_line(text: impl Into<String>, style: Style) -> Line {
    Line {
        text: text.into(),
        style,
        block: true,
    }
}

/// What drawing an item needs beyond the flow.
pub struct Ctx<'a> {
    pub flow: &'a Flow,
    pub tags: &'a [Item],
    pub color: bool,
}

/// The mark a tagged item shows before its name.
pub const TAG: &str = "◆ ";

/// The color a rule of `kind` is drawn in when it is in play.
fn rule_color(kind: Kind) -> super::color::Color {
    match kind {
        Kind::Display => palette::DISPLAY_RULES,
        Kind::Input => palette::INPUT_RULES,
    }
}

/// An item's rows in a column `w` wide.
pub fn lines(ctx: &Ctx<'_>, item: Item, w: usize) -> Vec<Line> {
    let flow = ctx.flow;
    let tag = if ctx.tags.contains(&item) { TAG } else { "" };
    // a block's text sits one cell in from its left edge; without color
    // its rows are bracketed, and the brackets take a cell on each side
    let inner = w.saturating_sub(if ctx.color { 1 } else { 2 });
    let named = |name: &str| {
        format!(
            "{tag}{}",
            mid(name, inner.saturating_sub(tag.chars().count()))
        )
    };
    let caption = |options: &[&str]| pick(inner, options);
    match item {
        Item::Base => Vec::new(),
        Item::Display(d) | Item::Input(d) => {
            let (devices, color) = match item {
                Item::Display(_) => (&flow.displays, palette::DISPLAYS),
                _ => (&flow.inputs, palette::INPUTS),
            };
            let Some(dev) = devices.get(d) else {
                return Vec::new();
            };
            if !dev.found {
                let style = Style::block(palette::GREY);
                let mut out = vec![block_line(named(&format!("* {}", dev.name)), style)];
                // the words on two rows where each word fits, else the short
                // form
                let fits = words::NOT_FOUND
                    .split(' ')
                    .all(|w| w.chars().count() <= inner);
                if fits {
                    out.extend(
                        wrap(words::NOT_FOUND, inner, 2)
                            .into_iter()
                            .map(|l| block_line(l, style)),
                    );
                } else {
                    out.push(block_line(pick(inner, &[words::NOT_FOUND_SHORT]), style));
                }
                return out;
            }
            let style = Style::block(color);
            let mut out = vec![block_line(named(&dev.name), style)];
            if !dev.detail.is_empty() {
                out.push(block_line(mid(&dev.detail, inner), style));
            }
            if dev.rule.is_none() {
                out.push(block_line(
                    caption(&[words::BY_POLICY, words::BY_POLICY_SHORT]),
                    style,
                ));
            }
            out
        }
        Item::Rule(r) => rule_lines(ctx, r, w, tag),
        Item::Head(h) => {
            let Some(head) = flow.heads.get(h) else {
                return Vec::new();
            };
            let style = Style {
                bold: true,
                ..Style::block(palette::HEADS)
            };
            let plain = Style::block(palette::HEADS);
            let mut out = vec![
                block_line(named(&head.name), style),
                block_line(caption(&[&words::consoles(head.consoles)]), plain),
            ];
            if flow.active.as_deref() == Some(head.name.as_str()) {
                out.push(block_line(
                    caption(&[words::ACTIVE_HEAD, words::ACTIVE]),
                    plain,
                ));
            }
            out
        }
        Item::HeadLook(h) => {
            let Some(head) = flow.heads.get(h) else {
                return Vec::new();
            };
            match flow.look_of(h) {
                Some(l) => look_lines(ctx, l, inner, tag, false),
                None => {
                    let style = Style::block(palette::LOOKS);
                    let mut out = vec![block_line(
                        named(&format!("{} {}", words::LOOK, head.look)),
                        style,
                    )];
                    if head.look != words::DEFAULT {
                        out.push(block_line(
                            caption(&[words::NO_SUCH_LOOK, words::NO_SUCH_LOOK_SHORT]),
                            Style::block(palette::ERROR),
                        ));
                    }
                    out
                }
            }
        }
        Item::Look(l) => look_lines(ctx, l, inner, tag, true),
        Item::Special(s) => {
            let what = match s {
                Special::Roaming => words::roaming(flow.active.as_deref()),
                Special::Ignore => words::IGNORE_DOES.to_owned(),
                Special::Off => words::OFF_DOES.to_owned(),
            };
            vec![block_line(
                format!("{tag}{:<9} {what}", s.name()),
                Style::block(palette::GREY),
            )]
        }
        Item::Policy => vec![block_line(
            format!(
                "{tag}{:<9} {}",
                words::policy(&flow.policy),
                words::POLICY_DOES
            ),
            Style::block(palette::GREY),
        )],
        Item::New(n) => {
            let (what, color) = match n {
                New::DisplayRule => (words::NEW_RULE, palette::DISPLAY_RULES),
                New::InputRule => (words::NEW_RULE, palette::INPUT_RULES),
                New::Head => (words::NEW_HEAD, palette::HEADS),
                New::Look => (words::NEW_LOOK, palette::LOOKS),
            };
            vec![text_line(
                pick(w, &[what, words::NEW_SHORT]),
                Style::text(color),
            )]
        }
    }
}

/// A look's rows: its name and size, its frit, and each font the frit
/// draws with, as a small tree; a font this machine lacks, and what the
/// check finds wrong with the look, in the error's color.
fn look_lines(ctx: &Ctx<'_>, l: usize, inner: usize, tag: &str, unused: bool) -> Vec<Line> {
    let Some(look) = ctx.flow.looks.get(l) else {
        return Vec::new();
    };
    let style = Style::block(palette::LOOKS);
    let error = Style::block(palette::ERROR);
    let size = words::look_size(look.points);
    let long = format!("{tag}{} {}, {size}", words::LOOK, look.name);
    let short = format!("{tag}{}, {size}", look.name);
    let mut out = vec![block_line(pick(inner, &[&long, &short]), style)];
    if !look.frit.is_empty() {
        out.push(block_line(
            pick(
                inner,
                &[
                    &format!("└ frit {}", look.frit),
                    &format!("└ {}", look.frit),
                ],
            ),
            style,
        ));
    }
    for (font, here) in &look.fonts {
        if *here {
            out.push(block_line(mid(&format!("  └ {font}"), inner), style));
        } else {
            out.push(block_line(
                mid(&format!("! {font} {}", words::MISSING), inner),
                error,
            ));
        }
    }
    if let Some(problem) = &look.problem {
        out.extend(
            wrap(&format!("! {problem}"), inner, 2)
                .into_iter()
                .map(|l| block_line(l, error)),
        );
    }
    if unused {
        out.extend(
            wrap(words::NO_HEAD_USES, inner, 2)
                .into_iter()
                .map(|l| block_line(l, style)),
        );
    }
    out
}

/// A rule's rows: its number, selector and target, on two rows where one
/// is too narrow, then what it does wrong in words.
fn rule_lines(ctx: &Ctx<'_>, r: usize, w: usize, tag: &str) -> Vec<Line> {
    let flow = ctx.flow;
    let Some(rule) = flow.rules.get(r) else {
        return Vec::new();
    };
    let n = r.saturating_add(1).to_string();
    let broken = flow.target(rule) == Target::Missing;
    let style = match (&rule.state, broken) {
        (RuleState::Error(_), _) | (_, true) => Style::text(palette::ERROR),
        (RuleState::Matches, false) => Style::text(rule_color(rule.kind)),
        _ => Style::text(palette::GREY),
    };
    let mark = if broken || matches!(rule.state, RuleState::Error(_)) {
        "! "
    } else {
        ""
    };
    let lead = format!("{tag}{mark}{n} ");
    let whole = format!("{lead}{} -> {}", rule.selector, rule.target);
    let mut out = if whole.chars().count() <= w {
        vec![text_line(whole, style)]
    } else {
        let room = w.saturating_sub(lead.chars().count());
        let indent = " ".repeat(lead.chars().count().min(w.saturating_sub(4)));
        vec![
            text_line(format!("{lead}{}", mid(&rule.selector, room)), style),
            text_line(
                pick(
                    w,
                    &[
                        &format!("{indent}-> {}", rule.target),
                        &format!("-> {}", rule.target),
                    ],
                ),
                style,
            ),
        ]
    };
    let note = if broken {
        Some((words::NO_SUCH_HEAD.to_owned(), Style::text(palette::ERROR)))
    } else {
        match &rule.state {
            RuleState::Matches => None,
            RuleState::MatchesNothing if general(&rule.selector) => Some((
                match rule.kind {
                    Kind::Display => words::MATCHES_NO_DISPLAY,
                    Kind::Input => words::MATCHES_NO_DEVICE,
                }
                .to_owned(),
                Style::text(palette::GREY),
            )),
            RuleState::MatchesNothing => None,
            RuleState::Shadowed(by) => Some((
                words::shadowed(by.saturating_add(1)),
                Style::text(palette::GREY),
            )),
            RuleState::NeedsUdev => {
                Some((words::NEEDS_UDEV.to_owned(), Style::text(palette::GREY)))
            }
            RuleState::Error(e) => Some((e.clone(), Style::text(palette::ERROR))),
        }
    };
    if let Some((note, style)) = note {
        out.extend(
            wrap(&note, w.saturating_sub(2), 2)
                .into_iter()
                .map(|l| text_line(format!("  {l}"), style)),
        );
    }
    out
}

/// An item placed in its column: its rows, from the top of its unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub item: Item,
    pub column: usize,
    /// The first column it covers and how many cells wide it is: the
    /// targets that are not heads span the heads and the look.
    pub x: usize,
    pub w: usize,
    /// Its first row in its unit, and its rows.
    pub row: usize,
    pub lines: Vec<Line>,
}

/// The five units that scroll: the displays, the display rules, the heads
/// with their looks, the input rules, and the input devices.
pub const UNITS: usize = 5;

/// The unit a column scrolls with.
pub fn unit_of(column: usize) -> usize {
    match column {
        0 | 1 => column,
        2 | 3 => 2,
        4 => 3,
        _ => 4,
    }
}

/// Every item of the flow but the base card, placed in its unit.
pub fn stack(ctx: &Ctx<'_>, g: &Geometry) -> Vec<Placed> {
    let flow = ctx.flow;
    let [_, drules, heads, looks, irules, inputs] = flow.columns();
    let width = |c: usize| g.w.get(c).copied().unwrap_or(0);
    let left = |c: usize| g.x.get(c).copied().unwrap_or(0);
    let mut out = Vec::new();
    // a column of blocks keeps a blank row between them, so each reads as
    // one shape; a column of rules stacks them
    let column = |items: &[Item], c: usize, gap: usize, out: &mut Vec<Placed>| {
        let mut row = 0usize;
        for item in items {
            let lines = lines(ctx, *item, width(c));
            let h = lines.len();
            out.push(Placed {
                item: *item,
                column: c,
                x: left(c),
                w: width(c),
                row,
                lines,
            });
            row = row.saturating_add(h).saturating_add(gap);
        }
    };
    let displays: Vec<Item> = (0..flow.displays.len()).map(Item::Display).collect();
    column(&displays, 0, 1, &mut out);
    column(&drules, 1, 0, &mut out);
    column(&irules, 4, 0, &mut out);
    column(&inputs, 5, 1, &mut out);
    // the heads beside their looks, then the looks no head uses, the new
    // items, and the targets that are not heads across both columns
    let mut row = 0usize;
    let place = |item: Item, c: usize, x: usize, w: usize, row: usize, out: &mut Vec<Placed>| {
        let lines = lines(ctx, item, w);
        let h = lines.len();
        out.push(Placed {
            item,
            column: c,
            x,
            w,
            row,
            lines,
        });
        h
    };
    let head_items: Vec<Item> = heads
        .iter()
        .copied()
        .filter(|i| matches!(i, Item::Head(_)))
        .collect();
    for item in &head_items {
        let Item::Head(h) = item else { continue };
        let a = place(*item, 2, left(2), width(2), row, &mut out);
        let b = place(Item::HeadLook(*h), 3, left(3), width(3), row, &mut out);
        row = row.saturating_add(a.max(b)).saturating_add(1);
    }
    for item in looks.iter().filter(|i| matches!(i, Item::Look(_))) {
        let h = place(*item, 3, left(3), width(3), row, &mut out);
        row = row.saturating_add(h).saturating_add(1);
    }
    let a = place(Item::New(New::Head), 2, left(2), width(2), row, &mut out);
    let b = place(Item::New(New::Look), 3, left(3), width(3), row, &mut out);
    row = row.saturating_add(a.max(b)).saturating_add(1);
    let across = left(3).saturating_add(width(3)).saturating_sub(left(2));
    for item in heads
        .iter()
        .filter(|i| matches!(i, Item::Special(_) | Item::Policy))
    {
        let h = place(*item, 2, left(2), across, row, &mut out);
        row = row.saturating_add(h);
    }
    out
}

/// Each unit's rows for items in `height` rows, and how far it scrolls: a
/// unit taller than the room gives its last row to the count of what is out
/// of sight, and scrolls only as far as showing `selected` needs.
fn scroll(placed: &[Placed], selected: Item, height: usize) -> [(usize, usize); UNITS] {
    let mut out = [(height, 0usize); UNITS];
    for (unit, slot) in out.iter_mut().enumerate() {
        let total = placed
            .iter()
            .filter(|p| unit_of(p.column) == unit)
            .map(|p| p.row.saturating_add(p.lines.len()))
            .max()
            .unwrap_or(0);
        if total <= height {
            continue;
        }
        let room = height.saturating_sub(1).max(1);
        let off = placed
            .iter()
            .find(|p| p.item == selected && unit_of(p.column) == unit)
            .map_or(0, |sel| {
                let end = sel.row.saturating_add(sel.lines.len());
                end.saturating_sub(room).min(sel.row)
            });
        *slot = (room, off);
    }
    out
}

/// The flow's columns as drawn on the screen.
pub struct Drawn {
    /// Each item shown, with its first screen row.
    pub shown: Vec<(Placed, usize)>,
}

/// Draws the column headings on `heading` and the columns from `top` for
/// `height` rows, with `selected` in reverse video; returns where each item
/// went.
pub fn draw_columns(
    canvas: &mut Canvas,
    ctx: &Ctx<'_>,
    selected: Item,
    heading: usize,
    top: usize,
    height: usize,
) -> Drawn {
    let g = geometry(canvas.cols);
    for (c, (names, color)) in HEADINGS.iter().enumerate() {
        let w = g.w.get(c).copied().unwrap_or(0);
        let style = Style {
            bold: true,
            ..Style::text(*color)
        };
        canvas.put(
            g.x.get(c).copied().unwrap_or(0),
            heading,
            &pick(w, names),
            style,
        );
    }
    let placed = stack(ctx, &g);
    let units = scroll(&placed, selected, height);
    let mut shown = Vec::new();
    let mut hidden = [(0usize, 0usize); UNITS];
    for p in placed {
        let unit = unit_of(p.column);
        let (room, off) = units.get(unit).copied().unwrap_or((height, 0));
        let start = p.row;
        let end = p.row.saturating_add(p.lines.len());
        if start < off || end.saturating_sub(off) > room {
            if let Some(h) = hidden.get_mut(unit) {
                if start < off {
                    h.0 = h.0.saturating_add(1);
                } else {
                    h.1 = h.1.saturating_add(1);
                }
            }
            continue;
        }
        let y = top.saturating_add(start).saturating_sub(off);
        for (i, line) in p.lines.iter().enumerate() {
            let style = if p.item == selected {
                line.style.reversed()
            } else {
                line.style
            };
            let text = if line.block && !ctx.color {
                format!("[{:<width$}]", line.text, width = p.w.saturating_sub(2))
            } else if line.block {
                format!(" {}", line.text)
            } else {
                line.text.clone()
            };
            let row = y.saturating_add(i);
            if line.block {
                canvas.fill(p.x, row, p.w, &text, style);
            } else {
                canvas.put(p.x, row, &text, style);
            }
        }
        shown.push((p, y));
    }
    // a unit with items out of sight says how many, on its last row
    for (unit, (above, below)) in hidden.iter().enumerate() {
        if *above == 0 && *below == 0 {
            continue;
        }
        let cols: Vec<usize> = (0..6).filter(|c| unit_of(*c) == unit).collect();
        let (Some(first), Some(last)) = (cols.first(), cols.last()) else {
            continue;
        };
        let x = g.x.get(*first).copied().unwrap_or(0);
        let w =
            g.x.get(*last)
                .copied()
                .unwrap_or(0)
                .saturating_add(g.w.get(*last).copied().unwrap_or(0))
                .saturating_sub(x);
        let row = top.saturating_add(height.saturating_sub(1));
        canvas.fill(
            x,
            row,
            w,
            &words::out_of_sight(*above, *below),
            Style::text(palette::GREY),
        );
    }
    let drawn = Drawn { shown };
    draw_lines(canvas, ctx, &g, &drawn);
    drawn
}

/// The column headings, the long form first.
const HEADINGS: [(&[&str], super::color::Color); 6] = [
    (&["DISPLAYS"], palette::DISPLAYS),
    (&["DISPLAY RULES", "DISP. RULES"], palette::DISPLAY_RULES),
    (&["HEADS"], palette::HEADS),
    (&["LOOK"], palette::LOOKS),
    (&["INPUT RULES"], palette::INPUT_RULES),
    (&["INPUT DEVICES", "DEVICES"], palette::INPUTS),
];

/// A line between two items: from the source's first row to the target's,
/// in a gap, solid or dashed.
struct Wire {
    gap: (usize, usize),
    /// The source's rows, any of which its line may leave from.
    from: Vec<usize>,
    to: usize,
    /// The target is on the left: an input's line runs toward the heads.
    leftward: bool,
    dashed: bool,
    style: Style,
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// The box-drawing character for the directions in `bits`.
fn joint(bits: u8, dashed: bool) -> char {
    match bits {
        b if b == UP | DOWN => {
            if dashed {
                '╎'
            } else {
                '│'
            }
        }
        b if b == LEFT | RIGHT => {
            if dashed {
                '╌'
            } else {
                '─'
            }
        }
        b if b == DOWN | RIGHT => '┌',
        b if b == DOWN | LEFT => '┐',
        b if b == UP | RIGHT => '└',
        b if b == UP | LEFT => '┘',
        b if b == UP | DOWN | RIGHT => '├',
        b if b == UP | DOWN | LEFT => '┤',
        b if b == LEFT | RIGHT | DOWN => '┬',
        b if b == LEFT | RIGHT | UP => '┴',
        b if b == UP | DOWN | LEFT | RIGHT => '┼',
        b if b & (UP | DOWN) != 0 => '│',
        _ => '─',
    }
}

/// The lines between the items shown: a device to its rule, a rule to its
/// target. A line from a device that is not found, or from a rule that
/// matches nothing present, is grey and dashed.
///
/// The lines of one gap share its first cell, where they run up and down.
/// A line leaves its source from the source's first row on which no line
/// of the gap enters another target, so that where one line turns into its
/// target, no other line starts beside it and seems to go there too.
fn draw_lines(canvas: &mut Canvas, ctx: &Ctx<'_>, g: &Geometry, drawn: &Drawn) {
    let flow = ctx.flow;
    let rows_of = |item: Item| -> Option<Vec<usize>> {
        drawn
            .shown
            .iter()
            .find(|(p, _)| p.item == item)
            .map(|(p, y)| (*y..y.saturating_add(p.lines.len().max(1))).collect())
    };
    let row_of = |item: Item| rows_of(item).and_then(|r| r.first().copied());
    let gap = |left: usize| {
        let x =
            g.x.get(left)
                .copied()
                .unwrap_or(0)
                .saturating_add(g.w.get(left).copied().unwrap_or(0));
        (x, g.x.get(left.saturating_add(1)).copied().unwrap_or(x))
    };
    let mut wires = Vec::new();
    let mut device_wires = |kind: Kind| {
        for (d, dev) in flow.devices(kind).iter().enumerate() {
            let Some(r) = dev.rule else { continue };
            let (item, g_at, color) = match kind {
                Kind::Display => (Item::Display(d), 0, palette::DISPLAYS),
                Kind::Input => (Item::Input(d), 4, palette::INPUTS),
            };
            if let (Some(from), Some(to)) = (rows_of(item), row_of(Item::Rule(r))) {
                wires.push(Wire {
                    gap: gap(g_at),
                    from,
                    to,
                    leftward: kind == Kind::Input,
                    dashed: !dev.found,
                    style: Style::text(if dev.found { color } else { palette::GREY }),
                });
            }
        }
    };
    device_wires(Kind::Display);
    device_wires(Kind::Input);
    for (r, rule) in flow.rules.iter().enumerate() {
        let target = match (flow.target(rule), rule.kind) {
            (Target::Head(h), Kind::Display) => Item::Head(h),
            (Target::Head(h), Kind::Input) => Item::HeadLook(h),
            (Target::Special(s), _) => Item::Special(s),
            (Target::Missing, _) => continue,
        };
        let (Some(from), Some(to)) = (rows_of(Item::Rule(r)), row_of(target)) else {
            continue;
        };
        let live = rule.state == RuleState::Matches;
        wires.push(Wire {
            gap: gap(if rule.kind == Kind::Display { 1 } else { 3 }),
            from,
            to,
            leftward: rule.kind == Kind::Input,
            dashed: !live,
            style: Style::text(if live {
                rule_color(rule.kind)
            } else {
                palette::GREY
            }),
        });
    }
    // each line leaves from the first of its source's rows that no line of
    // its gap ends on, but one to its own target
    let ends: Vec<((usize, usize), usize)> = wires.iter().map(|w| (w.gap, w.to)).collect();
    let starts: Vec<usize> = wires
        .iter()
        .enumerate()
        .map(|(i, w)| {
            // a row where another line ends on another target; lines to
            // the same target end on the same row and read as one
            let taken = |row: &usize| {
                *row != w.to
                    && ends
                        .iter()
                        .enumerate()
                        .any(|(j, (gap, to))| j != i && *gap == w.gap && to == row)
            };
            w.from
                .iter()
                .find(|r| !taken(r))
                .or_else(|| w.from.first())
                .copied()
                .unwrap_or(w.to)
        })
        .collect();
    let mut wires: Vec<(usize, Wire)> = starts.into_iter().zip(wires).collect();
    // the solid lines are drawn last, so where lines share a cell the line
    // in play shows its color
    wires.sort_by_key(|(_, w)| !w.dashed);
    let mut bits: BTreeMap<(usize, usize), u8> = BTreeMap::new();
    for (from, w) in &wires {
        let from = *from;
        let (start, end) = w.gap;
        if end <= start {
            continue;
        }
        let last = end.saturating_sub(1);
        let (bus, head) = if w.leftward {
            (last, start)
        } else {
            (start, last)
        };
        let (into_bus, out_of_bus) = if w.leftward {
            (RIGHT, LEFT)
        } else {
            (LEFT, RIGHT)
        };
        let mut add = |x: usize, y: usize, b: u8, canvas: &mut Canvas| {
            let e = bits.entry((x, y)).or_insert(0);
            *e |= b;
            canvas.put(x, y, &joint(*e, w.dashed).to_string(), w.style);
        };
        let (lo, hi) = (from.min(w.to), from.max(w.to));
        let toward = if w.to > from { DOWN } else { UP };
        let back = if w.to > from { UP } else { DOWN };
        if from == w.to {
            add(bus, from, into_bus | out_of_bus, canvas);
        } else {
            add(bus, from, into_bus | toward, canvas);
            for y in lo.saturating_add(1)..hi {
                add(bus, y, UP | DOWN, canvas);
            }
            add(bus, w.to, back | out_of_bus, canvas);
        }
        let (a, b) = (bus.min(head), bus.max(head));
        for x in a.saturating_add(1)..b {
            add(x, w.to, LEFT | RIGHT, canvas);
        }
        if head != bus {
            canvas.put(head, w.to, if w.leftward { "◄" } else { "►" }, w.style);
        }
    }
}

/// The item at cell (`col`, `row`), as a click finds it.
pub fn hit(drawn: &Drawn, col: usize, row: usize) -> Option<Item> {
    drawn.shown.iter().find_map(|(p, y)| {
        let rows = *y..y.saturating_add(p.lines.len());
        let cols = p.x..p.x.saturating_add(p.w);
        (rows.contains(&row) && cols.contains(&col)).then_some(p.item)
    })
}

/// One line of the route card: its text, the section color it is drawn in,
/// and the item it names, which Enter opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteLine {
    pub text: String,
    pub style: Style,
    pub item: Option<Item>,
}

/// The route card of `item`: a sentence of what it does, then each route
/// it is part of, top to bottom.
pub fn route_card(flow: &Flow, item: Item) -> (String, String, Vec<RouteLine>) {
    let title = words::route_title(describe_name(flow, item));
    let sentence = words::route_sentence(flow, item);
    let routes = flow.routes(item);
    let mut lines = Vec::new();
    let n = routes.len();
    for (k, route) in routes.iter().enumerate() {
        let when = (Some(
            flow.heads
                .get(route.head)
                .map(|h| h.name.as_str())
                .unwrap_or_default(),
        ) != flow.active.as_deref())
        .then(|| {
            flow.heads
                .get(route.head)
                .map(|h| h.name.clone())
                .unwrap_or_default()
        });
        lines.push(RouteLine {
            text: words::route_of(k.saturating_add(1), n, when.as_deref()),
            style: Style::text(palette::GREY),
            item: None,
        });
        for it in route.items(flow) {
            lines.push(route_line(flow, it, route));
        }
    }
    (title, sentence, lines)
}

/// An item's name with what tells it apart, for the route card's title.
fn describe_name(flow: &Flow, item: Item) -> String {
    match item {
        Item::Display(d) | Item::Input(d) => {
            let devices = if matches!(item, Item::Display(_)) {
                &flow.displays
            } else {
                &flow.inputs
            };
            devices.get(d).map_or_else(String::new, |d| {
                if d.detail.is_empty() {
                    d.name.clone()
                } else {
                    format!("{}, {}", d.name, d.detail)
                }
            })
        }
        _ => flow.name(item),
    }
}

/// One item's line on the route card, in its section's color.
fn route_line(flow: &Flow, item: Item, _route: &Route) -> RouteLine {
    let (section, text, color) = match item {
        Item::Display(d) => (
            words::SECTION_DISPLAY,
            describe_name(flow, item)
                + if flow.displays.get(d).is_some_and(|d| !d.found) {
                    ", configured, not found"
                } else {
                    ""
                },
            palette::DISPLAYS,
        ),
        Item::Input(d) => (
            words::SECTION_INPUT,
            describe_name(flow, item)
                + if flow.inputs.get(d).is_some_and(|d| !d.found) {
                    ", configured, not found"
                } else {
                    ""
                },
            palette::INPUTS,
        ),
        Item::Rule(r) => {
            let rule = flow.rules.get(r);
            let kind = rule.map_or(Kind::Display, |r| r.kind);
            (
                match kind {
                    Kind::Display => words::SECTION_DISPLAY_RULE,
                    Kind::Input => words::SECTION_INPUT_RULE,
                },
                rule.map_or_else(String::new, |rule| {
                    format!(
                        "{}, {} -> {}",
                        r.saturating_add(1),
                        rule.selector,
                        rule.target
                    )
                }),
                rule_color(kind),
            )
        }
        Item::Head(h) => (
            words::SECTION_HEAD,
            flow.heads.get(h).map_or_else(String::new, |head| {
                if flow.active.as_deref() == Some(head.name.as_str()) {
                    format!("{}, {}", head.name, words::ACTIVE_HEAD)
                } else {
                    head.name.clone()
                }
            }),
            palette::HEADS,
        ),
        Item::HeadLook(_) | Item::Look(_) => {
            let look = match item {
                Item::HeadLook(h) => flow.look_of(h),
                Item::Look(l) => Some(l),
                _ => None,
            };
            (
                words::SECTION_LOOK,
                look.and_then(|l| flow.looks.get(l)).map_or_else(
                    || flow.name(item),
                    |l| {
                        format!(
                            "{}, frit {}, {}",
                            l.name,
                            l.frit,
                            words::look_size(l.points)
                        )
                    },
                ),
                palette::LOOKS,
            )
        }
        _ => ("", flow.name(item), palette::GREY),
    };
    RouteLine {
        text: format!("{section:<14} {text}"),
        style: Style::text(color),
        item: Some(item),
    }
}

/// The card Enter opens below the columns: rows of spans, each a text and
/// its style.
pub fn card(flow: &Flow, item: Item) -> Vec<Vec<(String, Style)>> {
    let grey = Style::text(palette::GREY);
    let chosen = |s: &str, color| (format!(" {s} "), Style::block(color));
    let choice = |s: &str| (format!(" {s} "), Style::PLAIN);
    let choices = |label: &str, all: &[String], current: &str, color, typed: bool| {
        let mut row = vec![(format!("{label:<10}"), grey)];
        for c in all {
            row.push(if c == current {
                chosen(c, color)
            } else {
                choice(c)
            });
        }
        if typed {
            row.push((words::OR_TYPE.to_owned(), grey));
        }
        row
    };
    let line = |text: String, style: Style| vec![(text, style)];
    match item {
        Item::Rule(r) => {
            let Some(rule) = flow.rules.get(r) else {
                return Vec::new();
            };
            let color = rule_color(rule.kind);
            let kind = match rule.kind {
                Kind::Display => "display",
                Kind::Input => "input",
            };
            let mut targets: Vec<String> = flow.heads.iter().map(|h| h.name.clone()).collect();
            targets.extend(
                Special::ALL
                    .iter()
                    .filter(|s| s.kind() == rule.kind)
                    .map(|s| s.name().to_owned()),
            );
            if !targets.contains(&rule.target) {
                targets.push(rule.target.clone());
            }
            let mut selectors = vec![rule.selector.clone()];
            for dev in flow
                .devices(rule.kind)
                .iter()
                .filter(|d| d.found && d.rule == Some(r))
            {
                selectors.extend(dev.keys.iter().filter(|k| **k != rule.selector).cloned());
            }
            selectors.dedup();
            vec![
                line(
                    words::rule_card(
                        r.saturating_add(1),
                        rule.line,
                        kind,
                        &rule.selector,
                        &rule.target,
                    ),
                    Style {
                        bold: true,
                        ..Style::text(color)
                    },
                ),
                choices(words::SENDS_TO, &targets, &rule.target, color, false),
                choices(words::MATCHES, &selectors, &rule.selector, color, true),
            ]
        }
        Item::Display(d) | Item::Input(d) => {
            let (devices, color) = if matches!(item, Item::Display(_)) {
                (&flow.displays, palette::DISPLAYS)
            } else {
                (&flow.inputs, palette::INPUTS)
            };
            let Some(dev) = devices.get(d) else {
                return Vec::new();
            };
            let placed = match dev
                .rule
                .and_then(|r| flow.rules.get(r).map(|rule| (r, rule)))
            {
                Some((r, rule)) if dev.found => {
                    words::placed_by(r.saturating_add(1), &rule.selector, &rule.target)
                }
                Some((r, _)) => words::named_by(r.saturating_add(1)),
                None => words::placed_by_policy(&flow.policy),
            };
            vec![
                line(
                    describe_name(flow, item),
                    Style {
                        bold: true,
                        ..Style::text(color)
                    },
                ),
                line(words::match_keys(&dev.keys), Style::PLAIN),
                line(placed, grey),
            ]
        }
        Item::Head(h) => {
            let Some(head) = flow.heads.get(h) else {
                return Vec::new();
            };
            let mut looks: Vec<String> = flow.looks.iter().map(|l| l.name.clone()).collect();
            looks.push(words::DEFAULT.to_owned());
            let keys = ["consoles", "login", "when-display-lost", "isolated"]
                .iter()
                .map(|k| {
                    let v = if *k == "consoles" {
                        Some(head.consoles.to_string())
                    } else {
                        head.keys
                            .iter()
                            .find(|(n, _)| n == k)
                            .map(|(_, v)| v.clone())
                    };
                    format!("{k} {}", v.unwrap_or_else(|| words::AT_DEFAULT.to_owned()))
                })
                .collect::<Vec<_>>()
                .join("  ");
            vec![
                line(
                    words::head_card(
                        &head.name,
                        head.consoles,
                        flow.active.as_deref() == Some(head.name.as_str()),
                    ),
                    Style {
                        bold: true,
                        ..Style::text(palette::HEADS)
                    },
                ),
                choices(words::LOOK, &looks, &head.look, palette::LOOKS, false),
                line(keys, grey),
            ]
        }
        Item::HeadLook(_) | Item::Look(_) => {
            let l = match item {
                Item::HeadLook(h) => flow.look_of(h),
                Item::Look(l) => Some(l),
                _ => None,
            };
            let Some(look) = l.and_then(|l| flow.looks.get(l)) else {
                return vec![line(flow.name(item), Style::text(palette::LOOKS))];
            };
            let users: Vec<&str> = flow
                .heads
                .iter()
                .filter(|h| h.look == look.name)
                .map(|h| h.name.as_str())
                .collect();
            let fonts: Vec<&str> = look.fonts.iter().map(|(f, _)| f.as_str()).collect();
            vec![
                line(
                    words::look_card(&look.name, &look.frit, look.points),
                    Style {
                        bold: true,
                        ..Style::text(palette::LOOKS)
                    },
                ),
                line(words::look_fonts(&fonts), Style::PLAIN),
                line(words::look_users(&users), grey),
            ]
        }
        Item::Special(s) => vec![
            line(
                s.name().to_owned(),
                Style {
                    bold: true,
                    ..Style::PLAIN
                },
            ),
            line(
                match s {
                    Special::Roaming => words::roaming(flow.active.as_deref()),
                    Special::Ignore => words::IGNORE_DOES.to_owned(),
                    Special::Off => words::OFF_DOES.to_owned(),
                },
                Style::PLAIN,
            ),
        ],
        Item::Policy => vec![
            line(
                words::policy(&flow.policy),
                Style {
                    bold: true,
                    ..Style::PLAIN
                },
            ),
            line(words::POLICY_DOES.to_owned(), Style::PLAIN),
        ],
        Item::Base => {
            let set = flow
                .base
                .set
                .iter()
                .map(|(k, v)| {
                    if v.is_empty() {
                        k.clone()
                    } else {
                        format!("{k} {v}")
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            vec![
                line(
                    words::BASE_CARD.to_owned(),
                    Style {
                        bold: true,
                        ..Style::PLAIN
                    },
                ),
                line(set, Style::PLAIN),
                line(words::at_defaults(flow.base.defaults), grey),
            ]
        }
        Item::New(_) => vec![line(words::NEW_NOT_BUILT.to_owned(), grey)],
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::super::fixture;
    use super::*;

    #[test]
    fn the_columns_share_the_width_as_the_mockup_does() {
        let g = geometry(80);
        assert_eq!(g.w, [11, 13, 11, 11, 13, 12]);
        assert_eq!(g.x, [0, 13, 28, 40, 53, 68]);
        let g = geometry(100);
        assert_eq!(g.w, [13, 17, 11, 13, 17, 16]);
        assert_eq!(g.x[5].saturating_add(g.w[5]), 100);
    }

    fn ctx(flow: &Flow) -> Ctx<'_> {
        Ctx {
            flow,
            tags: &[],
            color: true,
        }
    }

    #[test]
    fn a_rule_too_long_for_its_column_takes_two_rows() {
        let f = fixture::flow();
        let l = lines(&ctx(&f), Item::Rule(0), 13);
        let texts: Vec<&str> = l.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["1 edid:…00001", "  -> desk"]);
        let l = lines(&ctx(&f), Item::Rule(0), 40);
        assert_eq!(l[0].text, "1 edid:AAA:0x0001:LARGE00001 -> desk");
    }

    #[test]
    fn each_state_has_its_words_and_its_color() {
        let mut f = fixture::flow();
        let grey = Style::text(palette::GREY);
        let l = lines(&ctx(&f), Item::Rule(2), 28);
        assert_eq!(l[0].style, grey);
        assert_eq!(l[1].text, format!("  {}", words::MATCHES_NO_DISPLAY));
        // a rule naming one device that is not found carries no words; its
        // device's block says it
        let l = lines(&ctx(&f), Item::Rule(3), 40);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].style, grey);
        let l = lines(&ctx(&f), Item::Input(0), 12);
        assert_eq!(l[0].text, "* 046…31c:0");
        assert!(l.iter().all(|l| l.style == Style::block(palette::GREY)));
        assert_eq!(
            l[1..]
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            "configured, not found"
        );
        // a rule that sends to a head that does not exist, in the error's
        // color, marked and said
        f.rules[0].target = "tv".into();
        let l = lines(&ctx(&f), Item::Rule(0), 40);
        assert_eq!(l[0].text, "! 1 edid:AAA:0x0001:LARGE00001 -> tv");
        assert_eq!(l[0].style, Style::text(palette::ERROR));
        assert_eq!(l[1].text, format!("  {}", words::NO_SUCH_HEAD));
        assert_eq!(l[1].style, Style::text(palette::ERROR));
    }

    #[test]
    fn a_tagged_item_shows_the_mark_before_its_name() {
        let f = fixture::flow();
        let tags = [Item::Input(1), Item::Rule(4)];
        let c = Ctx {
            flow: &f,
            tags: &tags,
            color: true,
        };
        assert_eq!(lines(&c, Item::Input(1), 12)[0].text, "◆ keyboard");
        assert!(
            lines(&c, Item::Rule(4), 40)[0]
                .text
                .starts_with("◆ 5 class:keyboard")
        );
        assert_eq!(lines(&c, Item::Input(2), 12)[0].text, "Power…utton");
    }

    fn drawn(f: &Flow, cols: usize, rows: usize, sel: Item) -> (Canvas, Drawn) {
        let mut canvas = Canvas::new(cols, rows);
        let d = draw_columns(&mut canvas, &ctx(f), sel, 0, 1, rows.saturating_sub(1));
        (canvas, d)
    }

    #[test]
    fn a_display_s_line_runs_to_its_rule_and_the_rule_s_to_its_head() {
        let f = fixture::flow();
        let (canvas, d) = drawn(&f, 80, 24, Item::Display(0));
        let g = geometry(80);
        let row = |i| {
            d.shown
                .iter()
                .find(|(p, _)| p.item == i)
                .map(|(_, y)| *y)
                .expect("shown")
        };
        // DP-8 and rule 1 share a row: a straight line into an arrow
        let y = row(Item::Display(0));
        assert_eq!(row(Item::Rule(0)), y);
        assert_eq!(canvas.at(g.x[0] + g.w[0], y), Some('─'));
        assert_eq!(canvas.at(g.x[1] - 1, y), Some('►'));
        // rule 3 matches nothing: its line up to desk is dashed where no
        // line in play shares its cells. Rule 2's line turns into panel on
        // rule 3's first row, so rule 3's line leaves from its next row,
        // and nothing joins rule 3's first row to panel's arrow
        let y3 = row(Item::Rule(2));
        let bus = g.x[1] + g.w[1];
        assert_eq!(row(Item::Head(1)), y3, "panel sits level with rule 3");
        assert_eq!(canvas.at(bus, y3), Some('├'), "no line from the left");
        assert_eq!(canvas.at(bus, y3 + 1), Some('┘'), "rule 3's own line");
        assert_eq!(canvas.at(bus, row(Item::Head(0)) + 1), Some('╎'));
        // rules 1 and 3 both go to desk, so rule 1 leaves from its first row
        assert_eq!(canvas.at(bus, row(Item::Head(0))), Some('┬'));
        // the keyboard's line runs left, into its rule
        let yk = row(Item::Input(1));
        let yr = row(Item::Rule(4));
        assert_eq!(canvas.at(g.x[4] + g.w[4], yr), Some('◄'));
        assert!(yk != yr || canvas.at(g.x[5] - 1, yk) == Some('─'));
    }

    #[test]
    fn a_click_finds_the_item_under_it() {
        let f = fixture::flow();
        let (_, d) = drawn(&f, 80, 24, Item::Display(0));
        let (p, y) = d
            .shown
            .iter()
            .find(|(p, _)| p.item == Item::Head(1))
            .expect("panel");
        assert_eq!(hit(&d, p.x + 1, y + 1), Some(Item::Head(1)));
        assert_eq!(hit(&d, 0, 0), None);
    }

    #[test]
    fn a_column_taller_than_the_room_scrolls_to_the_selection_and_counts_the_rest() {
        let mut f = fixture::flow();
        for i in 0..20 {
            f.rules.push(super::super::flow::Rule {
                kind: Kind::Input,
                line: 30 + i,
                selector: format!("name:extra{i}"),
                target: "desk".into(),
                state: RuleState::MatchesNothing,
            });
        }
        let last = f.rules.len() - 1;
        let (canvas, d) = drawn(&f, 80, 12, Item::Rule(last));
        assert!(d.shown.iter().any(|(p, _)| p.item == Item::Rule(last)));
        assert!(!d.shown.iter().any(|(p, _)| p.item == Item::Rule(3)));
        let g = geometry(80);
        let count: String = (g.x[4]..g.x[4] + g.w[4])
            .filter_map(|x| canvas.at(x, 11))
            .collect();
        assert!(count.starts_with('↑'), "{count}");
    }

    #[test]
    fn a_look_no_head_uses_says_so_in_d7_s_words() {
        let f = fixture::flow();
        let l = lines(&ctx(&f), Item::Look(2), 13);
        let said: Vec<&str> = l.iter().skip(2).map(|l| l.text.as_str()).collect();
        assert_eq!(said.join(" "), words::NO_HEAD_USES);
        assert!(said.iter().all(|s| s.chars().count() <= 12), "{said:?}");
    }

    #[test]
    fn the_route_card_says_what_is_wrong_with_a_rule_or_a_look() {
        use super::super::flow::RuleState;
        let sentence = |f: &Flow, item| route_card(f, item).1;
        let mut f = fixture::flow();
        f.rules[2].state = RuleState::Shadowed(0);
        assert_eq!(
            sentence(&f, Item::Rule(2)),
            "Rule 3 sends * to desk, but rule 1 comes first and matches everything it would, so it matches nothing."
        );
        f.rules[2].state = RuleState::NeedsUdev;
        assert!(
            sentence(&f, Item::Rule(2)).contains("needs udev, which this machine does not run")
        );
        f.rules[2].state = RuleState::Error("a selector key it does not know".into());
        assert!(
            sentence(&f, Item::Rule(2))
                .ends_with("the check finds an error in it: a selector key it does not know.")
        );
        f.looks[0].fonts[0].1 = false;
        assert_eq!(
            sentence(&f, Item::HeadLook(0)),
            "Look big draws desk, but this machine lacks Monoid, which its frit monoid names."
        );
        f.looks[0].problem = Some("frit monoid does not exist".into());
        assert!(
            sentence(&f, Item::HeadLook(0)).ends_with("problem in it: frit monoid does not exist.")
        );
        f.heads[1].look = "huge".into();
        assert_eq!(
            sentence(&f, Item::HeadLook(1)),
            "Head panel names look huge, which the file does not have, so it draws with the default look."
        );
        f.looks[2].problem = Some("frit minspec-16 does not read".into());
        assert!(
            sentence(&f, Item::Look(2)).starts_with("Look tiny is not used: no head uses it, but")
        );
    }

    #[test]
    fn the_route_card_lists_each_route_with_its_items_top_to_bottom() {
        let f = fixture::flow();
        let (title, sentence, lines) = route_card(&f, Item::Input(1));
        assert_eq!(title, "Route card: keyboard, usb-0:3.3");
        assert_eq!(
            sentence,
            "It types into desk, the active head, by roaming; into panel when panel is active."
        );
        let items: Vec<Option<Item>> = lines.iter().map(|l| l.item).collect();
        assert_eq!(items[0], None);
        assert_eq!(
            &items[1..7],
            [
                Some(Item::Display(0)),
                Some(Item::Rule(0)),
                Some(Item::Head(0)),
                Some(Item::HeadLook(0)),
                Some(Item::Rule(4)),
                Some(Item::Input(1)),
            ]
        );
        assert_eq!(lines[7].text, "route 2 of 2, when panel is active");
        let (_, sentence, lines) = route_card(&f, Item::Look(2));
        assert_eq!(sentence, words::LOOK_NOT_USED);
        assert!(lines.is_empty());
    }
}
