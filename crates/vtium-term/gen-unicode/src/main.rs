//! Generates vtium-term's Unicode tables from the Unicode Character Database.
//! README.md says what it writes and how to run it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "18.0.0";

/// The UCD files read, by base name, with their SHA-256 and the path under
/// `https://www.unicode.org/Public/18.0.0/ucd/` they come from.
const FILES: [(&str, &str, &str); 7] = [
    (
        "GraphemeBreakProperty.txt",
        "auxiliary/",
        "0839dcb79e4ac639ecd538b1abf7c9d22e3f9dd265b7e182d33627aa4d75b45a",
    ),
    (
        "GraphemeBreakTest.txt",
        "auxiliary/",
        "b0cf047ee94485bbdc846de2b902f5f8a815f6b674f9d04223cddadd91c9df31",
    ),
    (
        "emoji-data.txt",
        "emoji/",
        "80d00f8e616a0ef27fd6b8de3b758c06383b5d917e2977709578e68baf733bf1",
    ),
    (
        "emoji-variation-sequences.txt",
        "emoji/",
        "ff1707564aa1f1b2fcf4ec92d609d4cb26940bc0d4dcf07f5328ad6879e84da3",
    ),
    (
        "DerivedCoreProperties.txt",
        "",
        "09c928886a178fcafd93c29e4bd59073a058e5a100b716d425cb563ab50f68c9",
    ),
    (
        "EastAsianWidth.txt",
        "",
        "a0cf29eacd00cfcaec4381c6b7c281685f18dbb4e7ff82b4076ccb342ca839aa",
    ),
    (
        "DerivedGeneralCategory.txt",
        "extracted/",
        "d6b151d2d40ee9b1876d26f417980f45ffae47b6055ccf7203cb31f07a030f94",
    ),
];

/// The licence of the UCD files, copied into each output.
const LICENSE: &str = include_str!("unicode-license.txt");

const CODEPOINTS: usize = 0x11_0000;

/// Grapheme_Cluster_Break values by their code in a property word, with the
/// suffix of their constant in `data.rs`.
const GCB: [(&str, &str); 14] = [
    ("Other", "OTHER"),
    ("CR", "CR"),
    ("LF", "LF"),
    ("Control", "CONTROL"),
    ("Extend", "EXTEND"),
    ("ZWJ", "ZWJ"),
    ("Regional_Indicator", "REGIONAL_INDICATOR"),
    ("Prepend", "PREPEND"),
    ("SpacingMark", "SPACING_MARK"),
    ("L", "L"),
    ("V", "V"),
    ("T", "T"),
    ("LV", "LV"),
    ("LVT", "LVT"),
];

/// Indic_Conjunct_Break values by their code.
const INCB: [(&str, &str); 4] = [
    ("None", "NONE"),
    ("Linker", "LINKER"),
    ("Consonant", "CONSONANT"),
    ("Extend", "EXTEND"),
];

// A property word: bits 0-3 Grapheme_Cluster_Break, bit 4
// Extended_Pictographic, bits 5-6 Indic_Conjunct_Break, bits 7-8 the cell
// width, bit 9 a base of an emoji-style variation sequence.
const GCB_MASK: u16 = 0x000f;
const PICT: u16 = 1 << 4;
const INCB_SHIFT: u16 = 5;
const INCB_MASK: u16 = 0b11 << INCB_SHIFT;
const WIDTH_SHIFT: u16 = 7;
const WIDTH_MASK: u16 = 0b11 << WIDTH_SHIFT;
const VS16_BASE: u16 = 1 << 9;

/// EastAsianWidth.txt's header: unassigned codepoints in these ranges are W.
const EAW_DEFAULT_W: [(u32, u32); 5] = [
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xF900, 0xFAFF),
    (0x2_0000, 0x2_FFFD),
    (0x3_0000, 0x3_FFFD),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ucd] = args.as_slice() else {
        eprintln!("usage: gen-unicode UCD_DIR");
        return ExitCode::from(2);
    };
    match run(Path::new(ucd)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-unicode: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(ucd: &Path) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |name: &str| read_pinned(ucd, name);

    let props = build(&read)?;
    let (starts, words) = runs(&props);
    let data = root.join("src/unicode/data.rs");
    write(&data, &data_rs(&starts, &words))?;
    println!(
        "src/unicode/data.rs: Unicode {VERSION}, {} runs, {} bytes",
        starts.len(),
        starts.len() * 6
    );

    let cases = break_cases(&read("GraphemeBreakTest.txt")?)?;
    let test = root.join("src/unicode/grapheme_break_test.txt");
    write(&test, &break_test_txt(&cases))?;
    println!("src/unicode/grapheme_break_test.txt: {} cases", cases.len());
    Ok(())
}

/// A pinned file's text, once its SHA-256 matches the pin.
fn read_pinned(ucd: &Path, name: &str) -> Result<String, String> {
    let (_, dir, pin) = FILES
        .iter()
        .find(|(n, _, _)| *n == name)
        .ok_or_else(|| format!("{name}: not a pinned file"))?;
    let path = ucd.join(name);
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "{}: {e}; fetch it from https://www.unicode.org/Public/{VERSION}/ucd/{dir}{name}",
            path.display()
        )
    })?;
    let got = hex(&sha256(&bytes));
    if got != *pin {
        return Err(format!("{name}: sha256 {got}, pinned {pin}"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{name}: {e}"))
}

fn write(path: &PathBuf, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Each data line of a UCD file as its codepoint range and its fields after
/// the first, comments and blank lines dropped.
fn records(text: &str) -> impl Iterator<Item = Result<(usize, usize, Vec<&str>), String>> {
    text.lines().filter_map(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            return None;
        }
        let mut fields = line.split(';').map(str::trim);
        let range = fields.next().unwrap_or("");
        let (first, last) = range.split_once("..").unwrap_or((range, range));
        let parse = |s: &str| {
            usize::from_str_radix(s, 16)
                .ok()
                .filter(|&cp| cp < CODEPOINTS)
                .ok_or_else(|| format!("bad codepoint in {line:?}"))
        };
        Some(parse(first).and_then(|f| Ok((f, parse(last)?, fields.collect()))))
    })
}

/// Sets `v[first..=last]` to `x`.
fn fill<T: Copy>(v: &mut [T], first: usize, last: usize, x: T) {
    v[first..=last].fill(x);
}

/// Each codepoint's property word.
fn build(read: &dyn Fn(&str) -> Result<String, String>) -> Result<Vec<u16>, String> {
    let index = |table: &[(&str, &str)], name: &str| {
        table
            .iter()
            .position(|(n, _)| *n == name)
            .and_then(|i| u16::try_from(i).ok())
            .ok_or_else(|| format!("unknown property value {name:?}"))
    };

    let mut gcb = vec![0u16; CODEPOINTS];
    for r in records(&read("GraphemeBreakProperty.txt")?) {
        let (first, last, f) = r?;
        let v = index(&GCB, f.first().copied().unwrap_or(""))?;
        fill(&mut gcb, first, last, v);
    }

    // General_Category Mn, Me, Cf and Cc, which take no cell; U+00AD SOFT
    // HYPHEN takes one
    let mut gc_zero = vec![false; CODEPOINTS];
    for r in records(&read("DerivedGeneralCategory.txt")?) {
        let (first, last, f) = r?;
        if matches!(f.first().copied(), Some("Mn" | "Me" | "Cf" | "Cc")) {
            fill(&mut gc_zero, first, last, true);
        }
    }
    gc_zero[0xAD] = false;

    let mut eaw_wide = vec![false; CODEPOINTS];
    for (first, last) in EAW_DEFAULT_W {
        fill(&mut eaw_wide, first as usize, last as usize, true);
    }
    for r in records(&read("EastAsianWidth.txt")?) {
        let (first, last, f) = r?;
        let wide = matches!(f.first().copied(), Some("W" | "F"));
        fill(&mut eaw_wide, first, last, wide);
    }

    let mut pict = vec![false; CODEPOINTS];
    let mut emoji_presentation = vec![false; CODEPOINTS];
    for r in records(&read("emoji-data.txt")?) {
        let (first, last, f) = r?;
        match f.first().copied() {
            Some("Extended_Pictographic") => fill(&mut pict, first, last, true),
            Some("Emoji_Presentation") => fill(&mut emoji_presentation, first, last, true),
            _ => {}
        }
    }

    let mut incb = vec![0u16; CODEPOINTS];
    for r in records(&read("DerivedCoreProperties.txt")?) {
        let (first, last, f) = r?;
        if let ["InCB", value] = f.as_slice() {
            fill(&mut incb, first, last, index(&INCB, value)?);
        }
    }

    let mut vs16_base = vec![false; CODEPOINTS];
    for line in read("emoji-variation-sequences.txt")?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let seq: Vec<&str> = line
            .split(';')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if let [base, "FE0F"] = seq.as_slice() {
            let cp = usize::from_str_radix(base, 16)
                .ok()
                .filter(|&cp| cp < CODEPOINTS)
                .ok_or_else(|| format!("bad codepoint in {line:?}"))?;
            vs16_base[cp] = true;
        }
    }

    let code = |name: &str| index(&GCB, name);
    let (prepend, v, t) = (code("Prepend")?, code("V")?, code("T")?);
    let mut props = vec![0u16; CODEPOINTS];
    for (cp, p) in props.iter_mut().enumerate() {
        // the width rule src/unicode.rs states
        let width: u16 = if (gc_zero[cp] && gcb[cp] != prepend) || gcb[cp] == v || gcb[cp] == t {
            0
        } else if eaw_wide[cp] || emoji_presentation[cp] {
            2
        } else {
            1
        };
        *p = gcb[cp]
            | if pict[cp] { PICT } else { 0 }
            | incb[cp] << INCB_SHIFT
            | width << WIDTH_SHIFT
            | if vs16_base[cp] { VS16_BASE } else { 0 };
    }
    Ok(props)
}

/// The runs of equal property words: each run's first codepoint and word.
fn runs(props: &[u16]) -> (Vec<u32>, Vec<u16>) {
    let mut starts = Vec::new();
    let mut words: Vec<u16> = Vec::new();
    for (cp, &p) in props.iter().enumerate() {
        if words.last() != Some(&p) {
            starts.push(cp as u32);
            words.push(p);
        }
    }
    (starts, words)
}

/// `text` as `//` comment lines, `prefix` before each.
fn commented(text: &str, prefix: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.is_empty() {
            out.push_str(prefix.trim_end());
        } else {
            out.push_str(prefix);
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// `items` as the body of an array, as rustfmt lays out short elements: as
/// many to a line as fit in 100 columns at an indent of 4.
fn array_body(items: &[String]) -> String {
    let mut out = String::new();
    let mut line = String::new();
    for item in items {
        if !line.is_empty() && 4 + line.len() + 1 + item.len() + 1 > 100 {
            let _ = writeln!(out, "    {line}");
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(item);
        line.push(',');
    }
    if !line.is_empty() {
        let _ = writeln!(out, "    {line}");
    }
    out
}

fn data_rs(starts: &[u32], words: &[u16]) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-term/gen-unicode from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n\
         //\n\
         // The Unicode data it comes from is under this licence:\n\
         //\n"
    );
    out += &commented(LICENSE, "// ");
    out += "\n\
        //! Each codepoint's Unicode properties, as runs of codepoints with equal\n\
        //! properties. [`RUN_STARTS`] holds each run's first codepoint, ascending\n\
        //! from U+0000, and [`RUN_PROPS`] its property word, read through the\n\
        //! constants below.\n\n";
    let _ = writeln!(
        out,
        "/// The Unicode Character Database version of the tables.\n\
         pub(super) const VERSION: &str = \"{VERSION}\";\n"
    );
    let constant = |out: &mut String, doc: Option<&str>, name: &str, value: u16| {
        if let Some(doc) = doc {
            let _ = writeln!(out, "/// {doc}");
        }
        let _ = writeln!(out, "pub(super) const {name}: u16 = {value:#06x};");
    };
    constant(
        &mut out,
        Some("Grapheme_Cluster_Break, one of the `GCB_` values."),
        "GCB",
        GCB_MASK,
    );
    for (code, (_, name)) in (0u16..).zip(GCB) {
        constant(&mut out, None, &format!("GCB_{name}"), code);
    }
    constant(&mut out, Some("Extended_Pictographic."), "PICT", PICT);
    constant(
        &mut out,
        Some("Indic_Conjunct_Break, one of the `INCB_` values."),
        "INCB",
        INCB_MASK,
    );
    for (code, (_, name)) in (0u16..).zip(INCB) {
        constant(&mut out, None, &format!("INCB_{name}"), code << INCB_SHIFT);
    }
    constant(
        &mut out,
        Some("The cell width, one of the `WIDTH_` values."),
        "WIDTH",
        WIDTH_MASK,
    );
    for width in 0u16..3 {
        constant(
            &mut out,
            None,
            &format!("WIDTH_{width}"),
            width << WIDTH_SHIFT,
        );
    }
    constant(
        &mut out,
        Some("A base of an emoji-style variation sequence: VS16 follows it."),
        "VS16_BASE",
        VS16_BASE,
    );

    let starts: Vec<String> = starts.iter().map(|s| format!("{s:#08x}")).collect();
    let words: Vec<String> = words.iter().map(|w| format!("{w:#06x}")).collect();
    let _ = write!(
        out,
        "\n/// The first codepoint of each run.\n\
         pub(super) const RUN_STARTS: [u32; {n}] = [\n{starts}];\n\n\
         /// Each run's property word.\n\
         pub(super) const RUN_PROPS: [u16; {n}] = [\n{words}];\n",
        n = starts.len(),
        starts = array_body(&starts),
        words = array_body(&words),
    );
    out
}

/// A GraphemeBreakTest case: its codepoints, and whether a boundary falls
/// before each and after the last.
struct Case {
    cps: Vec<String>,
    breaks: Vec<bool>,
}

fn break_cases(text: &str) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut case = Case {
            cps: Vec::new(),
            breaks: Vec::new(),
        };
        for (i, token) in line.split_whitespace().enumerate() {
            if i % 2 == 0 {
                case.breaks.push(match token {
                    "÷" => true,
                    "×" => false,
                    _ => return Err(format!("bad break mark in {line:?}")),
                });
            } else {
                let cp = u32::from_str_radix(token, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("bad codepoint in {line:?}"))?;
                case.cps.push(format!("{:04X}", u32::from(cp)));
            }
        }
        if case.cps.is_empty() || case.breaks.len() != case.cps.len() + 1 {
            return Err(format!("bad case {line:?}"));
        }
        cases.push(case);
    }
    Ok(cases)
}

fn break_test_txt(cases: &[Case]) -> String {
    let mut out = format!(
        "# Generated by crates/vtium-term/gen-unicode from GraphemeBreakTest.txt,\n\
         # Unicode {VERSION}. Do not edit; rerun the generator.\n\
         #\n\
         # One case per line: its codepoints in hex, with ÷ where a cluster\n\
         # boundary falls and × where none does.\n\
         #\n\
         # The Unicode data it comes from is under this licence:\n\
         #\n"
    );
    out += &commented(LICENSE, "# ");
    for case in cases {
        out.push('\n');
        for (brk, cp) in case.breaks.iter().zip(&case.cps) {
            out.push_str(if *brk { "÷ " } else { "× " });
            out.push_str(cp);
            out.push(' ');
        }
        out.push_str(if case.breaks.last() == Some(&true) {
            "÷"
        } else {
            "×"
        });
    }
    out.push('\n');
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// SHA-256, FIPS 180-4.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // the message, a 1 bit, zeros to 56 bytes mod 64, and its length in bits
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());

    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (wi, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *wi = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (k, wi) in K.iter().zip(w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hi, x) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *hi = hi.wrapping_add(x);
        }
    }

    let mut out = [0u8; 32];
    for (chunk, hi) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = hi.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_fips_180_examples() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
