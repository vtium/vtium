//! Generates the console's cluster lookup tables from the Unicode Character
//! Database. README.md says what it writes and how to run it.

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = "18.0.0";

/// The UCD files read, by base name, with their SHA-256 and the path under
/// `https://www.unicode.org/Public/18.0.0/ucd/` they come from.
const FILES: [(&str, &str, &str); 3] = [
    (
        "UnicodeData.txt",
        "",
        "0736451de439ae7baf1425136617da495e09ee5afbe6e394374db7009ea08950",
    ),
    (
        "DerivedCoreProperties.txt",
        "",
        "09c928886a178fcafd93c29e4bd59073a058e5a100b716d425cb563ab50f68c9",
    ),
    (
        "DerivedNormalizationProps.txt",
        "",
        "98ac7f67d985fe781e317f6182e885e94cabb0c314769e6dd73e48b226931ccd",
    ),
];

/// The licence of the UCD files, copied into the output.
const LICENSE: &str = include_str!("unicode-license.txt");

const CODEPOINTS: usize = 0x11_0000;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ucd] = args.as_slice() else {
        eprintln!("usage: gen-lookup UCD_DIR");
        return ExitCode::from(2);
    };
    match run(Path::new(ucd)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gen-lookup: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(ucd: &Path) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |name: &str| read_pinned(ucd, name);
    let unicode_data = read("UnicodeData.txt")?;

    let mut ignorable = vec![false; CODEPOINTS];
    for r in records(&read("DerivedCoreProperties.txt")?) {
        let (first, last, f) = r?;
        if f.first() == Some(&"Default_Ignorable_Code_Point") {
            ignorable[first..=last].fill(true);
        }
    }
    let mut excluded = vec![false; CODEPOINTS];
    for r in records(&read("DerivedNormalizationProps.txt")?) {
        let (first, last, f) = r?;
        if f.first() == Some(&"Full_Composition_Exclusion") {
            excluded[first..=last].fill(true);
        }
    }

    let mut mark = vec![false; CODEPOINTS];
    let mut ccc = vec![0u8; CODEPOINTS];
    let mut compose: Vec<(u32, u32, u32)> = Vec::new();
    for line in unicode_data.lines() {
        let f: Vec<&str> = line.split(';').collect();
        let (Some(cp), Some(gc), Some(class), Some(decomposition)) =
            (f.first(), f.get(2), f.get(3), f.get(5))
        else {
            return Err(format!("bad UnicodeData line {line:?}"));
        };
        let cp = codepoint(cp)?;
        // no mark lies in a First..Last range, so each is its own line
        mark[cp] = matches!(*gc, "Mn" | "Mc" | "Me") && !ignorable[cp];
        ccc[cp] = class
            .parse()
            .map_err(|_| format!("bad combining class in {line:?}"))?;
        if decomposition.starts_with('<') || excluded[cp] {
            continue;
        }
        if let [a, b] = decomposition.split_whitespace().collect::<Vec<_>>().as_slice() {
            compose.push((codepoint(a)? as u32, codepoint(b)? as u32, cp as u32));
        }
    }
    compose.sort_unstable();

    let marks = ranges(&mark, |&m| m.then_some(()));
    let classes = ranges(&ccc, |&c| (c != 0).then_some(c));
    let ignorables = ranges(&ignorable, |&i| i.then_some(()));
    let out = root.join("src/deferred/data.rs");
    std::fs::write(&out, data_rs(&marks, &classes, &ignorables, &compose))
        .map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "src/deferred/data.rs: Unicode {VERSION}, {} mark ranges, {} class ranges, {} ignorable ranges, {} compositions",
        marks.len(),
        classes.len(),
        ignorables.len(),
        compose.len()
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

fn codepoint(s: &str) -> Result<usize, String> {
    usize::from_str_radix(s, 16)
        .ok()
        .filter(|&cp| cp < CODEPOINTS)
        .ok_or_else(|| format!("bad codepoint {s:?}"))
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
        Some(codepoint(first).and_then(|f| Ok((f, codepoint(last)?, fields.collect()))))
    })
}

/// The runs of codepoints `value` gives the same `Some` for: each run's
/// first and last codepoint and its value.
fn ranges<T, V: PartialEq + Copy>(v: &[T], value: impl Fn(&T) -> Option<V>) -> Vec<(u32, u32, V)> {
    let mut out: Vec<(u32, u32, V)> = Vec::new();
    for (cp, x) in (0u32..).zip(v) {
        let Some(x) = value(x) else {
            continue;
        };
        match out.last_mut() {
            Some((_, last, y)) if *y == x && *last + 1 == cp => *last = cp,
            _ => out.push((cp, cp, x)),
        }
    }
    out
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

fn data_rs(
    marks: &[(u32, u32, ())],
    classes: &[(u32, u32, u8)],
    ignorables: &[(u32, u32, ())],
    compose: &[(u32, u32, u32)],
) -> String {
    let mut out = format!(
        "// Generated by crates/vtium/gen-lookup from the Unicode Character\n\
         // Database {VERSION}. Do not edit; rerun the generator.\n\
         //\n\
         // The Unicode data it comes from is under this licence:\n\
         //\n"
    );
    out += &commented(LICENSE, "// ");
    out += "\n\
        //! What the console's lookup for a cluster reads (ADR-1200 D22): the\n\
        //! combining marks overstruck on a base, the default ignorables left\n\
        //! out, and the canonical classes and compositions a cluster's NFC\n\
        //! form is found by.\n\n";
    let pair = |&(a, b, ()): &(u32, u32, ())| format!("({a:#x}, {b:#x})");
    let marks: Vec<String> = marks.iter().map(pair).collect();
    let ignorables: Vec<String> = ignorables.iter().map(pair).collect();
    let classes: Vec<String> = classes
        .iter()
        .map(|&(a, b, c)| format!("({a:#x}, {b:#x}, {c})"))
        .collect();
    let compose: Vec<String> = compose
        .iter()
        .map(|&(a, b, c)| format!("({a:#x}, {b:#x}, {c:#x})"))
        .collect();
    let _ = write!(
        out,
        "/// The combining marks, General_Category Mn, Mc and Me less the default\n\
         /// ignorables, as ranges of codepoints, ascending.\n\
         pub(super) const MARKS: [(u32, u32); {}] = [\n{}];\n\n\
         /// The Default_Ignorable_Code_Point ranges, ascending.\n\
         pub(super) const IGNORABLES: [(u32, u32); {}] = [\n{}];\n\n\
         /// Each range of codepoints of one canonical combining class but 0,\n\
         /// ascending.\n\
         pub(super) const CLASSES: [(u32, u32, u8); {}] = [\n{}];\n\n\
         /// Each primary composite: the two codepoints of its canonical\n\
         /// decomposition and the composite, sorted by the two, those of\n\
         /// Full_Composition_Exclusion left out. Hangul syllables compose by\n\
         /// rule and are not listed.\n\
         pub(super) const COMPOSE: [(u32, u32, u32); {}] = [\n{}];\n",
        marks.len(),
        array_body(&marks),
        ignorables.len(),
        array_body(&ignorables),
        classes.len(),
        array_body(&classes),
        compose.len(),
        array_body(&compose),
    );
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
