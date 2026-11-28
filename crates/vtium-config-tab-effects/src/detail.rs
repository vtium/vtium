//! The detail that Enter opens on a vtlook, a color map or a shader,
//! which changes nothing. A vtlook's gives its file, whether it reads or
//! its error whole, its filters by layer in order with their settings, the
//! color maps it names and where each stands, and the looks that use it.
//! A color map's or a shader's gives its file, its state, its trust check
//! and the looks that use it. What ADR-1304 does not build yet, the
//! requirements and the displays a vtlook applies to, shows as planned.
//! The title stays on the first row while the lines under it scroll.
//!
//! References:
//! - ADR-1403 D10, the node `effects.effect`: a read-only detail, Up and
//!   Down scroll it, Esc goes back.
//! - ADR-1304 D6: a vtlook that does not read gives way to the built-in
//!   vtlook, and the report names the line, the layer, the filter and the
//!   range it needed.
//! - ADR-1304 D7 and D11: the requirements and the displays, slice 7.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::found::Resource;
use vtium_config_core::found::effects::{Built, From, Vtlook};
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_render::filter::Layer;

use crate::chain::{self, Spans};
use crate::list::{self, Item};
use crate::words;

/// The detail of `item`, each line as spans, the title first. A line
/// wider than `width` is wrapped onto the lines under it, so the detail
/// cuts nothing.
pub fn lines(shared: &Shared, item: Item, width: usize) -> Vec<Spans> {
    let effects = &shared.found.effects;
    let raw = match item {
        Item::Vtlook(i) => effects
            .vtlooks
            .get(i)
            .map_or_else(Vec::new, |v| vtlook(shared, v)),
        Item::Map(_) | Item::Shader(_) => {
            list::resource(effects, item).map_or_else(Vec::new, |r| resource(shared, item, r))
        }
    };
    let mut out = Vec::new();
    for line in raw {
        out.extend(wrap(line, width));
    }
    out
}

/// A line of the form label, value, where the label takes ten cells.
fn labelled(label: &str, value: String, role: Role) -> Spans {
    vec![(format!("   {label:<10}"), Role::Dim), (value, role)]
}

/// A vtlook's detail.
fn vtlook(shared: &Shared, v: &Vtlook) -> Vec<Spans> {
    let users = list::vtlook_users(shared, &v.name);
    let mut out = vec![
        vec![
            (
                format!(" {} {}", words::VTLOOK, chain::vtlook_name(v)),
                Role::Heading,
            ),
            (format!("   {}", words::from_whole(v.from)), Role::Row),
            (format!("   {}", words::used_by(&users)), Role::Dim),
        ],
        Vec::new(),
    ];
    let file = v.file.as_ref().map_or_else(
        || words::BUILT_IN_FILE.to_owned(),
        |f| canvas::printable(&f.display().to_string()),
    );
    out.push(labelled(words::D_FILE, file, Role::Row));
    let hides = v.from == From::Administrator
        && shared
            .found
            .effects
            .vtlooks
            .iter()
            .any(|p| p.from == From::Preset && p.hidden && p.name == v.name);
    if v.hidden {
        out.push(labelled("", words::HIDDEN_BY.to_owned(), Role::Dim));
    } else if hides {
        out.push(labelled("", words::HIDES.to_owned(), Role::Dim));
    }
    match &v.reads {
        Ok(()) => out.push(labelled(
            words::D_READS,
            words::READS.to_owned(),
            Role::Active,
        )),
        Err(error) => {
            out.push(labelled(
                words::D_READS,
                words::DOES_NOT_READ.to_owned(),
                Role::Warning,
            ));
            out.push(Vec::new());
            out.push(vec![(format!("   {error}"), Role::Warning)]);
            out.push(Vec::new());
            out.push(vec![(format!(" {}", words::FALLS_BACK), Role::Dim)]);
        }
    }
    out.push(Vec::new());
    for (layer, heading, none) in [
        (
            Layer::Spatial,
            words::SPATIAL_LAYER,
            words::SPATIAL_LAYER_NONE,
        ),
        (Layer::Color, words::COLOR_LAYER, words::COLOR_LAYER_NONE),
    ] {
        let filters: Vec<_> = v.filters.iter().filter(|f| f.layer == layer).collect();
        if filters.is_empty() {
            out.push(vec![(format!(" {none}"), Role::Row)]);
            continue;
        }
        out.push(vec![(format!(" {heading}"), Role::Row)]);
        for (n, f) in filters.iter().enumerate() {
            let head = format!(
                "   {}  {:<10} ",
                n.saturating_add(1),
                canvas::printable(&f.name)
            );
            let set = chain::settings(f);
            out.push(match f.built {
                Built::Built => vec![(head, Role::Row), (set, Role::Row)],
                Built::Planned { slice } => vec![
                    (head, Role::Dim),
                    (
                        if set.is_empty() {
                            words::planned_slice(slice)
                        } else {
                            format!("{} · {set}", words::planned_slice(slice))
                        },
                        Role::Dim,
                    ),
                ],
                Built::Unknown => vec![
                    (head, Role::Warning),
                    (words::UNKNOWN.to_owned(), Role::Warning),
                ],
            });
        }
    }
    if !v.files.is_empty() {
        out.push(Vec::new());
        out.push(vec![(format!(" {}", words::MAPS_NAMED), Role::Row)]);
        for f in &v.files {
            let path = canvas::printable(&f.display().to_string());
            let (mark, role) = shared
                .found
                .resource(f)
                .map_or((words::MAP_MISSING.to_owned(), Role::Missing), |r| {
                    list::resource_marks(shared, r)
                });
            out.push(vec![(format!("   {path}   "), Role::Row), (mark, role)]);
        }
    }
    if v.from != From::BuiltIn {
        out.push(Vec::new());
        out.push(vec![
            (format!(" {:<10} ", words::D_REQUIRES), Role::Dim),
            (words::requires_planned(), Role::Dim),
        ]);
        out.push(vec![
            (format!(" {:<10} ", words::D_DISPLAYS), Role::Dim),
            (words::requires_planned(), Role::Dim),
        ]);
    }
    if v.from == From::Preset {
        out.push(Vec::new());
        out.push(vec![(format!(" {}", words::CHANGE_PRESET), Role::Dim)]);
    }
    out
}

/// A color map's or a shader's detail.
fn resource(shared: &Shared, item: Item, r: &Resource) -> Vec<Spans> {
    let kind = match item {
        Item::Shader(_) => words::SHADER,
        _ => words::COLOR_MAP,
    };
    let uses = list::uses(shared, r);
    let used: Vec<String> = uses
        .iter()
        .map(|(look, through)| match through {
            Some(v) => format!("{look} through vtlook {}", canvas::printable(v)),
            None => look.clone(),
        })
        .collect();
    let mut out = vec![
        vec![
            (
                format!(" {kind} {}", canvas::printable(&r.name())),
                Role::Heading,
            ),
            (format!("   {}", words::used_by(&used)), Role::Dim),
        ],
        Vec::new(),
        labelled(
            words::D_FILE,
            canvas::printable(&r.path.display().to_string()),
            Role::Row,
        ),
    ];
    let (state, role) = chain::state(shared, r);
    out.push(labelled(words::D_STATE, state, role));
    if let Err(why) = &r.trust {
        out.push(labelled(words::D_TRUST, why.clone(), Role::Missing));
    }
    if r.hidden {
        out.push(labelled(
            "",
            words::HIDDEN_BY.replace("vtlook", kind),
            Role::Dim,
        ));
    }
    if let Item::Shader(i) = item
        && let Some(s) = shared.found.effects.shaders.get(i)
    {
        out.push(labelled(
            words::D_DIALECT,
            words::dialect(s.dialect),
            Role::Row,
        ));
        out.push(labelled(words::D_RUNS, words::shader_runs(), Role::Dim));
    }
    out
}

/// `line` in lines of `width` cells: whole where it fits, and otherwise
/// broken between words, each line under the first taking the first
/// span's indent.
fn wrap(line: Spans, width: usize) -> Vec<Spans> {
    let total: usize = line.iter().map(|(t, _)| canvas::cells(t)).sum();
    if total <= width || width < 8 {
        return vec![line];
    }
    let indent = line
        .first()
        .map_or(0, |(t, _)| t.chars().take_while(|c| *c == ' ').count())
        .min(width / 2);
    let mut out: Vec<Spans> = Vec::new();
    let mut cur: Spans = Vec::new();
    let mut used = 0usize;
    for (text, role) in line {
        for word in split_keep(&text) {
            let cells = canvas::cells(word);
            if used.saturating_add(cells) > width && used > indent {
                out.push(std::mem::take(&mut cur));
                cur.push((" ".repeat(indent), role));
                used = indent;
                let word = word.trim_start();
                let cells = canvas::cells(word);
                cur.push((canvas::fit(word, width.saturating_sub(indent)), role));
                used = used.saturating_add(cells);
                continue;
            }
            cur.push((word.to_owned(), role));
            used = used.saturating_add(cells);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `text` cut into words, each keeping the spaces before it.
fn split_keep(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = true;
    for (i, c) in text.char_indices() {
        if c == ' ' && !in_space {
            if let Some(w) = text.get(start..i) {
                out.push(w);
            }
            start = i;
        }
        in_space = c == ' ';
    }
    if let Some(w) = text.get(start..)
        && !w.is_empty()
    {
        out.push(w);
    }
    out
}

/// The most the detail scrolls: its lines under the title less those the
/// area shows.
pub fn last_top(lines: usize, area: Rect) -> usize {
    lines
        .saturating_sub(1)
        .saturating_sub(usize::from(area.height.saturating_sub(1)))
}

/// Draws the detail into `area`, its title on the first row and the
/// lines under it from `top`.
pub fn draw(lines: &[Spans], top: usize, buf: &mut Buffer, area: Rect, color: bool) {
    let rows = lines
        .iter()
        .take(1)
        .chain(lines.iter().skip(1).skip(top))
        .take(usize::from(area.height));
    for (y, line) in rows.enumerate() {
        put_spans(
            buf,
            area,
            u16::try_from(y).unwrap_or(u16::MAX),
            0,
            line,
            color,
        );
    }
}

/// Writes `line`'s spans from column `x` of row `y` of `area`, cut at its
/// right edge.
pub fn put_spans(buf: &mut Buffer, area: Rect, y: u16, x: usize, line: &Spans, color: bool) {
    let mut at = x;
    for (text, role) in line {
        at = list::put(buf, area, at, y, text, *role, color);
    }
}
