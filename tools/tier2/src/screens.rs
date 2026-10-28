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
//! and last the small displays in the PSF fonts `tools/testfont` writes
//! (vtium/vtium#75).

use vtium_render::Orientation;
use vtium_term::WidthMethod;

use crate::draw::{DecoSpec, Decorations, FontKind, Screen, TestFont};

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
            font: FontKind::Set {
                font: TestFont::Small8x8,
                fill: Some("unscii-8"),
            },
            ..Screen::new(
                "vitrified-320x240",
                "The small-320x240 screen drawn from a set vitrified as `vtium-vitrify create` builds it (ADR-1200, vtium/vtium#126): the 8x8 PSF2 test font, with the built-in Unscii-8 filling the characters it lacks. Correct: as small-320x240 in the same 40 by 30 cells of 8x8, but where Unscii-8 fills: the box's lines and the shades, Braille and sextants drawn in Unscii-8's glyphs, since the set's coverage gives the font those ranges over the sprites; and `中` and the snowman, which neither font holds, drawn as Unscii-8's U+25A1, the missing glyph, where small-320x240 draws `?`.",
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
                "The small-640x480 screen drawn from a set vitrified as `vtium-vitrify create` builds it (ADR-1200, vtium/vtium#126): the 8x16 PSF1 test font, with the built-in unifont filling the characters it lacks. Correct: as small-640x480 in the same 80 by 30 cells of 8x16, but where unifont fills: the light, double and rounded boxes' vertical lines, the shades and Braille drawn in unifont's glyphs, since the set's coverage gives the font those ranges over the sprites; and `中文` in unifont's two-cell glyphs and the snowman and the note in its one-cell glyphs, where small-640x480 draws `?`.",
                SMALL_80,
                80,
                30,
            )
        },
        source: Source::Vtium,
        ported: None,
    },
];
