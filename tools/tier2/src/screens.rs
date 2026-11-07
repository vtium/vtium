//! Tier 2's screens, each with the golden that judges it.
//!
//! The first goldens were drawn with an outline font, DejaVu Sans Mono
//! through FreeType; two of them judge vtium where no outline glyph shows,
//! the two block screens, which hold only sprites and spaces. vtium's own
//! screens follow: text in unifont, color glyphs from an RGBA strike of a
//! vitrified set built in memory (vtium/vtium#23), the small displays in
//! the PSF fonts `tools/testfont` writes (vtium/vtium#75), vtium's own
//! drawing over a frame, its notices and on-screen display, at two display
//! sizes (ADR-1401 D5, D7), and pictures. The outline screens come last,
//! drawn from the test font set of outline fonts that [`crate::fonts`]
//! fetches (ADR-1503 D9).

use vtium_render::{Depth, Mark, Orientation};
use vtium_term::WidthMethod;

use vtium::config::keys::{self, Scheme};

use crate::draw::{
    Alone, DecoSpec, Decorations, Feed, FontKind, InkFonts, Overlay, Screen, Spacing, Take,
    TestFont, Words,
};
use crate::notices;
use crate::pictures;

/// A screen, whose golden is `tools/tier2/goldens/<name>.ppm`.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    pub screen: Screen,
}

/// Block elements and box drawing stacked across rows and columns
/// in one picture.
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

/// The `vs16` screen.
const VS16: &[u8] = concat!(
    "a\u{2764}\u{fe0f}b  VS16, wide\r\n",
    "a\u{2764}b  no selector, narrow\r\n",
    "a\u{2764}\u{fe0e}b  VS15, narrow\r\n",
    "\x1b[4ma\u{2764}\u{fe0f}b\x1b[0m  underlined\r\n",
)
.as_bytes();

/// Text, then erases, an insert and a scroll under a pen of bold,
/// underline, strike and a red background.
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

/// The `wide-deco` screen: wide characters under each
/// underline style and strike, and one written at the last column.
const WIDE_DECO: &[u8] = concat!(
    "\x1b[4m日本語\x1b[0m \x1b[4:3m漢字\x1b[0m \x1b[4:4;58:5:196mカタカナ\x1b[0m\r\n",
    "\x1b[4:2m한글\x1b[0m \x1b[4:5m中文\x1b[0m \x1b[9m日本語\x1b[0m\r\n",
    "\x1b[4ma日b本c\x1b[0m\r\n",
    "\x1b[5;48H\x1b[4;9m語\x1b[0m",
)
.as_bytes();

/// The `ascii` screen.
const ASCII: &[u8] = concat!(
    "The quick brown fox jumps over the lazy dog.\r\n",
    "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG.\r\n",
    "0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~\r\n",
    "iiiiiiii llllllll WWWWWWWW mmmmmmmm\r\n",
    "func main() { return 0; }  // 80x24 \r\n",
)
.as_bytes();

/// The `bold` screen.
const BOLD: &[u8] = concat!(
    "regular \x1b[1mbold\x1b[0m regular\r\n",
    "\x1b[1mThe quick brown fox jumps over\r\n",
    "the lazy dog 0123456789 !@#$%^&*()\x1b[0m\r\n",
    "\x1b[1;31mbold red\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[1;34mbold blue\x1b[0m\r\n",
)
.as_bytes();

/// The `bold` screen in the outline fonts: the `bold` screen's lines, then
/// italic and bold italic, which the test font set draws from DejaVu Sans
/// Mono's Oblique and Bold Oblique faces.
const FACES: &[u8] = concat!(
    "regular \x1b[1mbold\x1b[0m regular\r\n",
    "\x1b[1mThe quick brown fox jumps over\r\n",
    "the lazy dog 0123456789 !@#$%^&*()\x1b[0m\r\n",
    "\x1b[1;31mbold red\x1b[0m \x1b[1;32mbold green\x1b[0m \x1b[1;34mbold blue\x1b[0m\r\n",
    "\x1b[3mitalic The quick brown fox\x1b[0m regular\r\n",
    "\x1b[1;3mbold italic jumps over the lazy dog\x1b[0m\r\n",
)
.as_bytes();

/// The `palette` screen.
const PALETTE: &[u8] = concat!(
    "\x1b[40m  \x1b[41m  \x1b[42m  \x1b[43m  \x1b[44m  \x1b[45m  \x1b[46m  \x1b[47m  \x1b[100m  \x1b[101m  \x1b[102m  \x1b[103m  \x1b[104m  \x1b[105m  \x1b[106m  \x1b[107m  \x1b[0m\r\n",
    "\x1b[30mAB\x1b[31mAB\x1b[32mAB\x1b[33mAB\x1b[34mAB\x1b[35mAB\x1b[36mAB\x1b[37mAB\x1b[90mAB\x1b[91mAB\x1b[92mAB\x1b[93mAB\x1b[94mAB\x1b[95mAB\x1b[96mAB\x1b[97mAB\x1b[0m\r\n",
    "\x1b[48;5;16m  \x1b[48;5;52m  \x1b[48;5;88m  \x1b[48;5;124m  \x1b[48;5;160m  \x1b[48;5;196m  \x1b[48;5;202m  \x1b[48;5;214m  \x1b[48;5;226m  \x1b[48;5;154m  \x1b[48;5;46m  \x1b[48;5;48m  \x1b[48;5;51m  \x1b[48;5;27m  \x1b[48;5;21m  \x1b[48;5;129m  \x1b[48;5;232m  \x1b[48;5;244m  \x1b[48;5;255m  \x1b[0m\r\n",
    "\x1b[48;2;255;0;0m  \x1b[48;2;223;32;0m  \x1b[48;2;191;64;0m  \x1b[48;2;159;96;0m  \x1b[48;2;127;128;0m  \x1b[48;2;95;160;0m  \x1b[48;2;63;192;0m  \x1b[48;2;31;224;0m  \x1b[48;2;0;255;0m  \x1b[0m\r\n",
).as_bytes();

/// The `inverse` screen.
const INVERSE: &[u8] = concat!(
    "normal \x1b[7minverse\x1b[0m normal\r\n",
    "\x1b[7mWhole line inverted\x1b[0m\r\n",
    "\x1b[31;42mred on green \x1b[7mswapped\x1b[0m\r\n",
    "\x1b[1;7mbold inverse\x1b[0m\r\n",
)
.as_bytes();

/// The `cjk` screen.
const CJK: &[u8] = concat!(
    "日本語の端末表示 テスト\r\n",
    "中文字符 漢字 ひらがな カタカナ\r\n",
    "한글 테스트\r\n",
    "ＡＢＣＤ０１２３ abcd 0123\r\n",
    "a日b本c語d\r\n",
    "\x1b[1m日本語 bold\x1b[0m \x1b[7m日本語 inverse\x1b[0m\r\n",
)
.as_bytes();

/// The `box` screen.
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

/// Links (OSC 8), one of them under the pointer: its pieces on several
/// rows share an id, so they are one link, in each of the styles a cell
/// can carry, and one wraps from the last column (ADR-1002, "Drawing").
const LINK_HOVER: &[u8] = concat!(
    "see \x1b]8;id=a;https://example.com/docs\x1b\\the docs\x1b]8;;\x1b\\ or \x1b]8;;https://example.com/b\x1b\\another link\x1b]8;;\x1b\\\r\n",
    "\x1b[4m\x1b]8;id=a;https://example.com/docs\x1b\\an underlined piece\x1b]8;;\x1b\\\x1b[0m plain text\r\n",
    "\x1b[4:3;58:5:46m\x1b]8;id=a;https://example.com/docs\x1b\\a green curl\x1b]8;;\x1b\\\x1b[0m, \x1b[4:3mthe text's curl\x1b[0m\r\n",
    "\x1b[31;47m\x1b]8;id=a;https://example.com/docs\x1b\\red on white\x1b]8;;\x1b\\\x1b[0m \x1b[7m\x1b]8;id=a;https://example.com/docs\x1b\\reversed\x1b]8;;\x1b\\\x1b[0m\r\n",
    "\x1b[5;29H\x1b]8;id=a;https://example.com/docs\x1b\\a piece that wraps\x1b]8;;\x1b\\",
)
.as_bytes();

const LINK_HOVER_DESC: &str = "Links (OSC 8) with the pointer over the first, whose pieces on \
every row share its id: plain, underlined, under a green SGR 58 curl, red on white, reversed, \
and one piece that wraps from the last column onto the next row. In unifont. Correct: every \
piece of the first link takes a dashed underline in place of its own, in the text's color or \
in green under the SGR 58 curl, the wrapped piece on both rows; `another link` and the text \
outside the links draw as plain text, and the curl outside the link stays curly.";

/// The `underline` screen.
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

/// The `strike` screen.
const STRIKE: &[u8] = concat!(
    "\x1b[9mstruck through gjpqy\x1b[0m plain\r\n",
    "\x1b[9;31mred strike\x1b[0m \x1b[9;42mstrike on green\x1b[0m\r\n",
    "\x1b[9;4mstrike and underline\x1b[0m \x1b[9;4:3;58:5:51mcurly too\x1b[0m\r\n",
    "\x1b[1;9mbold struck\x1b[0m \x1b[7;9minverse struck\x1b[0m\r\n",
    "\x1b[9m      \x1b[0m struck spaces\r\n",
)
.as_bytes();

/// The `deco-config` screen.
/// Also the `deco-force` screen's.
const DECO_CONFIG: &[u8] = concat!(
    "\x1b[4mconfigured underline\x1b[0m \x1b[9mconfigured strike\x1b[0m\r\n",
    "\x1b[4:3;58:5:46mSGR 58 green\x1b[0m \x1b[4:4;58:2::255:255:0mSGR 58 yellow\x1b[0m\r\n",
    "\x1b[32;4mgreen text\x1b[0m \x1b[33;9mstruck yellow text\x1b[0m\r\n",
    "\x1b[4:2;9mdouble and strike\x1b[0m\r\n",
)
.as_bytes();

/// The `combining` screen.
const COMBINING: &[u8] = concat!(
    "precomposed: é ã ñ\r\n",
    "decomposed:  e\u{301} a\u{303}\u{300} n\u{308}\r\n",
    "in words: cafe\u{301} na\u{303}\u{300}o n\u{308}ice\r\n",
    "\x1b[4mu\u{308}nderlined e\u{301}\x1b[0m \x1b[9mstruck a\u{303}\u{300}\x1b[0m\r\n",
    "bold: \x1b[1me\u{301} a\u{303}\u{300} n\u{308}\x1b[0m\r\n",
)
.as_bytes();

/// The `mosaic` screen.
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

/// The `emoji` screen.
const EMOJI: &[u8] = concat!(
    "😀 👍 a☺b  wide, wide, narrow text\r\n",
    "👨\u{200d}👩\u{200d}👧 👍🏽 🇯🇵 1\u{fe0f}\u{20e3}  family, tone, flag, keycap\r\n",
    "❤\u{fe0f} ❤\u{fe0e} ❤  VS16, VS15, none\r\n",
    "a😀b  between letters\r\n",
    "\x1b[4m😀\x1b[0m \x1b[9m😀\x1b[0m \x1b[7m😀\x1b[0m \x1b[4:3;58:5:196m👍🏽\x1b[0m  under, strike, reverse, curl\r\n",
    "🫨  missing\r\n",
    "\x1b[7;48H😀",
).as_bytes();

/// The `emoji-spaced` screen: a box whose sides must join from row to
/// row, around Latin text with ascenders, descenders and accents, emoji
/// alone, in clusters, between letters, under each decoration and on a
/// background; then blocks and lines, and underlined and struck Latin.
const EMOJI_SPACED: &[u8] = concat!(
    "┌──────────────────────────────────────────────┐\r\n",
    "│ Hxgjpqy ÀÉÎ ŞĘ  Latin on its baseline\x1b[48G│\r\n",
    "│ 😀 👍 🚀  wide emoji, never stretched\x1b[48G│\r\n",
    "│ 👨\u{200d}👩\u{200d}👧 👍🏽 🇯🇵  family, tone, flag\x1b[48G│\r\n",
    "│ a😀b a☺b  between letters, narrow text\x1b[48G│\r\n",
    "│ \x1b[4m😀 under\x1b[0m \x1b[9m😀 struck\x1b[0m \x1b[7m😀 rev\x1b[0m ",
    "\x1b[4:3;58:5:196m👍🏽curl\x1b[0m\x1b[48G│\r\n",
    "│ \x1b[44m on blue, the added rows too \x1b[0m\x1b[48G│\r\n",
    "└──────────────────────────────────────────────┘\r\n",
    "▀▄█▌▐░▒▓▖▝ ═║╬ ╭╮╰╯  blocks and lines fill\r\n",
    "\x1b[4mgjpq underlined\x1b[0m \x1b[9mgjpq struck\x1b[0m \x1b[4:3;9;31mboth, red\x1b[0m",
)
.as_bytes();

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

/// The `deco-config` settings: `underline-color=196`,
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
/// as vtium-config places a bitmap font in a larger cell.
const fn small(name: &'static str, desc: &'static str, overlay: Overlay) -> Case {
    Case {
        screen: Screen {
            overlay: Some(overlay),
            ..Screen::new(name, desc, OVERLAY_SHELL, 100, 30)
        },
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
    }
}

/// The screen the display guard lights for a notice alone (ADR-1401 D7,
/// D9): no session behind it and no cursor, so the terminal draws nothing
/// beneath the notice.
const BLANK: &[u8] = b"\x1b[?25l";

/// The notice `words` alone on the blank screen of the small display,
/// 800x480 as 100 by 30 cells.
const fn alone_small(name: &'static str, desc: &'static str, alone: Alone) -> Case {
    Case {
        screen: Screen {
            alone: Some(alone),
            ..Screen::new(name, desc, BLANK, 100, 30)
        },
    }
}

/// The notice `words` alone on the blank screen of the large display's
/// top band, 3840x540 as 160 by 10 cells of 24x54.
const fn alone_large(name: &'static str, desc: &'static str, alone: Alone) -> Case {
    Case {
        screen: Screen {
            font: FontKind::Frit {
                builtin: "unifont",
                width: 24,
                height: 54,
            },
            alone: Some(alone),
            ..Screen::new(name, desc, BLANK, 160, 10)
        },
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

const ANTI_LOCKOUT_DESC: &str = "vtium's notice that the anti-lockout guard sent every keyboard to the head desk (ADR-1401 D9), over a shell session. Correct: the notice saying no keyboard was reaching a screen that is on, so every keyboard now types into desk, then that a keyboard is sent elsewhere by changing its rule in /etc/vtium/vtium.conf and restarting vtium or rebooting, wrapped at a space where it is wider than the box, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const RESCUE_DESC: &str = "vtium's notice that the rescue key moved the keyboard at /dev/input/event3 while desk is the active head (ADR-1401 D9), over a shell session. Correct: the notice naming the keyboard's device and saying it types into the active screen, desk, until the next boot, then that a rule in /etc/vtium/vtium.conf keeps it there after that, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark, wrapped at a space onto a second line where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const RESCUE_NO_HEAD_DESC: &str = "vtium's notice that the rescue key moved the keyboard at /dev/input/event3 while no head is active (ADR-1401 D9), over a shell session. Correct: the notice naming the keyboard's device, saying it types into the active screen until the next boot and that no screen is active now, then to press Ctrl+Super+↑ to choose one, in the notices' pale red box with a dark red edge across the top, wrapped at a space onto a second line where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const ONESHOT_DESC: &str = "vtium's notice that the head's session ended under `oneshot` and will not start again (vtium/vtium#156), over the session's last screen. Correct: the notice saying the session has ended and will not start again, then to press Ctrl+Super+↑ to switch to another screen, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark, wrapped at a space where it is wider than the box; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const NO_CONSOLE_OPENED_DESC: &str = "vtium's notice that a console the open chord opened did not open, because its login could not start (ADR-1401 D5), a notice of a problem that shows whatever `osd` says, over a shell session. Correct: the notice `Console not opened: login could not start.` on one line, in the notices' pale red box with a dark red edge across the top, the width of the display, its text dark, its first letter half a cell in from the edge; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const HEAD_FILE_DESC: &str = "vtium's notice of partly broken sections of vtium.conf (ADR-1401 D7, ADR-1403 D3): its rule on line 4 names the head `desc`, which the file does not declare, so that rule alone is dropped. Correct: two lines in the notices' pale red box with a dark red edge across the top, the console line `config partly invalid, running what's valid`, naming no error, since the log keeps the errors, then its next step, `Run vtium check-config /etc/vtium/vtium.conf to see why.`; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const HEAD_FILE_TWO_DESC: &str = "vtium's notice of broken sections of vtium.conf under the open policy (ADR-1401 D7, ADR-1403 D3): a key the head section does not take on line 3 drops the head, and with it every rule naming it, so nothing usable survives and the shipped rules run. Correct: two lines in the notices' pale red box with a dark red edge across the top, the console line `invalid config skipped, using builtin`, naming no error, then its next step, `Run vtium check-config /etc/vtium/vtium.conf to see why.`; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const PANIC_DESC: &str = "vtium's panic screen (ADR-1401 D7): the sections of a closed vtium.conf have four errors and no head survives them, so vtium runs nothing and shows this on the boot display for as long as it runs, with no session behind it. Correct: one pale red box with a dark red edge across the top of an otherwise empty screen, holding, each on a line of its own and wrapped at a space where it is wider than the box, `vtium: config invalid, policy closed: input off`; that the errors in /etc/vtium/vtium.conf leave no head that can run; the file's first three errors, on lines 4, 5 and 7, each starting with the file and its line; `More errors: 1`; and last, that nothing on this console takes input and to run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media for the full report; and below the box, the screen's background and nothing else.";

const PANIC_SMALL_DESC: &str = "vtium's panic screen (ADR-1401 D7) on minspec's 320x240 display floor, in unifont as 40 by 15 cells, too small for the whole report. Correct: the box keeps the report's first two lines, `vtium: config invalid, policy closed: input off` and that the errors in /etc/vtium/vtium.conf leave no head that can run, and its last, that nothing on this console takes input and to run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media for the full report, each wrapped at a space; between them, the file's first error, on line 4, then `More errors: 3`, which counts the two errors the screen has no room for and the one past the three it lists, so no error goes uncounted and no line is cut; below the box, the screen's background and nothing else.";

const ALONE_NO_DISPLAY_DESC: &str = "vtium's notice that no head has a display, alone on the boot display (ADR-1401 D9): the heads of a closed vtium.conf have no display lit, so the display guard lights the boot display for this notice alone, with no session behind it, until a head lights. Correct: one pale red box with a dark red edge across the top of an otherwise empty screen, holding `vtium: no display for any head` and, on the next line, to connect a display that a rule in /etc/vtium/vtium.conf sends to a head; below the box, the screen's background and nothing else.";

const ALONE_PARTLY_DESC: &str = "vtium's notice of partly broken sections of vtium.conf, alone on the boot display (ADR-1401 D7, D9, ADR-1403 D3): under the closed policy the heads that survive have no display lit, so the display guard lights the boot display for the start's notice alone, with no session behind it, until a head lights. Correct: one pale red box with a dark red edge across the top of an otherwise empty screen, holding `config partly invalid, running what's valid` and, on the next line, `Run vtium check-config /etc/vtium/vtium.conf to see why.`; below the box, the screen's background and nothing else.";

const ALONE_STOPPED_DESC: &str = "vtium's notice that a crash loop stopped every head under the closed policy, alone on the boot display (ADR-1401 D1): a session kept ending as soon as it started, so vtium hung every session up and holds its VT with this notice for as long as it runs, with no session behind it. Correct: one pale red box with a dark red edge across the top of an otherwise empty screen, holding the notice that a session kept ending as soon as it started, so vtium stopped every screen and nothing here takes input, then to fix the login command in /etc/vtium/vtium.conf and restart vtium or reboot, wrapped at a space where it is wider than the box; below the box, the screen's background and nothing else.";

const OSD_FULL_DESC: &str = "vtium's on-screen display under `osd = full` on the active head, alone (ADR-1401 D5, vtium/vtium#169). Correct: a dark box with a grey edge across the top, the width of the display, holding `● desk · console 2 of 3` in light text, its first letter half a cell in from the edge, the filled dot meaning desk is the active head; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_FULL_OTHER_DESC: &str = "vtium's on-screen display under `osd = full` on a head that is not the active one (ADR-1401 D5, vtium/vtium#169). Correct: a dark box with a grey edge across the top, the width of the display, holding `○ side · console 1 of 1` in light text, the hollow dot meaning side is not the active head; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_MINIMAL_DESC: &str = "vtium's on-screen display under `osd = minimal`, the default, on the active head (ADR-1401 D5, vtium/vtium#169). Correct: the session as the terminal drew it, and a filled circle in the top-right cell, light on the on-screen display's dark grey; every other cell untouched.";

const OSD_MINIMAL_OTHER_DESC: &str = "vtium's on-screen display under `osd = minimal` on a head that is not the active one (ADR-1401 D5, vtium/vtium#169). Correct: the session as the terminal drew it, and a hollow circle in the top-right cell, light on the on-screen display's dark grey; every other cell untouched.";

const OSD_FULL_NOTICE_DESC: &str = "vtium's on-screen display under `osd = full` sharing its box with the notice that no keyboard was found (ADR-1401 D5, D7). Correct: one box across the top in the notices' colors, pale red with a dark red edge and dark text, since a notice is in it: `● desk · console 2 of 3` on its first line and the notice on the line beneath; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const OSD_MINIMAL_NOTICE_DESC: &str = "vtium's on-screen display under `osd = minimal`, the default, with the notice that no keyboard was found, as a head shows them at the start (ADR-1401 D5, D7). Correct: the notice in the notices' pale red box with a dark red edge across the top, and the filled circle drawn over the top-right cell, at the box's top right, the notice's line ending before that cell; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const NOTICES_FOUR_DESC: &str = "Four notices at once, the most a head shows (ADR-1401 D7): one error in vtium.conf's sections, no keyboard found, the anti-lockout guard and the rescue key, oldest first. Correct: one pale red box with a dark red edge across the top holding the four, the two lines of the error's notice first, its console line and its next step, each notice starting on a line of its own and wrapping at a space where it is wider than the box; the box as tall as the lines it holds; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.";

const PICTURES_KITTY_DESC: &str = "Kitty pictures over unifont text and colored cells (ADR-1100 D4, D5; ADR-1303 D2): across the top, one at its own size, then pictures shrunk by 1.5, 2.5, 4 and 8, grown by 2 and 3, and one shrunk across and grown down; below them a picture below the backgrounds, a picture below the text, two half transparent pictures above it, and a half transparent picture over an opaque one. Correct: each picture fills its cells' box, filtered smoothly with no lost lines; the picture below the backgrounds shows only through cells of the default background; text draws over the picture below the text; the half transparent pictures let the text and cells beneath show through.";

const PICTURES_RIGHT_DESC: &str = "The kitty picture screen on a display turned a quarter right, 5 pixels wider and 3 taller than its grid, so its margins are odd (ADR-1303 D3). Correct: the same screen turned, every picture turned with its cells and placed on the same cells.";

const PICTURES_SELECTION_DESC: &str = "The kitty picture screen with a selection from the fourth cell of the fifth row to the twenty-first of the tenth (ADR-1100 D4): the selection tints the cells and the pictures over them at SELECTION_TINT. Correct: the selected cells and the parts of pictures over them half tinted with the selection's color, the rest as the kitty screen.";

const PICTURES_SIXEL_DESC: &str = "Sixel pictures made in a 10x20 cell and drawn in unifont's 8x16 (ADR-1100 D8): two shrunk by 1.25, one 40x40 and one 70x36, and a small one 10x18. Correct: each sixel's tiles of eight colors and its black diagonals, shrunk smoothly into the cells the picture covers.";

const PICTURES_BUDGET_DESC: &str = "A 160x64 display, whose pictures' bound is two screens of it, 81,920 bytes (ADR-1100 D5, ADR-1303 D2): a 200x200 picture drawn in 8 by 2 cells, whose own pixels pass the bound and whose drawn copy fits, and one drawn over 40 by 12 cells, far past the display, whose copy passes it too, scaled for each frame. Correct: both pictures drawn smoothly as the CPU path keeps or scales them, the second cut at the display's edge.";

const PICTURES_TILED_DESC: &str = "Three pictures wider and taller than 64 pixels, which the GPU path holds in textures of at most 64 a side for this screen (ADR-1303 D2): one grown by about 2, one shrunk by 1.5 and one shrunk by 4 below the text. Correct: each picture whole and smooth, with no seam where its tiles meet on the GPU path.";

/// A row of text that pictures cover, 47 characters long.
macro_rules! text_row {
    () => {
        "sixels over text, text over sixels, sixels over\r\n"
    };
}

/// The text below one layer's picture in `img-bands` and `kitty-bands`,
/// on the row `$row` from 1: a word, blue cells, a word, green cells and
/// a word.
macro_rules! band_text {
    ($row:literal) => {
        concat!(
            "\x1b[",
            $row,
            ";16HText \x1b[44m    \x1b[0m over \x1b[42m  \x1b[0m pic"
        )
    };
}

/// Each layer's name, and the text its picture covers.
const BANDS: &[u8] = concat!(
    "lowest band",
    band_text!(2),
    "\x1b[4;1Hmiddle band",
    band_text!(5),
    "\x1b[7;1Htop band",
    band_text!(8),
)
.as_bytes();

/// Five rows of text under `img-sixel`'s sixels.
const SIXEL_ROWS: &[u8] = concat!(
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!()
)
.as_bytes();

/// Eight rows of text under `img-moves`'s pictures.
const MOVES_ROWS: &[u8] = concat!(
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
)
.as_bytes();

/// Six rows of text under `img-ed2`'s pictures.
const ED2_ROWS: &[u8] = concat!(
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
    text_row!(),
)
.as_bytes();

/// `img-scale`'s labels, on the eighth row under each picture.
const SCALE_LABELS: &[u8] =
    b"\x1b[8;2H1:1\x1b[8;8Hfit 10x5\x1b[8;20Hshrunk 12x2\x1b[8;34H5x8 cells";

/// The ten rows `img-region` scrolls.
const REGION_ROWS: &[u8] =
    b"row 0\r\nrow 1\r\nrow 2\r\nrow 3\r\nrow 4\r\nrow 5\r\nrow 6\r\nrow 7\r\nrow 8\r\nrow 9";

/// The banner screens' scroll region, the last two rows, with the cursor
/// on the last.
const BANNER_REGION: &[u8] = b"\x1b[9;10r\x1b[10;1H";

/// The text `kitty-text`'s disc covers, and the screen's title.
const KITTY_TEXT_ROWS: &[u8] = b"kitty pictures under and over text\x1b[3;26Hfirst row, around it\x1b[4;26Hmiddle row, erased\x1b[5;26Hlast row, around it";

/// The text under `sixel-vte`'s second sixel.
const SIXEL_VTE_TEXT: &[u8] = concat!(
    "sixel strings through the vte\r\n",
    "\r\n",
    "                    text text text text\r\n",
    "                    text text text text\r\n",
)
.as_bytes();

/// `sixel-vte`'s sixels, written by hand as a program sends them: four
/// bands of red, green, blue and white bars, 40x24, with `TEXT` written
/// across their second row; and orange stripes 5 pixels wide with clear
/// gaps between them, 30x24, which leave their unset pixels clear.
const SIXEL_VTE: &[u8] = concat!(
    "\x1b[3;2H\x1bPq\"1;1;40;24",
    "#1;2;100;0;0#2;2;0;100;0#3;2;0;0;100#4;2;100;100;100",
    "#1!10~#2!10~#3!10~#4!10~-",
    "#1!10~#2!10~#3!10~#4!10~-",
    "#1!10~#2!10~#3!10~#4!10~-",
    "#1!10~#2!10~#3!10~#4!10~",
    "\x1b\\",
    "\x1b[4;4HTEXT",
    "\x1b[3;21H\x1bP0;1q\"1;1;30;24#5;2;100;50;0",
    "#5!5~!5?!5~!5?!5~!5?-",
    "#5!5~!5?!5~!5?!5~!5?-",
    "#5!5~!5?!5~!5?!5~!5?-",
    "#5!5~!5?!5~!5?!5~!5?",
    "\x1b\\",
)
.as_bytes();

/// Feeds `sixel-vte`'s sixels.
fn sixel_vte(f: &mut Feed) -> Result<(), String> {
    f.bytes(SIXEL_VTE)
}

const IMG_BANDS_DESC: &str = "One picture in each layer a kitty picture can draw in (ADR-1100 D4), its left half opaque and its right half at half alpha, each over a row of text in DejaVu Sans Mono at 16 pixels with blue and green cells: below the cell backgrounds at the top, below the text in the middle, above the text at the bottom. Each image is sent first and then put at its cell by a command of its own. Correct: in the lowest band the picture shows only through cells on the default background, so the blue and green cells stay solid, and the text draws over it; in the middle band the picture covers the backgrounds, the blue cells hidden under its opaque half and the green ones tinted under its clear half, and the text draws over it; in the top band the opaque half hides the text and the clear half shows it dimmed.";
const KITTY_BANDS_DESC: &str = "The img-bands screen with each picture sent and placed by one kitty command at the cursor, which DECSC and DECRC keep where it was, in chunks of 4096 base64 characters. Correct: the same as img-bands, pixel for pixel.";
const IMG_SIXEL_DESC: &str = "Three sixels over five rows of text in DejaVu Sans Mono at 16 pixels: a gradient 3 pixels narrower and 5 shorter than 10 by 3 cells, a disc whose corners are clear, and a gradient that `TEXT` is written over afterwards. Correct: the left picture hides the text under it, its right and bottom edges ending inside their cells, where the text past them shows; the disc hides the text inside it and the text shows around it; the right picture has a hole where `TEXT` was written over it, and the word draws there as plain text on the background, since a character written into a cell cuts a sixel there (ADR-1100 D3).";
const SIXEL_VTE_DESC: &str = "Two sixel strings written by hand, as a program sends them, over text in DejaVu Sans Mono at 16 pixels: at the left, red, green, blue and white bars 10 pixels wide and 24 tall, with `TEXT` written across their second row; at the right, orange stripes 5 pixels wide with clear gaps between them, over two rows of text, a string that asks for its unset pixels to stay clear. Correct: at the left the four bars side by side, `TEXT` on the background with no picture behind it, and the bars' lower 8 pixel rows left only in the two cells the word does not reach; at the right the stripes, with the text under them showing through the gaps.";
const IMG_SCALE_DESC: &str = "Kitty pictures at their own size and scaled, above `1:1`, `fit 10x5`, `shrunk 12x2` and `5x8 cells` in DejaVu Sans Mono at 16 pixels: a 40x20 gradient at its own size; the same image put again fitted to 10 by 5 cells; 480x160 stripes a pixel wide fitted to 12 by 2 cells; and a 20x16 gradient made while the terminal told programs a cell of 5x8 pixels, at its own size there, 4 by 2 of those cells. Correct: the gradients smooth with no steps; the fitted one 90x45, keeping its shape, centred in its box of 90x80; the stripes shrunk to an even olive 96x32, centred in their 108x32 box, with no moire; the last picture drawn over 4 by 2 of the font's 9x16 cells, 36x32.";
const IMG_REGION_DESC: &str = "A scroll of two lines in a region from the third row to the eighth, over `row 0` to `row 9` in DejaVu Sans Mono at 16 pixels, with three pictures on the screen: a sixel inside the region from its top row, a kitty picture of 6 by 2 cells inside it, and a kitty picture of 6 by 4 cells that starts above it. Correct: the rows read `row 0`, `row 1`, `row 4` to `row 7`, two empty rows, `row 8` and `row 9`; the sixel lost the rows that left the region and its lower half sits in the third and fourth rows; the small kitty picture moved up two rows and its top row, pushed past the region's top, is cut, so only its lower half shows, in the third row; the tall kitty picture, which crosses the region's edge, stays where it was (ADR-1100 D3).";
const IMG_ROTATED_DESC: &str = "A display turned a quarter right, 24 by 6 cells of DejaVu Sans Mono at 16 pixels: a line of text, and one half-transparent picture placed twice, at its own size above the text and fitted to 6 by 3 cells below it. Correct: the text reads from top to bottom and the pictures turn with it, each in its own cells, the opaque half of each toward the top of the image; the text under the first picture hidden by its opaque half and dimmed under its clear half, and drawn over the second.";
const IMG_MOVES_DESC: &str = "Four frames into a double-buffered display over eight rows of text in DejaVu Sans Mono at 16 pixels. Before the first frame, two kitty pictures and a sixel disc; before the third, the first kitty placement moved by a put with the same placement id, the second picture's image sent again, which removes its placement, the sixel's two rows erased from its first column to the end, and a new picture placed below the text. Both buffers must then hold what a fresh draw of the last screen draws. Correct: the moved picture at its new place and no trace of its old one, where the text shows whole; no trace of the removed picture at the right, where the text shows whole; the sixel gone with the text of its rows from its first column; the new picture under the text of its rows.";
const IMG_BANNER_BG_DESC: &str = "A picture the size of the screen sent again as the same image before each of three frames, which removes the last one's placement, and put at the top left below the cell backgrounds; after it, a banner line of blue and green cells that scrolls in the bottom two rows, in DejaVu Sans Mono at 16 pixels. Both buffers must then hold what a fresh draw of the last screen draws. Correct: the last frame's picture alone, `banner 1` and `banner 2` in the bottom rows, their blue and green cells solid over the picture.";
const IMG_BANNER_TEXT_DESC: &str = "The img-banner-bg screen with the picture below the text and over the cell backgrounds. Correct: the last frame's picture under every cell, the banner's blue and green cells hidden under it, and only the banner's letters drawn over it.";
const IMG_ED2_DESC: &str = "A sixel and a kitty disc below the text over six rows of text, shown for two frames; then ED 2 and `cleared`, in DejaVu Sans Mono at 16 pixels. Both buffers must then hold what a fresh draw of the last screen draws. Correct: only the word `cleared`, with no pixel of either picture left, since ED 2 removes every placement on the screen (ADR-1100 D8).";
const KITTY_TEXT_DESC: &str = "Text and kitty pictures in DejaVu Sans Mono at 16 pixels. At the left, an opaque gradient sent as RGB below the text, placed with the cursor moving past it, so the `<after` that follows starts on its last row just past its right edge; then `TEXT`, white on red, and `on it` written over its middle row. At the right, a disc above three rows of text, the middle row then erased from the twentieth column by EL. Correct: the gradient whole under the white `TEXT`, with no red showing, since the picture covers the cell backgrounds; the disc whole, since only ED 2 removes a kitty picture; the words of the first and last rows showing only outside the disc, and nothing left of the middle row's.";
const KITTY_BANNER_BG_DESC: &str = "The img-banner-bg screen with each frame's picture sent and placed by one kitty command at the top left, as RGBA, RGB and RGBA in turn, each replacing the last image. Correct: the same as img-banner-bg, pixel for pixel.";
const KITTY_BANNER_TEXT_DESC: &str = "The img-banner-text screen with each frame's picture sent and placed by one kitty command at the top left, as RGBA, RGB and RGBA in turn, each replacing the last image. Correct: the same as img-banner-text, pixel for pixel.";

/// A picture screen: vtium's golden, drawn by vtium.
/// A screen of `cols` by `rows` in the test frit of outline fonts
/// (ADR-1503 D9), with its color emoji font.
const fn outline(
    name: &'static str,
    desc: &'static str,
    bytes: &'static [u8],
    cols: usize,
    rows: usize,
) -> Screen {
    Screen {
        font: FontKind::Outline {
            color: true,
            spacing: None,
        },
        ..Screen::new(name, desc, bytes, cols, rows)
    }
}

/// 12 per cent of line spacing on the 16-pixel text: 1.92 rows, which
/// the console rounds to 2 and splits evenly, 1 above the text band and 1
/// below it (ADR-1302 D13). DejaVu Sans Mono's 9x16 cell becomes 9x18.
const SPACED_12: Spacing = Spacing {
    percent: 12,
    above: 1,
    below: 1,
};

/// 30 per cent of line spacing on the 16-pixel text: 4.8 rows, which the
/// console rounds to 5 and splits with the odd row above, 3 above the
/// text band and 2 below it (ADR-1302 D13). The cell becomes 9x21.
const SPACED_30: Spacing = Spacing {
    percent: 30,
    above: 3,
    below: 2,
};

/// [`outline`]'s screen with each row made taller by `spacing`, set by
/// the test frit's `line-spacing=` (ADR-1302 D13).
const fn spaced(
    name: &'static str,
    desc: &'static str,
    bytes: &'static [u8],
    (cols, rows): (usize, usize),
    spacing: Spacing,
) -> Screen {
    Screen {
        font: FontKind::Outline {
            color: true,
            spacing: Some(spacing),
        },
        ..outline(name, desc, bytes, cols, rows)
    }
}

const fn picture(screen: Screen) -> Case {
    Case { screen }
}

/// What the `mosaic-spaced-` screens show and what correct looks like,
/// after the sentence that gives each one's spacing.
macro_rules! mosaic_spaced {
    () => {
        "Every Braille pattern, sextant and octant in codepoint order; then checkerboards of sextants and of octants, a grid of eight-dot Braille, a Braille wave, and sextant and octant bands that cross the boundaries between rows; then the checkerboards and grid in yellow on blue. Correct: vtium's sprites stretch to the whole taller cell, so the dots keep one 2x4 grid in every cell and the checkerboards, the grid and the bands run from row to row with no seam and no gap where the rows were added; the labels in DejaVu Sans Mono at their own size on the baseline, with blank rows above and below them, each cell of a label holding its pixels with no spacing."
    };
}

/// What the `emoji-spaced-` screens show and what correct looks like,
/// after the sentence that gives each one's spacing.
macro_rules! emoji_spaced {
    () => {
        "A box of light lines around Latin letters with ascenders, descenders and accents; three wide emoji; a ZWJ family, a skin tone and a flag; an emoji between letters and a narrow text smiley; emoji under underline, strike, reverse and a red curl; text on a blue background; then blocks, double lines and rounded corners, and Latin underlined, struck, and both in red. The emoji come from Noto Color Emoji 2.051, the text from DejaVu Sans Mono. No Arabic is drawn: its marks would reach into the added rows under fit=room, which draws as natural until the right-to-left work builds it. Correct: the box's sides and the blocks and lines stretch to the whole taller cell and join from row to row; each Latin letter at DejaVu Sans Mono's own size on its baseline in the 16-row text band, with the added rows blank above and below it, the accents above À, É and Î whole, reaching above the text band into the added rows (ADR-1306 D1, D8); each emoji whole, the size it is with no spacing, inside the text band and never stretched; each underline and strike moved down with the text band; the blue background filling the whole cell, the added rows included. Every cell but the sprites' holds in its text band the pixels it has with no spacing."
    };
}

/// The ink screens' takes: every cell height from 12 to 32 pixels, each
/// with nothing fed after the bytes, then `more`. `N` is 21 more than `M`.
const fn sweep<const M: usize, const N: usize>(more: [Take; M]) -> [Take; N] {
    assert!(N == 21 + M, "21 heights and the takes after them");
    let mut all = [Take::at(12); N];
    let mut i = 0;
    while i < N {
        all[i] = if i < 21 {
            Take::at(12 + i as u32)
        } else {
            more[i - 21]
        };
        i += 1;
    }
    all
}

/// A take at 16 pixels with `tail` fed after the bytes.
const fn at_16(tail: &'static [u8]) -> Take {
    Take {
        height: 16,
        tail,
        blink_hidden: false,
    }
}

/// A take at 16 pixels in blinking text's hidden phase.
const BLINK_HIDDEN_16: Take = Take {
    height: 16,
    tail: b"",
    blink_hidden: true,
};

/// The `overflow-accents` screen, 18 by 4 cells in the ink test font:
/// accented capitals on the grid's first row, plain, reversed and red;
/// `gjpqy___` and ç on backgrounds that alternate from cell to cell; under
/// them the capitals À, É, Î, Ấ and Ộ, É as E with a combining acute, and
/// H with combining marks, on alternating backgrounds the other way round,
/// then a blinking ÉÎ and a reversed ÀÉÎ; and a last row of `gjpqy___`,
/// plain and reversed.
const OVERFLOW_ACCENTS: &[u8] = concat!(
    "ÀÉÎẤỘ \x1b[7mÀÉÎ\x1b[27m \x1b[31mÉÎ\x1b[39m\r\n",
    "\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24mç\x1b[48;5;58m_\x1b[49m\r\n",
    "\x1b[48;5;58mÀ\x1b[48;5;24mÉ\x1b[48;5;58mÎ\x1b[48;5;24mẤ\x1b[48;5;58mỘ\x1b[48;5;24mE\u{301}\x1b[48;5;58mH\u{301}\x1b[48;5;24mH\u{302}\u{323}\x1b[49m \x1b[5mÉÎ\x1b[25m \x1b[7mÀÉÎ\x1b[27m\r\n",
    "gjpqy___ \x1b[7mgjpqy\x1b[27m",
)
.as_bytes();

/// `overflow-accents`' takes: every height from 12 to 32 pixels; then at
/// 16 pixels a block, an underline and a bar cursor on the É of the third
/// row and on the j above it; then the blinking letters in their hidden
/// phase.
const OVERFLOW_ACCENTS_TAKES: [Take; 28] = sweep([
    at_16(b"\x1b[?25h\x1b[2 q\x1b[3;2H"),
    at_16(b"\x1b[?25h\x1b[2 q\x1b[2;2H"),
    at_16(b"\x1b[?25h\x1b[4 q\x1b[3;2H"),
    at_16(b"\x1b[?25h\x1b[4 q\x1b[2;2H"),
    at_16(b"\x1b[?25h\x1b[6 q\x1b[3;2H"),
    at_16(b"\x1b[?25h\x1b[6 q\x1b[2;2H"),
    BLINK_HIDDEN_16,
]);

/// The `overflow-marks` screen, 16 by 3 cells in the ink test font: a
/// row of empty cells on alternating backgrounds; Arabic letters with marks
/// above and below, beh with fatha, beh with kasra, lam with shadda and
/// fatha, noon with damma, alef with sukun, beh with kasratan, lam with
/// shadda, fatha and the superscript alef, and beh with the superscript
/// alef, then lam with shadda and fatha and noon with damma again; and
/// another row of empty cells on alternating backgrounds.
const OVERFLOW_MARKS: &[u8] = concat!(
    "\x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[49m\r\n",
    "\x1b[48;5;58m\u{628}\u{64e}\x1b[48;5;24m\u{628}\u{650}\x1b[48;5;58m\u{644}\u{651}\u{64e}\x1b[48;5;24m\u{646}\u{64f}\x1b[48;5;58m\u{627}\u{652}\x1b[48;5;24m\u{628}\u{64d}\x1b[48;5;58m\u{644}\u{651}\u{64e}\u{670}\x1b[48;5;24m\u{628}\u{670}\x1b[49m \x1b[48;5;24m\u{644}\u{651}\u{64e}\x1b[48;5;58m\u{646}\u{64f}\x1b[49m\r\n",
    "\x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[48;5;58m \x1b[48;5;24m \x1b[49m",
)
.as_bytes();

/// Every height from 12 to 32 pixels.
const SWEEP: [Take; 21] = sweep([]);

/// The `overflow-meet` screen, 14 by 3 cells in the ink test font: red
/// ç over green É, the cedilla and the acute meeting where the rows meet;
/// the same on blue and olive cells; and red ç over green Ấ.
const OVERFLOW_MEET: &[u8] = concat!(
    "\x1b[31mçççç \x1b[48;5;24mçç\x1b[49m çç\x1b[39m\r\n",
    "\x1b[32mÉÉÉÉ \x1b[48;5;58mÉÉ\x1b[49m ẤẤ\x1b[39m\r\n",
)
.as_bytes();

/// The ink screens drawn at 16 and 32 pixels.
const AT_16_AND_32: [Take; 2] = [Take::at(16), Take::at(32)];

/// The `overflow-spaced` screen, 16 by 3 cells in the ink test font with
/// line spacing: `gjpqy___` twice on alternating backgrounds; the
/// accented capitals and Arabic letters with marks; and `gjpqy___` on
/// alternating backgrounds.
const OVERFLOW_SPACED: &[u8] = concat!(
    "\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[49m\r\n",
    "ÀÉÎẤỘ \u{628}\u{64e}\u{644}\u{651}\u{64e}\u{628}\u{64d}\u{646}\u{64f}\u{644}\u{651}\u{64e}\u{670} ÉÎ\r\n",
    "\x1b[48;5;58m_\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[49m",
)
.as_bytes();

/// The `overflow-pictures` screen, 24 by 6 cells in the ink test font:
/// accented capitals in the third row, under the four pictures of the
/// second row, and Arabic letters with kasra and kasratan in the fourth,
/// over the four pictures of the fifth ([`pictures::overflow`]); the
/// pictures' rows hold no text.
const OVERFLOW_PICTURES: &[u8] = concat!(
    "\r\n",
    "\r\n",
    "ÀÉÎẤ  ÀÉÎẤ  ÀÉÎẤ  ÀÉÎẤ\r\n",
    "\u{628}\u{64d}\u{628}\u{650}\u{628}\u{64d}\u{628}\u{650}  \u{628}\u{64d}\u{628}\u{650}\u{628}\u{64d}\u{628}\u{650}  \u{628}\u{64d}\u{628}\u{650}\u{628}\u{64d}\u{628}\u{650}  \u{628}\u{64d}\u{628}\u{650}\u{628}\u{64d}\u{628}\u{650}",
)
.as_bytes();

/// The `overhang-italic` screen, 20 by 3 cells in the ink test font: an
/// italic run on backgrounds that alternate from cell to cell, then
/// italic words underlined and struck; an italic Arabic run, then a
/// blinking italic word beside a steady one; and the same letters upright.
const OVERHANG_ITALIC: &[u8] = concat!(
    "\x1b[3m\x1b[48;5;24mH\x1b[48;5;58mI\x1b[48;5;24mE\x1b[48;5;58mW\x1b[48;5;24mm\x1b[48;5;58mA\x1b[48;5;24mO\x1b[48;5;58mƒ\x1b[49m \x1b[4mHIE\x1b[24m \x1b[9mOWm\x1b[29m\x1b[23m\r\n",
    "\x1b[3m\u{628}\u{644}\u{646}\u{627}\u{628} \x1b[5mHIE\x1b[25m HIE\x1b[23m\r\n",
    "HIEWmAOƒ HIE OWm",
)
.as_bytes();

/// `overhang-italic`'s takes: at 16 pixels, with no cursor, with a block
/// cursor on the leaning I of the first row and on the space just after
/// the run, and with the blinking word hidden; then at 32 pixels.
const OVERHANG_ITALIC_TAKES: [Take; 5] = [
    Take::at(16),
    at_16(b"\x1b[?25h\x1b[2 q\x1b[1;2H"),
    at_16(b"\x1b[?25h\x1b[2 q\x1b[1;9H"),
    BLINK_HIDDEN_16,
    Take::at(32),
];

/// The `overhang-wm` screen, 14 by 2 cells in the ink test font: W and m,
/// then ƒ, whose hooks reach both sides, between them, on alternating
/// backgrounds; then the same in bold.
const OVERHANG_WM: &[u8] = concat!(
    "WWWWmmmm \x1b[48;5;24mW\x1b[48;5;58mƒ\x1b[48;5;24mm\x1b[48;5;58mƒ\x1b[48;5;24mW\x1b[49m\r\n",
    "\x1b[1mWWWWmmmm \x1b[48;5;24mW\x1b[48;5;58mƒ\x1b[48;5;24mm\x1b[48;5;58mƒ\x1b[48;5;24mW\x1b[0m",
)
.as_bytes();

/// The `overhang-underscore` screen, 12 by 2 cells in the ink test font:
/// underscores and `gjpqy` on alternating backgrounds, twice, so a row's
/// bottom meets the next row's top.
const OVERHANG_UNDERSCORE: &[u8] = concat!(
    "\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58mg\x1b[48;5;24mj\x1b[48;5;58mp\x1b[48;5;24mq\x1b[48;5;58my\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24m_\x1b[49m\r\n",
    "\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[48;5;24mg\x1b[48;5;58mj\x1b[48;5;24mp\x1b[48;5;58mq\x1b[48;5;24my\x1b[48;5;58m_\x1b[48;5;24m_\x1b[48;5;58m_\x1b[49m",
)
.as_bytes();

/// The `overhang-wide` screen, 12 by 4 cells in the ink test font: the
/// ideograph 一 between narrow letters and beside another on alternating
/// backgrounds; at the grid's last two columns; and written at the last
/// column of the third row, which wraps it to the start of the fourth.
const OVERHANG_WIDE: &[u8] = concat!(
    "H一H \x1b[48;5;24m一\x1b[48;5;58m一\x1b[49m H\r\n",
    "HIE\x1b[2;11H一",
    "\x1b[3;1HHIE\x1b[3;12H一",
)
.as_bytes();

/// A screen of `cols` by `rows` in the ink test font (`crate::inkfont`),
/// drawn once for each of `takes`, stacked from the top.
const fn ink(
    name: &'static str,
    desc: &'static str,
    bytes: &'static [u8],
    (cols, rows): (usize, usize),
    takes: &'static [Take],
) -> Screen {
    Screen {
        font: FontKind::Ink {
            takes,
            spacing: None,
            fonts: InkFonts::Test,
        },
        ..Screen::new(name, desc, bytes, cols, rows)
    }
}

/// `screen` on a 1-bit display, named `name`.
const fn one_bit(name: &'static str, screen: Screen) -> Screen {
    Screen {
        name,
        desc: ONE_BIT_DESC,
        depth: Depth::One,
        ..screen
    }
}

/// The ink screen `screen` drawn in DejaVu Sans Mono and Noto Sans CJK in
/// place of the ink test font, with the same bytes and takes, named
/// `name`.
const fn dejavu(name: &'static str, screen: Screen) -> Screen {
    let FontKind::Ink { takes, spacing, .. } = screen.font else {
        panic!("an ink screen");
    };
    Screen {
        name,
        desc: DEJAVU_DESC,
        font: FontKind::Ink {
            takes,
            spacing,
            fonts: InkFonts::DejaVu,
        },
        ..screen
    }
}

const DEJAVU_DESC: &str = "The screen of the same name without `-dejavu`, with the same bytes, takes, selection and pictures, drawn in the test font set without its color emoji font: DejaVu Sans Mono's Regular, Bold, Oblique and Bold Oblique faces, then Noto Sans Hebrew and Noto Sans Arabic, which draw the Hebrew and Arabic blocks, then Noto Sans CJK, as the console draws a font set, and in the built-in font for what none holds, such as the precomposed Ấ, which DejaVu Sans Mono lacks. The Arabic comes from Noto Sans Arabic at DejaVu Sans Mono's size, each letter in the joined form its neighbours give it, the run drawn right to left (ADR-1004 D3, D6), and lam with shadda, fatha and superscript alef drawn whole as the font shapes it. Noto Sans Arabic has no italic face, so italic Arabic draws as its regular glyphs slanted. Correct: each glyph where its font puts it, and the ink a real font sends past its cell drawn into the neighbouring cells by the rules the test-font screen's caption gives; where DejaVu Sans Mono's ink stays in its cell, as it does for most glyphs at most heights, nothing reaches the cells around it.";

/// The ink screens, each drawn in the ink test font, on a 1-bit display,
/// in DejaVu Sans Mono and in DejaVu Sans Mono on a 1-bit display.
const fn ink_cases(names: [&'static str; 3], screen: Screen) -> [Case; 4] {
    let real = dejavu(names[1], screen);
    [
        Case { screen },
        Case {
            screen: one_bit(names[0], screen),
        },
        Case { screen: real },
        Case {
            screen: one_bit(names[2], real),
        },
    ]
}

const OVERFLOW_ACCENTS_SCREEN: Screen = Screen {
    selection: Some(((8, 1), (11, 2))),
    ..ink(
        "overflow-accents",
        OVERFLOW_ACCENTS_DESC,
        OVERFLOW_ACCENTS,
        (18, 4),
        &OVERFLOW_ACCENTS_TAKES,
    )
};

const OVERFLOW_SPACED_SCREEN: Screen = Screen {
    font: FontKind::Ink {
        takes: &AT_16_AND_32,
        spacing: Some(SPACED_30),
        fonts: InkFonts::Test,
    },
    ..ink(
        "overflow-spaced",
        OVERFLOW_SPACED_DESC,
        OVERFLOW_SPACED,
        (16, 3),
        &AT_16_AND_32,
    )
};

const OVERFLOW_PICTURES_SCREEN: Screen = Screen {
    pictures: Some(pictures::overflow),
    ..ink(
        "overflow-pictures",
        OVERFLOW_PICTURES_DESC,
        OVERFLOW_PICTURES,
        (24, 6),
        &AT_16,
    )
};

const OVERHANG_ITALIC_SCREEN: Screen = Screen {
    selection: Some(((2, 0), (4, 0))),
    ..ink(
        "overhang-italic",
        OVERHANG_ITALIC_DESC,
        OVERHANG_ITALIC,
        (20, 3),
        &OVERHANG_ITALIC_TAKES,
    )
};

/// Every ink screen: four cases each, from [`ink_cases`].
const INK_CASES: [[Case; 4]; 9] = [
    ink_cases(
        [
            "overflow-accents-1bit",
            "overflow-accents-dejavu",
            "overflow-accents-dejavu-1bit",
        ],
        OVERFLOW_ACCENTS_SCREEN,
    ),
    ink_cases(
        [
            "overflow-marks-1bit",
            "overflow-marks-dejavu",
            "overflow-marks-dejavu-1bit",
        ],
        ink(
            "overflow-marks",
            OVERFLOW_MARKS_DESC,
            OVERFLOW_MARKS,
            (16, 3),
            &SWEEP,
        ),
    ),
    ink_cases(
        [
            "overflow-meet-1bit",
            "overflow-meet-dejavu",
            "overflow-meet-dejavu-1bit",
        ],
        ink(
            "overflow-meet",
            OVERFLOW_MEET_DESC,
            OVERFLOW_MEET,
            (14, 3),
            &AT_16_AND_32,
        ),
    ),
    ink_cases(
        [
            "overflow-spaced-1bit",
            "overflow-spaced-dejavu",
            "overflow-spaced-dejavu-1bit",
        ],
        OVERFLOW_SPACED_SCREEN,
    ),
    ink_cases(
        [
            "overflow-pictures-1bit",
            "overflow-pictures-dejavu",
            "overflow-pictures-dejavu-1bit",
        ],
        OVERFLOW_PICTURES_SCREEN,
    ),
    ink_cases(
        [
            "overhang-italic-1bit",
            "overhang-italic-dejavu",
            "overhang-italic-dejavu-1bit",
        ],
        OVERHANG_ITALIC_SCREEN,
    ),
    ink_cases(
        [
            "overhang-wm-1bit",
            "overhang-wm-dejavu",
            "overhang-wm-dejavu-1bit",
        ],
        ink(
            "overhang-wm",
            OVERHANG_WM_DESC,
            OVERHANG_WM,
            (14, 2),
            &AT_16_AND_32,
        ),
    ),
    ink_cases(
        [
            "overhang-underscore-1bit",
            "overhang-underscore-dejavu",
            "overhang-underscore-dejavu-1bit",
        ],
        ink(
            "overhang-underscore",
            OVERHANG_UNDERSCORE_DESC,
            OVERHANG_UNDERSCORE,
            (12, 2),
            &SWEEP,
        ),
    ),
    ink_cases(
        [
            "overhang-wide-1bit",
            "overhang-wide-dejavu",
            "overhang-wide-dejavu-1bit",
        ],
        ink(
            "overhang-wide",
            OVERHANG_WIDE_DESC,
            OVERHANG_WIDE,
            (12, 4),
            &AT_16_AND_32,
        ),
    ),
];

/// One drawing at 16 pixels.
const AT_16: [Take; 1] = [Take::at(16)];

const ONE_BIT_DESC: &str = "The screen of the same name without `-1bit`, drawn the same way on a 1-bit display, every pixel black or white. Correct: the same shapes as on the color screen; ink that reaches into a neighbouring cell lit or unlit like the text it belongs to, over that cell's own lit or unlit background; the blue and olive cells both unlit, being under luma 128, so the text on them is lit.";

const OVERFLOW_ACCENTS_DESC: &str = "Accented capitals whose accents pass the top of their row, in the ink test font built in memory, drawn at every cell height from 12 to 32 pixels from the top down, then seven more times at 16 pixels: with a block, an underline and a bar cursor on the É of the third row and on the j above it, and with blinking text in its hidden phase. Each drawing is 18 by 4 cells: À, É, Î, Ấ and Ộ, then ÀÉÎ reversed and ÉÎ in red, on the grid's first row; gjpqy___ twice and ç_ on cells that alternate between blue and olive; below them À, É, Î, Ấ, Ộ, E with a combining acute, H with a combining acute and H with a combining circumflex and dot below, on the alternating cells the other way round, a blinking ÉÎ and ÀÉÎ reversed; and gjpqy___ and a reversed gjpqy in the last row. A selection runs from the ninth cell of the second row to the twelfth of the third. Correct (ADR-1306 D1, D4, D6): on the first row each accent cut at the grid's top edge; in the third row each accent whole in the cell above its letter, over that cell's blue or olive background, which never paints over it, Ấ's acute reaching half a cell up; E with the combining acute drawn as É is; Ộ's dot whole in the row below; where the second row's descenders and the third row's accents meet, each cell's own glyph over the ink it receives; the selected capitals in the first eight cells of the third row, under unselected cells, their accents there in the colors they have unselected; the reversed ÀÉÎ of the third row, unselected, under selected cells, their accents there in the selection's text color, since a selection takes over the ink inside it; the blinking ÉÎ, selected under selected cells, their accents cut at the top of their row in every take, since a blinking letter's ink stays in its own cell. In the cursor takes, at 16 pixels, where the j above the É hides most of its acute: a block cursor on the É draws its cell in the cursor's colors and leaves the acute in the j's cell in the colors it has unselected; a block cursor on the j draws the acute inside it in the cursor's text color; an underline cursor on the É underlines its cell, and on the j the acute crosses the underline; a bar cursor on the É stands in place of the É, its acute still above it, and on the j it stands in place of the j, over the acute. In the last take the blinking letters dimmed, as vtium draws blinking text in its hidden phase, and every other pixel as in the 16-pixel take of the sweep.";

const OVERFLOW_MARKS_DESC: &str = "Arabic letters with marks above and below their row, in the ink test font built in memory, drawn at every cell height from 12 to 32 pixels from the top down. Each drawing is 16 by 3 cells: a row of empty cells that alternate between blue and olive; beh with fatha, beh with kasra, lam with shadda and fatha, noon with damma, alef with sukun, beh with kasratan, lam with shadda, fatha and the superscript alef, and beh with the superscript alef, on alternating cells, then lam with shadda and fatha and noon with damma again; and another row of empty alternating cells. The program writes the letters in the order they are read, and vtium draws them right to left in the order the bidirectional algorithm gives, one cell each, each cell keeping its background (ADR-1004 D3): the two letters after the space at the left end of the row, then the space, then the first eight letters, beh with fatha rightmost. Each letter takes the joined form its neighbours give it where the font holds one; the ink test font holds none, so each draws its own glyph (ADR-1004 D6). Correct (ADR-1306 D1, D2): each mark above its letter whole in the row above, where the font puts it, over that row's backgrounds; kasra and kasratan whole in the row below; lam with shadda and fatha at its full height, the lam's stem passing into the row above; the superscript alef, which stands past the one row ink may reach, moved down or scaled with its letter until it ends inside the row above, at every height.";

const OVERFLOW_MEET_DESC: &str = "A descender in one row meeting an accent in the row below, in the ink test font built in memory, drawn at 16 and at 32 pixels. Each drawing is 14 by 3 cells: red ç four times, twice on blue and twice more, over green É four times, twice on olive, and green Ấ twice. Correct (ADR-1306 D4): each cedilla reaching down into the row below and each acute reaching up into the row above, crossing where the rows meet. Each crossing lies in a cell where one of the two inks is that cell's own glyph, and a cell's own glyph draws over the ink it receives: in the upper row the red cedilla lies over the green acute, and in the lower row the green acute lies over the red cedilla. The blue and olive backgrounds lie under the ink of both rows.";

const OVERFLOW_SPACED_DESC: &str = "The accents and marks with line spacing, in the ink test font built in memory with line-spacing=30, drawn at 16 and at 32 pixels. Each drawing is 16 by 3 cells: gjpqy___ twice on cells that alternate between blue and olive; À, É, Î, Ấ and Ộ, then beh with fatha, lam with shadda and fatha, beh with kasratan, noon with damma and lam with shadda, fatha and the superscript alef, then ÉÎ, the Arabic drawn right to left as the bidirectional algorithm orders it, beh with fatha rightmost, in each letter's joined form where the font holds one (ADR-1004 D3, D6); and _gjpqy___ twice on alternating cells. Correct (ADR-1306 D1, D8): the letters at the font's own size on their baselines in the text band, the rows the spacing adds blank around them; each accent and mark whole, using the added rows above and below the text band before it reaches the row beyond, and reaching that row where the added rows are too few; the backgrounds filling the whole taller cells.";

const OVERFLOW_PICTURES_DESC: &str = "Accents and Arabic marks reaching into the cells of pictures, in the ink test font built in memory at 16 pixels, 24 by 6 cells: in the second row, which holds no text, a kitty picture 4 cells wide below the cell backgrounds, one below the text at z -1, one above the text at z 0 and a sixel; under each, ÀÉÎẤ in the third row; Arabic beh with kasratan and with kasra in the fourth row, each run of four drawn right to left as the bidirectional algorithm orders it, and joined where the font holds the joined forms (ADR-1004 D3, D6), over the same four pictures in the fifth. Correct (ADR-1306 D7): the accents and the marks drawn over the picture below the backgrounds and the picture below the text, as the text is; hidden under the picture above the text and under the sixel, as the text of those cells is.";

const OVERHANG_ITALIC_DESC: &str = "Italic text whose letters lean into the next cell, in the ink test font built in memory, which has no italic face, so the glyph helper slants its regular glyphs by 0.2 about the baseline (ADR-1302 D4). Drawn at 16 pixels with no cursor, with a block cursor on the leaning I of the first row and on the space after the run, with blinking text in its hidden phase, and at 32 pixels. Each drawing is 20 by 3 cells: HIEWmAOƒ italic on cells that alternate between blue and olive, then HIE underlined and OWm struck; an italic run of Arabic letters, beh, lam, noon, alef and beh, drawn right to left as the bidirectional algorithm orders it, and joined where the font holds the joined forms (ADR-1004 D3, D6), then a blinking italic HIE beside a steady one; and the same Latin letters upright. A selection covers the second to the fifth cell of the first row. Correct (ADR-1302 D1, D4, D7, D8): each italic letter's top leaning into the cell on its right and its foot into the cell on its left, unbroken across the cell edges and over the blue and olive backgrounds; the underline and strike straight; the Arabic leaning right like the Latin; the selected letters in the selection's colors and the ink they send into the unselected cells beside them in the text's colors; the cursor on the I with the I in the cursor's colors and the ink of its neighbours over it; the blinking HIE's lean cut at its own cells in every take; in the blinking take the blinking HIE dimmed, as vtium draws blinking text in its hidden phase, and the steady HIE beside it as in the first take.";

const OVERHANG_WM_DESC: &str = "W and m in the ink test font built in memory, whose W fills its advance, so its ink sets the cell's width, drawn at 16 and at 32 pixels. Each drawing is 14 by 2 cells: WWWWmmmm, then W, ƒ, m, ƒ, W on cells that alternate between blue and olive; and the same in bold. Correct (ADR-1302 D1, D2, D4): each regular W and m inside its own cell, touching its cell's sides; the ƒ's hooks reaching a quarter of a cell into the cells on both sides, over the W and m there and their backgrounds; the bold letters a pixel or more wider, grown sideways by synthetic bold, and where the growth passes a cell's side it reaches into the cell beside it, as at 16 pixels, where it lands in the first, second, third and ninth cells of the bold row.";

const OVERHANG_UNDERSCORE_DESC: &str = "The underscore and gjpqy in the ink test font built in memory, drawn at every cell height from 12 to 32 pixels from the top down. Each drawing is 12 by 2 cells: ___gjpqy___ on cells that alternate between blue and olive, twice, the second row's colors the other way round. Correct (ADR-1302 D3): each underscore whole at the bottom of its cell, running unbroken from cell to cell, and every descender ending inside its own row, so no ink of the first row reaches the second at any height.";

const OVERHANG_WIDE_DESC: &str = "The ideograph 一, two cells wide, in the ink test font built in memory, whose bar reaches a quarter of a cell past each side, drawn at 16 and at 32 pixels. Each drawing is 12 by 4 cells: H, 一, H, then 一 twice on blue and olive cells, and H; HIE and 一 in the last two columns; HIE and 一 written at the last column of the third row, which wraps it to the start of the fourth. Correct (ADR-1302 D1, D5): the bar running into the H on each side and into the neighbouring ideograph, over the backgrounds there; in the last two columns the bar reaching left into the cell before it and cut at the grid's right edge; the wrapped ideograph at the start of the fourth row, its bar cut at the grid's left edge, and the third row's last cell empty.";

/// The right-to-left screens' lines (ADR-1004 D14), one paragraph to a
/// row: Latin around the Hebrew word shalom and a bracketed 1; Arabic
/// marhaban bil-alam, then a colon, 123 and Latin in brackets; the Hebrew
/// word tapuz, 42 in square brackets, Latin, the Arabic burtuqal and 7 in
/// braces; the Arabic as-salamu alaykum, whose lam and alef join into
/// their ligature, then a date; and the Hebrew shalom olam with olam in
/// brackets, then 3.14.
const BIDI_MIXED: &[u8] = concat!(
    "abc \u{5e9}\u{5dc}\u{5d5}\u{5dd} (1) def\r\n",
    "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}: 123 (abc)\r\n",
    "\u{5ea}\u{5e4}\u{5d5}\u{5d6} [42] and \u{628}\u{631}\u{62a}\u{642}\u{627}\u{644} {7}\r\n",
    "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} \u{639}\u{644}\u{64a}\u{643}\u{645} 2024-11-06\r\n",
    "\u{5e9}\u{5dc}\u{5d5}\u{5dd} (\u{5e2}\u{5d5}\u{5dc}\u{5dd}) 3.14",
)
.as_bytes();

/// [`BIDI_MIXED`] with every paragraph right to left: SCP 2 at the first
/// position of the first paragraph, which each new paragraph takes.
const BIDI_MIXED_RTL: &[u8] = concat!(
    "\x1b[2 k",
    "abc \u{5e9}\u{5dc}\u{5d5}\u{5dd} (1) def\r\n",
    "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}: 123 (abc)\r\n",
    "\u{5ea}\u{5e4}\u{5d5}\u{5d6} [42] and \u{628}\u{631}\u{62a}\u{642}\u{627}\u{644} {7}\r\n",
    "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} \u{639}\u{644}\u{64a}\u{643}\u{645} 2024-11-06\r\n",
    "\u{5e9}\u{5dc}\u{5d5}\u{5dd} (\u{5e2}\u{5d5}\u{5dc}\u{5dd}) 3.14",
)
.as_bytes();

/// Explicit mode, BDSM reset: two left-to-right paragraphs a program laid
/// out itself in the order they are drawn, shalom and marhaban bil-alam
/// written reversed, then two right-to-left paragraphs after SCP 2, abc
/// and 123 written reversed.
const BIDI_EXPLICIT: &[u8] = concat!(
    "\x1b[8labc \u{5dd}\u{5d5}\u{5dc}\u{5e9} (1) def\r\n",
    "123 \u{645}\u{644}\u{627}\u{639}\u{644}\u{627}\u{628} \u{627}\u{628}\u{62d}\u{631}\u{645}\r\n",
    "\x1b[2 k\u{5e9}\u{5dc}\u{5d5}\u{5dd} cba (1)\r\n",
    "\u{645}\u{631}\u{62d}\u{628}\u{627} 321 [\u{628}]",
)
.as_bytes();

/// Shalom between Latin, and marhaban before Latin, the cursor, shown, on
/// the third letter of marhaban, hah, the third cell the program wrote.
const BIDI_CURSOR: &[u8] = concat!(
    "abc \u{5e9}\u{5dc}\u{5d5}\u{5dd} def\r\n",
    "\u{645}\u{631}\u{62d}\u{628}\u{627} abc",
    "\x1b[2;3H",
)
.as_bytes();

/// Arabic for the built-in font: marhaban bil-alam, a colon and 123;
/// as-salamu alaykum; and lam with alef alone, after beh, and in brackets.
const BIDI_UNIFONT: &[u8] = concat!(
    "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}: 123\r\n",
    "\u{627}\u{644}\u{633}\u{644}\u{627}\u{645} \u{639}\u{644}\u{64a}\u{643}\u{645}\r\n",
    "\u{644}\u{627} \u{628}\u{644}\u{627} (\u{644}\u{627})",
)
.as_bytes();

/// The right-to-left screens (ADR-1004 D14), each followed by its twin on
/// a 1-bit display.
const BIDI_CASES: [Case; 14] = bidi_cases([
    outline("bidi-mixed", BIDI_MIXED_DESC, BIDI_MIXED, 26, 5),
    outline("bidi-mixed-rtl", BIDI_MIXED_RTL_DESC, BIDI_MIXED_RTL, 26, 5),
    outline("bidi-explicit", BIDI_EXPLICIT_DESC, BIDI_EXPLICIT, 18, 4),
    Screen {
        tail: b"\x1b[2 q\x1b[?25h",
        phase: crate::draw::Phase {
            cursor_on: true,
            blink_hidden: false,
        },
        ..outline(
            "bidi-cursor-block",
            BIDI_CURSOR_BLOCK_DESC,
            BIDI_CURSOR,
            14,
            2,
        )
    },
    Screen {
        tail: b"\x1b[6 q\x1b[?25h",
        phase: crate::draw::Phase {
            cursor_on: true,
            blink_hidden: false,
        },
        ..outline("bidi-cursor-bar", BIDI_CURSOR_BAR_DESC, BIDI_CURSOR, 14, 2)
    },
    Screen {
        selection: Some(((2, 0), (9, 0))),
        ..outline("bidi-selection", BIDI_SELECTION_DESC, BIDI_MIXED, 26, 5)
    },
    Screen {
        font: FontKind::Builtin,
        ..Screen::new("bidi-unifont", BIDI_UNIFONT_DESC, BIDI_UNIFONT, 20, 3)
    },
]);

/// Each of `screens`, then the same screen on a 1-bit display, named with
/// `-1bit` after its name.
const fn bidi_cases(screens: [Screen; 7]) -> [Case; 14] {
    const NAMES: [&str; 7] = [
        "bidi-mixed-1bit",
        "bidi-mixed-rtl-1bit",
        "bidi-explicit-1bit",
        "bidi-cursor-block-1bit",
        "bidi-cursor-bar-1bit",
        "bidi-selection-1bit",
        "bidi-unifont-1bit",
    ];
    let mut all = [Case { screen: screens[0] }; 14];
    let mut i = 0;
    while i < 7 {
        all[2 * i] = Case { screen: screens[i] };
        all[2 * i + 1] = Case {
            screen: Screen {
                desc: BIDI_ONE_BIT_DESC,
                ..one_bit(NAMES[i], screens[i])
            },
        };
        i += 1;
    }
    all
}

const BIDI_ONE_BIT_DESC: &str = "The screen of the same name without `-1bit`, drawn the same way on a 1-bit display, every pixel black or white. Correct: the same letters in the same places and forms as on the color screen, lit on an unlit background, with a cursor or a selection drawn lit and its text unlit.";

const BIDI_MIXED_DESC: &str = "Right-to-left text mixed with Latin, digits and brackets, in the default mode: vtium runs the Unicode bidirectional algorithm over each paragraph, and every paragraph runs left to right (ADR-1004 D3, D5). Drawn in the test font set at 16 pixels, 26 by 5 cells, with Hebrew from Noto Sans Hebrew and Arabic from Noto Sans Arabic. The program writes each row in the order it is read: abc, the Hebrew word shalom, (1) and def; the Arabic words marhaban bil-alam, a colon, 123 and (abc); the Hebrew word tapuz, [42], and, the Arabic word burtuqal and {7}; the Arabic words as-salamu alaykum and the date 2024-11-06; and the Hebrew words shalom and olam, olam in brackets, then 3.14. Correct: every row starts at the left edge. Each run of Hebrew or Arabic reads right to left, so shalom's shin is its rightmost letter, and the words of a run stand from right to left in the order they were written. The first row reads abc (1) shalom def: the number and its brackets go with the Hebrew word before them. The second row reads 123, the colon, the two Arabic words and (abc): the colon and 123 go with the Arabic before them. The third row reads [42], tapuz, and, {7}, burtuqal. In the fourth row the date's three numbers stand in right-to-left order, 06-11-2024, each number still read left to right, because numbers after Arabic letters take the Arabic text's direction. The fifth row reads 3.14, then olam in its brackets, then shalom. Digits always read left to right. A bracket inside right-to-left text draws the glyph of its mirror image, so each pair still opens toward what it holds. Each Arabic letter takes the form its neighbours give it, so each word is joined, and the lam and alef of salam draw as their single ligature across their two cells.";

const BIDI_MIXED_RTL_DESC: &str = "The bidi-mixed screen with every paragraph right to left: the program sends SCP 2 (CSI 2 SP k) before its first row, and each row after it takes that direction (ADR-1004 D2, D5). Drawn in the test font set at 16 pixels, 26 by 5 cells. Correct: every row ends at the right edge, with its empty cells at the left. Each row reads from the right in the order it was written: the first row has abc at the right edge, then shalom, then (1), then def at the left. Latin words and numbers still read left to right inside their own runs. Brackets inside right-to-left text are mirrored, and the Arabic letters are joined, with the lam-alef ligature in salam.";

const BIDI_EXPLICIT_DESC: &str = "Explicit mode, which a program that lays out right-to-left text itself turns on with BDSM reset (CSI 8 l): vtium runs no algorithm and draws each row's cells in the order the program wrote them (ADR-1004 D4). Drawn in the test font set at 16 pixels, 18 by 4 cells. The first two rows are left-to-right paragraphs whose text the program has already put in the order it is drawn: abc, shalom spelled backwards, (1) and def; and 123, then marhaban bil-alam with each word spelled backwards and the two words swapped. Then the program sends SCP 2, and the last two rows are right-to-left paragraphs: shalom, cba and (1); and marhaban, 321 and beh in square brackets. Correct: the first two rows start at the left edge, and their Hebrew and Arabic read right to left because the program wrote them backwards: abc shalom (1) def, and 123 then marhaban bil-alam. The Arabic letters are joined, read from right to left as they are drawn. The last two rows end at the right edge, the first character the program wrote rightmost: shalom at the right, then abc, then (1) at the left; and marhaban at the right, joined, then 123, then beh between square brackets at the left. In those two rows every bracket draws its mirror image, so each pair still opens toward what it holds.";

const BIDI_CURSOR_BLOCK_DESC: &str = "A block cursor on a right-to-left letter, drawn in the test font set at 16 pixels, 14 by 2 cells: abc, shalom and def, then marhaban and abc, with the cursor shown on hah, the third letter of marhaban, its shape set by DECSCUSR 2. The cursor stays on the cell the program wrote, wherever that cell is drawn (ADR-1004 D9). Correct: marhaban reads right to left with its letters joined, and the block covers hah, the middle letter of the five, in the third cell from the left, drawn in the cursor's colors.";

const BIDI_CURSOR_BAR_DESC: &str = "The bidi-cursor-block screen with a bar cursor, set by DECSCUSR 6, on hah, the third letter of marhaban. Correct: the bar stands on the right side of the cursor's cell, the third from the left, because hah reads right to left; a bar on a left-to-right letter stands on the left side of its cell (ADR-1004 D9). The bar takes the place of the letter in its cell, as a bar cursor does on any letter.";

const BIDI_SELECTION_DESC: &str = "A selection across a row that mixes directions, on the bidi-mixed screen: the pointer drags from the c of abc, the third cell from the left of the first row, to the tenth cell from the left, where the first row draws the vav of shalom. The selection holds the cells between those two in the order the program wrote them (ADR-1004 D10). Correct: the first row reads abc (1) shalom def. The selected cells are c, the space after it, and shin, lamed and vav, the first three letters of shalom, which stand at the right end of the word as it is drawn. So the selection shows in two pieces: c and the space at the left, and the three letters of shalom further right, with (1) and the final mem unselected between them.";

const BIDI_UNIFONT_DESC: &str = "Arabic in the built-in font, GNU Unifont, as the console draws it for a display with no font set, 20 by 3 cells at 16 pixels: marhaban bil-alam, a colon and 123; as-salamu alaykum; and lam with alef alone, after beh, and in brackets. Unifont is a bitmap font, which the glyph helper does not shape, so each joined letter draws the glyph Unifont has for the Arabic presentation form of its joined form (ADR-1004 D6). Correct: the words read right to left, each letter in its initial, medial or final form, so each word is joined. Lam with alef draws Unifont's lam-alef ligature, isolated or final, which Unifont draws one cell wide, centred across the ligature's two cells. The brackets round the last lam-alef are mirrored, since they stand in right-to-left text.";

/// Every case, in the order the report lists them: the screens below,
/// then the ink screens, then the right-to-left screens.
pub const CASES: &[Case] =
    &join::<{ BASE.len() + 4 * INK_CASES.len() + BIDI_CASES.len() }>(BASE, &INK_CASES, &BIDI_CASES);

/// `base` followed by each of `ink`'s cases, then `rest`, `N` in all.
const fn join<const N: usize>(base: &[Case], ink: &[[Case; 4]], rest: &[Case]) -> [Case; N] {
    assert!(
        N == base.len() + 4 * ink.len() + rest.len(),
        "every case once"
    );
    let mut all = [base[0]; N];
    let mut i = 0;
    while i < base.len() {
        all[i] = base[i];
        i += 1;
    }
    let mut k = 0;
    while k < ink.len() {
        let mut j = 0;
        while j < 4 {
            all[base.len() + 4 * k + j] = ink[k][j];
            j += 1;
        }
        k += 1;
    }
    let mut r = 0;
    while r < rest.len() {
        all[base.len() + 4 * ink.len() + r] = rest[r];
        r += 1;
    }
    all
}

/// The screens before the ink screens.
const BASE: &[Case] = &[
    Case {
        screen: Screen {
            font: FontKind::Sprites {
                width: 9,
                height: 16,
            },
            ..Screen::new("blocks", BLOCKS_DESC, BLOCKS, 48, 10)
        },
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
    },
    Case {
        screen: Screen::new(
            "wide-deco-unifont",
            "Wide characters under each underline style and under strike, a decorated run \
             mixing narrow and wide, and a decorated wide character written at the last \
             column, which wraps to the start of the next row. Correct: \
             every decoration spans both cells of its wide character.",
            WIDE_DECO,
            48,
            10,
        ),
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
    },
    Case {
        screen: Screen {
            pointer: Some((5, 0)),
            ..Screen::new("link-hover-unifont", LINK_HOVER_DESC, LINK_HOVER, 40, 7)
        },
    },
    Case {
        screen: Screen {
            pointer: Some((5, 0)),
            depth: Depth::One,
            ..Screen::new(
                "link-hover-unifont-1bit",
                "link-hover-unifont on a 1-bit display. Correct: the hovered link's dashes lit like its text, unlit on the white and reversed pieces, where the text is unlit too.",
                LINK_HOVER,
                40,
                7,
            )
        },
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
    },
    Case {
        screen: Screen {
            display: Some((640, 480)),
            font: FontKind::Psf(TestFont::Small8x16),
            ..Screen::new(
                "small-640x480",
                "A 640x480 display drawn with an 8x16 PSF1 font as 80 by 30 cells (vtium/vtium#75): the small-320x240 screen's parts at 80 columns, with `ls -l`, three boxes in light, double, heavy and rounded lines around bold, underlined, struck, reversed, italic, dim and blinking words, ramps through the 256-color palette and in 24-bit color, more sprites and Latin-1, a combining mark, and the decorations in color: each underline style in its own SGR 58 color, green struck text, and blue text struck over a red curl. Correct: as small-320x240, at 80 columns: the italic word leaning, sheared by 0.2 about the font's baseline a pixel row at a time, since the font has no italic, the dim word fainter, the blinking word drawn as text, and a combined character as its base letter.",
                SMALL_80,
                80,
                30,
            )
        },
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
    },
    Case {
        screen: Screen {
            font: FontKind::Set {
                font: TestFont::Small8x8,
                fill: Some("unscii-8"),
            },
            ..Screen::new(
                "vitrified-320x240",
                "The small-320x240 screen drawn from the glyphs vtium-config's engine generates from a frit naming the 8x8 PSF2 test font, with the built-in Unscii-8 filling the characters it lacks (ADR-1301 D2, vtium/vtium#126, vtium/vtium#145). Correct: as small-320x240 in the same 40 by 30 cells of 8x8, the box, shades, Braille and sextants still sprites, since the sprites win over a fill font's glyphs; but `中` and the snowman, which neither font holds, drawn as Unscii-8's U+25A1, the missing glyph, where small-320x240 draws `?`.",
                SMALL_40,
                40,
                30,
            )
        },
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
                "The small-640x480 screen drawn from the glyphs vtium-config's engine generates from a frit naming the 8x16 PSF1 test font, with the built-in unifont filling the characters it lacks (ADR-1301 D2, vtium/vtium#126, vtium/vtium#145). Correct: as small-640x480 in the same 80 by 30 cells of 8x16, the boxes, shades and Braille still sprites, since the sprites win over a fill font's glyphs; but `中文` in unifont's two-cell glyphs and the snowman and the note in its one-cell glyphs, where small-640x480 draws `?`.",
                SMALL_80,
                80,
                30,
            )
        },
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
        "notice-no-console-opened-800x480",
        NO_CONSOLE_OPENED_DESC,
        shown(&[notices::no_console_opened]),
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
    alone_small("panic-800x480", PANIC_DESC, notices::panic_screen),
    alone_small(
        "notice-alone-no-display-800x480",
        ALONE_NO_DISPLAY_DESC,
        notices::no_display,
    ),
    alone_small(
        "notice-alone-partly-800x480",
        ALONE_PARTLY_DESC,
        notices::partly,
    ),
    alone_small(
        "notice-alone-stopped-800x480",
        ALONE_STOPPED_DESC,
        notices::stopped,
    ),
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
        "notice-no-console-opened-3840x540",
        NO_CONSOLE_OPENED_DESC,
        shown(&[notices::no_console_opened]),
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
    alone_large("panic-3840x540", PANIC_DESC, notices::panic_screen),
    alone_large(
        "notice-alone-no-display-3840x540",
        ALONE_NO_DISPLAY_DESC,
        notices::no_display,
    ),
    alone_large(
        "notice-alone-partly-3840x540",
        ALONE_PARTLY_DESC,
        notices::partly,
    ),
    alone_large(
        "notice-alone-stopped-3840x540",
        ALONE_STOPPED_DESC,
        notices::stopped,
    ),
    Case {
        screen: Screen {
            alone: Some(notices::panic_screen),
            ..Screen::new("panic-320x240", PANIC_SMALL_DESC, BLANK, 40, 15)
        },
    },
    scheme(
        "scheme-light-notice",
        "The example configuration's `light` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session on a light terminal, OSC 11's white behind OSC 10's dark grey. Correct: a dark red `#991b1b` box with a darker `#450a0a` edge across the top, the width of the display, holding the notice in white `#ffffff`; the filled circle in dark `#1f2933` on pale grey `#f3f4f6` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them, dark on white.",
        OSD_SHELL_LIGHT,
        OSD_MINIMAL_NOTICE,
        &keys::LIGHT,
    ),
    scheme(
        "scheme-light-osd",
        "The example configuration's `light` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over the same session on a light terminal. Correct: a pale grey `#f3f4f6` box with a grey `#9ca3af` edge across the top, the width of the display, holding `● desk · console 2 of 3` in dark `#1f2933`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them, dark on white.",
        OSD_SHELL_LIGHT,
        OSD_FULL,
        &keys::LIGHT,
    ),
    scheme(
        "scheme-dark-notice",
        "The example configuration's `dark` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session. Correct: a red `#b91c1c` box with a light red `#f87171` edge across the top, the width of the display, holding the notice in pale `#fee2e2`; the filled circle in light grey `#d1d5db` on near black `#111827` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_MINIMAL_NOTICE,
        &keys::DARK,
    ),
    scheme(
        "scheme-dark-osd",
        "The example configuration's `dark` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over a shell session. Correct: a near black `#111827` box with a grey `#4b5563` edge across the top, the width of the display, holding `● desk · console 2 of 3` in light grey `#d1d5db`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_FULL,
        &keys::DARK,
    ),
    scheme(
        "scheme-color-blind-safe-notice",
        "The example configuration's `color-blind-safe` color scheme (ADR-1401 D5) on the notice that no keyboard was found, with the on-screen display's dot under `osd = minimal`, over a shell session. Correct: a bright orange `#ffb000` box with a darker orange `#fe6100` edge across the top, the width of the display, holding the notice in near black `#1c1917`; the filled circle in near white `#eef2ff` on dark blue `#0b2a5b` over the top-right cell, at the box's top right; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_MINIMAL_NOTICE,
        &keys::COLOR_BLIND_SAFE,
    ),
    scheme(
        "scheme-color-blind-safe-osd",
        "The example configuration's `color-blind-safe` color scheme (ADR-1401 D5) on the on-screen display's line under `osd = full`, over a shell session. Correct: a dark blue `#0b2a5b` box with a blue `#648fff` edge across the top, the width of the display, holding `● desk · console 2 of 3` in near white `#eef2ff`; the row just below the box partly covered, on purpose; and the rows below it as the terminal drew them.",
        OSD_SHELL,
        OSD_FULL,
        &keys::COLOR_BLIND_SAFE,
    ),
    picture(Screen {
        pictures: Some(pictures::kitty),
        ..Screen::new(
            "pictures-kitty",
            PICTURES_KITTY_DESC,
            pictures::KITTY_TEXT,
            64,
            12,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::kitty),
        orientation: Orientation::Right,
        display: Some((64 * 8 + 5, 12 * 16 + 3)),
        ..Screen::new(
            "pictures-kitty-right",
            PICTURES_RIGHT_DESC,
            pictures::KITTY_TEXT,
            64,
            12,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::kitty),
        selection: Some(((3, 4), (20, 9))),
        ..Screen::new(
            "pictures-selection",
            PICTURES_SELECTION_DESC,
            pictures::KITTY_TEXT,
            64,
            12,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::sixels),
        made_cell: Some((10, 20)),
        ..Screen::new(
            "pictures-sixel",
            PICTURES_SIXEL_DESC,
            pictures::SIXEL_TEXT,
            56,
            6,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::budget),
        ..Screen::new(
            "pictures-budget",
            PICTURES_BUDGET_DESC,
            b"budget: copy, then scaled each frame",
            20,
            4,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::tiled),
        largest_texture: Some(64),
        ..Screen::new(
            "pictures-tiled",
            PICTURES_TILED_DESC,
            b"pictures held in tiles of 64 pixels on the GPU path\r\ntext under the third",
            80,
            16,
        )
    }),
    // The outline screens (ADR-1503 D9), in the test font set's outline
    // fonts, which `make tier2-fonts` fetches
    Case {
        screen: outline(
            "ascii",
            "Printable ASCII in the regular face: a pangram in both cases, digits and punctuation, runs of narrow and wide letters, and a line of code. In DejaVu Sans Mono at 16 pixels, its 9x16 cell. Correct: every glyph whole, anti-aliased and inside its own cell, with columns aligned down the screen.",
            ASCII,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "bold",
            "Bold beside regular, a bold pangram over two lines, bold in red, green and blue, then italic beside regular and bold italic. In DejaVu Sans Mono at 16 pixels, with its Bold, Oblique and Bold Oblique faces. Correct: the bold text drawn from DejaVu Sans Mono Bold, heavier than the regular text and at the same cell width; the colored words in their colors; the italic text slanted, drawn from DejaVu Sans Mono Oblique, and the bold italic from DejaVu Sans Mono Bold Oblique, a letter's ink leaning into the cell beside it where the face draws it there (ADR-1306 D12).",
            FACES,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "palette",
            "The 16 ANSI background colors, the 16 foreground colors on letter pairs, a ramp through the 256-color palette, and a 24-bit gradient from red to green. In DejaVu Sans Mono at 16 pixels. Correct: each block filled edge to edge in its own color, with no gaps between cells, and each letter pair in its color.",
            PALETTE,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "inverse",
            "Reverse video (SGR 7): one word, a whole line, red on green swapped, and bold with reverse. In DejaVu Sans Mono at 16 pixels. Correct: foreground and background exchanged, so the swapped word is green on red, and the bold reversed text drawn from the bold face.",
            INVERSE,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "box",
            "Box drawing in light and double lines, shades and block elements, eighth blocks, and rounded, heavy and diagonal pieces, with the word cell in DejaVu Sans Mono at 16 pixels. Correct: vtium's sprites draw the lines and blocks, so lines join from cell to cell with no gaps and blocks fill their 9x16 cells.",
            BOX,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "underline",
            "Each underline style (single, double, curly, dotted, dashed) in the text color and in an SGR 58 color, over descenders; a red curl ended by SGR 59; underlines on a background, in reverse and in bold. In DejaVu Sans Mono at 16 pixels, the underline placed where the font's own tables put it. Correct: the five styles distinct, the colored ones in their SGR 58 color, the line after SGR 59 in the text color, and the dashed line one dash in each cell, from the cell's left edge for five eighths of its width.",
            UNDERLINE,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "strike",
            "Strike-through (SGR 9) alone, in color, on a background, with straight and curly underlines, in bold, in reverse, and over spaces. In DejaVu Sans Mono at 16 pixels, the strike placed where the font's own tables put it. Correct: a line through every struck cell, the spaces included, at the same height in every cell.",
            STRIKE,
            48,
            10,
        ),
    },
    Case {
        screen: Screen {
            decorations: DECO_CONF,
            ..outline(
                "deco-config",
                "Underline and strike with the configured colors (underline-color=196, strike-color=#00c0ff), beside SGR 58 colors and colored text. In DejaVu Sans Mono at 16 pixels. Correct: underlines and strikes without an SGR 58 color take the configured colors; an SGR 58 color still applies.",
                DECO_CONFIG,
                48,
                10,
            )
        },
    },
    Case {
        screen: Screen {
            decorations: Decorations {
                force: true,
                ..DECO_CONF
            },
            ..outline(
                "deco-force",
                "The deco-config screen with force-decoration-colors set. In DejaVu Sans Mono at 16 pixels. Correct: every underline and strike in the configured color, the SGR 58 ones included.",
                DECO_CONFIG,
                48,
                10,
            )
        },
    },
    Case {
        screen: outline(
            "combining",
            "Accented letters precomposed and as a base letter with combining marks, in words, underlined, struck and in bold. In DejaVu Sans Mono at 16 pixels, whose glyphs vtium-config's engine shapes for each cluster. Correct: each mark sits on its base letter, matching the precomposed form where there is one; the grave over the tilde on each a passes the top of its row and draws whole in the row above, where the font puts it (ADR-1306 D1, D2).",
            COMBINING,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "mosaic",
            "Every Braille pattern, sextant and octant in codepoint order; then checkerboards of sextants and of octants, a grid of eight-dot Braille, a Braille wave, and sextant and octant bands that cross the boundaries between rows; then the checkerboards and grid in yellow on blue; in the 9x16 cell DejaVu Sans Mono gives at 16 pixels. Correct: vtium's sprites draw them all, square dots on the same 2x4 grid in every cell and blocks on exact halves and thirds or quarters of the cell, so the checkerboards and bands run across cells with no seam.",
            MOSAIC,
            72,
            16,
        ),
    },
    Case {
        screen: Screen {
            font: FontKind::Outline {
                color: false,
                spacing: None,
            },
            ..Screen::new(
                "emoji-mono",
                "The emoji screen in the test font set without its color emoji font: DejaVu Sans Mono, then Noto Sans CJK, at 16 pixels. Correct: no emoji in color; each emoji drawn in the text color as the glyph DejaVu Sans Mono or Noto Sans CJK has for it, else as the built-in font's, at the same places and widths as on the emoji screen.",
                EMOJI,
                48,
                10,
            )
        },
    },
    Case {
        screen: Screen {
            next: Some(
                b"\x1b[2;1Hline 2 CHANGED \x1b[4:3mcurly\x1b[0m\x1b[4;1H\x1b[9mnew struck\x1b[0m",
            ),
            ..outline(
                "redraw",
                "Four frames into a double-buffered display: the second part rewrites line 2 and adds a struck line 4, and both buffers must then hold it and equal a fresh draw. In DejaVu Sans Mono at 16 pixels. Correct: line 2 reads 'line 2 CHANGED curly' with nothing left of the first part's text.",
                b"unchanged line\r\nline two, before\r\nline three stays\r\n",
                48,
                10,
            )
        },
    },
    // The outline screens that waited for fixes to wide characters,
    // emoji presentation and the console's notices (vtium/vtium#323,
    // #324, #325, #326)
    Case {
        screen: outline(
            "cjk",
            "Double-width Japanese, Chinese and Korean, full-width Latin, narrow letters between wide characters, and wide text in bold and in reverse. The wide characters come from Noto Sans CJK, the narrow ones from DejaVu Sans Mono, at 16 pixels in the 9x16 cell. Correct: each wide character drawn by Noto Sans CJK across two cells, the bold ones emboldened, since the font set names no bold CJK face, and the narrow letters between them in their own columns.",
            CJK,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "emoji",
            "Emoji: two wide emoji and a narrow text smiley; a ZWJ family, a skin tone, a flag and a keycap; a heart with VS16, VS15 and no selector; an emoji between letters; emoji under underline, strike, reverse and a red curl; the shaking face of Unicode 15; and an emoji written at the last column, which wraps. The emoji come from Noto Color Emoji 2.051, the text from DejaVu Sans Mono, at 16 pixels. Correct: each emoji in color, whole and fitted to its two cells whatever the foreground and reverse; the family, the tone, the flag and the keycap one picture each where the font holds one; the VS16 heart in color across two cells, the other hearts and the smiley one cell each in the text color.",
            EMOJI,
            48,
            10,
        ),
    },
    Case {
        screen: outline(
            "vs16",
            "A heart with VS16, with no selector and with VS15, and the VS16 heart underlined, by the Unicode width method (mode 2027 set). At 16 pixels in the test font set. Correct: the VS16 hearts take two cells and draw as Noto Color Emoji's red heart, the b after each a column further right; the other hearts one cell each, DejaVu Sans Mono's text heart in the text color.",
            VS16,
            32,
            5,
        ),
    },
    Case {
        screen: outline(
            "wide-deco",
            "Wide characters under each underline style and under strike, a decorated run mixing narrow and wide, and a decorated wide character written at the last column, which wraps to the start of the next row. Noto Sans CJK and DejaVu Sans Mono at 16 pixels. Correct: every decoration spans both cells of its wide character, the dashed line one dash in each cell.",
            WIDE_DECO,
            48,
            10,
        ),
    },
    Case {
        screen: Screen {
            overlay: Some(Overlay {
                line: Some(notices::render_timer),
                notices: &[],
                mark: None,
                scheme: None,
            }),
            ..outline(
                "osd",
                "The render timer's line, `59.9 fps 12.345 ms 100.0%`, drawn by the console's overlay as the on-screen display's line over three rows of text in DejaVu Sans Mono at 16 pixels. Correct: the line in the on-screen display's colors in a box across the top of the display, over the text there; its characters drawn by the built-in font in the 9x16 cell, since the console draws its notices and the on-screen display from the built-in font whatever the head's font set is; the row just below the box partly covered, on purpose, since the box is painted over the terminal's text and is not a whole number of rows tall; and the rows below it as the terminal drew them.",
                OSD_TEXT,
                48,
                10,
            )
        },
    },
    // The spaced outline screens: the test font set with line spacing
    // (ADR-1302 D13), each screen at an even split and an odd one
    Case {
        screen: spaced(
            "mosaic-spaced-12",
            concat!(
                "The mosaic screen with line spacing: the test font set's line-spacing=12 adds 12 per cent of the 16-pixel text height, 1.92 pixel rows, which round to 2, 1 above the text band and 1 below it, so DejaVu Sans Mono's 9x16 cell is 9x18. ",
                mosaic_spaced!(),
                " The labels sit 1 row lower in the cell than with no spacing."
            ),
            MOSAIC,
            (72, 16),
            SPACED_12,
        ),
    },
    Case {
        screen: spaced(
            "mosaic-spaced-30",
            concat!(
                "The mosaic screen with line spacing: the test font set's line-spacing=30 adds 30 per cent of the 16-pixel text height, 4.8 pixel rows, which round to 5, 3 above the text band and 2 below it, the odd row above, so DejaVu Sans Mono's 9x16 cell is 9x21. ",
                mosaic_spaced!(),
                " The labels sit 3 rows lower in the cell than with no spacing."
            ),
            MOSAIC,
            (72, 16),
            SPACED_30,
        ),
    },
    Case {
        screen: spaced(
            "emoji-spaced-12",
            concat!(
                "Emoji and Latin text with line spacing: the test font set's line-spacing=12 adds 12 per cent of the 16-pixel text height, 1.92 pixel rows, which round to 2, 1 above the text band and 1 below it, so DejaVu Sans Mono's 9x16 cell is 9x18. ",
                emoji_spaced!(),
                " Each text band sits 1 row lower in its cell than with no spacing."
            ),
            EMOJI_SPACED,
            (48, 10),
            SPACED_12,
        ),
    },
    Case {
        screen: spaced(
            "emoji-spaced-30",
            concat!(
                "Emoji and Latin text with line spacing: the test font set's line-spacing=30 adds 30 per cent of the 16-pixel text height, 4.8 pixel rows, which round to 5, 3 above the text band and 2 below it, the odd row above, so DejaVu Sans Mono's 9x16 cell is 9x21. ",
                emoji_spaced!(),
                " Each text band sits 3 rows lower in its cell than with no spacing."
            ),
            EMOJI_SPACED,
            (48, 10),
            SPACED_30,
        ),
    },
    // The outline picture screens: pictures over text in the test font
    // set, sent as kitty commands and sixel strings through the
    // terminal's own handlers (ADR-1100)
    picture(Screen {
        pictures: Some(pictures::img_bands),
        ..outline("img-bands", IMG_BANDS_DESC, BANDS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::kitty_bands),
        ..outline("kitty-bands", KITTY_BANDS_DESC, BANDS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_sixel),
        ..outline("img-sixel", IMG_SIXEL_DESC, SIXEL_ROWS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(sixel_vte),
        ..outline("sixel-vte", SIXEL_VTE_DESC, SIXEL_VTE_TEXT, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_scale),
        ..outline("img-scale", IMG_SCALE_DESC, SCALE_LABELS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_region),
        ..outline("img-region", IMG_REGION_DESC, REGION_ROWS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_rotated),
        orientation: Orientation::Right,
        ..outline(
            "img-rotated",
            IMG_ROTATED_DESC,
            b"rotated: pictures turn with the text",
            24,
            6,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::img_moves),
        next: Some(b""),
        ..outline("img-moves", IMG_MOVES_DESC, MOVES_ROWS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_banner_bg),
        next: Some(b""),
        ..outline("img-banner-bg", IMG_BANNER_BG_DESC, BANNER_REGION, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::img_banner_text),
        next: Some(b""),
        ..outline(
            "img-banner-text",
            IMG_BANNER_TEXT_DESC,
            BANNER_REGION,
            48,
            10,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::img_ed2),
        next: Some(b"\x1b[2J\x1b[Hcleared"),
        ..outline("img-ed2", IMG_ED2_DESC, ED2_ROWS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::kitty_text),
        ..outline("kitty-text", KITTY_TEXT_DESC, KITTY_TEXT_ROWS, 48, 10)
    }),
    picture(Screen {
        pictures: Some(pictures::kitty_banner_bg),
        next: Some(b""),
        ..outline(
            "kitty-banner-bg",
            KITTY_BANNER_BG_DESC,
            BANNER_REGION,
            48,
            10,
        )
    }),
    picture(Screen {
        pictures: Some(pictures::kitty_banner_text),
        next: Some(b""),
        ..outline(
            "kitty-banner-text",
            KITTY_BANNER_TEXT_DESC,
            BANNER_REGION,
            48,
            10,
        )
    }),
];

/// Three rows of text, which the `osd` screen draws the render timer's
/// line over.
const OSD_TEXT: &[u8] = concat!(
    "sixels over text, text over sixels, sixels over\r\n",
    "sixels over text, text over sixels, sixels over\r\n",
    "sixels over text, text over sixels, sixels over\r\n",
)
.as_bytes();

/// The outline screens whose drawing shows a defect, each listed in
/// `pending.tsv` with the issue that owes the fix. `tier2 run` does not
/// draw them, so no golden records the defect; `tier2 render` draws them
/// with the others, so the defect can be seen. When its fix lands, a
/// screen moves to [`CASES`], its golden is added and its pending row
/// goes.
pub const WAITING: &[Case] = &[];
