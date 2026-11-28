//! The inputs as the Flow page sorts them: keyboards, pointers, and the
//! other inputs, such as a power button or a lid switch, which no path can
//! take. The other inputs fold into one grey box at the end of PARTS'
//! inputs, and Enter on that box opens their list, each with the rule that
//! catches it ([`draw_others`]). This module also says which terminals'
//! paths hold a device, and words an input's port as a reader knows it.
//!
//! References:
//! - ADR-1403 D7, "Parts from the devices present": the inputs a path can
//!   take, by their class, and the box that folds the others.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::flow::{Device, Flow, Kind};
use vtium_config_widgets::canvas::fit;
use vtium_config_widgets::roles::Role;

use crate::page::draw::Sheet;
use crate::paths::{self, ANY_KEYBOARD, ANY_POINTER, Path};
use crate::words;

/// What sort of device a device is, by the classes it offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Keyboard,
    Pointer,
    /// Neither a keyboard nor a pointer: a power button, a receiver's
    /// System Control node, an audio jack.
    Other,
    /// A display.
    Screen,
}

/// The classes that make an input a pointer.
const POINTERS: [&str; 6] = [
    "class:mouse",
    "class:touchpad",
    "class:touchscreen",
    "class:trackball",
    "class:pointingstick",
    "class:tablet",
];

/// The sort of the input `dev`.
pub fn sort(dev: &Device) -> Sort {
    if dev.keys.iter().any(|k| k == "class:keyboard") {
        Sort::Keyboard
    } else if dev.keys.iter().any(|k| POINTERS.contains(&k.as_str())) {
        Sort::Pointer
    } else {
        Sort::Other
    }
}

/// An input's port in a reader's words: `usb-0:3.3:1.0` reads `USB 3.3`,
/// the keyboard controller of a laptop reads `built in`, and a port of
/// another kind is given as it is.
pub fn port_words(detail: &str) -> String {
    if let Some(at) = detail.rfind("usb-") {
        let rest = detail.get(at.saturating_add(4)..).unwrap_or_default();
        // the bus number before the first `:`, and the interface after the
        // second, say nothing a reader looks for
        let mut parts = rest.split(':');
        let first = parts.next().unwrap_or_default();
        let port = parts.next().unwrap_or(first);
        return format!("USB {port}");
    }
    if detail.contains("i8042") {
        return "built in".to_owned();
    }
    detail.to_owned()
}

/// The word the box of other inputs gives one of them: a kind of input a
/// reader knows, from the name it reports.
fn other_word(name: &str) -> String {
    let lower = name.to_lowercase();
    let word = if lower.contains("power button") {
        "power button"
    } else if lower.contains("sleep button") {
        "sleep button"
    } else if lower.contains("consumer control") || lower.contains("system control") {
        "media keys"
    } else if lower.contains("headphone") || lower.contains("mic") || lower.contains("jack") {
        "audio jacks"
    } else if lower.contains("pc speaker") {
        "PC speaker"
    } else if lower.contains("lid switch") {
        "lid switch"
    } else if lower.contains("video bus") {
        "display keys"
    } else {
        return name.to_owned();
    };
    word.to_owned()
}

/// What the box of other inputs says under its title: the kinds of input
/// it holds, each once, with the power and sleep buttons as one.
pub fn others_detail(names: &[String]) -> String {
    let mut found: Vec<String> = Vec::new();
    for n in names {
        let w = other_word(n);
        if !found.contains(&w) {
            found.push(w);
        }
    }
    let power = found.iter().position(|w| w == "power button");
    let sleep = found.iter().position(|w| w == "sleep button");
    if let (Some(p), Some(s)) = (power, sleep) {
        if let Some(w) = found.get_mut(p) {
            "power and sleep buttons".clone_into(w);
        }
        found.remove(s);
    }
    found.join(", ")
}

/// The connected inputs the box of other inputs folds, by their index in
/// the flow's inputs.
pub fn folded(flow: &Flow) -> Vec<usize> {
    (0..flow.inputs.len())
        .filter(|d| {
            flow.inputs
                .get(*d)
                .is_some_and(|x| x.found && sort(x) == Sort::Other)
        })
        .collect()
}

/// Whether the input match `sel` names the device `dev`.
pub fn names(dev: &Device, sel: &str) -> bool {
    sel == "*"
        || crate::parts::matches(dev, sel)
        || (sel.starts_with("path:") && dev.keys.iter().any(|k| k.starts_with(sel)))
        || (sel == ANY_POINTER && sort(dev) == Sort::Pointer)
        || (sel == ANY_KEYBOARD
            && dev
                .keys
                .iter()
                .any(|k| paths::expand(ANY_KEYBOARD).contains(&k.as_str())))
}

/// The terminals whose paths hold the device `dev`: for a display, those
/// whose displays match it, a glob such as `connector:*/eDP-1` included,
/// or take every display; for an input, those whose parts match it.
pub fn holders(paths: &[Path], kind: Kind, dev: &Device) -> Vec<String> {
    paths
        .iter()
        .filter(|p| match kind {
            Kind::Display => p.displays.iter().any(|s| crate::parts::matches(dev, s)),
            Kind::Input => p.inputs.iter().any(|s| names(dev, s)),
        })
        .filter_map(|p| p.name.clone())
        .collect()
}

/// The rule that catches the input `dev`, in words, or that no rule names
/// it.
fn caught(flow: &Flow, dev: &Device) -> String {
    dev.rule
        .and_then(|r| flow.rules.get(r).map(|x| (r, x)))
        .map_or_else(
            || words::NO_RULE.to_owned(),
            |(r, x)| words::caught_by(r.saturating_add(1), &x.selector, &x.target),
        )
}

/// Draws the list of the other inputs into `area`, under the screen's
/// breadcrumb: a heading with what they are, then each input's name and
/// the rule that catches it. A list longer than the area ends with a line
/// that counts the inputs it leaves out.
pub fn draw_others(flow: &Flow, buf: &mut Buffer, area: Rect, color: bool) {
    let mut s = Sheet::new(buf, area, color);
    s.crumb(words::OTHERS_CRUMB);
    let x = s.put(1, 2, words::OTHERS_TITLE, Role::Heading);
    s.put(x.saturating_add(3), 2, words::OTHERS_ABOUT, Role::Dim);
    let width = usize::from(area.width);
    // the names take a column as wide as the longest, up to half the width
    let devices: Vec<&Device> = folded(flow)
        .into_iter()
        .filter_map(|d| flow.inputs.get(d))
        .collect();
    let name_w = devices
        .iter()
        .map(|d| vtium_config_widgets::canvas::cells(&d.name))
        .max()
        .unwrap_or(0)
        .min(width / 2)
        .max(8);
    let rows = usize::from(area.height).saturating_sub(3);
    let more = devices.len() > rows;
    let shown = if more { rows.saturating_sub(1) } else { rows };
    for (i, dev) in devices.iter().take(shown).enumerate() {
        let y = 3usize.saturating_add(i);
        s.put(3, y, &fit(&dev.name, name_w), Role::Row);
        let rx = 3usize.saturating_add(name_w).saturating_add(4);
        s.put(
            rx,
            y,
            &fit(
                &caught(flow, dev),
                width.saturating_sub(rx).saturating_sub(1),
            ),
            Role::Dim,
        );
    }
    if more {
        let left = devices.len().saturating_sub(shown);
        s.put(
            3,
            3usize.saturating_add(shown),
            &words::more_others(left),
            Role::Dim,
        );
    }
}

/// The full text of the other inputs' list, which F1 opens: every input
/// with its rule, one a line.
pub fn others_text(flow: &Flow) -> String {
    let mut out = format!("{}: {}.", words::OTHERS_TITLE, words::OTHERS_ABOUT);
    for d in folded(flow) {
        if let Some(dev) = flow.inputs.get(d) {
            out.push('\n');
            out.push_str(&format!("{}: {}", dev.name, caught(flow, dev)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_reads_as_a_reader_knows_it() {
        assert_eq!(port_words("pci-0000:15:00.0-usb-0:3.3:1.0"), "USB 3.3");
        assert_eq!(port_words("isa0060/serio0 i8042"), "built in");
        assert_eq!(
            port_words("LNXPWRBN/button/input0"),
            "LNXPWRBN/button/input0"
        );
    }

    #[test]
    fn the_box_of_other_inputs_names_each_kind_once() {
        let names = [
            "Power Button".to_owned(),
            "Sleep Button".to_owned(),
            "Logitech USB Receiver Consumer Control".to_owned(),
            "Logitech USB Receiver System Control".to_owned(),
        ];
        assert_eq!(others_detail(&names), "power and sleep buttons, media keys");
    }
}
