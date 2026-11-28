//! The levels of the interface tree and the key table's rows that act in
//! each: what kind of place a node is, and which row of the node a key
//! chooses.
//!
//! References:
//! - ADR-1206 D3: the navigation module reads the compiled tables and
//!   looks up each key's row for the node that is open.
//! - ADR-1403 D10: the kinds of level, and the rows that are the same
//!   everywhere.

use vtium_config_widgets::contract::{self, Row};
use vtium_config_widgets::keys::Key;

/// What kind of place a node of the interface tree is, which decides the
/// rows that are the same everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A tab with nothing open over it, such as `fonts`.
    View,
    /// A level open over a tab, such as a detail, a card or a list.
    Layer,
    /// A value or a pattern being typed.
    Typing,
    /// A menu, or a submenu, open over a tab.
    Menu,
    /// A question over everything.
    Question,
    /// The screen below the minimum size.
    TooSmall,
}

/// The kind of the node `id`.
pub fn kind(id: &str) -> Kind {
    match id {
        "any.too-small" => Kind::TooSmall,
        "any.search" | "any.find" => Kind::Typing,
        _ if id.starts_with("any.question.") => Kind::Question,
        _ if id.starts_with("any.menu.") && !id.starts_with("any.menu.file.manage") => Kind::Menu,
        _ if id.ends_with(".typing") => Kind::Typing,
        _ if !id.contains('.') => Kind::View,
        _ => Kind::Layer,
    }
}

/// The row of the node `node` that `key` chooses: the first row whose key
/// column names the key, then a row for any letter where the key is a
/// character typed, then a row for any other key. `None` where the node
/// has no row for the key, so the key does nothing there.
///
/// An Alt letter matches regardless of case, so Caps Lock and Shift change
/// nothing. The rows of the top row's Alt letters are the shell's to read
/// against the tabs' letters, so they never match here (see
/// [`super::Nav::key`]).
pub fn row_for(node: &str, key: &Key) -> Option<&'static Row> {
    let node = contract::node(node)?;
    let rows: Vec<&'static Row> = node.rows().filter(|r| r.supported).collect();
    rows.iter()
        .copied()
        .find(|r| names(r.key, key))
        .or_else(|| {
            key.typed()
                .and_then(|_| rows.iter().copied().find(|r| r.key == "any letter"))
        })
        .or_else(|| rows.iter().copied().find(|r| r.key == "any other key"))
}

/// The row of the node `node` whose id is `id`, where the node lists it.
pub fn listed(node: &str, id: &str) -> Option<&'static Row> {
    contract::node(node)?.rows().find(|r| r.id == id)
}

/// Whether the key column `column` names `key`: one of the keys it lists
/// after commas, such as `Ctrl-S, Enter`, or one of the letters it lists
/// after spaces, such as `n o u d s c e r h q`.
fn names(column: &str, key: &Key) -> bool {
    column.split(", ").any(|one| {
        if one.starts_with("Click") || one.starts_with("Wheel") || one.starts_with("any ") {
            return false;
        }
        if one.contains(' ') {
            return key
                .typed()
                .is_some_and(|c| one.split(' ').any(|l| l.chars().eq(std::iter::once(c))));
        }
        let Some(named) = Key::named(one) else {
            return false;
        };
        match named.alt_letter() {
            Some(letter) => key.is_alt(letter),
            None => named == *key,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtium_config_widgets::keys::{Code, Mods};

    #[test]
    fn each_node_id_has_its_kind() {
        assert_eq!(kind("fonts"), Kind::View);
        assert_eq!(kind("looks-resources"), Kind::View);
        assert_eq!(kind("fonts.font"), Kind::Layer);
        assert_eq!(kind("any.menu.file"), Kind::Menu);
        assert_eq!(kind("any.menu.file.open"), Kind::Menu);
        assert_eq!(kind("any.menu.file.manage"), Kind::Layer);
        assert_eq!(kind("any.menu.file.manage.typing"), Kind::Typing);
        assert_eq!(kind("any.question.confirm"), Kind::Question);
        assert_eq!(kind("any.search"), Kind::Typing);
        assert_eq!(kind("any.too-small"), Kind::TooSmall);
    }

    #[test]
    fn a_key_chooses_the_row_its_node_lists_for_it() {
        let id = |node, key| row_for(node, &key).map(|r| r.id);
        assert_eq!(id("fonts", Key::ctrl('z')), Some("ctrl-z.view"));
        assert_eq!(id("fonts.font", Key::ctrl('z')), Some("ctrl-z.layer"));
        assert_eq!(
            id("any.menu.file", Key::ctrl('z')),
            Some("ctrl-z.layer.menu")
        );
        assert_eq!(id("fonts", Key::char('q')), Some("q.view"));
        assert_eq!(
            id("fonts", Key::plain(Code::Tab).with(Mods::SHIFT)),
            Some("shift-tab.view")
        );
        assert_eq!(
            id("any.menu.file", Key::char('u')),
            Some("letters.layer.menu.file")
        );
        assert_eq!(id("any.menu.file", Key::char('x')), None);
        assert_eq!(
            id("any.question.confirm", Key::char('x')),
            Some("other.question")
        );
        assert_eq!(id("fonts", Key::plain(Code::F(10))), Some("f10.view"));
    }
}
