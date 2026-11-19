//! The Help tab: the Help page, which says what each tab does and lists
//! its keys and the commands, and the About page. Left and Right switch
//! between the two pages, and the moving keys scroll the page.
//!
//! References:
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.
//! - ADR-1206 D4: each tab's layout belongs to the tab, as a function of
//!   the tab's state, the shared part and the size.

use ratatui_core::text::{Line, Span};

use super::super::canvas::{printable, width, wrap};
use super::super::color::{Style, palette, sgr_cells};
use super::super::contract::{NODES, Node};
use super::super::input::Key;
use super::super::model::{Shared, Size};
use super::super::words;
use super::Command;

/// The widest a key column of the Help page grows; keys wider than this
/// push their meaning onto the rows below.
const HELP_KEYS_MAX: usize = 16;

/// A line of `text` in `style`.
fn line(text: impl AsRef<str>, style: Style) -> Line<'static> {
    Line::from(span(text, style))
}

/// A span of `text` in `style`. The Help page has no 24-bit color, so its
/// cells hold the same colors whichever way the view reads them.
fn span(text: impl AsRef<str>, style: Style) -> Span<'static> {
    Span::styled(printable(text.as_ref()), style.cell())
}

/// `text` as lines of the page, wrapped by words to the screen's width.
fn prose(text: &str, cols: usize) -> Vec<Line<'static>> {
    wrap(text, cols.saturating_sub(2), usize::MAX)
        .into_iter()
        .map(|l| line(format!(" {l}"), Style::PLAIN))
        .collect()
}

/// A line the banner styled itself with SGR, as spans in the styles its
/// sequences set.
fn styled(text: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (c, style) in sgr_cells(text) {
        let c = if c.is_control() { '?' } else { c };
        match spans.last_mut() {
            Some(s) if s.style == style => s.content.to_mut().push(c),
            _ => spans.push(Span::styled(c.to_string(), style)),
        }
    }
    Line::from(spans)
}

/// One node's section of the Help page, from the contract: what the node
/// is, in bold, then each key that acts there and what it does, the
/// meaning wrapped under its own column.
fn section(node: &Node, cols: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if node.help.is_empty() {
        return lines;
    }
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    for l in wrap(node.what, cols.saturating_sub(2), usize::MAX) {
        lines.push(line(format!(" {l}"), bold));
    }
    let keys_width = node
        .help
        .iter()
        .map(|(keys, _)| keys.chars().count())
        .filter(|w| *w <= HELP_KEYS_MAX)
        .max()
        .unwrap_or(0);
    let indent = keys_width.saturating_add(5);
    for (keys, does) in node.help {
        let mut does = wrap(does, cols.saturating_sub(indent).max(1), usize::MAX).into_iter();
        if keys.chars().count() > keys_width {
            lines.push(line(format!("   {keys}"), Style::PLAIN));
        } else {
            let first = does.next().unwrap_or_default();
            lines.push(Line::from(vec![
                span(
                    format!("   {keys:<keys_width$}  "),
                    Style::text(palette::GREY),
                ),
                span(first, Style::PLAIN),
            ]));
        }
        for l in does {
            lines.push(line(format!("{:indent$}{l}", ""), Style::PLAIN));
        }
    }
    lines.push(Line::default());
    lines
}

/// Where the Help tab stands: the page it shows, and the page's first row
/// in sight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Help {
    about: bool,
    top: usize,
}

/// How long the page is, and how many of its rows the screen shows at
/// once, which update reads from the screen's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    /// The page's rows.
    pub rows: usize,
    /// The rows the screen shows at once.
    pub room: usize,
}

impl Help {
    /// The page the tab shows, a line a row, at `size`. The Help page says
    /// what each view does and lists its keys, the route card and the
    /// commands, from the contract and the catalog the command line's
    /// help draws on. The About page is what `vtium-config --help about`
    /// prints, from the same source.
    pub fn page(&self, shared: &Shared, size: Size) -> Vec<Line<'static>> {
        let cols = size.cols;
        let bold = Style {
            bold: true,
            ..Style::PLAIN
        };
        let mut lines = Vec::new();
        if self.about {
            let mut out = Vec::new();
            crate::banner::about(&mut out);
            for l in String::from_utf8_lossy(&out).lines() {
                if l.contains('\x1b') {
                    // the banner, styled by its own module; it was drawn
                    // for this terminal's width, and a line wider is left
                    // out
                    if width(l) <= cols {
                        lines.push(styled(l));
                    }
                } else {
                    lines.extend(prose(l, cols));
                }
            }
            return lines;
        }
        for node in NODES {
            if node.supported {
                lines.extend(section(node, cols));
                continue;
            }
            // a view whose node is still planned keeps its hand-written
            // words
            let Some((_, name, does, keys)) = words::HELP_PLANNED.iter().find(|p| p.0 == node.id)
            else {
                continue;
            };
            // the Fonts view's keys in a vtium console, where `s` saves
            let (does, keys) = if *name == words::FONTS && shared.text_preview() {
                (words::HELP_FONTS_LIVE, words::KEYS_FONTS_LIVE)
            } else {
                (*does, *keys)
            };
            for (i, l) in wrap(
                &format!("{name} {does}"),
                cols.saturating_sub(2),
                usize::MAX,
            )
            .into_iter()
            .enumerate()
            {
                if i == 0 {
                    let rest = l.strip_prefix(name).unwrap_or(&l).to_owned();
                    lines.push(Line::from(vec![
                        span(format!(" {name}"), bold),
                        span(rest, Style::PLAIN),
                    ]));
                } else {
                    lines.push(line(format!(" {l}"), Style::PLAIN));
                }
            }
            for l in wrap(keys, cols.saturating_sub(6), usize::MAX) {
                lines.push(line(format!("     {l}"), Style::text(palette::GREY)));
            }
            lines.push(Line::default());
        }
        lines.push(Line::default());
        lines.extend(prose(words::HELP_COMMANDS, cols));
        let usage = crate::help::usage();
        let commands = usage
            .split_once("Commands:\n")
            .map(|(_, rest)| rest.split("\n\n").next().unwrap_or_default())
            .unwrap_or_default();
        // a command's line wider than the screen gives its synopsis a row
        // of its own and goes on under it
        let room = cols.saturating_sub(1);
        for l in commands.lines() {
            if l.chars().count() <= room {
                lines.push(line(format!(" {l}"), Style::PLAIN));
                continue;
            }
            let (synopsis, does) = l
                .trim_start()
                .split_once("  ")
                .map_or((l.trim(), ""), |(a, b)| (a, b.trim()));
            lines.push(line(format!("   {synopsis}"), Style::PLAIN));
            for d in wrap(does, cols.saturating_sub(8), usize::MAX) {
                if !d.is_empty() {
                    lines.push(line(format!("       {d}"), Style::PLAIN));
                }
            }
        }
        lines
    }

    /// Whether the tab shows the About page.
    pub fn about(&self) -> bool {
        self.about
    }

    /// The page's first row in sight.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The node of the interface tree the tab is at, which has no level
    /// that opens over it.
    pub fn node(&self) -> &'static str {
        "help"
    }

    /// A key the navigation module hands to the tab. The top row of the
    /// page moves by a line, or by the rows the screen shows, and stops
    /// where the page's last row is at the bottom of the screen.
    pub fn update(&mut self, key: Key, page: Page) -> Vec<Command> {
        match key {
            Key::Left | Key::Right => {
                self.about = !self.about;
                self.top = 0;
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let last = page.rows.saturating_sub(page.room);
                let step = page.room.max(1);
                let at = self.top.min(last);
                self.top = match key {
                    Key::Up => at.saturating_sub(1),
                    Key::Down => at.saturating_add(1),
                    Key::PageUp => at.saturating_sub(step),
                    Key::PageDown => at.saturating_add(step),
                    Key::Home => 0,
                    _ => last,
                }
                .min(last);
            }
            _ => {}
        }
        Vec::new()
    }
}
