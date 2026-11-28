//! The Actions menu: what acts at once on this machine or on the console,
//! outside the document, and so never waits for Save. The open tab's own
//! items follow the menu's three, after a separator.
//!
//! References:
//! - ADR-1403 D11, "The menus": Actions' items, letters and shortcuts.
//! - ADR-1206 D9: a tab adds its own items to the Actions menu.

use vtium_config_core::ui::Action;
use vtium_config_widgets::menu::Item;
use vtium_config_widgets::tab::MenuItem;

use super::{Choice, Context, Line, NOT_BUILT, Then};
use crate::nav::levels;

/// The Actions menu's lines, with `tab_items`, the open tab's own items,
/// after them.
pub fn lines(cx: &Context<'_>, tab_items: Vec<MenuItem<Action>>) -> Vec<Line> {
    // Try live acts on what the open screen highlights, through the
    // screen's own `t` row where it has one.
    let try_live = levels::row_for(cx.node, &vtium_config_widgets::keys::Key::char('t'))
        .filter(|r| r.id.starts_with("t."))
        .map_or("t.view.fonts", |r| r.id);
    // rebuild-shell-live builds the three, and until then each is greyed
    // out
    let mut lines = vec![
        Line::Choice(Choice {
            item: Item::new("Try live")
                .letter('t')
                .shortcut("t")
                .greyed(NOT_BUILT),
            then: Then::Act(Action::of(try_live)),
        }),
        Line::Choice(Choice {
            item: Item::new("Read devices again")
                .letter('r')
                .shortcut("d")
                .greyed(NOT_BUILT),
            then: Then::Act(Action::of("d.view.flow")),
        }),
        Line::Choice(Choice {
            item: Item::new("Install as a system font…")
                .letter('i')
                .greyed(NOT_BUILT),
            then: Then::Nothing,
        }),
    ];
    if !tab_items.is_empty() {
        lines.push(Line::Separator);
    }
    lines.extend(tab_items.into_iter().map(|t| {
        Line::Choice(Choice {
            item: Item::from(&t),
            then: Then::Act(t.action),
        })
    }));
    lines
}
