//! What sits beside the list: the highlighted vtlook's filter chain, read
//! left to right in the order vtium runs it, with one line for each filter
//! and its settings under it; or the facts a color map's or a shader's
//! detail lists. A filter vtium does not build yet is marked planned with
//! the slice of ADR-1304 that builds it, in words, so the mark shows where
//! no color does.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Effects bullet: the chain
//!   `frame → spatial: none → turn → tint → contrast → screen`, a line
//!   for each filter's settings under it, and the picture after.
//! - ADR-1304 D1 and D8: the two layers, the spatial filters before the
//!   turn and the color filters after it.
//! - ADR-1304 D13: the slices that build the spatial layer.

use vtium_config_core::found::Resource;
use vtium_config_core::found::effects::{Built, Filter, From, Vtlook};
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_render::filter::Layer;

use crate::list::{self, Item};
use crate::words;

/// One line of the pane: its spans, each with the role it is drawn in.
pub type Spans = Vec<(String, Role)>;

/// The pane beside the list for `item`, or for the filter line where it
/// is `None`, cut to `width` cells: each line as spans. The picture's
/// frame follows these lines, where the item has a picture.
pub fn lines(shared: &Shared, item: Option<Item>, width: usize) -> Vec<Spans> {
    let effects = &shared.found.effects;
    match item {
        None => Vec::new(),
        Some(Item::Vtlook(i)) => effects
            .vtlooks
            .get(i)
            .map_or_else(Vec::new, |v| vtlook(shared, v, width)),
        Some(it @ (Item::Map(_) | Item::Shader(_))) => {
            list::resource(effects, it).map_or_else(Vec::new, |r| resource(shared, it, r))
        }
    }
}

/// The name a vtlook shows: No filter for the built-in one.
pub fn vtlook_name(v: &Vtlook) -> String {
    if v.from == From::BuiltIn {
        words::NO_FILTER.to_owned()
    } else {
        canvas::printable(&v.name)
    }
}

/// A vtlook's pane: its name, where it comes from and the looks that use
/// it; its chain; a line for each filter; and, for one that does not read,
/// its first error in one line.
fn vtlook(shared: &Shared, v: &Vtlook, width: usize) -> Vec<Spans> {
    let users = list::vtlook_users(shared, &v.name);
    let mut out = vec![
        vec![
            (vtlook_name(v), Role::Heading),
            (
                format!(
                    "  {} · {}",
                    words::from_short(v.from),
                    words::used_by(&users)
                ),
                Role::Row,
            ),
        ],
        Vec::new(),
    ];
    out.extend(chain(v, width));
    out.push(Vec::new());
    if v.filters.is_empty() {
        out.push(vec![(words::NO_FILTERS.to_owned(), Role::Dim)]);
    }
    for f in &v.filters {
        out.push(filter_line(f));
    }
    if !v.files.is_empty() {
        let names: Vec<String> = v
            .files
            .iter()
            .map(|p| {
                p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| canvas::printable(&n.to_string_lossy()),
                )
            })
            .collect();
        out.push(vec![(words::maps(&names), Role::Row)]);
    }
    if let Err(error) = &v.reads {
        out.push(vec![(words::does_not_read(error), Role::Warning)]);
    }
    out
}

/// The chain's steps in the order vtium runs them: the frame, the
/// spatial filters or `spatial: none`, the turn, the color filters, and
/// the screen.
pub fn steps(v: &Vtlook) -> Vec<(String, Role)> {
    let named = |f: &Filter| match f.built {
        Built::Built => (canvas::printable(&f.name), Role::Row),
        Built::Planned { .. } => (words::chain_planned(&canvas::printable(&f.name)), Role::Dim),
        Built::Unknown => (
            words::chain_unknown(&canvas::printable(&f.name)),
            Role::Warning,
        ),
    };
    let mut out = vec![(words::FRAME.to_owned(), Role::Row)];
    let spatial: Vec<_> = v
        .filters
        .iter()
        .filter(|f| f.layer == Layer::Spatial)
        .map(named)
        .collect();
    if spatial.is_empty() {
        out.push((words::SPATIAL_NONE.to_owned(), Role::Dim));
    }
    out.extend(spatial);
    out.push((words::TURN.to_owned(), Role::Row));
    out.extend(
        v.filters
            .iter()
            .filter(|f| f.layer != Layer::Spatial)
            .map(named),
    );
    out.push((words::SCREEN.to_owned(), Role::Row));
    out
}

/// The chain as lines of `width` cells, each step joined to the next by
/// an arrow, a line breaking only between steps.
fn chain(v: &Vtlook, width: usize) -> Vec<Spans> {
    let mut out: Vec<Spans> = Vec::new();
    let mut line: Spans = Vec::new();
    let mut used = 0usize;
    for (i, (step, role)) in steps(v).into_iter().enumerate() {
        let arrow = if i == 0 { "" } else { words::ARROW };
        let cells = canvas::cells(arrow).saturating_add(canvas::cells(&step));
        if !line.is_empty() && used.saturating_add(cells) > width {
            out.push(std::mem::take(&mut line));
            used = 0;
        }
        if !arrow.is_empty() {
            line.push((arrow.to_owned(), Role::Dim));
            used = used.saturating_add(canvas::cells(arrow));
        }
        used = used.saturating_add(canvas::cells(&step));
        line.push((step, role));
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

/// The settings of a filter, joined in one line.
pub fn settings(f: &Filter) -> String {
    f.settings
        .iter()
        .map(|(k, v)| format!("{} {}", canvas::printable(k), canvas::printable(v)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A filter's line: its name, its settings, and its planned or unknown
/// mark in words.
fn filter_line(f: &Filter) -> Spans {
    let name = format!("{:<10} ", canvas::printable(&f.name));
    let set = settings(f);
    match f.built {
        Built::Built => vec![(name, Role::Row), (set, Role::Row)],
        // the mark comes first, so a narrow pane still shows it
        Built::Planned { slice } => {
            let mut s = vec![(name, Role::Dim), (words::planned_slice(slice), Role::Dim)];
            if !set.is_empty() {
                s.push((format!(" · {set}"), Role::Dim));
            }
            s
        }
        Built::Unknown => vec![
            (name, Role::Warning),
            (words::UNKNOWN.to_owned(), Role::Warning),
        ],
    }
}

/// A color map's or a shader's pane: its kind and name, its file, its
/// state, the looks that use it, and for a shader its dialect and when it
/// runs.
fn resource(shared: &Shared, item: Item, r: &Resource) -> Vec<Spans> {
    let kind = match item {
        Item::Shader(_) => words::SHADER,
        _ => words::COLOR_MAP,
    };
    let (marks, role) = state(shared, r);
    let uses = list::uses(shared, r);
    let used = if uses.is_empty() {
        words::NO_LOOK.to_owned()
    } else {
        uses.iter()
            .map(|(look, through)| match through {
                Some(v) => format!("{look}, through the vtlook {}", canvas::printable(v)),
                None => look.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let label = |l: &str| format!("{l:<8}");
    let mut out = vec![
        vec![(
            format!("{kind} {}", canvas::printable(&r.name())),
            Role::Heading,
        )],
        Vec::new(),
        vec![
            (label(words::FILE), Role::Dim),
            (canvas::printable(&r.path.display().to_string()), Role::Row),
        ],
        vec![(label(words::STATE), Role::Dim), (marks, role)],
        vec![(label(words::USED_BY), Role::Dim), (used, Role::Row)],
    ];
    if let Item::Shader(i) = item
        && let Some(s) = shared.found.effects.shaders.get(i)
    {
        out.push(vec![
            (label(words::DIALECT), Role::Dim),
            (words::dialect(s.dialect), Role::Row),
        ]);
        out.push(vec![
            (label(words::RUNS), Role::Dim),
            (words::shader_runs(), Role::Dim),
        ]);
    }
    out
}

/// A resource's state in words: active or not, and trusted, failing the
/// trust rule, or in the person's own folder.
pub fn state(shared: &Shared, r: &Resource) -> (String, Role) {
    if r.own {
        return (words::own_detail(r.kind.words()), Role::Missing);
    }
    let on = list::active(shared, r);
    let mark = if on { words::ACTIVE } else { words::NOT_ACTIVE };
    let trust = if r.trust.is_ok() {
        words::TRUSTED
    } else {
        words::FAILS_TRUST
    };
    let role = match (on, r.trust.is_ok()) {
        (true, true) => Role::Active,
        (_, false) => Role::Missing,
        (false, true) => Role::Inactive,
    };
    (format!("{mark} · {trust}"), role)
}

/// The pane's lines as plain text, each whole, for F1.
pub fn plain(lines: &[Spans]) -> String {
    lines
        .iter()
        .map(|l| l.iter().map(|(t, _)| t.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
