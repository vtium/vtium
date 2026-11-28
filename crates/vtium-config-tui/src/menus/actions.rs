//! The Actions menu: what acts at once on this machine or on the console,
//! outside the document, and so never waits for Save. The open tab's own
//! items follow the menu's three, after a separator.
//!
//! Each item is greyed out with its reason where it cannot act: Try live
//! outside a vtium console, with `allow-font-ops` off, before this
//! terminal has answered, or on a screen with nothing to try; Install as
//! a system font… anywhere but on a font file of the person's own.
//!
//! References:
//! - ADR-1403 D11, "The menus": Actions' items, letters and shortcuts,
//!   each greyed out with its reason where it cannot run.
//! - ADR-1206 D9: a tab adds its own items to the Actions menu.

use vtium_config_core::ui::{Action, Live};
use vtium_config_widgets::menu::Item;
use vtium_config_widgets::tab::MenuItem;

use super::{Choice, Context, Line, Then};
use crate::live::words;
use crate::nav::levels;

/// The row of Read devices again, which the shell acts on from any tab.
pub const READ_DEVICES: &str = "d.view.flow";

/// The action of Install as a system font…, which the shell acts on with
/// the font file the open tab highlights.
pub const SYSTEM_FONT: &str = "actions.system-font";

/// The Actions menu's lines, with `tab_items`, the open tab's own items,
/// after them. `own_font` says whether the open tab highlights a font file
/// from the person's own folders, which Install as a system font… copies.
pub fn lines(cx: &Context<'_>, tab_items: Vec<MenuItem<Action>>, own_font: bool) -> Vec<Line> {
    // Try live acts on what the open screen highlights, through the
    // screen's own `t` row, and is greyed out where the screen has none or
    // this terminal cannot switch its font
    let try_row = levels::row_for(cx.node, &vtium_config_widgets::keys::Key::char('t'))
        .filter(|r| r.id.starts_with("t."))
        .map(|r| r.id);
    let try_greyed = match cx.shared.console.live {
        Live::NotVtium => Some(words::NOT_VTIUM),
        Live::OpsOff => Some(words::OPS_OFF),
        Live::Asking => Some(words::ASKING),
        Live::On => try_row.is_none().then_some(words::NOTHING_TO_TRY),
    };
    let mut try_live = Item::new("Try live").letter('t').shortcut("t");
    if let Some(why) = try_greyed {
        try_live = try_live.greyed(why);
    }
    let mut install = Item::new("Install as a system font…").letter('i');
    if !own_font {
        install = install.greyed(words::NOT_OWN);
    }
    let mut lines = vec![
        Line::Choice(Choice {
            item: try_live,
            then: try_row.map_or(Then::Nothing, |row| Then::Act(Action::of(row))),
        }),
        Line::Choice(Choice {
            item: Item::new("Read devices again").letter('r').shortcut("d"),
            then: Then::Act(Action::of(READ_DEVICES)),
        }),
        Line::Choice(Choice {
            item: install,
            then: Then::Act(Action::of(SYSTEM_FONT)),
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
