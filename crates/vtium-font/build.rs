//! Packs the built-in sets (ADR-1200 D7): `data/unifont.hex` into
//! `OUT_DIR/unifont.vtglass` with the `unifont` feature, and
//! `data/unscii-8.hex` into `OUT_DIR/unscii-8.vtglass` with the `unscii`
//! feature. Each is a vitrified set (ADR-1200 D2) written by the set
//! writer's own code, which this script includes, its 1-bit glyph chunks
//! each deflated so the binary carries the font compressed.
//!
//! The hex files are trusted input, checked into the repository: a
//! malformed line stops the build.

#[allow(dead_code)]
#[path = "src/bounds.rs"]
mod bounds;
#[allow(dead_code)]
#[path = "src/sha256.rs"]
mod sha256;
#[allow(dead_code)]
#[path = "src/set"]
mod set {
    pub mod format;
    // the deflate, whatever the crate's features: the built-in sets are
    // always deflated
    pub mod pack;
    pub mod write;
}
#[allow(dead_code)]
#[path = "src/sprite"]
mod sprite {
    pub mod spans;
}

use std::path::PathBuf;

use set::format::{FORMAT_BITS, MAJOR, MINOR, SOURCE_SPRITES};
use set::write::{RawSet, RawStrike, encode};

/// A built-in set to pack: its hex file, in the `codepoint:bits` form
/// unifont's tools write, and what its one strike and provenance hold.
struct Builtin {
    name: &'static str,
    feature: &'static str,
    hex: &'static str,
    width: u16,
    height: u16,
    baseline: u16,
    source: &'static str,
    license: &'static str,
}

const BUILTINS: [Builtin; 2] = [
    // Unifont's glyphs stand on row 14 of 16, two rows of descent
    Builtin {
        name: "unifont",
        feature: "CARGO_FEATURE_UNIFONT",
        hex: "data/unifont.hex",
        width: 8,
        height: 16,
        baseline: 14,
        source: "GNU Unifont 17.0.03, unifont-all",
        license: "OFL-1.1",
    },
    // Unscii-8's stand on row 7 of 8, one row of descent
    Builtin {
        name: "unscii-8",
        feature: "CARGO_FEATURE_UNSCII",
        hex: "data/unscii-8.hex",
        width: 8,
        height: 8,
        baseline: 7,
        source: "Unscii 2.1, unscii-8.hex",
        license: "public domain",
    },
];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    for f in [
        "src/bounds.rs",
        "src/sha256.rs",
        "src/set/format.rs",
        "src/set/write.rs",
        "src/sprite/spans.rs",
    ] {
        println!("cargo::rerun-if-changed={f}");
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    for b in &BUILTINS {
        println!("cargo::rerun-if-changed={}", b.hex);
        if std::env::var_os(b.feature).is_some() {
            let bytes = pack(b);
            std::fs::write(out.join(format!("{}.vtglass", b.name)), bytes).expect("write a set");
        }
    }
}

/// The built-in set `b`, as bytes.
fn pack(b: &Builtin) -> Vec<u8> {
    let hex = std::fs::read(b.hex).unwrap_or_else(|e| panic!("read {}: {e}", b.hex));
    let source_sha = sha256::hex(&sha256::Sha256::digest(&hex));
    let hex = String::from_utf8(hex).expect("a hex file is UTF-8");
    // a narrow glyph's hex digits: two a byte, a byte a row up to 8 pixels
    let narrow = usize::from(b.width.div_ceil(8)) * usize::from(b.height) * 2;

    let mut glyphs = Vec::new();
    let mut map = Vec::new();
    let mut last = None;
    for (n, line) in hex.lines().enumerate() {
        let (cp, bits) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("{}:{}: no ':'", b.hex, n + 1));
        let cp = u32::from_str_radix(cp, 16).expect("a hex codepoint");
        assert!(
            char::from_u32(cp).is_some(),
            "U+{cp:04X} is not a character"
        );
        assert!(last < Some(cp), "U+{cp:04X} out of order");
        last = Some(cp);
        let wide = match bits.len() {
            len if len == narrow => false,
            len if len == 2 * narrow => true,
            len => panic!("{}:{}: {len} hex digits", b.hex, n + 1),
        };
        let glyph: Vec<u8> = (0..bits.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&bits[i..i + 2], 16).expect("hex digits"))
            .collect();
        map.push((cp, u32::try_from(glyphs.len()).expect("glyphs fit u32")));
        glyphs.push((wide, glyph));
    }

    // the lines vtium-font gives a face with no metrics of its own
    // (Decorations::for_height), so a scaled strike keeps them
    let h = b.height;
    let thick = (h / 16).clamp(1, (h / 4).max(1));
    let strike = RawStrike {
        style: 0,
        format: FORMAT_BITS,
        cell_width: b.width,
        cell_height: h,
        baseline: b.baseline,
        underline: h - thick,
        underline_thickness: thick,
        strike: (h / 2).min(h - thick),
        strike_thickness: thick,
        // built in, the chunks are deflated to fit the binary's budget
        // (ADR-1200 D7)
        compress: true,
        glyphs,
        map,
        clusters: Vec::new(),
    };
    let meta = [
        ("name", b.name.to_owned()),
        ("format", format!("{MAJOR}.{MINOR}")),
        ("tool", format!("vtium-font {}", env!("CARGO_PKG_VERSION"))),
        ("source", b.source.to_owned()),
        ("source-sha256", source_sha),
        ("source-format", "unifont-hex".to_owned()),
        ("license", b.license.to_owned()),
    ];
    let set = RawSet {
        meta: meta.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        // the sprites for every range, as the C reference drew over its
        // unifont and the goldens hold; Unscii-8, which has no C reference
        // behind it, draws them the same
        policy: [SOURCE_SPRITES; 6],
        strikes: vec![strike],
    };
    encode(&set).unwrap_or_else(|e| panic!("the built-in {} set: {e}", b.name))
}
