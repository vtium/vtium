//! Each key's reader and source: the crate that reads the key's value, and
//! the decision, specification or kmscon key the key comes from.
//!
//! `docs/config.md` shows both beside each key, and nothing else reads them.
//! They live in this table, apart from the key table, so that the console
//! binary carries none of this text: no code the binary runs reads
//! [`NOTES`], so the linker leaves the table and its strings out. That
//! saves about 12 KB of the console's size budget, the 4,325,376 bytes that
//! tier 3 holds the minspec binary to. Each note names its key by the key's
//! constant. [`NOTES`] holds exactly as many notes as [`KEYS`] has keys, so
//! a key added without a note fails the build, and a test fails on a note
//! that names a key twice or names no key.
//!
//! References:
//! - ADR-1200 D19: the minspec binary's size budget.

use super::*;

/// Where the keys vtium takes for itself come from (ADR-1005 D10).
const OWN_KEYS_CITE: &str = "[the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)";

/// Where the console and focus keys come from (ADR-1005 D10, ADR-1401).
const HEAD_KEYS_CITE: &str = "[the heads issue](https://github.com/vtium/vtium/issues/156); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)";

/// A key's reader and source, which `docs/config.md` shows beside the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    /// The key's name, as [`Key::name`] gives it.
    pub key: &'static str,
    /// The crate that reads the value.
    pub reader: &'static str,
    /// Where the key comes from: the decision or specification that defines
    /// it, or the released kmscon's key it came from.
    pub cite: &'static str,
}

/// The note of `key`, read by `reader` and coming from `cite`.
const fn note(key: Key, reader: &'static str, cite: &'static str) -> Note {
    Note {
        key: key.name,
        reader,
        cite,
    }
}

/// The note of each key, in the order the keys are defined in `keys.rs`.
pub static NOTES: [Note; KEYS.len()] = [
    note(CONFIGDIR, "vtium", "kmscon's `configdir` key"),
    note(HELP, "vtium", "kmscon's `help` key"),
    note(VERSION, "vtium", "kmscon's `version` key"),
    note(DEBUG, "vtium", "kmscon's `debug` key"),
    note(VT, "vtium-platform", "kmscon's `vt` key"),
    note(SWITCHVT, "vtium-platform", "kmscon's `switchvt` key"),
    note(LOGIN, "vtium", "kmscon's `login` key"),
    note(ONESHOT, "vtium", "kmscon's `oneshot` key"),
    note(
        HEAD_CONSOLES_MAX,
        "vtium",
        "[the heads issue](https://github.com/vtium/vtium/issues/156)",
    ),
    note(
        OSD,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        OSD_TIMEOUT,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        FOCUS_MARK,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        NOTICE_FOREGROUND,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        NOTICE_BACKGROUND,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        NOTICE_BORDER,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        KERNEL_MESSAGES,
        "vtium",
        "[the boot messages issue](https://github.com/vtium/vtium/issues/266)",
    ),
    note(
        OSD_FOREGROUND,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        OSD_BACKGROUND,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        OSD_BORDER,
        "vtium",
        "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
    ),
    note(
        CONSOLES_MAX,
        "vtium",
        "[the heads issue](https://github.com/vtium/vtium/issues/156)",
    ),
    note(TERM, "vtium", "kmscon's `term` key"),
    note(RESET_ENV, "vtium", "kmscon's `reset-env` key"),
    note(
        BACKSPACE_DELETE,
        "vtium-platform",
        "kmscon's `backspace-delete` key",
    ),
    note(SB_SIZE, "vtium-term", "kmscon's `sb-size` key"),
    note(
        SB_SIZE_CAP,
        "vtium-term",
        "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
    ),
    note(
        SB_MEMORY,
        "vtium-term",
        "[the scrollback memory issue](https://github.com/vtium/vtium/issues/131)",
    ),
    note(BELL, "vtium", "kmscon's `bell` key"),
    note(
        BLINK,
        "vtium",
        "kmscon's `blink` key; the maintainer's decision of 2026-11-16",
    ),
    note(
        BLINK_TEXT,
        "vtium-render",
        "[the blink options issue](https://github.com/vtium/vtium/issues/81); the maintainer's decision of 2026-11-03",
    ),
    note(
        BLINK_DECORATIONS,
        "vtium-render",
        "[the blink options issue](https://github.com/vtium/vtium/issues/81)",
    ),
    note(
        SYNC_TIMEOUT,
        "vtium",
        "[the modes issue](https://github.com/vtium/vtium/issues/12)",
    ),
    note(
        DPMS_TIMEOUT,
        "vtium-platform",
        "kmscon's `dpms-timeout` key",
    ),
    note(
        PROGRAM_BLANK,
        "vtium",
        "[the screen blanking issue](https://github.com/vtium/vtium/issues/79)",
    ),
    note(
        DISPLAY_WAIT,
        "vtium",
        "[the configuration issue](https://github.com/vtium/vtium/issues/21); [the issue about X starting during vtium's first frame](https://github.com/vtium/vtium/issues/270)",
    ),
    note(
        DISPLAY_BUFFERS,
        "vtium-platform",
        "[the displays issue](https://github.com/vtium/vtium/issues/18)",
    ),
    note(HWACCEL, "vtium", "kmscon's `hwaccel` key"),
    note(
        USE_ORIGINAL_MODE,
        "vtium-platform",
        "kmscon's `use-original-mode` key; [the native mode issue](https://github.com/vtium/vtium/issues/5)",
    ),
    note(MODE, "vtium-platform", "kmscon's `mode` key"),
    note(PALETTE, "vtium-term", "kmscon's `palette` key"),
    note(PALETTE_BLACK, "vtium-term", "kmscon's `palette-black` key"),
    note(PALETTE_RED, "vtium-term", "kmscon's `palette-red` key"),
    note(PALETTE_GREEN, "vtium-term", "kmscon's `palette-green` key"),
    note(
        PALETTE_YELLOW,
        "vtium-term",
        "kmscon's `palette-yellow` key",
    ),
    note(PALETTE_BLUE, "vtium-term", "kmscon's `palette-blue` key"),
    note(
        PALETTE_MAGENTA,
        "vtium-term",
        "kmscon's `palette-magenta` key",
    ),
    note(PALETTE_CYAN, "vtium-term", "kmscon's `palette-cyan` key"),
    note(
        PALETTE_LIGHT_GREY,
        "vtium-term",
        "kmscon's `palette-light-grey` key",
    ),
    note(
        PALETTE_DARK_GREY,
        "vtium-term",
        "kmscon's `palette-dark-grey` key",
    ),
    note(
        PALETTE_LIGHT_RED,
        "vtium-term",
        "kmscon's `palette-light-red` key",
    ),
    note(
        PALETTE_LIGHT_GREEN,
        "vtium-term",
        "kmscon's `palette-light-green` key",
    ),
    note(
        PALETTE_LIGHT_YELLOW,
        "vtium-term",
        "kmscon's `palette-light-yellow` key",
    ),
    note(
        PALETTE_LIGHT_BLUE,
        "vtium-term",
        "kmscon's `palette-light-blue` key",
    ),
    note(
        PALETTE_LIGHT_MAGENTA,
        "vtium-term",
        "kmscon's `palette-light-magenta` key",
    ),
    note(
        PALETTE_LIGHT_CYAN,
        "vtium-term",
        "kmscon's `palette-light-cyan` key",
    ),
    note(PALETTE_WHITE, "vtium-term", "kmscon's `palette-white` key"),
    note(
        PALETTE_FOREGROUND,
        "vtium-term",
        "kmscon's `palette-foreground` key",
    ),
    note(
        PALETTE_BACKGROUND,
        "vtium-term",
        "kmscon's `palette-background` key",
    ),
    note(
        UNDERLINE_COLOR,
        "vtium-render",
        "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
    ),
    note(
        STRIKE_COLOR,
        "vtium-render",
        "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
    ),
    note(
        FORCE_DECORATION_COLORS,
        "vtium-render",
        "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
    ),
    note(
        CLIPBOARD_WRITE,
        "vtium",
        "[the OSC 52 copy issue](https://github.com/vtium/vtium/issues/39)",
    ),
    note(XKB_MODEL, "vtium-platform", "kmscon's `xkb-model` key"),
    note(XKB_LAYOUT, "vtium-platform", "kmscon's `xkb-layout` key"),
    note(XKB_VARIANT, "vtium-platform", "kmscon's `xkb-variant` key"),
    note(XKB_OPTIONS, "vtium-platform", "kmscon's `xkb-options` key"),
    note(XKB_KEYMAP, "vtium-platform", "kmscon's `xkb-keymap` key"),
    note(
        XKB_COMPOSE_FILE,
        "vtium-platform",
        "kmscon's `xkb-compose-file` key",
    ),
    note(
        XKB_REPEAT_DELAY,
        "vtium-platform",
        "kmscon's `xkb-repeat-delay` key",
    ),
    note(
        XKB_REPEAT_RATE,
        "vtium-platform",
        "kmscon's `xkb-repeat-rate` key",
    ),
    note(MOUSE, "vtium-platform", "kmscon's `mouse` key"),
    note(
        NATURAL_SCROLLING,
        "vtium",
        "kmscon's `natural-scrolling` key",
    ),
    note(
        POINTER_CHAR,
        "vtium-render",
        "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
    ),
    note(
        POINTER_THEME,
        "vtium",
        "the decision on pointer pictures from a file, ADR-1309",
    ),
    note(
        POINTER_SIZE,
        "vtium",
        "the decision on pointer pictures from a file, ADR-1309",
    ),
    note(SOFT_CURSOR, "vtium", "kmscon's `soft-cursor` key"),
    note(
        POINTER_HIDE_TIMEOUT,
        "vtium-platform",
        "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
    ),
    note(
        POINTER_HIDE_WHILE_TYPING,
        "vtium",
        "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
    ),
    note(FONT_ENGINE, "vtium-font", "kmscon's `font-engine` key"),
    note(
        FONT_NAME,
        "vtium-font",
        "kmscon's `font-name` key; [the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126)",
    ),
    note(
        FONT_FRITS,
        "vtium",
        "[the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126); the maintainer's decision of 2026-11-01",
    ),
    note(
        FONT_PATHS,
        "vtium",
        "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
    ),
    note(
        FONT_HELPER,
        "vtium",
        "[the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145); the maintainer's decision of 2026-11-01",
    ),
    note(
        FONT_SIZE,
        "vtium-font",
        "kmscon's `font-size` key; vtium-font's `CELL_SIDE_MAX`; [the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145)",
    ),
    note(
        FONT_HINTING,
        "vtium-font",
        "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
    ),
    note(
        FONT_LINE_SPACING,
        "vtium-font",
        "[the line spacing issue](https://github.com/vtium/vtium/issues/277)",
    ),
    note(
        FONT_LIGATURES,
        "vtium-render",
        "[the programming ligatures issue](https://github.com/vtium/vtium/issues/23)",
    ),
    note(
        CELL,
        "vtium",
        "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
    ),
    note(
        ALLOW_FONT_OPS,
        "vtium",
        "[the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126)",
    ),
    note(
        ALLOW_WINDOW_OPS,
        "vtium",
        "[the font and window operations issue](https://github.com/vtium/vtium/issues/44)",
    ),
    note(
        ALLOW_DISPLAY_OPS,
        "vtium",
        "[the display hand-back issue](https://github.com/vtium/vtium/issues/36)",
    ),
    note(
        GLYPH_SOURCE,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_BOX,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_BLOCK,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_BRAILLE,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_SEXTANTS,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_OCTANTS,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        GLYPH_SOURCE_LEGACY,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        MISSING,
        "vtium-font",
        "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
    ),
    note(
        TEXT_DIRECTION,
        "vtium-term",
        "[the right-to-left text issue](https://github.com/vtium/vtium/issues/43)",
    ),
    note(
        CLUSTER_WIDTH,
        "vtium-term",
        "[the syllabic scripts issue](https://github.com/vtium/vtium/issues/144)",
    ),
    note(
        GLYPH_CACHE,
        "vtium-render",
        "[the CPU compositor issue](https://github.com/vtium/vtium/issues/16)",
    ),
    note(
        DISPLAY_DEPTH,
        "vtium-render",
        "[the 1-bit displays issue](https://github.com/vtium/vtium/issues/135)",
    ),
    note(
        DISPLAY_DPI,
        "vtium",
        "[the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145)",
    ),
    note(
        VTLOOK,
        "vtium",
        "[the frame filters issue](https://github.com/vtium/vtium/issues/219)",
    ),
    note(
        RENDER_TIMER,
        "vtium-render",
        "[the CPU compositor issue](https://github.com/vtium/vtium/issues/16)",
    ),
    note(
        KEY_PASTE,
        "vtium",
        "xterm's `Shift <KeyPress> Insert` translation; [the selection issue](https://github.com/vtium/vtium/issues/95)",
    ),
    note(KEY_REBOOT, "vtium", "the `grab-reboot` key that kmscon had"),
    note(
        KEY_POWER_OFF,
        "vtium",
        "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
    ),
    note(
        KEY_SUSPEND,
        "vtium",
        "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
    ),
    note(KEY_SCROLL_UP, "vtium", OWN_KEYS_CITE),
    note(KEY_SCROLL_DOWN, "vtium", OWN_KEYS_CITE),
    note(KEY_PAGE_UP, "vtium", OWN_KEYS_CITE),
    note(KEY_PAGE_DOWN, "vtium", OWN_KEYS_CITE),
    note(
        KEY_PROMPT_UP,
        "vtium",
        "[the OSC 133 prompt marks issue](https://github.com/vtium/vtium/issues/40); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)",
    ),
    note(
        KEY_PROMPT_DOWN,
        "vtium",
        "[the OSC 133 prompt marks issue](https://github.com/vtium/vtium/issues/40); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)",
    ),
    note(
        KEY_LINK_HINTS,
        "vtium",
        "[the keyboard link mode issue](https://github.com/vtium/vtium/issues/106)",
    ),
    note(KEY_ZOOM_IN, "vtium", OWN_KEYS_CITE),
    note(KEY_ZOOM_OUT, "vtium", OWN_KEYS_CITE),
    note(KEY_ROTATE_CW, "vtium", OWN_KEYS_CITE),
    note(KEY_ROTATE_CCW, "vtium", OWN_KEYS_CITE),
    note(KEY_RESET, "vtium", OWN_KEYS_CITE),
    note(KEY_CONSOLE_NEXT, "vtium", HEAD_KEYS_CITE),
    note(KEY_CONSOLE_PREVIOUS, "vtium", HEAD_KEYS_CITE),
    note(KEY_CONSOLE_OPEN, "vtium", HEAD_KEYS_CITE),
    note(KEY_CONSOLE_CLOSE, "vtium", HEAD_KEYS_CITE),
    note(KEY_FOCUS_NEXT, "vtium", HEAD_KEYS_CITE),
    note(KEY_FOCUS_PREVIOUS, "vtium", HEAD_KEYS_CITE),
    note(
        IMAGE_STORAGE_LIMIT,
        "vtium-image",
        "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); Ghostty's `image-storage-limit`",
    ),
    note(
        MAX_GRAPHIC_SIZE,
        "vtium-image",
        "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `maxGraphicSize`",
    ),
    note(
        NUM_COLOR_REGISTERS,
        "vtium-term",
        "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `numColorRegisters`",
    ),
    note(
        PRIVATE_COLOR_REGISTERS,
        "vtium-term",
        "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `privateColorRegisters`",
    ),
    note(
        IMAGE_HELPER,
        "vtium",
        "[the image helper issue](https://github.com/vtium/vtium/issues/31); the maintainer's decision of 2026-11-01",
    ),
    note(
        SIXEL,
        "vtium-term",
        "[the sixel issue](https://github.com/vtium/vtium/issues/48)",
    ),
    note(
        KITTY_GRAPHICS,
        "vtium-term",
        "[the kitty graphics issue](https://github.com/vtium/vtium/issues/49)",
    ),
    note(
        LOGIN_IMAGE,
        "vtium",
        "[the login picture issue](https://github.com/vtium/vtium/issues/63); Ghostty's `background-image`",
    ),
    note(
        LOGIN_IMAGE_FIT,
        "vtium",
        "[the login picture issue](https://github.com/vtium/vtium/issues/63); Ghostty's `background-image-fit`",
    ),
    note(
        LOGIN_IMAGE_ROWS,
        "vtium",
        "[the login picture issue](https://github.com/vtium/vtium/issues/63)",
    ),
    note(
        LOGIN_IMAGE_POSITION,
        "vtium",
        "[the login picture issue](https://github.com/vtium/vtium/issues/63)",
    ),
    note(
        LOGIN_IMAGE_SCALE,
        "vtium",
        "[the login picture issue](https://github.com/vtium/vtium/issues/63)",
    ),
];

/// The note of `key`, or `None` when [`NOTES`] has none for it.
pub fn of(key: &Key) -> Option<&'static Note> {
    NOTES.iter().find(|n| n.key == key.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every key of the table has exactly one note, and every note names a
    // key of the table, so docs/config.md shows a reader and a source for
    // each key.
    #[test]
    fn every_key_has_one_note() {
        for k in &KEYS {
            let count = NOTES.iter().filter(|n| n.key == k.name).count();
            assert_eq!(count, 1, "{} has {count} notes", k.name);
        }
        for n in &NOTES {
            assert!(
                KEYS.iter().any(|k| k.name == n.key),
                "the note of {} names no key",
                n.key
            );
            assert!(!n.reader.is_empty(), "{} has no reader", n.key);
            assert!(!n.cite.is_empty(), "{} has no source", n.key);
        }
    }
}
