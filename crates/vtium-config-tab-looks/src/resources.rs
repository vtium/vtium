//! The font sets, the color schemes and the pointer themes: the facts
//! beside the list where one is highlighted, the detail Enter opens, and
//! Space, which activates a color scheme file or a pointer theme.
//!
//! A detail is read-only: a title, then one labelled line for each fact
//! the old screen spelled out in a paragraph. A font set's detail gives
//! whether the console reads it, its fonts by role with each one's mark,
//! and what it still misses. A color scheme's gives its file or that it is
//! built in, its state, the looks that use it, and its 16 colors with its
//! foreground and background. A pointer theme's gives its folder, its
//! state and trust mark, and the looks that use it.
//!
//! References:
//! - ADR-1205 D6 and D10: the pointer themes and the color schemes in the Looks tab, each activated by Space.
//! - ADR-1205 D4: a resource is active only while the batch holds it and it passes the trust rule; one that fails counts as missing.
//! - ADR-1403 D11, "One set of marks for a font": the marks' words and roles.

use std::path::{Path, PathBuf};

use vtium_config_core::batch::{self, Change as Batch};
use vtium_config_core::document::{Change, Edit};
use vtium_config_core::fontuse::Used;
use vtium_config_core::found::{Activation, Origin, Resource};
use vtium_config_core::ui::{Change as UiChange, Command, Message, Shared};
use vtium_config_widgets::roles::Role;
use vtium_term::{Palette, Rgb};

use crate::card;
use crate::list::{self, Item, Scheme};
use crate::missing;
use crate::{sizes, words};

/// One row of a detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// A line of text in its parts, each in its role.
    Text(Vec<(String, Role)>),
    /// Four of a color scheme's colors, each with its number.
    Swatches(Vec<(u8, Rgb)>),
    /// A line of text in a scheme's foreground on its background.
    Sample { fg: Rgb, bg: Rgb },
}

impl Row {
    /// A row of plain text.
    pub fn plain(text: impl Into<String>) -> Row {
        Row::Text(vec![(text.into(), Role::Row)])
    }

    /// A row of text in `role`.
    pub fn in_role(text: impl Into<String>, role: Role) -> Row {
        Row::Text(vec![(text.into(), role)])
    }

    /// A blank row.
    pub fn blank() -> Row {
        Row::Text(Vec::new())
    }

    /// A labelled fact: the label, then `parts`.
    pub fn fact(label: &str, parts: Vec<(String, Role)>) -> Row {
        let mut out = vec![(format!("{label:<10}"), Role::Dim)];
        out.extend(parts);
        Row::Text(out)
    }

    /// The row's words, as F1's text and a line without color show them.
    pub fn words(&self) -> String {
        match self {
            Row::Text(parts) => parts.iter().map(|(t, _)| t.as_str()).collect(),
            Row::Swatches(s) => s
                .iter()
                .map(|(n, c)| format!("{n:>2} {}", hex(*c)))
                .collect::<Vec<_>>()
                .join("  "),
            Row::Sample { fg, bg } => words::sample_words(&hex(*fg), &hex(*bg)),
        }
    }
}

/// A detail: its title, what sits at the right of the title, and its rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    pub title: String,
    pub right: String,
    pub rows: Vec<Row>,
}

impl Detail {
    /// The detail as plain text, which F1 opens.
    pub fn text(&self) -> String {
        let mut out = self.title.clone();
        if !self.right.is_empty() {
            out.push_str(" · ");
            out.push_str(&self.right);
        }
        out.push_str("\n\n");
        for row in &self.rows {
            out.push_str(row.words().trim_end());
            out.push('\n');
        }
        out
    }
}

/// `#rrggbb`.
pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// The detail of `item`, where it is a font set, a color scheme or a
/// pointer theme.
pub fn detail(shared: &Shared, item: &Item, used: &Used) -> Option<Detail> {
    match item {
        Item::FontSet(name) => Some(font_set(shared, name, used)),
        Item::Scheme(s) => scheme(shared, *s),
        Item::Pointer(at) => pointer(shared, *at),
        Item::Look(_) | Item::NoLooks => None,
    }
}

/// Where a resource stands, in the words and roles of the marks: active,
/// not active, turned off on purpose, failing the trust rule, or in the
/// person's own folder.
pub fn state(shared: &Shared, r: &Resource) -> Vec<(String, Role)> {
    let w = |t: &str, role| (t.to_owned(), role);
    if r.own {
        return vec![w(words::NOT_USABLE_YET, Role::Warning)];
    }
    if r.trust.is_err() {
        return vec![
            w(words::NOT_ACTIVE_DOT, Role::Inactive),
            w(words::FAILS_TRUST, Role::Warning),
        ];
    }
    let active = card::active(shared, r);
    let off = matches!(
        r.activation(&shared.document().content().active),
        Activation::Installed(Origin::TurnedOff { .. })
            | Activation::Draft(vtium_config_core::batch::Change::Deactivate)
    );
    let mut out = if active {
        vec![w(words::ACTIVE, Role::Active)]
    } else if off {
        vec![w(words::TURNED_OFF, Role::Inactive)]
    } else {
        vec![w(words::NOT_ACTIVE, Role::Inactive)]
    };
    out.push(w(words::TRUSTED, Role::Row));
    out
}

/// The looks of the document that use the resource at `path`, by name,
/// `vtium.conf` for its top level.
fn users_of(shared: &Shared, path: &Path) -> Vec<String> {
    let folders: Vec<&Path> = shared
        .found
        .palettes
        .folders
        .iter()
        .map(PathBuf::as_path)
        .collect();
    let none = |_: &str| Vec::new();
    batch::uses(
        path,
        shared.document().text(),
        &batch::Places {
            frits: None,
            palettes: &folders,
            vtlook: &none,
        },
    )
    .into_iter()
    .map(|u| words::user(u.look.as_deref()))
    .collect()
}

/// The detail of the font set `name`.
fn font_set(shared: &Shared, name: &str, used: &Used) -> Detail {
    let looks: Vec<String> = list::looks(shared.document().text())
        .into_iter()
        .filter(|l| card::font_set(shared, l).as_deref() == Some(name))
        .map(|l| l.name)
        .collect();
    let mut rows = Vec::new();
    let sets = &shared.document().content().font_sets;
    if sets.removed.contains(name) {
        rows.push(Row::in_role(words::REMOVED_AT_SAVE, Role::Warning));
    }
    let reads = sizes::font_set(shared, name);
    rows.push(Row::plain(match &reads {
        Some(Ok(_)) => words::READS.to_owned(),
        Some(Err(why)) => words::does_not_read_why(why),
        None => words::NO_SUCH_SET.to_owned(),
    }));
    rows.push(Row::blank());
    rows.push(Row::in_role(words::FONTS_BY_ROLE, Role::Heading));
    let roles = missing::set_roles(used, name);
    if roles.is_empty() {
        if let Some(Ok(frit)) = &reads {
            for source in &frit.sources {
                let role = source
                    .layer
                    .as_deref()
                    .map_or_else(|| words::ROLE_TEXT.to_owned(), words::layer_role);
                rows.push(Row::plain(format!(
                    "  {role:<14}{}",
                    words::named(&source.spelling)
                )));
            }
        }
        rows.push(Row::in_role(words::NOT_CHECKED, Role::Dim));
    } else {
        for r in &roles {
            let mut parts = vec![(format!("  {:<14}{:<34}", r.role, r.font), Role::Row)];
            if let Some(state) = &r.state {
                parts.extend(missing::state_mark(state));
            }
            rows.push(Row::Text(parts));
        }
        rows.push(Row::blank());
        rows.push(Row::in_role(words::STILL_MISSES, Role::Heading));
        for line in missing::set_misses(used, name) {
            rows.push(Row::plain(format!("  {line}")));
        }
    }
    Detail {
        title: words::font_set_title(name),
        right: words::used_by(&looks),
        rows,
    }
}

/// A color scheme's name and colors.
fn scheme_colors(shared: &Shared, s: Scheme) -> Option<(String, Result<Palette, String>)> {
    match s {
        Scheme::BuiltIn(name) => Palette::named(name).map(|p| (name.to_owned(), Ok(p))),
        Scheme::File(at) => shared
            .found
            .palettes
            .files
            .get(at)
            .map(|f| (f.name.clone(), f.colors.clone())),
    }
}

/// The detail of the color scheme `s`.
fn scheme(shared: &Shared, s: Scheme) -> Option<Detail> {
    let (name, colors) = scheme_colors(shared, s)?;
    let mut rows = Vec::new();
    let users = match s {
        Scheme::BuiltIn(n) => {
            rows.push(Row::fact(
                words::FILE,
                vec![(words::BUILT_IN.to_owned(), Role::Row)],
            ));
            rows.push(Row::fact(
                words::STATE,
                vec![(words::ALWAYS_ACTIVE.to_owned(), Role::Active)],
            ));
            batch::drawing_palette(shared.document().text(), n)
                .iter()
                .map(|l| words::user(l.as_deref()))
                .collect()
        }
        Scheme::File(at) => {
            let f = shared.found.palettes.files.get(at)?;
            let r = &f.resource;
            rows.push(Row::fact(
                words::FILE,
                vec![(path_text(&r.path), Role::Row)],
            ));
            rows.push(Row::fact(words::STATE, state(shared, r)));
            if let Err(why) = &r.trust {
                rows.push(Row::fact(words::TRUST, vec![(why.clone(), Role::Warning)]));
            }
            users_of(shared, &r.path)
        }
    };
    rows.push(Row::fact(
        words::USED_BY,
        vec![(words::looks(&users), Role::Row)],
    ));
    rows.push(Row::blank());
    match colors {
        Ok(p) => {
            rows.push(Row::in_role(words::COLORS_HEADING, Role::Heading));
            for four in 0u8..4 {
                rows.push(Row::Swatches(
                    (0u8..4)
                        .map(|i| {
                            let n = four.saturating_mul(4).saturating_add(i);
                            (n, p.color(n))
                        })
                        .collect(),
                ));
            }
            rows.push(Row::plain(words::fg_bg(&hex(p.fg()), &hex(p.bg()))));
            rows.push(Row::Sample {
                fg: p.fg(),
                bg: p.bg(),
            });
        }
        Err(why) => rows.push(Row::in_role(words::does_not_read_why(&why), Role::Warning)),
    }
    Some(Detail {
        title: words::scheme_title(&name),
        right: String::new(),
        rows,
    })
}

/// A path as the screen shows it, with any control character replaced.
fn path_text(path: &Path) -> String {
    vtium_config_widgets::canvas::printable(&path.display().to_string())
}

/// The detail of the pointer theme at `at`.
fn pointer(shared: &Shared, at: usize) -> Option<Detail> {
    let r = shared.found.pointers.get(at)?;
    let folder = r.path.extension().is_none_or(|e| e != "svg");
    let mut rows = vec![Row::fact(
        if folder { words::FOLDER } else { words::FILE },
        vec![(path_text(&r.path), Role::Row)],
    )];
    if folder {
        rows.push(Row::in_role(words::WHOLE_FOLDER, Role::Dim));
    }
    rows.push(Row::fact(words::STATE, state(shared, r)));
    if let Err(why) = &r.trust {
        rows.push(Row::fact(words::TRUST, vec![(why.clone(), Role::Warning)]));
    }
    rows.push(Row::fact(
        words::USED_BY,
        vec![(words::looks(&users_of(shared, &r.path)), Role::Row)],
    ));
    if r.trust.is_err() {
        rows.push(Row::blank());
        rows.push(Row::in_role(words::FAILS_COUNTS_MISSING, Role::Dim));
    }
    Some(Detail {
        title: words::pointer_title(&list::pointer_name(&r.name())),
        right: String::new(),
        rows,
    })
}

/// The resource Space acts on for `item`: a color scheme file or a
/// pointer theme.
fn resource<'a>(shared: &'a Shared, item: &Item) -> Option<&'a Resource> {
    match item {
        Item::Scheme(Scheme::File(at)) => shared.found.palettes.files.get(*at).map(|f| &f.resource),
        Item::Pointer(at) => shared.found.pointers.get(*at),
        _ => None,
    }
}

/// Space on `item`: activates a color scheme file or a pointer theme that
/// is not active, or deactivates one that is and that no look uses, as
/// one change. A built-in scheme is always active, and a resource that
/// fails the trust rule or sits in the person's own folder cannot be
/// activated; each says so. Deactivating what a look uses first asks a
/// question, which the second half of this tab's crate adds, so Space
/// leaves such a resource as it is.
pub fn space(shared: &Shared, item: &Item) -> Vec<Command> {
    let say = |line: String| vec![Command::Change(UiChange::Say(Message { line, more: None }))];
    if let Item::Scheme(Scheme::BuiltIn(name)) = item {
        return say(words::always_active(name));
    }
    let Some(r) = resource(shared, item) else {
        return Vec::new();
    };
    let name = item.name(shared);
    if r.own || r.trust.is_err() {
        let why = r.trust.as_ref().err().cloned().unwrap_or_default();
        return vec![Command::Change(UiChange::Say(Message {
            line: words::cannot_activate(&name),
            more: (!why.is_empty()).then_some(why),
        }))];
    }
    let drafted = shared.document().content().active.get(&r.path);
    let (change, edit_name) = if card::active(shared, r) {
        if !users_of(shared, &r.path).is_empty() {
            return Vec::new();
        }
        match drafted {
            Some(Batch::Activate) => (None, words::deactivated(&name)),
            _ => (Some(Batch::Deactivate), words::deactivated(&name)),
        }
    } else {
        match drafted {
            Some(Batch::Deactivate) => (None, words::activated(&name)),
            _ => (Some(Batch::Activate), words::activated(&name)),
        }
    };
    vec![Command::Change(UiChange::Document(Change::new(
        edit_name,
        Edit::Active {
            path: r.path.clone(),
            change,
        },
    )))]
}
