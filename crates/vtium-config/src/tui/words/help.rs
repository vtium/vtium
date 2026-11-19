//! What the Help tab says: the tab's name and the page of keys and
//! commands.

use super::*;

/// The Help tab's name in the tab bar.
pub const HELP: &str = "Help";

/// What the Help view says of each view whose node of the interface tree
/// is still planned: the node's id, the view's name, what it does and its
/// keys. The Help view writes every other view's keys from the contract.
pub const HELP_PLANNED: [(&str, &str, &str, &str); 3] = [
    (
        "looks",
        LOOKS,
        "lists the console's font sizes, automatic first, with a preview of each. Later it will manage the looks, each a font set and a size.",
        KEYS_LOOKS,
    ),
    (
        "fonts",
        FONTS,
        "lists the font sets (frits) and the built-in fonts, with any font files each is missing. It shows each as an image of the console, or with s as a preview of every character it can display. On a vtium console, t tries a font live. Press n to make a new font set.",
        KEYS_FONTS,
    ),
    (
        "effects",
        EFFECTS,
        "lists the vtlooks, color maps and shaders that a look can use, with whether each is active and passes the trust rule. Space activates or deactivates a color map or shader, and Enter opens an item's detail.",
        KEYS_EFFECTS,
    ),
];

/// What the Help view says of the Fonts view in a vtium console, which
/// previews as text (ADR-1201 D4).
pub const HELP_FONTS_LIVE: &str = "lists the font sets (frits) and the built-in fonts, with any font files each is missing. Below the list, the console draws every character of the preview in the font it uses now. Press t to try the highlighted font on the console, k to keep it, and s to save it in the draft. Press n to make a new font set.";

pub const HELP_COMMANDS: &str = "Commands, which work in a terminal of any size:";
