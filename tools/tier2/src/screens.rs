//! Tier 2's screens, each with the golden that judges it.
//!
//! The C reference's tier 2 (`tests/test_pixels.c` at the kmscon commit
//! `provenance.tsv` names) drew every screen with an outline font, DejaVu Sans Mono through
//! FreeType, so its goldens judge vtium only where no outline glyph shows:
//! the two block screens, which hold only sprites and spaces. The others
//! are re-baselined when the Rust font layer lands (vtium/vtium#24), as
//! kmscon's ADR-304 §6 decides (adr-cite-ignore: kmscon's record).
//! The screens after them are vtium's, drawn in unifont, for the harness
//! work the C reference had queued (aaronsb/kmscon#8, #10, #70, #118, #122),
//! then color glyphs from an RGBA strike of a vitrified set built in memory
//! (vtium/vtium#23), and last the small displays in the PSF fonts
//! `tools/testfont` writes (vtium/vtium#75), and vtium's own drawing over
//! a frame, its notices and on-screen display, at two display sizes
//! (ADR-1401 D5, D7).

use vtium_render::{Depth, Mark, Orientation};
use vtium_term::WidthMethod;

use vtium::config::keys::{SCHEMES, Scheme};

use crate::draw::{DecoSpec, Decorations, FontKind, Overlay, Screen, TestFont, Words};
use crate::notices;

/// Where a screen's golden lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The C reference's, copied from its `tests/goldens/tier2/` into
    /// `tools/tier2/reference/goldens/<name>.ppm`.
    Kmscon,
    /// vtium's own, `tools/tier2/goldens/<name>.ppm`.
    Vtium,
}

/// A screen and where its golden lives.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub screen: Screen,
    pub source: Source,
    /// The C reference's screen in `tests/test_pixels.c` whose bytes this
    /// one carries, checked against the copy on every run.
    pub ported: Option<&'static str>,
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

/// Text to the right and bottom edges, colored cells in the last column
/// and row, and a wide character whose second column the display lacks.
const EDGES: &[u8] = concat!(
    "edges\x1b[41m R\x1b[0m\x1b[42mG\x1b[0m\r\n",
    "\x1b[44m          \x1b[0m\r\n",
    "\x1b[4;9m0123456\x1b[0m\r\n",
    "\x1b[3;1H\x1b[30;43mlast\x1b[0m",
    "\x1b[1;11H\x1b[4m中\x1b[0m",
    "\x1b[3;11H\x1b[7m語\x1b[0m",
    "\x1b[4;8H\x1b[45m    \x1b[0m",
)
.as_bytes();

const EDGES_DESC: &str = "A grid of 12 by 4 cells on a display that holds 11 and a half \
columns and 4 rows and 3 pixel rows: text and colored cells reach the last whole column and \
row, and two wide characters start in the last whole column, an underlined one in the first \
row and a reversed one in the third, with a cell of the default background between them. \
Correct: the grid centred, with the 5 spare pixel columns and 3 spare pixel rows around it in \
the default background; each wide character's left half, with its underline or reverse, in \
the last whole column, cut at the margin";

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

/// The C reference's `ascii` screen.
const ASCII: &[u8] = concat!(
    "The quick brown fox jumps over the lazy dog.\r\n",
    "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG.\r\n",
    "0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~\r\n",
    "iiiiiiii llllllll WWWWWWWW mmmmmmmm\r\n",
    "func main() { return 0; }  // 80x24 \r\n",
)
.as_bytes();

/// The C reference's `bold` screen.
const BOLD: &[u8] = concat!(
    "regular \x1b[1mbold\x1b[0m regular\r\n",
    "\x1b[1mThe quick brown fox jumps over\r\n",
    "the lazy dog 0123456789 !@#$%^&*()\x1b[0m\r\n",
    "\x1b[1;31mbold red\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[1;34mbold blue\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `palette` screen.
const PALETTE: &[u8] = concat!(
    "\x1b[40m  \x1b[41m  \x1b[42m  \x1b[43m  \x1b[44m  \x1b[45m  \x1b[46m  \x1b[47m  \x1b[100m  \x1b[101m  \x1b[102m  \x1b[103m  \x1b[104m  \x1b[105m  \x1b[106m  \x1b[107m  \x1b[0m\r\n",
    "\x1b[30mAB\x1b[31mAB\x1b[32mAB\x1b[33mAB\x1b[34mAB\x1b[35mAB\x1b[36mAB\x1b[37mAB\x1b[90mAB\x1b[91mAB\x1b[92mAB\x1b[93mAB\x1b[94mAB\x1b[95mAB\x1b[96mAB\x1b[97mAB\x1b[0m\r\n",
    "\x1b[48;5;16m  \x1b[48;5;52m  \x1b[48;5;88m  \x1b[48;5;124m  \x1b[48;5;160m  \x1b[48;5;196m  \x1b[48;5;202m  \x1b[48;5;214m  \x1b[48;5;226m  \x1b[48;5;154m  \x1b[48;5;46m  \x1b[48;5;48m  \x1b[48;5;51m  \x1b[48;5;27m  \x1b[48;5;21m  \x1b[48;5;129m  \x1b[48;5;232m  \x1b[48;5;244m  \x1b[48;5;255m  \x1b[0m\r\n",
    "\x1b[48;2;255;0;0m  \x1b[48;2;223;32;0m  \x1b[48;2;191;64;0m  \x1b[48;2;159;96;0m  \x1b[48;2;127;128;0m  \x1b[48;2;95;160;0m  \x1b[48;2;63;192;0m  \x1b[48;2;31;224;0m  \x1b[48;2;0;255;0m  \x1b[0m\r\n",
).as_bytes();

/// The C reference's `inverse` screen.
const INVERSE: &[u8] = concat!(
    "normal \x1b[7minverse\x1b[0m normal\r\n",
    "\x1b[7mWhole line inverted\x1b[0m\r\n",
    "\x1b[31;42mred on green \x1b[7mswapped\x1b[0m\r\n",
    "\x1b[1;7mbold inverse\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `cjk` screen.
const CJK: &[u8] = concat!(
    "日本語の端末表示 テスト\r\n",
    "中文字符 漢字 ひらがな カタカナ\r\n",
    "한글 테스트\r\n",
    "ＡＢＣＤ０１２３ abcd 0123\r\n",
    "a日b本c語d\r\n",
    "\x1b[1m日本語 bold\x1b[0m \x1b[7m日本語 inverse\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `box` screen.
const BOX: &[u8] = concat!(
    "┌──────┬──────┐ ╔══════╦══════╗\r\n",
    "│ cell │ cell │ ║ cell ║ cell ║\r\n",
    "├──────┼──────┤ ╠══════╬══════╣\r\n",
    "│      │      │ ║      ║      ║\r\n",
    "└──────┴──────┘ ╚══════╩══════╝\r\n",
    "░░▒▒▓▓██ ▀▄▌▐ ▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉█\r\n",
    "╭─╮ ╰─╯ ┏━┓ ┗━┛ ┃ ╱╲╳\r\n",
)
.as_bytes();

/// The C reference's `underline` screen.
const UNDERLINE: &[u8] = concat!(
    "\x1b[4mSingle gjpqy\x1b[0m  \x1b[4;58:5:196mSingle gjpqy\x1b[0m\r\n",
    "\x1b[4:2mDouble gjpqy\x1b[0m  \x1b[4:2;58:2::0:200:255mDouble gjpqy\x1b[0m\r\n",
    "\x1b[4:3mCurly  gjpqy\x1b[0m  \x1b[4:3;58:5:46mCurly  gjpqy\x1b[0m\r\n",
    "\x1b[4:4mDotted gjpqy\x1b[0m  \x1b[4:4;58:2::255:128:0mDotted gjpqy\x1b[0m\r\n",
    "\x1b[4:5mDashed gjpqy\x1b[0m  \x1b[4:5;58:5:201mDashed gjpqy\x1b[0m\r\n",
    "\x1b[31;4:3;58:5:226mred, yellow curl\x1b[59m then fg\x1b[0m\r\n",
    "\x1b[44;4mon blue\x1b[0m \x1b[7;4minverse\x1b[0m \x1b[1;4:2mbold double\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `strike` screen.
const STRIKE: &[u8] = concat!(
    "\x1b[9mstruck through gjpqy\x1b[0m plain\r\n",
    "\x1b[9;31mred strike\x1b[0m \x1b[9;42mstrike on green\x1b[0m\r\n",
    "\x1b[9;4mstrike and underline\x1b[0m \x1b[9;4:3;58:5:51mcurly too\x1b[0m\r\n",
    "\x1b[1;9mbold struck\x1b[0m \x1b[7;9minverse struck\x1b[0m\r\n",
    "\x1b[9m      \x1b[0m struck spaces\r\n",
)
.as_bytes();

/// The C reference's `deco-config` screen.
/// Also the `deco-force` screen's.
const DECO_CONFIG: &[u8] = concat!(
    "\x1b[4mconfigured underline\x1b[0m \x1b[9mconfigured strike\x1b[0m\r\n",
    "\x1b[4:3;58:5:46mSGR 58 green\x1b[0m \x1b[4:4;58:2::255:255:0mSGR 58 yellow\x1b[0m\r\n",
    "\x1b[32;4mgreen text\x1b[0m \x1b[33;9mstruck yellow text\x1b[0m\r\n",
    "\x1b[4:2;9mdouble and strike\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `combining` screen.
const COMBINING: &[u8] = concat!(
    "precomposed: é ã ñ\r\n",
    "decomposed:  e\u{301} a\u{303}\u{300} n\u{308}\r\n",
    "in words: cafe\u{301} na\u{303}\u{300}o n\u{308}ice\r\n",
    "\x1b[4mu\u{308}nderlined e\u{301}\x1b[0m \x1b[9mstruck a\u{303}\u{300}\x1b[0m\r\n",
    "bold: \x1b[1me\u{301} a\u{303}\u{300} n\u{308}\x1b[0m\r\n",
)
.as_bytes();

/// The C reference's `mosaic` screen.
const MOSAIC: &[u8] = concat!(
    "braille ⠀⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟⠠⠡⠢⠣⠤⠥⠦⠧⠨⠩⠪⠫⠬⠭⠮⠯⠰⠱⠲⠳⠴⠵⠶⠷⠸⠹⠺⠻⠼⠽⠾⠿\r\n",
    "        ⡀⡁⡂⡃⡄⡅⡆⡇⡈⡉⡊⡋⡌⡍⡎⡏⡐⡑⡒⡓⡔⡕⡖⡗⡘⡙⡚⡛⡜⡝⡞⡟⡠⡡⡢⡣⡤⡥⡦⡧⡨⡩⡪⡫⡬⡭⡮⡯⡰⡱⡲⡳⡴⡵⡶⡷⡸⡹⡺⡻⡼⡽⡾⡿\r\n",
    "        ⢀⢁⢂⢃⢄⢅⢆⢇⢈⢉⢊⢋⢌⢍⢎⢏⢐⢑⢒⢓⢔⢕⢖⢗⢘⢙⢚⢛⢜⢝⢞⢟⢠⢡⢢⢣⢤⢥⢦⢧⢨⢩⢪⢫⢬⢭⢮⢯⢰⢱⢲⢳⢴⢵⢶⢷⢸⢹⢺⢻⢼⢽⢾⢿\r\n",
    "        ⣀⣁⣂⣃⣄⣅⣆⣇⣈⣉⣊⣋⣌⣍⣎⣏⣐⣑⣒⣓⣔⣕⣖⣗⣘⣙⣚⣛⣜⣝⣞⣟⣠⣡⣢⣣⣤⣥⣦⣧⣨⣩⣪⣫⣬⣭⣮⣯⣰⣱⣲⣳⣴⣵⣶⣷⣸⣹⣺⣻⣼⣽⣾⣿\r\n",
    "sextants🬀🬁🬂🬃🬄🬅🬆🬇🬈🬉🬊🬋🬌🬍🬎🬏🬐🬑🬒🬓🬔🬕🬖🬗🬘🬙🬚🬛🬜🬝🬞🬟🬠🬡🬢🬣🬤🬥🬦🬧🬨🬩🬪🬫🬬🬭🬮🬯🬰🬱🬲🬳🬴🬵🬶🬷🬸🬹🬺🬻\r\n",
    "octants 𜴀𜴁𜴂𜴃𜴄𜴅𜴆𜴇𜴈𜴉𜴊𜴋𜴌𜴍𜴎𜴏𜴐𜴑𜴒𜴓𜴔𜴕𜴖𜴗𜴘𜴙𜴚𜴛𜴜𜴝𜴞𜴟𜴠𜴡𜴢𜴣𜴤𜴥𜴦𜴧𜴨𜴩𜴪𜴫𜴬𜴭𜴮𜴯𜴰𜴱𜴲𜴳𜴴𜴵𜴶𜴷𜴸𜴹𜴺𜴻𜴼𜴽𜴾𜴿\r\n",
    "        𜵀𜵁𜵂𜵃𜵄𜵅𜵆𜵇𜵈𜵉𜵊𜵋𜵌𜵍𜵎𜵏𜵐𜵑𜵒𜵓𜵔𜵕𜵖𜵗𜵘𜵙𜵚𜵛𜵜𜵝𜵞𜵟𜵠𜵡𜵢𜵣𜵤𜵥𜵦𜵧𜵨𜵩𜵪𜵫𜵬𜵭𜵮𜵯𜵰𜵱𜵲𜵳𜵴𜵵𜵶𜵷𜵸𜵹𜵺𜵻𜵼𜵽𜵾𜵿\r\n",
    "        𜶀𜶁𜶂𜶃𜶄𜶅𜶆𜶇𜶈𜶉𜶊𜶋𜶌𜶍𜶎𜶏𜶐𜶑𜶒𜶓𜶔𜶕𜶖𜶗𜶘𜶙𜶚𜶛𜶜𜶝𜶞𜶟𜶠𜶡𜶢𜶣𜶤𜶥𜶦𜶧𜶨𜶩𜶪𜶫𜶬𜶭𜶮𜶯𜶰𜶱𜶲𜶳𜶴𜶵𜶶𜶷𜶸𜶹𜶺𜶻𜶼𜶽𜶾𜶿\r\n",
    "        𜷀𜷁𜷂𜷃𜷄𜷅𜷆𜷇𜷈𜷉𜷊𜷋𜷌𜷍𜷎𜷏𜷐𜷑𜷒𜷓𜷔𜷕𜷖𜷗𜷘𜷙𜷚𜷛𜷜𜷝𜷞𜷟𜷠𜷡𜷢𜷣𜷤𜷥\r\n",
    "\r\n",
    "🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠀⡠⠔⠉⠉⠑⠤⡀⠀⠀⠀⠀⠀⠀⠀⠀ 🬭🬭🬭🬭 𜷋𜷋𜷋𜷋\r\n",
    "🬤🬤🬤🬤🬤🬤🬤🬤 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠜⠀⠀⠀⠀⠀⠀⠈⢢⠀⠀⠀⠀⠀⠀⢀ 🬰🬰🬰🬰 𜷒𜷒𜷒𜷒\r\n",
    "🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿ ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠑⠢⣀⣀⡠⠒⠁ 🬂🬂🬂🬂 𜴂𜴂𜴂𜴂\r\n",
    "\r\n",
    "\x1b[33;44m🬗🬗🬗🬗🬗🬗🬗🬗 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿\r\n",
    "🬤🬤🬤🬤🬤🬤🬤🬤 𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉 ⣿⣿⣿⣿⣿⣿⣿⣿\x1b[0m",
)
.as_bytes();

/// The C reference's `emoji` screen.
const EMOJI: &[u8] = concat!(
    "😀 👍 a☺b  wide, wide, narrow text\r\n",
    "👨\u{200d}👩\u{200d}👧 👍🏽 🇯🇵 1\u{fe0f}\u{20e3}  family, tone, flag, keycap\r\n",
    "❤\u{fe0f} ❤\u{fe0e} ❤  VS16, VS15, none\r\n",
    "a😀b  between letters\r\n",
    "\x1b[4m😀\x1b[0m \x1b[9m😀\x1b[0m \x1b[7m😀\x1b[0m \x1b[4:3;58:5:196m👍🏽\x1b[0m  under, strike, reverse, curl\r\n",
    "🫨  missing\r\n",
    "\x1b[7;48H😀",
).as_bytes();

/// Color glyphs from an RGBA strike (vtium/vtium#23): beside text, on
/// colored backgrounds, reversed, under each decoration, in the narrow
/// presentation, one the set lacks, and one written at the last column.
const EMOJI_RGBA: &[u8] = concat!(
    "a😀b🚀c❤\u{fe0f}d \x1b[1m😀\x1b[0m  text beside color, bold\r\n",
    "\x1b[44m 😀 🚀 ❤\u{fe0f} \x1b[43m 😀 🚀 \x1b[48;5;28m 😀 \x1b[0m on blue, yellow, green\r\n",
    "\x1b[7m 😀 🚀 ❤\u{fe0f} \x1b[0m \x1b[7;34;43m 😀 \x1b[0m  reversed, blue on yellow\r\n",
    "\x1b[4m😀🚀❤\u{fe0f}\x1b[0m \x1b[4:3;58:5:196m😀🚀\x1b[0m \x1b[4:2;44m😀\x1b[0m ",
    "\x1b[4;58;2;0;255;0m🚀\x1b[0m  underlines\r\n",
    "\x1b[9m😀🚀❤\u{fe0f}\x1b[0m \x1b[9;31m😀\x1b[0m \x1b[9;4;41m🚀\x1b[0m \x1b[7;9m😀\x1b[0m  struck\r\n",
    "a❤b a❤\u{fe0e}b a❤\u{fe0f}b \x1b[41ma❤b\x1b[0m  none, VS15, VS16\r\n",
    "lacks \u{1f44d}, text \x1b[32mgreen\x1b[0m \x1b[1;34mbold blue\x1b[0m\r\n",
    "the grin wraps from the last column",
    "\x1b[8;48H😀",
)
.as_bytes();

/// The 320x240 floor (vtium/vtium#75): a full-width title, a shell session
/// with a line that wraps, a box of sprites around styled words, the 16
/// background and foreground colors, block, shade, Braille and sextant
/// sprites, Latin-1 and punctuation the font holds, two characters it
/// lacks, the underline styles, text ending in the last column, and an
/// editor's status line across the last row.
const SMALL_40: &[u8] = concat!(
    "\x1b[7m 320x240, 8x8 PSF2 font: 40x30 cells    \x1b[0m\r\n",
    "login: pi\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ ls\r\n",
    "\x1b[1;34mbin\x1b[0m  \x1b[1;32mrun.sh\x1b[0m  notes.txt  \x1b[36mlib.so\x1b[0m\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ echo a line past 40 cells wraps\r\n",
    "a line past 40 cells wraps\r\n",
    "\r\n",
    "┌──────────┬─────────┐\r\n",
    "│ \x1b[1mbold\x1b[0m     │ \x1b[4munder\x1b[0m   │\r\n",
    "├──────────┼─────────┤\r\n",
    "│ \x1b[9mstruck\x1b[0m   │ \x1b[7mreverse\x1b[0m │\r\n",
    "└──────────┴─────────┘\r\n",
    "\x1b[40m  \x1b[41m  \x1b[42m  \x1b[43m  \x1b[44m  \x1b[45m  \x1b[46m  \x1b[47m  ",
    "\x1b[100m  \x1b[101m  \x1b[102m  \x1b[103m  \x1b[104m  \x1b[105m  \x1b[106m  \x1b[107m  \x1b[0m\r\n",
    "\x1b[30;47mAB\x1b[31;49mAB\x1b[32mAB\x1b[33mAB\x1b[34mAB\x1b[35mAB\x1b[36mAB\x1b[37mAB",
    "\x1b[90mAB\x1b[91mAB\x1b[92mAB\x1b[93mAB\x1b[94mAB\x1b[95mAB\x1b[96mAB\x1b[97mAB\x1b[0m\r\n",
    "░▒▓█ ▀▄▌▐ ▁▂▃▄▅▆▇█ ⠿⣿⡇ 🬗🬤🬗🬤\r\n",
    "café naïve £5 ½ ¿¡ ©® «» ×÷ ÆØÅ\r\n",
    "“quotes” – — … • € ←↑→↓\r\n",
    "lacks: 中 wide, \u{2603} narrow\r\n",
    "\x1b[4:3mcurly\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:4mdots\x1b[0m \x1b[4:5mdash\x1b[0m ",
    "\x1b[4;58:5:196mred\x1b[0m\r\n",
    "\x1b[4:3;58:5:196mcurl\x1b[0m \x1b[4:2;58:5:46mdouble\x1b[0m \x1b[4:4;58:5:226mdots\x1b[0m ",
    "\x1b[4:5;58:5:201mdash\x1b[0m \x1b[9;31mstruck\x1b[0m \x1b[9;4;36;58:5:226mboth\x1b[0m\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ vi notes.txt",
    "\x1b[28;31H\x1b[42mright edge\x1b[0m",
    "\x1b[30;1H\x1b[7m\"notes.txt\" 3L, 42B            1,1   All\x1b[0m",
)
.as_bytes();

/// The 640x480 display: the 320x240 screen's parts at 80 columns, with the
/// 256-color palette's ramps, and a line past 80 cells that wraps.
const SMALL_80: &[u8] = concat!(
    "\x1b[7m 640x480, 8x16 PSF1 font: 80x30 cells, where everything vtium supports is usable\x1b[0m\r\n",
    "login: pi\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ ls -l\r\n",
    "total 3\r\n",
    "drwxr-xr-x 2 pi pi 4096 Oct 27 12:00 \x1b[1;34mbin\x1b[0m\r\n",
    "-rwxr-xr-x 1 pi pi  210 Oct 27 12:00 \x1b[1;32mrun.sh\x1b[0m\r\n",
    "-rw-r--r-- 1 pi pi   42 Oct 27 12:00 notes.txt\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ echo a line longer than eighty cells wraps to the next row, as a shell's does\r\n",
    "a line longer than eighty cells wraps to the next row, as a shell's does\r\n",
    "\r\n",
    "┌────────────┬────────────┬────────────┬────────────┐ ╔══════╗ ┏━━━━━━┓ ╭──────╮\r\n",
    "│ \x1b[1mbold\x1b[0m       │ \x1b[4munderline\x1b[0m  │ \x1b[9mstruck\x1b[0m     │ \x1b[7mreverse\x1b[0m    │ ║ \x1b[3mital\x1b[0m ║ ┃ \x1b[2mdim\x1b[0m  ┃ │ \x1b[5mblink\x1b[0m│\r\n",
    "└────────────┴────────────┴────────────┴────────────┘ ╚══════╝ ┗━━━━━━┛ ╰──────╯\r\n",
    "\x1b[40m  \x1b[41m  \x1b[42m  \x1b[43m  \x1b[44m  \x1b[45m  \x1b[46m  \x1b[47m  ",
    "\x1b[100m  \x1b[101m  \x1b[102m  \x1b[103m  \x1b[104m  \x1b[105m  \x1b[106m  \x1b[107m  \x1b[0m ",
    "\x1b[30;47mAB\x1b[31;49mAB\x1b[32mAB\x1b[33mAB\x1b[34mAB\x1b[35mAB\x1b[36mAB\x1b[37mAB",
    "\x1b[90mAB\x1b[91mAB\x1b[92mAB\x1b[93mAB\x1b[94mAB\x1b[95mAB\x1b[96mAB\x1b[97mAB\x1b[0m\r\n",
    "\x1b[48;5;16m  \x1b[48;5;52m  \x1b[48;5;88m  \x1b[48;5;124m  \x1b[48;5;160m  \x1b[48;5;196m  \x1b[48;5;202m  \x1b[48;5;214m  \x1b[48;5;226m  \x1b[48;5;154m  ",
    "\x1b[48;5;46m  \x1b[48;5;48m  \x1b[48;5;51m  \x1b[48;5;27m  \x1b[48;5;21m  \x1b[48;5;129m  \x1b[48;5;232m  \x1b[48;5;244m  \x1b[48;5;255m  \x1b[0m ",
    "\x1b[48;2;255;0;0m  \x1b[48;2;191;64;0m  \x1b[48;2;127;128;0m  \x1b[48;2;63;192;0m  \x1b[48;2;0;255;0m  \x1b[0m\r\n",
    "░▒▓█ ▀▄▌▐ ▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉ ⠁⠃⠇⡇⣇⣧⣷⣿ 🬀🬁🬂🬃🬄🬅 ▖▗▘▝▙▛▜▟\r\n",
    "café naïve £5 ½ ¿¡ ©® «» ×÷ ÆØÅ àéîõü ÀÉÎÕÜ ß µ ¶ § ° ±\r\n",
    "“quotes” ‘single’ – en — em … • €12 ←↑→↓ \u{fffd}\r\n",
    "the font lacks: 中文 wide, \u{2603}\u{266a} narrow, a\u{301} combined\r\n",
    "\x1b[4mone\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdotted\x1b[0m \x1b[4:5mdashed\x1b[0m ",
    "\x1b[4:3;58:5:196mred curl\x1b[0m \x1b[9;31mred strike\x1b[0m \x1b[1;4mbold under\x1b[0m\r\n",
    "\x1b[4;58:5:46mgreen under\x1b[0m \x1b[4:2;58:5:226myellow double\x1b[0m \x1b[4:4;58:5:201mmagenta dots\x1b[0m ",
    "\x1b[4:5;58:5:51mcyan dash\x1b[0m \x1b[9;32mgreen strike\x1b[0m \x1b[9;4:3;34;58:5:196mblue, red curl\x1b[0m\r\n",
    "\x1b[1;32mpi@sbc\x1b[0m:\x1b[1;34m~\x1b[0m$ vi notes.txt",
    "\x1b[28;71H\x1b[42mright edge\x1b[0m",
    "\x1b[30;1H\x1b[7m\"notes.txt\" 3L, 42B                                            1,1           All\x1b[0m",
)
.as_bytes();

/// The C reference's `deco-config` settings: `underline-color=196`,
/// `strike-color=#00c0ff`.
const DECO_CONF: Decorations = Decorations {
    underline: Some(DecoSpec::Index(196)),
    strike: Some(DecoSpec::Rgb(0x00, 0xc0, 0xff)),
    force: false,
};

/// The geometry screen in `orientation`, named `name`.
const fn edges(name: &'static str, orientation: Orientation) -> Case {
    Case {
        screen: Screen {
            orientation,
            display: Some((11 * 8 + 5, 4 * 16 + 3)),
            ..Screen::new(name, EDGES_DESC, EDGES, 12, 4)
        },
        source: Source::Vtium,
        ported: None,
    }
}

/// A light theme set by OSC 10 and 11, for a 1-bit display: default
/// text, a red word, colored and reversed cells, decorations and sprites
/// (vtium/vtium#135).
const LIGHT: &[u8] = concat!(
    "\x1b]10;rgb:20/20/20\x1b\\\x1b]11;rgb:f0/f0/f0\x1b\\",
    "default text on a light background\r\n",
    "\x1b[31mred\x1b[0m \x1b[44mon blue\x1b[0m \x1b[7mreversed\x1b[0m\r\n",
    "\x1b[4munder\x1b[0m \x1b[9mstruck\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdots\x1b[0m\r\n",
    "\x1b[47mon grey\x1b[0m \x1b[40mon black\x1b[0m \x1b[30;107mblack on white\x1b[0m\r\n",
    "\u{2591}\u{2592}\u{2593}\u{2588} \u{2500}\u{2500}\u{252c}\u{2500}\u{2500} \u{2502}ok\u{2502}",
)
.as_bytes();

/// A shell session beneath the on-screen display (ADR-1401 D5,
/// vtium/vtium#169).
const OSD_SHELL: &[u8] = concat!(
    "desk login: pi\r\n",
    "Password:\r\n",
    "pi@desk:~$ ls\r\n",
    "\x1b[1;34mbin\x1b[0m  notes.txt  \x1b[1;32mrun.sh\x1b[0m\r\n",
    "pi@desk:~$ ",
)
.as_bytes();

/// The same shell session on a light terminal, OSC 11's white behind
/// OSC 10's dark grey, for the light color scheme (ADR-1401 D5).
const OSD_SHELL_LIGHT: &[u8] = concat!(
    "\x1b]10;rgb:1f/29/33\x1b\\\x1b]11;rgb:ff/ff/ff\x1b\\",
    "desk login: pi\r\n",
    "Password:\r\n",
    "pi@desk:~$ ls\r\n",
    "\x1b[1;34mbin\x1b[0m  notes.txt  \x1b[1;32mrun.sh\x1b[0m\r\n",
    "pi@desk:~$ ",
)
.as_bytes();

/// A longer shell session for the overlay screens at their two display
/// sizes, so text shows beneath the tallest box and below it.
const OVERLAY_SHELL: &[u8] = concat!(
    "desk login: pi\r\n",
    "Password:\r\n",
    "Last login: Fri Oct 30 21:14:02 on tty2\r\n",
    "pi@desk:~$ ls\r\n",
    "\x1b[1;34mbin\x1b[0m  notes.txt  \x1b[1;32mrun.sh\x1b[0m\r\n",
    "pi@desk:~$ cat notes.txt\r\n",
    "buy milk\r\n",
    "call the plumber about the kitchen tap\r\n",
    "\x1b[31mback up the laptop\x1b[0m before friday\r\n",
    "pi@desk:~$ uname -sr\r\n",
    "Linux 7.2.11-arch1-1\r\n",
    "pi@desk:~$ ",
)
.as_bytes();

/// The overlay screens' two displays (ADR-1401 D5, D7): the operator's
/// small panel, 800x480 in unifont's 8x16 cells, 100 by 30; and a large
/// display in the 24x54 cell of the operator's 7680x2160 one, as its top
/// band where the bars are: 3840x540, 160 by 10 cells, which keeps a large
/// golden small. Each glyph is unifont's tripled on its cell's bottom rows,
/// as vtium-vitrify places a bitmap font in a larger cell.
const fn small(name: &'static str, desc: &'static str, overlay: Overlay) -> Case {
    Case {
        screen: Screen {
            overlay: Some(overlay),
            ..Screen::new(name, desc, OVERLAY_SHELL, 100, 30)
        },
        source: Source::Vtium,
        ported: None,
    }
}

const fn large(name: &'static str, desc: &'static str, overlay: Overlay) -> Case {
    Case {
        screen: Screen {
            font: FontKind::Frit {
                builtin: "unifont",
                width: 24,
                height: 54,
            },
            overlay: Some(overlay),
            ..Screen::new(name, desc, OVERLAY_SHELL, 160, 10)
        },
        source: Source::Vtium,
        ported: None,
    }
}

/// One of the example configuration's other color schemes (ADR-1401 D5)
/// on `overlay`, over the shell session `bytes` in unifont, 100 by 8
/// cells: the width of the operator's small panel and the top of it,
/// where the bars are.
const fn scheme(
    name: &'static str,
    desc: &'static str,
    bytes: &'static [u8],
    overlay: Overlay,
    scheme: &'static Scheme,
) -> Case {
    Case {
        screen: Screen {
            overlay: Some(Overlay {
                scheme: Some(scheme),
                ..overlay
            }),
            ..Screen::new(name, desc, bytes, 100, 8)
        },
        source: Source::Vtium,
        ported: None,
    }
}

/// An overlay of `notices` alone.
const fn shown(notices: &'static [Words]) -> Overlay {
    Overlay {
        line: None,
        notices,
        mark: None,
        scheme: None,
    }
}

const OSD_FULL: Overlay = Overlay {
    line: Some(notices::osd_active),
    notices: &[],
    mark: None,
    scheme: None,
};

const OSD_FULL_OTHER: Overlay = Overlay {
    line: Some(notices::osd_other),
    ..OSD_FULL
};

const OSD_MINIMAL: Overlay = Overlay {
    line: None,
    notices: &[],
    mark: Some(Mark::Focus),
    scheme: None,
};

const OSD_MINIMAL_OTHER: Overlay = Overlay {
    mark: Some(Mark::NoFocus),
    ..OSD_MINIMAL
};

const OSD_FULL_NOTICE: Overlay = Overlay {
    notices: &[notices::no_keyboard],
    ..OSD_FULL
};

const OSD_MINIMAL_NOTICE: Overlay = Overlay {
    notices: &[notices::no_keyboard],
    ..OSD_MINIMAL
};

/// The four notices a head shows at most, those `NOTICES_MAX` names.
const FOUR: &[Words] = &[
    notices::head_file_one_error,
    notices::no_keyboard,
    notices::anti_lockout,
    notices::rescue,
];

const NO_KEYBOARD_DESC: &str = "vtium's notice that the start found no keyboard (ADR-1401 D8), over a shell session. Correct: the notice on one line, saying no keyboard was found and to plug one in, in a pale red box with a dark red edge across the top, the width of the display, its text dark, its first letter half a cell in from the edge; the session's first row hidden behind the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const ANTI_LOCKOUT_DESC: &str = "vtium's notice that the anti-lockout guard sent every keyboard to the head desk (ADR-1401 D9), over a shell session. Correct: the notice saying no keyboard was reaching a screen that is on, so every keyboard now types into desk, then that a keyboard is sent elsewhere by changing its rule in /etc/vtium/heads.conf and restarting vtium or rebooting, wrapped at a space where it is wider than the box, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const RESCUE_DESC: &str = "vtium's notice that the rescue key moved the keyboard at /dev/input/event3 while desk is the active head (ADR-1401 D9), over a shell session. Correct: the notice naming the keyboard's device and saying it types into the active screen, desk, until the next boot, then that a rule in /etc/vtium/heads.conf keeps it there after that, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark, wrapped at a space onto a second line where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const RESCUE_NO_HEAD_DESC: &str = "vtium's notice that the rescue key moved the keyboard at /dev/input/event3 while no head is active (ADR-1401 D9), over a shell session. Correct: the notice naming the keyboard's device, saying it types into the active screen until the next boot and that no screen is active now, then to press Ctrl+Super+↑ to choose one, in the notices' pale red box with a dark red edge across the top, wrapped at a space onto a second line where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const ONESHOT_DESC: &str = "vtium's notice that the head's session ended under `oneshot` and will not start again (vtium/vtium#156), over the session's last screen. Correct: the notice saying the session has ended and will not start again, then to press Ctrl+Super+↑ to switch to another screen, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark, wrapped at a space where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const HEAD_FILE_DESC: &str = "vtium's notice of a head file with one error (ADR-1401 D7): its rule on line 4 names the head `desc`, which the file does not declare. Correct: two lines in the notices' pale red box with a dark red edge across the top, each starting with the head file's path, /etc/vtium/heads.conf: the first with `:4:` and the error, the second saying the file has errors, so vtium ignored it and uses its built-in rules, and to fix the errors, check the file with `vtium check-config`, then restart vtium or reboot; each wrapped at a space where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const HEAD_FILE_TWO_DESC: &str = "vtium's notice of a head file with two errors (ADR-1401 D7): a key the head section does not take on line 3, and the rule on line 5 naming the head `desc`, which the file does not declare. Correct: three lines in the notices' pale red box with a dark red edge across the top, each starting with /etc/vtium/heads.conf: the error at `:3:`, the error at `:5:`, and the line saying the file has errors, so vtium ignored it and uses its built-in rules, and to fix the errors, check the file with `vtium check-config`, then restart vtium or reboot; each wrapped at a space where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_FULL_DESC: &str = "vtium's on-screen display under `osd = full` on the active head, alone (ADR-1401 D5, vtium/vtium#169). Correct: a dark box with a grey edge across the top, the width of the display, holding `● desk · console 2 of 3` in light text, its first letter half a cell in from the edge, the filled dot meaning desk is the active head; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_FULL_OTHER_DESC: &str = "vtium's on-screen display under `osd = full` on a head that is not the active one (ADR-1401 D5, vtium/vtium#169). Correct: a dark box with a grey edge across the top, the width of the display, holding `○ side · console 1 of 1` in light text, the hollow dot meaning side is not the active head; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_MINIMAL_DESC: &str = "vtium's on-screen display under `osd = minimal`, the default, on the active head (ADR-1401 D5, vtium/vtium#169). Correct: the session as the terminal drew it, and a filled circle in the top-right cell, light on the on-screen display's dark grey; every other cell untouched.";

const OSD_MINIMAL_OTHER_DESC: &str = "vtium's on-screen display under `osd = minimal` on a head that is not the active one (ADR-1401 D5, vtium/vtium#169). Correct: the session as the terminal drew it, and a hollow circle in the top-right cell, light on the on-screen display's dark grey; every other cell untouched.";

const OSD_FULL_NOTICE_DESC: &str = "vtium's on-screen display under `osd = full` sharing its box with the notice that no keyboard was found (ADR-1401 D5, D7). Correct: one box across the top in the notices' colors, pale red with a dark red edge and dark text, since a notice is in it: `● desk · console 2 of 3` on its first line and the notice on the line beneath; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_MINIMAL_NOTICE_DESC: &str = "vtium's on-screen display under `osd = minimal`, the default, with the notice that no keyboard was found, as a head shows them at the start (ADR-1401 D5, D7). Correct: the notice in the notices' pale red box with a dark red edge across the top, and the filled circle drawn over the top-right cell, at the box's top right, the notice's line ending before that cell; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const NOTICES_FOUR_DESC: &str = "Four notices at once, the most a head shows (ADR-1401 D7): a head file with one error, no keyboard found, the anti-lockout guard and the rescue key, oldest first. Correct: one pale red box with a dark red edge across the top holding the four, the head file's two lines first, each notice starting on a line of its own and wrapping at a space where it is wider than the box; the box as tall as the lines it holds; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

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
        ported: Some("blocks"),
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
        ported: Some("blocks-spaced"),
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
        ported: None,
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
        ported: Some("vs16"),
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
        ported: Some("vs16"),
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
        ported: Some("wide-deco"),
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
        ported: Some("redraw"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "ascii-unifont",
                "Printable ASCII in the regular face: a pangram in both cases, digits and punctuation, runs of narrow and wide letters, and a line of code. In unifont. Correct: every glyph whole and inside its own cell, with columns aligned down the screen.",
                ASCII,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("ascii"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "bold-unifont",
                "Bold beside regular, a bold pangram over two lines, and bold in red, green and blue, in unifont. Correct: the bold text drawn as unifont's glyphs widened by a pixel, at the same cell width; the colored words in their colors.",
                BOLD,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("bold"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "palette-unifont",
                "The 16 ANSI background colors, the 16 foreground colors on letter pairs, a ramp through the 256-color palette, and a 24-bit gradient from red to green. In unifont. Correct: each block filled edge to edge in its own color, with no gaps between cells.",
                PALETTE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("palette"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "inverse-unifont",
                "Reverse video (SGR 7): one word, a whole line, red on green swapped, and bold with reverse. In unifont. Correct: foreground and background exchanged, so the swapped word is green on red.",
                INVERSE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("inverse"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "cjk-unifont",
                "Double-width Japanese, Chinese and Korean, full-width Latin, narrow letters between wide characters, and wide text in bold and in reverse. In unifont. Correct: each wide character takes two cells and the narrow letters between them keep their own columns.",
                CJK,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("cjk"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "box-unifont",
                "Box drawing in light and double lines, shades and block elements, eighth blocks, and rounded, heavy and diagonal pieces. In unifont. Correct: lines join from cell to cell with no gaps, and blocks fill their cells.",
                BOX,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("box"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "underline-unifont",
                "Each underline style (single, double, curly, dotted, dashed) in the text color and in an SGR 58 color, over descenders; a red curl ended by SGR 59; underlines on a background, in reverse and in bold. In unifont. Correct: the five styles distinct, the colored ones in their SGR 58 color, and the line after SGR 59 in the text color.",
                UNDERLINE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("underline"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "strike-unifont",
                "Strike-through (SGR 9) alone, in color, on a background, with straight and curly underlines, in bold, in reverse, and over spaces. In unifont. Correct: a line through the middle of every struck cell, the spaces included.",
                STRIKE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("strike"),
    },
    Case {
        screen: Screen {
            decorations: DECO_CONF,
            ..Screen::new(
                "deco-config-unifont",
                "Underline and strike with the configured colors (underline-color=196, strike-color=#00c0ff), beside SGR 58 colors and colored text. In unifont. Correct: underlines and strikes without an SGR 58 color take the configured colors; an SGR 58 color still applies.",
                DECO_CONFIG,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("deco-config"),
    },
    Case {
        screen: Screen {
            decorations: Decorations {
                force: true,
                ..DECO_CONF
            },
            ..Screen::new(
                "deco-force-unifont",
                "The deco-config screen with force-decoration-colors set. In unifont. Correct: every underline and strike in the configured color, the SGR 58 ones included.",
                DECO_CONFIG,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("deco-force"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "combining-unifont",
                "Accented letters precomposed and as a base letter with combining marks, in words, underlined, struck and in bold, in unifont, which shapes nothing and draws a cluster as its first character. Correct: each decomposed cluster in one cell, showing its base letter alone; each precomposed letter with its accent.",
                COMBINING,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("combining"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "mosaic-unifont",
                "Every Braille pattern, sextant and octant in codepoint order; then checkerboards of sextants and of octants, a grid of eight-dot Braille, a Braille wave, and sextant and octant bands that cross the boundaries between rows; then the checkerboards and grid in yellow on blue; in unifont's 8x16 cells. Correct: square dots on the same 2x4 grid in every cell; blocks on exact halves and thirds or quarters of the cell, so the checkerboards and bands run across cells with no seam.",
                MOSAIC,
                72,
                16,
            )
        },
        source: Source::Vtium,
        ported: Some("mosaic"),
    },
    Case {
        screen: Screen {
            ..Screen::new(
                "emoji-unifont",
                "The emoji screen in unifont, which has no color glyphs: two wide emoji and a narrow text smiley; a ZWJ family, a skin tone, a flag and a keycap; a heart with VS16, VS15 and no selector; an emoji between letters; emoji under underline, strike, reverse and a red curl; a codepoint of Unicode 15; and an emoji written at the last column, which wraps. Correct: each emoji its unifont glyph in the text color, a cluster drawn as its first character centred in its cells; the emoji and the VS16 heart two cells wide, the other hearts and the smiley one.",
                EMOJI,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: Some("emoji"),
    },
    Case {
        screen: Screen {
            font: FontKind::ColorSet { height: 16 },
            ..Screen::new(
                "emoji-rgba",
                "Color glyphs (vtium/vtium#23), drawn from a vitrified set built in memory: unifont's ASCII in a 1-bit strike and a grin, a rocket and a heart in a premultiplied RGBA strike at the same 8x16 cell, each two cells wide with edges of partial alpha and the rocket's flame translucent. The glyphs between letters and in bold; on blue, yellow and 256-color green backgrounds; reversed, plain and blue on yellow; under a single underline, a red curl, a double line on blue and a 24-bit green line; struck plain, in red, over an underline on red, and reversed; the heart with no selector, VS15 and VS16, and with no selector on red; an emoji the set lacks; and a grin written at the last column, which wraps to row 9. Correct: each color glyph in its own colors whatever the foreground, bold and reverse, its edges and the flame blended into the cell's background, which shows through around and under it; underlines below the glyphs and strikes over them in their own colors; the heart two cells wide with VS16 and cut to its middle cell otherwise; the lacking emoji as `?` centred in two cells; the text glyphs drawn as in unifont.",
                EMOJI_RGBA,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            display: Some((320, 240)),
            font: FontKind::Psf(TestFont::Small8x8),
            ..Screen::new(
                "small-320x240",
                "minspec's display floor (vtium/vtium#75): a 320x240 display drawn with an 8x8 PSF2 font as 40 by 30 cells. A reversed title across the first row; a shell session, its line past 40 cells wrapping to the next row; a box of sprites around bold, underlined, struck and reversed words; the 16 background colors and 16 foreground colors; shade, block, eighth, Braille and sextant sprites; Latin-1 and punctuation the font holds; a wide and a narrow character it lacks; the five underline styles and a red underline; the underline styles in SGR 58 colors, red struck text, and cyan text struck with a yellow underline; `right edge` ending in the last column of row 28; an editor's status line reversed across the last row. Correct: every glyph and sprite inside its 8x8 cell, the box lines joined, the colors filling their cells, each lacking character drawn as the missing glyph, every decoration inside its cell and below the letters, each colored line in its own color and told apart from the glyphs and the other lines, and nothing past the 40th column or the 30th row.",
                SMALL_40,
                40,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            display: Some((640, 480)),
            font: FontKind::Psf(TestFont::Small8x16),
            ..Screen::new(
                "small-640x480",
                "A 640x480 display drawn with an 8x16 PSF1 font as 80 by 30 cells (vtium/vtium#75): the small-320x240 screen's parts at 80 columns, with `ls -l`, three boxes in light, double, heavy and rounded lines around bold, underlined, struck, reversed, italic, dim and blinking words, ramps through the 256-color palette and in 24-bit color, more sprites and Latin-1, a combining mark, and the decorations in color: each underline style in its own SGR 58 color, green struck text, and blue text struck over a red curl. Correct: as small-320x240, at 80 columns: the italic word drawn upright, since the font has no italic, the dim word fainter, the blinking word drawn as text, and a combined character as its base letter.",
                SMALL_80,
                80,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            display: Some((320, 240)),
            font: FontKind::Psf(TestFont::Small8x8),
            depth: Depth::One,
            ..Screen::new(
                "small-320x240-1bit",
                "small-320x240 on a 1-bit display (vtium/vtium#135). Correct: only black and white pixels; text on the default background lit whatever its color, the dark blue and red included; the reversed title and status line and every background of luma 128 or more lit with their glyphs unlit, and darker backgrounds unlit with their glyphs lit; a foreground equal to its background, as black on the black background block, invisible; every underline, strike and sprite lit like the text in its cell; the border unlit.",
                SMALL_40,
                40,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            display: Some((640, 480)),
            font: FontKind::Psf(TestFont::Small8x16),
            depth: Depth::One,
            ..Screen::new(
                "small-640x480-1bit",
                "small-640x480 on a 1-bit display (vtium/vtium#135). Correct: as small-320x240-1bit, at 80 columns; the dim word drawn as plain lit text; the 256-color and 24-bit ramps lit where a block's luma is 128 or more and unlit under it.",
                SMALL_80,
                80,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            depth: Depth::One,
            ..Screen::new(
                "palette-unifont-1bit",
                "palette-unifont on a 1-bit display (vtium/vtium#135). Correct: the background blocks and the ramps lit where their luma is 128 or more and unlit under it; every letter pair lit, black ones included, on the default background.",
                PALETTE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            depth: Depth::One,
            ..Screen::new(
                "underline-unifont-1bit",
                "underline-unifont on a 1-bit display (vtium/vtium#135). Correct: every underline style lit, in the text color or in an SGR 58 color alike; on blue, under luma 128, the cell unlit with its glyph and underline lit, and in reverse the cell lit with its glyph and underline unlit; the curl's coverage cut at half.",
                UNDERLINE,
                48,
                10,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            depth: Depth::One,
            ..Screen::new(
                "light-theme-1bit",
                "A light theme, OSC 11's #f0f0f0 behind OSC 10's #202020, on a 1-bit display (vtium/vtium#135). Correct: the screen and border lit, as a light default background lights a mono panel; default and red text unlit on it; the grey cell and `black on white` lit with their letters unlit, on the default's side of the luma cut; the blue, reversed and black cells unlit with their letters lit, flipping from the screen; the underlines, strike and box lines unlit like the text; the shades as patterns.",
                LIGHT,
                40,
                5,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            font: FontKind::Set {
                font: TestFont::Small8x8,
                fill: Some("unscii-8"),
            },
            ..Screen::new(
                "vitrified-320x240",
                "The small-320x240 screen drawn from the glyphs vtium-vitrify's engine generates from a frit naming the 8x8 PSF2 test font, with the built-in Unscii-8 filling the characters it lacks (ADR-1301 D2, vtium/vtium#126, vtium/vtium#145). Correct: as small-320x240 in the same 40 by 30 cells of 8x8, the box, shades, Braille and sextants still sprites, since the sprites win over a fill font's glyphs; but `中` and the snowman, which neither font holds, drawn as Unscii-8's U+25A1, the missing glyph, where small-320x240 draws `?`.",
                SMALL_40,
                40,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            display: Some((640, 480)),
            font: FontKind::Set {
                font: TestFont::Small8x16,
                fill: Some("unifont"),
            },
            ..Screen::new(
                "vitrified-640x480",
                "The small-640x480 screen drawn from the glyphs vtium-vitrify's engine generates from a frit naming the 8x16 PSF1 test font, with the built-in unifont filling the characters it lacks (ADR-1301 D2, vtium/vtium#126, vtium/vtium#145). Correct: as small-640x480 in the same 80 by 30 cells of 8x16, the boxes, shades and Braille still sprites, since the sprites win over a fill font's glyphs; but `中文` in unifont's two-cell glyphs and the snowman and the note in its one-cell glyphs, where small-640x480 draws `?`.",
                SMALL_80,
                80,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            overlay: Some(Overlay {
                line: Some(notices::osd_active),
                notices: &[],
                mark: None,
                scheme: None,
            }),
            ..Screen::new(
                "osd-full",
                "vtium's on-screen display under `osd = full` over a shell session, in unifont, 40 by 6 cells (ADR-1401 D5, vtium/vtium#169). Correct: a dark box with a grey edge across the top, the width of the display, holding `● desk · console 2 of 3` in light text, its first letter half a cell in from the edge, the filled dot meaning desk is the active head; the session's first row covered by the box and its second row partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.",
                OSD_SHELL,
                40,
                6,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    Case {
        screen: Screen {
            overlay: Some(Overlay {
                line: None,
                notices: &[],
                mark: Some(Mark::Focus),
                scheme: None,
            }),
            ..Screen::new(
                "osd-minimal",
                "vtium's on-screen display under `osd = minimal`, the default, over the same shell session (ADR-1401 D5, vtium/vtium#169). Correct: the session as the terminal drew it, and a filled circle in the top-right cell, light on the on-screen display's dark grey and cut at the cell's right edge, meaning this head is the active head; every other cell untouched.",
                OSD_SHELL,
                40,
                6,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
    small(
        "notice-no-keyboard-800x480",
        NO_KEYBOARD_DESC,
        shown(&[notices::no_keyboard]),
    ),
    small(
        "notice-anti-lockout-800x480",
        ANTI_LOCKOUT_DESC,
        shown(&[notices::anti_lockout]),
    ),
    small(
        "notice-rescue-800x480",
        RESCUE_DESC,
        shown(&[notices::rescue]),
    ),
    small(
        "notice-rescue-no-head-800x480",
        RESCUE_NO_HEAD_DESC,
        shown(&[notices::rescue_no_head]),
    ),
    small(
        "notice-oneshot-800x480",
        ONESHOT_DESC,
        shown(&[notices::oneshot]),
    ),
    small(
        "notice-head-file-800x480",
        HEAD_FILE_DESC,
        shown(&[notices::head_file_one_error]),
    ),
    small(
        "notice-head-file-two-800x480",
        HEAD_FILE_TWO_DESC,
        shown(&[notices::head_file_two_errors]),
    ),
    small("osd-full-800x480", OSD_FULL_DESC, OSD_FULL),
    small(
        "osd-full-other-800x480",
        OSD_FULL_OTHER_DESC,
        OSD_FULL_OTHER,
    ),
    small("osd-minimal-800x480", OSD_MINIMAL_DESC, OSD_MINIMAL),
    small(
        "osd-minimal-other-800x480",
        OSD_MINIMAL_OTHER_DESC,
        OSD_MINIMAL_OTHER,
    ),
    small(
        "osd-full-notice-800x480",
        OSD_FULL_NOTICE_DESC,
        OSD_FULL_NOTICE,
    ),
    small(
        "osd-minimal-notice-800x480",
        OSD_MINIMAL_NOTICE_DESC,
        OSD_MINIMAL_NOTICE,
    ),
    small("notices-four-800x480", NOTICES_FOUR_DESC, shown(FOUR)),
    large(
        "notice-no-keyboard-3840x540",
        NO_KEYBOARD_DESC,
        shown(&[notices::no_keyboard]),
    ),
    large(
        "notice-anti-lockout-3840x540",
        ANTI_LOCKOUT_DESC,
        shown(&[notices::anti_lockout]),
    ),
    large(
        "notice-rescue-3840x540",
        RESCUE_DESC,
        shown(&[notices::rescue]),
    ),
    large(
        "notice-rescue-no-head-3840x540",
        RESCUE_NO_HEAD_DESC,
        shown(&[notices::rescue_no_head]),
    ),
    large(
        "notice-oneshot-3840x540",
        ONESHOT_DESC,
        shown(&[notices::oneshot]),
    ),
    large(
        "notice-head-file-3840x540",
        HEAD_FILE_DESC,
        shown(&[notices::head_file_one_error]),
    ),
    large(
        "notice-head-file-two-3840x540",
        HEAD_FILE_TWO_DESC,
        shown(&[notices::head_file_two_errors]),
    ),
    large("osd-full-3840x540", OSD_FULL_DESC, OSD_FULL),
    large(
        "osd-full-other-3840x540",
        OSD_FULL_OTHER_DESC,
        OSD_FULL_OTHER,
    ),
    large("osd-minimal-3840x540", OSD_MINIMAL_DESC, OSD_MINIMAL),
    large(
        "osd-minimal-other-3840x540",
        OSD_MINIMAL_OTHER_DESC,
        OSD_MINIMAL_OTHER,
    ),
    large(
        "osd-full-notice-3840x540",
        OSD_FULL_NOTICE_DESC,
        OSD_FULL_NOTICE,
    ),
    large(
        "osd-minimal-notice-3840x540",
        OSD_MINIMAL_NOTICE_DESC,
        OSD_MINIMAL_NOTICE,
    ),
    large("notices-four-3840x540", NOTICES_FOUR_DESC, shown(FOUR)),
    scheme(
        "scheme-light-notice",
        "The example configuration's `light` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session on a light terminal, OSC 11's white behind OSC 10's dark grey. Correct: a dark red `#991b1b` box with a darker `#450a0a` edge across the top, the width of the display, holding the notice in white `#ffffff`; the filled circle in dark `#1f2933` on pale grey `#f3f4f6` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them, dark on white.",
        OSD_SHELL_LIGHT,
        OSD_MINIMAL_NOTICE,
        &SCHEMES[0],
    ),
    scheme(
        "scheme-light-osd",
        "The example configuration's `light` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over the same session on a light terminal. Correct: a pale grey `#f3f4f6` box with a grey `#9ca3af` edge across the top, the width of the display, holding `● desk · console 2 of 3` in dark `#1f2933`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them, dark on white.",
        OSD_SHELL_LIGHT,
        OSD_FULL,
        &SCHEMES[0],
    ),
    scheme(
        "scheme-dark-notice",
        "The example configuration's `dark` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session. Correct: a deep red `#7f1d1d` box with a light red `#f87171` edge across the top, the width of the display, holding the notice in pale `#fee2e2`; the filled circle in light grey `#d1d5db` on near black `#111827` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_MINIMAL_NOTICE,
        &SCHEMES[1],
    ),
    scheme(
        "scheme-dark-osd",
        "The example configuration's `dark` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over a shell session. Correct: a near black `#111827` box with a grey `#4b5563` edge across the top, the width of the display, holding `● desk · console 2 of 3` in light grey `#d1d5db`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_FULL,
        &SCHEMES[1],
    ),
    scheme(
        "scheme-color-blind-safe-notice",
        "The example configuration's `color-blind-safe` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session. Correct: a bright orange `#ffb000` box with a darker orange `#fe6100` edge across the top, the width of the display, holding the notice in near black `#1c1917`; the filled circle in near white `#eef2ff` on dark blue `#0b2a5b` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_MINIMAL_NOTICE,
        &SCHEMES[2],
    ),
    scheme(
        "scheme-color-blind-safe-osd",
        "The example configuration's `color-blind-safe` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over a shell session. Correct: a dark blue `#0b2a5b` box with a blue `#648fff` edge across the top, the width of the display, holding `● desk · console 2 of 3` in near white `#eef2ff`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_FULL,
        &SCHEMES[2],
    ),
];
