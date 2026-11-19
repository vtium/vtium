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
use super::super::model::{Shared, Size, View};
use super::super::words;
use super::{Command, Room, Tab};

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

/// The Help page or the About page laid out at a size: its lines, and the
/// row where each of its entries starts. An entry is a run of rows that
/// belong together, such as a key with its meaning wrapped under it, or a
/// paragraph.
#[derive(Default)]
struct Laid {
    lines: Vec<Line<'static>>,
    starts: Vec<usize>,
}

impl Laid {
    /// Adds `lines` to the page as one entry.
    fn entry(&mut self, lines: impl IntoIterator<Item = Line<'static>>) {
        self.starts.push(self.lines.len());
        self.lines.extend(lines);
    }
}

/// One node's section of the Help page, from the contract: what the node
/// is, in bold, then each key that acts there and what it does, the
/// meaning wrapped under its own column. What the node is makes one entry,
/// and each key with its meaning another.
fn section(node: &Node, cols: usize, page: &mut Laid) {
    if node.help.is_empty() {
        return;
    }
    let bold = Style {
        bold: true,
        ..Style::PLAIN
    };
    page.entry(
        wrap(node.what, cols.saturating_sub(2), usize::MAX)
            .into_iter()
            .map(|l| line(format!(" {l}"), bold)),
    );
    let keys_width = node
        .help
        .iter()
        .map(|(keys, _)| keys.chars().count())
        .filter(|w| *w <= HELP_KEYS_MAX)
        .max()
        .unwrap_or(0);
    let indent = keys_width.saturating_add(5);
    for (keys, does) in node.help {
        let mut lines = Vec::new();
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
        page.entry(lines);
    }
    page.entry([Line::default()]);
}

/// The node of the interface tree that a tab of the bar shows: `fonts`
/// for the Fonts tab, and the same for the form for a new font set, which
/// belongs to it.
fn tab_node(view: View) -> &'static str {
    match view {
        View::Fonts | View::New => "fonts",
        View::Effects => "effects",
        View::Looks => "looks",
        View::Flow => "flow",
        View::Settings => "settings",
        View::Help => "help",
    }
}

/// The nodes of the interface tree in the order the Help page describes
/// them: the TUI's own node first, then each tab in the tab bar's order
/// followed by the nodes under it, then what opens over any tab. The tree
/// file keeps its own order, which is not the bar's, so the page sorts the
/// nodes by the tab each one sits under. Within a tab, the nodes keep the
/// tree's order.
///
/// References:
/// - ADR-1205 D1: the six tabs and their order.
fn in_bar_order() -> Vec<&'static Node> {
    let rank = |node: &Node| {
        let root = node.id.split('.').next().unwrap_or(node.id);
        if root == "tui" {
            return 0;
        }
        View::TABS
            .iter()
            .position(|v| tab_node(*v) == root)
            .map_or(View::TABS.len().saturating_add(1), |at| {
                at.saturating_add(1)
            })
    };
    let mut nodes: Vec<&Node> = NODES.iter().collect();
    nodes.sort_by_key(|n| rank(n));
    nodes
}

/// Where the Help tab stands: the page it shows, and the page's first row
/// in sight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Help {
    about: bool,
    top: usize,
}

/// The page's rows, and how many of them the screen shows at once, which
/// update reads from the screen's layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The page's rows.
    pub rows: Rows,
    /// The rows the screen shows at once.
    pub room: usize,
}

/// How many rows the page has, and where each of its entries starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rows {
    pub len: usize,
    /// The first row of each entry, in order, from row 0.
    pub starts: Vec<usize>,
}

impl Help {
    /// The page the tab shows, a line a row, at `size`. The Help page says
    /// what each view does and lists its keys, the route card and the
    /// commands, from the contract and the catalog the command line's
    /// help draws on. The About page is what `vtium-config --help about`
    /// prints, from the same source.
    pub fn page(&self, shared: &Shared, size: Size) -> Vec<Line<'static>> {
        self.laid(shared, size).lines
    }

    /// How many rows the page has at `size`, and where each of its entries
    /// starts.
    pub fn rows(&self, shared: &Shared, size: Size) -> Rows {
        let laid = self.laid(shared, size);
        Rows {
            len: laid.lines.len(),
            starts: laid.starts,
        }
    }

    /// The page the tab shows at `size`, with the row where each entry
    /// starts.
    fn laid(&self, shared: &Shared, size: Size) -> Laid {
        let cols = size.cols;
        let bold = Style {
            bold: true,
            ..Style::PLAIN
        };
        let mut page = Laid::default();
        if self.about {
            let mut out = Vec::new();
            crate::banner::about(&mut out);
            for l in String::from_utf8_lossy(&out).lines() {
                if l.contains('\x1b') {
                    // the banner, styled by its own module; it was drawn
                    // for this terminal's width, and a line wider is left
                    // out
                    if width(l) <= cols {
                        page.entry([styled(l)]);
                    }
                } else {
                    page.entry(prose(l, cols));
                }
            }
            return page;
        }
        for node in in_bar_order() {
            if node.supported {
                section(node, cols, &mut page);
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
            // what the view does and its keys make one entry
            let mut lines = Vec::new();
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
            page.entry(lines);
            page.entry([Line::default()]);
        }
        page.entry([Line::default()]);
        page.entry(prose(words::HELP_COMMANDS, cols));
        let usage = crate::help::usage();
        let commands = usage
            .split_once("Commands:\n")
            .map(|(_, rest)| rest.split("\n\n").next().unwrap_or_default())
            .unwrap_or_default();
        // a command's line wider than the screen gives its synopsis a row
        // of its own and goes on under it; a line indented under a command
        // goes on with that command's entry
        let room = cols.saturating_sub(1);
        let mut entries: Vec<Vec<Line<'static>>> = Vec::new();
        for l in commands.lines() {
            let mut lines = Vec::new();
            if l.chars().count() <= room {
                lines.push(line(format!(" {l}"), Style::PLAIN));
            } else {
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
            match entries.last_mut() {
                Some(entry) if l.starts_with("   ") => entry.extend(lines),
                _ => entries.push(lines),
            }
        }
        for entry in entries {
            page.entry(entry);
        }
        page
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
    /// page moves by a line, or by a screen, and stops where the page's
    /// last row is at the bottom of the screen. A screen's step keeps each
    /// entry whole on one screen: PgDn starts the next screen at the entry
    /// that the screen cuts or that starts below it, and PgUp starts the
    /// screen at the first entry that fits whole above the old top. Only
    /// an entry taller than the screen is cut, and then the step is a
    /// whole screen.
    pub fn update(&mut self, key: Key, page: Page) -> Vec<Command> {
        match key {
            Key::Left | Key::Right => {
                self.about = !self.about;
                self.top = 0;
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let last = page.rows.len.saturating_sub(page.room);
                let step = page.room.max(1);
                let at = self.top.min(last);
                let starts = &page.rows.starts;
                self.top = match key {
                    Key::Up => at.saturating_sub(1),
                    Key::Down => at.saturating_add(1),
                    Key::PageUp => {
                        let above = at.saturating_sub(step);
                        starts
                            .iter()
                            .copied()
                            .find(|s| (above..at).contains(s))
                            .unwrap_or(above)
                    }
                    Key::PageDown => {
                        let below = at.saturating_add(step);
                        starts
                            .iter()
                            .copied()
                            .rev()
                            .find(|s| *s > at && *s <= below)
                            .unwrap_or(below)
                    }
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

impl Tab for Help {
    fn name(&self) -> &'static str {
        words::HELP
    }

    /// The Help tab's tab bar shows no action.
    fn actions(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn node(&self) -> &'static str {
        Help::node(self)
    }

    fn page(&self, shared: &Shared, size: Size) -> Option<(Vec<Line<'static>>, usize)> {
        Some((Help::page(self, shared, size), self.top()))
    }

    /// Install and Save do not act where the tab bar shows no action;
    /// `o` still turns `allow-font-ops` on.
    fn takes_action(&self, action: char) -> bool {
        !matches!(action, 'i' | 's')
    }

    /// The page's rows on the screen of `room`, scrolled in the rows of
    /// the body.
    fn update(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command> {
        let rows = self.rows(shared, room.size);
        Help::update(
            self,
            key,
            Page {
                rows,
                room: room.body,
            },
        )
    }
}
