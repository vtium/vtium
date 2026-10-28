//! The card: vtium's test card as a byte stream, in blocks of 40x30 cells.
//!
//! A block is one 320x240 screen at 8x8 (#118). Each is written cell by
//! cell from its own top left with CUP, so the same block draws the same
//! cells wherever a page places it, and a page holds as many as its grid
//! fits. Every line ends with SGR 0, so no pen crosses into another line.
//!
//! The blocks: the header and text; the attributes; color; the sprites,
//! symbols and emoji, ending with a wide character at the right margin.
//! Every escape is one ADR-1000 lists as supported; `tools/specimens`'
//! README lists what the card leaves out and why.

use std::fmt::Write as _;

use crate::profiles::{Glyphs, Layout, Profile};

pub const BLOCK_COLS: usize = 40;
pub const BLOCK_ROWS: usize = 30;
pub const BLOCKS: usize = 4;

/// The blocks' titles, in order.
pub const TITLES: [&str; BLOCKS] = ["Text", "Attributes", "Color", "Sprites and symbols"];

/// The row of the last block whose line ends in a wide character at the
/// grid's last column, which wraps whole to the row below's first column.
/// That row's first two columns are blank in every block.
const MARGIN_ROW: usize = 28;

/// One block being written: its place on the page.
struct Block<'a> {
    out: &'a mut String,
    top: usize,
    left: usize,
}

impl Block<'_> {
    /// `text` at the block's `row` and `col`, then SGR 0.
    fn put(&mut self, row: usize, col: usize, text: &str) {
        debug_assert!(row < BLOCK_ROWS && col < BLOCK_COLS);
        let _ = write!(
            self.out,
            "\x1b[{};{}H{text}\x1b[m",
            self.top + row + 1,
            self.left + col + 1
        );
    }

    /// The block's title across its first row, reversed, and its number at
    /// the right of its last row, dim.
    fn frame(&mut self, n: usize, title: &str) {
        self.put(0, 0, &format!("\x1b[1;7m{:<40}", format!(" {title}")));
        let foot = format!("{}/{}", n + 1, BLOCKS);
        self.put(
            BLOCK_ROWS - 1,
            BLOCK_COLS - foot.len(),
            &format!("\x1b[2m{foot}"),
        );
    }
}

/// SGR `params` as an escape.
fn sgr(params: &str) -> String {
    format!("\x1b[{params}m")
}

/// The characters `first` to `last`, every `step`th.
fn run(first: u32, last: u32, step: usize) -> String {
    (first..=last)
        .step_by(step)
        .filter_map(char::from_u32)
        .collect()
}

/// Page `page` of the card for `profile` on `layout`: a cleared screen, the
/// cursor hidden, and the page's blocks.
pub fn page(profile: &Profile, layout: &Layout, page: usize) -> Vec<u8> {
    let mut out = String::from("\x1b[?25l\x1b[H\x1b[2J");
    for (n, top, left) in layout.blocks(page) {
        let mut b = Block {
            out: &mut out,
            top,
            left,
        };
        b.frame(n, TITLES[n]);
        match n {
            0 => text(&mut b, profile, layout),
            1 => attributes(&mut b),
            2 => color(&mut b),
            _ => sprites(&mut b, layout),
        }
    }
    out.into_bytes()
}

/// The header naming the profile, then text in the scripts vtium draws.
fn text(b: &mut Block<'_>, p: &Profile, l: &Layout) {
    let set = match p.glyphs {
        Glyphs::Builtin { set, .. } => set,
        Glyphs::Pending { font, .. } => font,
    };
    let cell = match p.glyphs {
        Glyphs::Builtin { .. } => {
            format!("{}x{}", p.width / l.cols as u32, p.height / l.rows as u32)
        }
        Glyphs::Pending { .. } => "-".to_owned(),
    };
    b.put(0, 0, &format!("\x1b[1;7m VTium test card{:>23} ", p.name));
    b.put(
        1,
        1,
        &format!("display \x1b[1m{}x{}\x1b[m {}", p.width, p.height, p.depth),
    );
    b.put(
        2,
        1,
        &format!(
            "cell \x1b[1m{cell}\x1b[m grid \x1b[1m{}x{}\x1b[m",
            l.cols, l.rows
        ),
    );
    b.put(3, 1, &format!("set \x1b[1m{set}\x1b[m  class {}", p.class));
    b.put(4, 0, &"─".repeat(BLOCK_COLS));
    let dim = |s: &str| format!("\x1b[2m{s}");
    b.put(5, 0, &dim("ASCII"));
    b.put(6, 0, &run(0x21, 0x48, 1));
    b.put(7, 0, &run(0x49, 0x70, 1));
    b.put(8, 0, &run(0x71, 0x7e, 1));
    b.put(9, 0, &dim("Latin-1 and Latin Extended"));
    b.put(10, 0, "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖØÙÚÛÜÝÞß");
    b.put(11, 0, "àáâãäåæçèéêëìíîïðñòóôõöøùúûüýþÿ ¡¿«»");
    b.put(12, 0, "ĀāĂăĄąĆćČčĎďĒēĘęĚěĞğĪīĮįİıŁłŃńŇňŐőŒœŘř");
    b.put(13, 0, "ŚśŞşŠšŢţŤťŮůŰűŸŹźŻżŽžƒǍǎǦǧȘșȚț");
    b.put(14, 0, &dim("Greek and Cyrillic"));
    b.put(15, 0, "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ ΆΈΉΊΌΎΏ");
    b.put(16, 0, "αβγδεζηθικλμνξοπρςστυφχψω άέήίόύώϊϋ");
    b.put(17, 0, "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ ЄІЇЎ");
    b.put(18, 0, "абвгдеёжзийклмнопрстуфхцчшщъыьэюя єіїў");
    b.put(19, 0, &dim("Combining marks, then precomposed"));
    b.put(
        20,
        0,
        "e\u{301}a\u{300}o\u{302}n\u{303}u\u{308}c\u{327}a\u{30a}z\u{30c} q\u{323}\u{307} | éàôñüçåž",
    );
    b.put(21, 0, &dim("CJK and Hangul, two cells each"));
    b.put(22, 0, "漢字 中文 日本語 ひらがな カタカナ");
    b.put(23, 0, "한국어 한글 훈민정음 ＡＢＣ１２３");
    b.put(24, 0, "a中b文c|字|");
    b.put(25, 0, &dim("Punctuation and symbols"));
    b.put(26, 0, "‘’ “” – — … • † ‡ ‰ ′ ″ ‹› ¶ § © ® ™");
    b.put(27, 0, "←↑→↓↔↕⇐⇒⇔ ∀∂∃∅∇∈∉∏∑−√∞∧∨∩∪≈≠≡≤≥");
    b.put(28, 0, "€£¥¢₹₽₩₿ ½¼¾ ⁰¹²³ⁿ ₀₁₂ ♠♣♥♦ ★☆");
}

/// The attributes vtium draws, each on a line with its SGR.
fn attributes(b: &mut Block<'_>) {
    const SAMPLE: &str = "Sphinx of black quartz";
    let lines: [(&str, &str); 9] = [
        ("plain", ""),
        ("1 bold", "1"),
        ("2 dim", "2"),
        ("3 italic", "3"),
        ("7 reverse", "7"),
        ("8 conceal", "8"),
        ("9 strike", "9"),
        ("1;2", "1;2"),
        ("1;7;9", "1;7;9"),
    ];
    for (i, (label, params)) in lines.iter().enumerate() {
        b.put(1 + i, 0, &format!("{label:<10}{}{SAMPLE}", sgr(params)));
    }
    // the resets, one a word: italic, underline and strike go in turn, then
    // bold, blink and conceal together
    b.put(
        10,
        0,
        "22-29 off \x1b[1;3;4;5;8;9mSph\x1b[23minx\x1b[24m of\x1b[29m black\x1b[22;25;28m quartz",
    );
    b.put(11, 0, "\x1b[2mUnderline styles, SGR 4:n");
    let styles = ["single", "double", "curly", "dotted", "dashed"];
    for (i, style) in styles.iter().enumerate() {
        let n = i + 1;
        b.put(12 + i, 0, &format!("4:{n} {style:<7}\x1b[4:{n}m{SAMPLE}"));
    }
    b.put(17, 0, "\x1b[2mIn colors, SGR 58");
    let colored: [(&str, &str); 5] = [
        ("58;5;196", "4:1;58;5;196"),
        ("58:2::0:192:255", "4:2;58:2::0:192:255"),
        ("58;5;46", "4:3;58;5;46"),
        ("58:5:226", "4:4;58:5:226"),
        ("58;2;255;128;0", "4:5;58;2;255;128;0"),
    ];
    for (i, (label, params)) in colored.iter().enumerate() {
        b.put(
            18 + i,
            0,
            &format!("{label:<16}{}quartz judge", sgr(params)),
        );
    }
    b.put(23, 0, "\x1b[2mTogether");
    b.put(
        24,
        0,
        "\x1b[38;5;45;4:3;58;5;196mcyan, a red curl\x1b[m \x1b[9;31mred, struck",
    );
    b.put(25, 0, "\x1b[36;9;4;58;5;226mcyan struck, yellow underline");
    b.put(26, 0, "\x1b[7;4;58;5;201mreversed, magenta underline");
    // SGR 59 puts the underline back in the text's color
    b.put(27, 0, "\x1b[2;4;33;58;5;196;59mdim yellow, underlined");
    b.put(
        28,
        0,
        "\x1b[1;21;38;2;255;160;0mbold orange, SGR 21 doubled",
    );
}

/// The 16 colors, the 256-color palette, 24-bit color, and the grays a
/// gray panel's levels and conversion show on.
fn color(b: &mut Block<'_>) {
    b.put(1, 0, "\x1b[2m16 colors: SGR 30-37 and 90-97, bold");
    for (row, base) in [(2, 30), (3, 90)] {
        let s: String = (0..8)
            .map(|i| format!("{} {}", sgr(&(base + i).to_string()), base + i))
            .collect();
        b.put(row, 0, &s);
    }
    let s: String = (0..8)
        .map(|i| format!("\x1b[1;{}m 1;{}", 30 + i, 30 + i))
        .collect();
    b.put(4, 0, &s);
    b.put(5, 0, "\x1b[2mBackgrounds: SGR 40-47 and 100-107");
    for (row, base) in [(6, 40), (7, 100)] {
        let s: String = (0..8)
            .map(|i| format!("{}{:^5}", sgr(&(base + i).to_string()), base + i))
            .collect();
        b.put(row, 0, &s);
    }
    b.put(8, 0, "\x1b[2m256 colors: the cube, the 16, the grays");
    for g in 0..6 {
        let s: String = (0..6)
            .flat_map(|r| (0..6).map(move |bl| 16 + 36 * r + 6 * g + bl))
            .map(|i| format!("\x1b[48;5;{i}m "))
            .collect();
        b.put(9 + g, 0, &s);
    }
    let s: String = (0..16)
        .chain(232..256)
        .map(|i| format!("\x1b[48:5:{i}m "))
        .collect();
    b.put(15, 0, &s);
    b.put(16, 0, "\x1b[2m24-bit color: SGR 38;2 and 48;2");
    let step = |i: usize| hue(i * 360 / BLOCK_COLS);
    let s: String = (0..BLOCK_COLS)
        .map(|i| {
            let (r, g, bl) = step(i);
            format!("\x1b[48;2;{r};{g};{bl}m ")
        })
        .collect();
    b.put(17, 0, &s);
    let ramp = |i: usize, n: usize| (i * 255 / (n - 1)) as u8;
    let s: String = (0..BLOCK_COLS)
        .map(|i| {
            let v = ramp(i, BLOCK_COLS);
            format!("\x1b[38;2;0;{v};0;48;2;{v};0;{}m▀", 255 - v)
        })
        .collect();
    b.put(18, 0, &s);
    let s: String = "every letter its own hue"
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let (r, g, bl) = hue(i * 15);
            format!("\x1b[1;38;2;{r};{g};{bl}m{c}")
        })
        .collect();
    b.put(19, 0, &s);
    b.put(20, 0, "\x1b[2mReverse, dim, and SGR 39 and 49");
    b.put(
        21,
        0,
        "\x1b[31;44m red on blue \x1b[7m reversed \x1b[27;2m dimmed ",
    );
    b.put(
        22,
        0,
        "\x1b[38;5;208;48;5;18m orange on navy \x1b[2m dim \x1b[22;7m reversed \x1b[27;39;49m default",
    );
    b.put(23, 0, "\x1b[2mGrays: 232-255, then 24-bit steps");
    let s: String = (232..256).map(|i| format!("\x1b[48;5;{i}m ")).collect();
    b.put(24, 0, &s);
    let s: String = (0..BLOCK_COLS)
        .map(|i| {
            let v = ramp(i, BLOCK_COLS);
            format!("\x1b[48:2::{v}:{v}:{v}m ")
        })
        .collect();
    b.put(25, 0, &s);
    const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWX";
    for (row, on) in [(26, "49"), (27, "48;2;255;255;255")] {
        let s: String = (232..256)
            .zip(LETTERS.chars())
            .map(|(i, c)| format!("\x1b[{on};38;5;{i}m{c}"))
            .collect();
        b.put(row, 0, &s);
    }
    let half = BLOCK_COLS / 2;
    let s: String = ["49", "48;2;255;255;255"]
        .iter()
        .flat_map(|on| {
            (0..half).map(move |i| {
                let v = ramp(i, half);
                format!("\x1b[{on};38;2;{v};{v};{v}m{}", char::from(b'a' + i as u8))
            })
        })
        .collect();
    b.put(28, 0, &s);
}

/// The color `deg` degrees round the hue circle, at full saturation.
fn hue(deg: usize) -> (u8, u8, u8) {
    let x = |d: usize| ((d % 60) * 255 / 60) as u8;
    match (deg % 360) / 60 {
        0 => (255, x(deg), 0),
        1 => (255 - x(deg), 255, 0),
        2 => (0, 255, x(deg)),
        3 => (0, 255 - x(deg), 255),
        4 => (x(deg), 0, 255),
        _ => (255, 0, 255 - x(deg)),
    }
}

/// One period of a sine wave, 20 dots wide and 8 tall: each dot's height.
const WAVE: [usize; 20] = [4, 5, 6, 6, 7, 7, 7, 6, 6, 5, 4, 2, 1, 1, 0, 0, 0, 1, 1, 2];

/// The Braille cells of a wave 80 dots wide and 8 tall: two rows of 40.
fn braille_wave() -> [String; 2] {
    // the dot bits of a Braille cell's column and row
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    let mut cells = [[0u32; BLOCK_COLS]; 2];
    for x in 0..BLOCK_COLS * 2 {
        let y = 7 - WAVE[x % WAVE.len()];
        cells[y / 4][x / 2] |= BITS[x % 2][y % 4];
    }
    cells.map(|row| {
        row.iter()
            .filter_map(|&bits| char::from_u32(0x2800 + bits))
            .collect()
    })
}

/// vtium's sprites, then symbols and emoji, then a wide character at the
/// grid's right margin.
fn sprites(b: &mut Block<'_>, l: &Layout) {
    let boxes: [(&str, [char; 6]); 4] = [
        ("light", ['┌', '─', '┐', '│', '└', '┘']),
        ("heavy", ['┏', '━', '┓', '┃', '┗', '┛']),
        ("double", ['╔', '═', '╗', '║', '╚', '╝']),
        ("round", ['╭', '─', '╮', '│', '╰', '╯']),
    ];
    for (i, (name, [tl, h, tr, v, bl, br])) in boxes.iter().enumerate() {
        let h7: String = std::iter::repeat_n(*h, 7).collect();
        b.put(1, i * 10, &format!("{tl}{h7}{tr}"));
        b.put(2, i * 10, &format!("{v}{name:^7}{v}"));
        b.put(3, i * 10, &format!("{bl}{h7}{br}"));
    }
    b.put(4, 0, "┼┬┴├┤╋┳┻┣┫╬╦╩╠╣╪╫┿╂ ┄┅┈┉╌╍ ╱╲╳ ╴╵╶╷");
    b.put(5, 0, "▀▄█▌▐ ▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉ ▖▗▘▙▚▛▜▝▞▟");
    b.put(6, 0, "░░▒▒▓▓██ ▔▕ \x1b[32m▓▒░\x1b[33m░▒▓\x1b[m ⣿⡇⠿⠛");
    let [top, bottom] = braille_wave();
    b.put(7, 0, &format!("\x1b[36m{top}"));
    b.put(8, 0, &format!("\x1b[36m{bottom}"));
    let s: String = (0..BLOCK_COLS as u32)
        .filter_map(|i| char::from_u32(0x2800 + i * 255 / 39))
        .collect();
    b.put(9, 0, &s);
    b.put(10, 0, &run(0x1fb00, 0x1fb27, 1));
    b.put(
        11,
        0,
        &format!("{} {}", run(0x1fb28, 0x1fb3b, 1), run(0x1cd00, 0x1cd12, 1)),
    );
    b.put(12, 0, &run(0x1cd13, 0x1cde5, 6));
    b.put(13, 0, &run(0x1fb3c, 0x1fb63, 1));
    b.put(14, 0, &run(0x1fb64, 0x1fb8b, 1));
    b.put(
        15,
        0,
        &format!(
            "{}{}{}{}",
            run(0x1fb8c, 0x1fb92, 1),
            run(0x1fb94, 0x1fbaf, 1),
            run(0x1fbbd, 0x1fbbf, 1),
            run(0x1cea0, 0x1cea1, 1)
        ),
    );
    b.put(
        16,
        0,
        &format!("{}{}", run(0x1fbce, 0x1fbef, 1), run(0x1cea2, 0x1cea7, 1)),
    );
    b.put(17, 0, "\x1b[2mNerd Font private use");
    b.put(
        18,
        0,
        "\u{e0a0} \u{e0b0}\u{e0b1}\u{e0b2}\u{e0b3} \u{f015} \u{f07b} \u{f09b} \u{e7a8} \u{f121} \u{f0e7} \u{f303}",
    );
    b.put(19, 0, "\x1b[2mEmoji, a ZWJ sequence and a flag");
    b.put(20, 0, "😀 🎉 🚀 👍 🐧 ❤ 👩\u{200d}💻 🇫🇮");
    b.put(21, 0, "\x1b[2mVS15 text and VS16 emoji presentation");
    b.put(
        22,
        0,
        "☺\u{fe0e}|☺\u{fe0f}| ❤\u{fe0e}|❤\u{fe0f}| ✔\u{fe0e}|✔\u{fe0f}| #\u{fe0f}\u{20e3}|",
    );
    b.put(27, 0, "\x1b[2mA wide character at the margin wraps:");
    // the dots run to the grid's second last column, past the block when the
    // grid is wider; the last block has no neighbour to its right
    let dots = l.cols - (b.left + 1);
    debug_assert!(dots >= BLOCK_COLS - 1);
    let mut line = format!("\x1b[{};{}H\x1b[33m", b.top + MARGIN_ROW + 1, b.left + 1);
    line.extend(std::iter::repeat_n('·', dots));
    line.push_str("界\x1b[m");
    b.out.push_str(&line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_is_two_rows_of_braille() {
        let [top, bottom] = braille_wave();
        assert_eq!(top.chars().count(), BLOCK_COLS);
        assert_eq!(bottom.chars().count(), BLOCK_COLS);
        assert!(
            top.chars()
                .chain(bottom.chars())
                .all(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
        );
    }

    #[test]
    fn the_hue_circle_starts_red_and_turns_through_green_and_blue() {
        assert_eq!(hue(0), (255, 0, 0));
        assert_eq!(hue(120), (0, 255, 0));
        assert_eq!(hue(240), (0, 0, 255));
        assert_eq!(hue(359), (255, 0, 5));
    }
}
