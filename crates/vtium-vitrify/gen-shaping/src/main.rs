//! Generates vtium-vitrify's shaping tables from the Unicode Character
//! Database. README.md says what it writes and how to run it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = "18.0.0";

/// The UCD files read, by base name, with their SHA-256 and the path under
/// `https://www.unicode.org/Public/18.0.0/` they come from.
const FILES: [(&str, &str, &str); 5] = [
    (
        "emoji-sequences.txt",
        "emoji/",
        "1823dce71f3dd9cb0ad1976797baee4ffbdc1909756f43af8ce4c57f732843a4",
    ),
    (
        "emoji-zwj-sequences.txt",
        "emoji/",
        "f61b5213bdf85a57741a9f903cb38d0f39a3d13be6ff4aca3b39857e859a9191",
    ),
    (
        "ArabicShaping.txt",
        "ucd/",
        "8ccde4ebd070500a68e8bcb9d5514522fb0bebe57b9ebad3ff925b8e1e669f83",
    ),
    (
        "UnicodeData.txt",
        "ucd/",
        "0736451de439ae7baf1425136617da495e09ee5afbe6e394374db7009ea08950",
    ),
    (
        "Scripts.txt",
        "ucd/",
        "0071fd81b6aeae25f6e8bce8efec3066a6476a91b49bdb2f52dc76e817862a6a",
    ),
];

/// The licence of the UCD files, copied into the output.
const LICENSE: &str = include_str!("unicode-license.txt");

/// The scripts whose letters join, and so take joining forms built ahead
/// of time: those HarfRust 0.12 shapes with its Arabic shaper.
const JOINING_SCRIPTS: [&str; 2] = ["Arabic", "Syriac"];

/// The scripts shaped on demand: those HarfRust 0.12 shapes with its Indic,
/// Khmer, Myanmar and Universal Shaping Engine shapers, whose syllables
/// are open sets no table lists.
const ON_DEMAND_SCRIPTS: [&str; 94] = [
    "Bengali",
    "Devanagari",
    "Gujarati",
    "Gurmukhi",
    "Kannada",
    "Malayalam",
    "Oriya",
    "Tamil",
    "Telugu",
    "Khmer",
    "Myanmar",
    "Tibetan",
    "Mongolian",
    "Sinhala",
    "Buhid",
    "Hanunoo",
    "Tagalog",
    "Tagbanwa",
    "Limbu",
    "Tai_Le",
    "Buginese",
    "Kharoshthi",
    "Syloti_Nagri",
    "Tifinagh",
    "Balinese",
    "Nko",
    "Phags_Pa",
    "Cham",
    "Kayah_Li",
    "Lepcha",
    "Rejang",
    "Saurashtra",
    "Sundanese",
    "Egyptian_Hieroglyphs",
    "Javanese",
    "Kaithi",
    "Meetei_Mayek",
    "Tai_Tham",
    "Tai_Viet",
    "Batak",
    "Brahmi",
    "Mandaic",
    "Chakma",
    "Miao",
    "Sharada",
    "Takri",
    "Duployan",
    "Grantha",
    "Khojki",
    "Khudawadi",
    "Mahajani",
    "Manichaean",
    "Modi",
    "Pahawh_Hmong",
    "Psalter_Pahlavi",
    "Siddham",
    "Tirhuta",
    "Ahom",
    "Multani",
    "Adlam",
    "Bhaiksuki",
    "Marchen",
    "Newa",
    "Masaram_Gondi",
    "Soyombo",
    "Zanabazar_Square",
    "Dogra",
    "Gunjala_Gondi",
    "Hanifi_Rohingya",
    "Makasar",
    "Medefaidrin",
    "Old_Sogdian",
    "Sogdian",
    "Elymaic",
    "Nandinagari",
    "Nyiakeng_Puachue_Hmong",
    "Wancho",
    "Chorasmian",
    "Dives_Akuru",
    "Khitan_Small_Script",
    "Yezidi",
    "Cypro_Minoan",
    "Old_Uyghur",
    "Tangsa",
    "Toto",
    "Vithkuqi",
    "Kawi",
    "Nag_Mundari",
    "Garay",
    "Gurung_Khema",
    "Kirat_Rai",
    "Ol_Onal",
    "Sunuwar",
    "Todhri",
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ucd] = args.as_slice() else {
        eprintln!("usage: gen-shaping UCD_DIR");
        return ExitCode::from(2);
    };
    match run(Path::new(ucd)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-shaping: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(ucd: &Path) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |name: &str| read_pinned(ucd, name);
    let mut emoji = Vec::new();
    for name in ["emoji-sequences.txt", "emoji-zwj-sequences.txt"] {
        emoji.extend(sequences(&read(name)?)?);
    }
    emoji.sort();
    emoji.dedup();
    let scripts = scripts(&read("Scripts.txt")?)?;
    let joining = joining(&read("ArabicShaping.txt")?, &scripts)?;
    let decomposed = decomposed(&read("UnicodeData.txt")?)?;
    let on_demand = on_demand(&scripts);
    let path = root.join("src/shaping/data.rs");
    let text = data_rs(&emoji, &joining, &decomposed, &on_demand);
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "src/shaping/data.rs: Unicode {VERSION}, {} emoji sequences, {} joining letters, \
         {} decomposed keys, {} scripts shaped on demand",
        emoji.len(),
        joining.len(),
        decomposed.len(),
        on_demand.len()
    );
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
            "{}: {e}; fetch it from https://www.unicode.org/Public/{VERSION}/{dir}{name}",
            path.display()
        )
    })?;
    let got = hex(&sha256(&bytes));
    if got != *pin {
        return Err(format!("{name}: sha256 {got}, pinned {pin}"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{name}: {e}"))
}

/// Each data line's fields, comments and blank lines dropped.
fn fields(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines().filter_map(|line| {
        let data = line.split('#').next().unwrap_or("").trim();
        (!data.is_empty()).then(|| data.split(';').map(str::trim).collect())
    })
}

fn codepoint(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

/// A field of one codepoint or a range `A..B`, as its first and last.
fn range(s: &str) -> Result<(u32, u32), String> {
    match s.split_once("..") {
        Some((a, b)) => Ok((codepoint(a)?, codepoint(b)?)),
        None => codepoint(s).map(|c| (c, c)),
    }
}

/// The RGI emoji sequences of more than one scalar value: every type but
/// `Basic_Emoji`.
fn sequences(text: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut out = Vec::new();
    for f in fields(text) {
        let (Some(cps), Some(kind)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        if *kind == "Basic_Emoji" {
            continue;
        }
        let seq = cps
            .split_whitespace()
            .map(codepoint)
            .collect::<Result<Vec<_>, _>>()?;
        if seq.len() < 2 {
            return Err(format!("a {kind} of one scalar value: {cps}"));
        }
        out.push(seq);
    }
    Ok(out)
}

/// Each codepoint's script, by its Scripts.txt name.
fn scripts(text: &str) -> Result<BTreeMap<u32, String>, String> {
    let mut out = BTreeMap::new();
    for f in fields(text) {
        let (Some(r), Some(name)) = (f.first(), f.get(1)) else {
            return Err(format!("a line of fewer than two fields: {f:?}"));
        };
        let (a, b) = range(r)?;
        for c in a..=b {
            out.insert(c, (*name).to_owned());
        }
    }
    Ok(out)
}

/// The joining letters of [`JOINING_SCRIPTS`]: each dual-joining (`D`) or
/// right-joining (`R`) letter, with 1 for the LAM joining group and 2 for
/// ALEF.
fn joining(text: &str, scripts: &BTreeMap<u32, String>) -> Result<Vec<(u32, char, u8)>, String> {
    let mut out = Vec::new();
    for f in fields(text) {
        let (Some(cp), Some(kind), Some(group)) = (f.first(), f.get(2), f.get(3)) else {
            return Err(format!("a line of fewer than four fields: {f:?}"));
        };
        let c = codepoint(cp)?;
        let script = scripts.get(&c).map_or("", String::as_str);
        if !JOINING_SCRIPTS.contains(&script) || !matches!(*kind, "D" | "R") {
            continue;
        }
        let group = match *group {
            "LAM" => 1,
            "ALEF" => 2,
            _ => 0,
        };
        out.push((c, if *kind == "D" { 'D' } else { 'R' }, group));
    }
    Ok(out)
}

/// The canonical decompositions of two or more scalar values, each key
/// the precomposed character decomposed one step at a time from its first
/// scalar: `U+1EBF` gives `U+00EA U+0301` and `U+0065 U+0302 U+0301`.
/// Hangul syllables decompose by rule, not in UnicodeData.txt, and are
/// left out.
fn decomposed(text: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for f in fields(text) {
        let (Some(cp), Some(dec)) = (f.first(), f.get(5)) else {
            return Err(format!("a line of fewer than six fields: {f:?}"));
        };
        if dec.is_empty() || dec.starts_with('<') {
            continue;
        }
        let d = dec
            .split_whitespace()
            .map(codepoint)
            .collect::<Result<Vec<_>, _>>()?;
        if d.len() >= 2 {
            map.insert(codepoint(cp)?, d);
        }
    }
    let mut keys = BTreeSet::new();
    for d in map.values() {
        let mut key = d.clone();
        keys.insert(key.clone());
        while let Some(more) = key.first().and_then(|c| map.get(c)) {
            key.splice(0..1, more.iter().copied());
            keys.insert(key.clone());
        }
    }
    Ok(keys.into_iter().collect())
}

/// Each script of [`ON_DEMAND_SCRIPTS`] and its ranges, in that order.
fn on_demand(scripts: &BTreeMap<u32, String>) -> Vec<(&'static str, Vec<(u32, u32)>)> {
    ON_DEMAND_SCRIPTS
        .iter()
        .map(|&name| {
            let mut ranges: Vec<(u32, u32)> = Vec::new();
            for (&c, s) in scripts {
                if s != name {
                    continue;
                }
                match ranges.last_mut() {
                    Some(r) if r.1 + 1 == c => r.1 = c,
                    _ => ranges.push((c, c)),
                }
            }
            (name, ranges)
        })
        .collect()
}

/// `text` as `//` comment lines.
fn commented(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        out.push_str(if line.is_empty() { "//" } else { "// " });
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn seq(s: &[u32]) -> String {
    let items: Vec<String> = s.iter().map(|c| format!("0x{c:x}")).collect();
    format!("&[{}]", items.join(", "))
}

fn data_rs(
    emoji: &[Vec<u32>],
    joining: &[(u32, char, u8)],
    decomposed: &[Vec<u32>],
    on_demand: &[(&str, Vec<(u32, u32)>)],
) -> String {
    let mut out = format!(
        "// Generated by crates/vtium-vitrify/gen-shaping from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n//\n\
         // The Unicode data it comes from is under this licence:\n//\n{}\n",
        commented(LICENSE)
    );
    out.push_str(
        "/// The RGI emoji sequences of more than one scalar value, sorted:\n\
         /// keycaps, flags, tag sequences, modifier sequences and ZWJ sequences\n\
         /// (emoji-sequences.txt and emoji-zwj-sequences.txt).\n\
         pub const EMOJI: &[&[u32]] = &[\n",
    );
    for s in emoji {
        let _ = writeln!(out, "    {},", seq(s));
    }
    out.push_str(
        "];\n\n/// The joining letters of the scripts the Arabic shaper takes, Arabic\n\
         /// and Syriac (ArabicShaping.txt): each letter, its joining type, `D`\n\
         /// for dual-joining or `R` for right-joining, and its joining group, 1\n\
         /// for LAM, 2 for ALEF, else 0.\n\
         pub const JOINING: &[(u32, char, u8)] = &[\n",
    );
    for (c, kind, group) in joining {
        let _ = writeln!(out, "    (0x{c:x}, '{kind}', {group}),");
    }
    out.push_str(
        "];\n\n/// The canonical decompositions of two or more scalar values, each\n\
         /// decomposed one step at a time from its first scalar, sorted\n\
         /// (UnicodeData.txt).\n\
         pub const DECOMPOSED: &[&[u32]] = &[\n",
    );
    for s in decomposed {
        let _ = writeln!(out, "    {},", seq(s));
    }
    out.push_str(
        "];\n\n/// The scripts shaped on demand, each with its ranges (Scripts.txt):\n\
         /// those the Indic, Khmer, Myanmar and Universal Shaping Engine shapers\n\
         /// take.\n\
         pub const ON_DEMAND: &[(&str, &[(u32, u32)])] = &[\n",
    );
    for (name, ranges) in on_demand {
        let items: Vec<String> = ranges
            .iter()
            .map(|(a, b)| format!("(0x{a:x}, 0x{b:x})"))
            .collect();
        let _ = writeln!(out, "    (\"{name}\", &[{}]),", items.join(", "));
    }
    out.push_str("];\n");
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
    }

    #[test]
    fn a_decomposition_is_keyed_at_each_step() {
        let text = "00EA;E;Ll;0;L;0065 0302;;;;N;;;;;\n\
                    1EBF;E;Ll;0;L;00EA 0301;;;;N;;;;;\n";
        let keys = decomposed(text).unwrap();
        assert!(keys.contains(&vec![0xea, 0x301]));
        assert!(keys.contains(&vec![0x65, 0x302, 0x301]));
        assert!(keys.contains(&vec![0x65, 0x302]));
    }
}
