//! What the Looks tab says: the tab's name, the list of sizes, the density
//! its preview uses, the caption of the console's own preview and the tab's
//! key line.

use super::*;

/// The Looks tab's name in the tab bar.
pub const LOOKS: &str = "Looks";

pub const CONSOLE_SIZE_CAPTION: &str = "These characters are drawn at the console's current size. A size you choose takes effect when vtium starts on this console again.";

/// The size that lets vtium choose.
pub const AUTOMATIC: &str = "automatic";

pub fn points(n: u32) -> String {
    format!("{n} pt")
}

pub fn density(dpi: u32, from: impl Display) -> String {
    format!("The preview uses {dpi} dpi, {from}.")
}

pub const AUTOMATIC_PICTURE: &str =
    "Automatic lets vtium pick the size for each display, so there is no preview.";

pub const DPI_FROM_CONF: &str = "set by display-dpi in vtium.conf";

pub const DPI_FROM_EDID: &str = "measured from the display";

pub const DPI_ASSUMED: &str = "assumed, because no display reports its size";

/// The Looks view, which holds the sizes until looks are built.
pub const LOOKS_SIZES: &str = "Font sizes for the console, in points.";

/// The keys, on the last line of each view.
pub const KEYS_LOOKS: &str = "Enter choose  Tab next view  i install  q quit without installing";
