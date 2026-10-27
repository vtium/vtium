//! Tier 2's screens, each with the golden that judges it.
//!
//! The C reference's tier 2 (`tests/test_pixels.c` at the pinned kmscon
//! commit) drew every screen with an outline font, DejaVu Sans Mono through
//! FreeType, so its goldens judge vtium only where no outline glyph shows:
//! the two block screens, which hold only sprites and spaces. ADR-304 §6
//! re-baselines the others when the Rust font layer lands (vtium/vtium#23).
//! The screens after them are vtium's, drawn in unifont, for the harness
//! work the C reference had queued (aaronsb/kmscon#8, #10, #70, #118, #122).

use vtium_render::Orientation;
use vtium_term::WidthMethod;

use crate::draw::{FontKind, Screen};

/// Where a screen's golden lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The C reference's, `tests/goldens/tier2/<name>.ppm` at the pinned
    /// kmscon commit.
    Kmscon,
    /// vtium's own, `tools/tier2/goldens/<name>.ppm`.
    Vtium,
}

/// A screen and where its golden lives.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub screen: Screen,
    pub source: Source,
}

/// Block elements and box drawing stacked across rows and columns
/// (aaronsb/kmscon#46): the C reference's `BLOCKS_BYTES`.
const BLOCKS: &[u8] = concat!(
    "██████ ▄▄▄▄ ┌──┬──┐ ╔══╦══╗ ┏━━┳━━┓ ╭──╮\r\n",
    "██████ ████ │  │  │ ║  ║  ║ ┃  ┃  ┃ │  │\r\n",
    "██████ ▀▀▀▀ ├──┼──┤ ╠══╬══╣ ┣━━╋━━┫ ╰──╯\r\n",
    "▐▌▐▌▐▌ ▖▗▘▝ │  │  │ ║  ║  ║ ┃  ┃  ┃ ╱╲╳\r\n",
    "▐▌▐▌▐▌ ▙▛▜▟ └──┴──┘ ╚══╩══╝ ┗━━┻━━┛ ▚▞\r\n",
    "▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉█ ▔▕ ░▒▓█ ┄┈╌ ┅┉╍\r\n",
    "┆┊╎ ┇┋╏ ╞╡╟╢ ╒╕╓╖ ┍┑┎┒ ┝┥┠┨ ╼╽╾╿ ╴╵╶╷\r\n",
    "┆┊╎ ┇┋╏ ╘╛╙╜ ╪╫┿╂ ┕┙┖┚ ╀╁╃╄ ╅╆╇╈ ╉╊\r\n",
    "\x1b[33;44m██▀▄▌▐░▒▓ ┼╋╬ ╭╮╰╯\x1b[0m\r\n",
)
.as_bytes();

const BLOCKS_DESC: &str = "Block elements and box drawing stacked across rows and columns: \
full blocks, halves that meet across a row and side by side, eighths, quadrants, shades, and \
light, double, heavy, rounded, diagonal and dashed lines, then blocks and lines in yellow on \
blue. Correct: blocks fill their cells and lines run from cell to cell with no gap between \
rows or columns.";

/// Text to the right and bottom edges, coloured cells in the last column
/// and row, and a wide character whose second column the display lacks.
const EDGES: &[u8] = concat!(
    "edges\x1b[41m R\x1b[0m\x1b[42mG\x1b[0m\r\n",
    "\x1b[44m          \x1b[0m\r\n",
    "\x1b[4;9m0123456\x1b[0m\r\n",
    "\x1b[3;1H\x1b[30;43mlast\x1b[0m",
    "\x1b[1;11H\x1b[4m中\x1b[0m",
    "\x1b[2;11H\x1b[7m語\x1b[0m",
    "\x1b[4;8H\x1b[45m    \x1b[0m",
)
.as_bytes();

const EDGES_DESC: &str = "A grid of 12 by 4 cells on a display that holds 11 and a half \
columns and 4 rows and 3 pixel rows: text and coloured cells reach the last whole column and \
row, and two wide characters, one underlined and one in reverse, start in the last whole \
column. Correct: the 5 pixel columns at the right and the 3 pixel rows at the bottom in the \
default background; each wide character's left half, with its underline or reverse, in the \
last whole column, cut at the margin.";

/// The C reference's `vs16` screen.
const VS16: &[u8] = concat!(
    "a\u{2764}\u{fe0f}b  VS16, wide\r\n",
    "a\u{2764}b  no selector, narrow\r\n",
    "a\u{2764}\u{fe0e}b  VS15, narrow\r\n",
    "\x1b[4ma\u{2764}\u{fe0f}b\x1b[0m  underlined\r\n",
)
.as_bytes();

/// Text, then erases, an insert and a scroll under a pen of bold,
/// underline, strike and a red background (aaronsb/kmscon#122,
/// aaronsb/libtsm#13).
const ERASE: &[u8] = concat!(
    "line one stays as it is\r\n",
    "line two is erased from here to its end\r\n",
    "line three gets two cells inserted\r\n",
    "line four stays as it is\r\n",
    "line five is erased from here down",
    "\x1b[1;4;9;41m",
    "\x1b[2;13H\x1b[K",
    "\x1b[3;1H\x1b[2@",
    "\x1b[5;19H\x1b[J",
    "\x1b[T",
    "\x1b[0m",
)
.as_bytes();

/// The C reference's `wide-deco` screen: wide characters under each
/// underline style and strike, and one written at the last column.
const WIDE_DECO: &[u8] = concat!(
    "\x1b[4m日本語\x1b[0m \x1b[4:3m漢字\x1b[0m \x1b[4:4;58:5:196mカタカナ\x1b[0m\r\n",
    "\x1b[4:2m한글\x1b[0m \x1b[4:5m中文\x1b[0m \x1b[9m日本語\x1b[0m\r\n",
    "\x1b[4ma日b本c\x1b[0m\r\n",
    "\x1b[5;48H\x1b[4;9m語\x1b[0m",
)
.as_bytes();

/// The geometry screen in `orientation`, named `name`.
const fn edges(name: &'static str, orientation: Orientation) -> Case {
    Case {
        screen: Screen {
            orientation,
            display: Some((11 * 8 + 5, 4 * 16 + 3)),
            ..Screen::new(name, EDGES_DESC, EDGES, 12, 4)
        },
        source: Source::Vtium,
    }
}

/// Every case, in the order the report lists them.
pub const CASES: &[Case] = &[
    Case {
        screen: Screen {
            font: FontKind::Sprites {
                width: 9,
                height: 16,
            },
            ..Screen::new("blocks", BLOCKS_DESC, BLOCKS, 48, 10)
        },
        source: Source::Kmscon,
    },
    // DejaVu Sans Mono at 22 pixels, its line height and 6 pixel rows more
    Case {
        screen: Screen {
            font: FontKind::Sprites {
                width: 12,
                height: 28,
            },
            ..Screen::new("blocks-spaced", BLOCKS_DESC, BLOCKS, 48, 10)
        },
        source: Source::Kmscon,
    },
    edges("edges", Orientation::Normal),
    edges("edges-right", Orientation::Right),
    edges("edges-upside-down", Orientation::UpsideDown),
    edges("edges-left", Orientation::Left),
    Case {
        screen: Screen::new(
            "erase-pen",
            "Five lines of text, then, under a pen of bold, underline, strike and a red \
             background: an erase from the middle of line 2 to its end, two cells inserted at \
             the start of line 3, an erase from the middle of line 5 to the end of the screen, \
             and a scroll down of one line. Correct: every erased, inserted and scrolled-in \
             cell plain red, with no underline or strike; the top row red, each line a row \
             lower, and line 3 two cells to the right.",
            ERASE,
            40,
            6,
        ),
        source: Source::Vtium,
    },
    Case {
        screen: Screen::new(
            "vs16-unifont",
            "A heart with VS16, with no selector and with VS15, and the VS16 heart \
             underlined, by the Unicode width method (mode 2027 set). Correct: the VS16 heart \
             takes two cells, the text heart centred in them and the b after it a column \
             further right; the other hearts one cell each.",
            VS16,
            32,
            5,
        ),
        source: Source::Vtium,
    },
    Case {
        screen: Screen {
            width_method: WidthMethod::Wcwidth,
            ..Screen::new(
                "vs16-wcwidth",
                "The vs16-unifont screen by wcwidth (grapheme-width=wcwidth, mode 2027 reset). \
                 Correct: every heart one cell, the VS16 one included, so each b sits right \
                 after its heart.",
                VS16,
                32,
                5,
            )
        },
        source: Source::Vtium,
    },
    Case {
        screen: Screen::new(
            "wide-deco-unifont",
            "Wide characters under each underline style and under strike, a decorated run \
             mixing narrow and wide, and a decorated wide character written at the last \
             column, which wraps to the start of the next row (aaronsb/kmscon#70). Correct: \
             every decoration spans both cells of its wide character.",
            WIDE_DECO,
            48,
            10,
        ),
        source: Source::Vtium,
    },
    Case {
        screen: Screen {
            next: Some(
                b"\x1b[2;1Hline 2 CHANGED \x1b[4:3mcurly\x1b[0m\x1b[4;1H\x1b[9mnew struck\x1b[0m",
            ),
            ..Screen::new(
                "redraw-unifont",
                "Four frames into a double-buffered display: the second part rewrites line 2 \
                 and adds a struck line 4, and both buffers must then hold it and equal a \
                 fresh draw. Correct: line 2 reads 'line 2 CHANGED curly' with nothing left of \
                 the first part's text.",
                b"unchanged line\r\nline two, before\r\nline three stays\r\n",
                48,
                10,
            )
        },
        source: Source::Vtium,
    },
];
