//! Packs `data/unifont.hex` into the built-in unifont set when the
//! `unifont` feature is on: a vitrified set (ADR-1200 D2, D7) written by
//! the set writer's own code, which this script includes, its 1-bit glyph
//! chunks each deflated so the binary carries the font compressed.
//! `src/unifont.rs` includes it from `OUT_DIR/unifont.glass`.
//!
//! The hex file is trusted input, checked into the repository: a malformed
//! line stops the build.

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

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=data/unifont.hex");
    for f in [
        "src/bounds.rs",
        "src/sha256.rs",
        "src/set/format.rs",
        "src/set/write.rs",
        "src/sprite/spans.rs",
    ] {
        println!("cargo::rerun-if-changed={f}");
    }
    if std::env::var_os("CARGO_FEATURE_UNIFONT").is_none() {
        return;
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let hex = std::fs::read("data/unifont.hex").expect("read data/unifont.hex");
    let source_sha = sha256::hex(&sha256::Sha256::digest(&hex));
    let hex = String::from_utf8(hex).expect("data/unifont.hex is UTF-8");

    let mut glyphs = Vec::new();
    let mut map = Vec::new();
    let mut last = None;
    for (n, line) in hex.lines().enumerate() {
        let (cp, bits) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("data/unifont.hex:{}: no ':'", n + 1));
        let cp = u32::from_str_radix(cp, 16).expect("a hex codepoint");
        assert!(
            char::from_u32(cp).is_some(),
            "U+{cp:04X} is not a character"
        );
        assert!(last < Some(cp), "U+{cp:04X} out of order");
        last = Some(cp);
        let wide = match bits.len() {
            32 => false,
            64 => true,
            len => panic!("data/unifont.hex:{}: {len} hex digits", n + 1),
        };
        let glyph: Vec<u8> = (0..bits.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&bits[i..i + 2], 16).expect("hex digits"))
            .collect();
        map.push((cp, u32::try_from(glyphs.len()).expect("glyphs fit u32")));
        glyphs.push((wide, glyph));
    }

    // Unifont's glyphs stand on row 14 of 16, two rows of descent; the
    // lines are those vtium-font gives a 16-pixel face with no metrics of
    // its own (Decorations::for_height).
    let strike = RawStrike {
        style: 0,
        format: FORMAT_BITS,
        cell_width: 8,
        cell_height: 16,
        baseline: 14,
        underline: 15,
        underline_thickness: 1,
        strike: 8,
        strike_thickness: 1,
        // built in, the chunks are deflated to fit the binary's budget
        // (ADR-1200 D7)
        compress: true,
        glyphs,
        map,
    };
    let meta = [
        ("name", "unifont".to_owned()),
        ("format", format!("{MAJOR}.{MINOR}")),
        ("tool", format!("vtium-font {}", env!("CARGO_PKG_VERSION"))),
        ("source", "GNU Unifont 17.0.03, unifont-all".to_owned()),
        ("source-sha256", source_sha),
        ("source-format", "unifont-hex".to_owned()),
        ("license", "OFL-1.1".to_owned()),
    ];
    let set = RawSet {
        meta: meta.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        // the sprites for every range, as the C reference drew over its
        // unifont and the goldens hold
        policy: [SOURCE_SPRITES; 6],
        strikes: vec![strike],
    };
    let bytes = encode(&set).unwrap_or_else(|e| panic!("the built-in unifont set: {e}"));
    std::fs::write(out.join("unifont.glass"), bytes).expect("write unifont.glass");
}
