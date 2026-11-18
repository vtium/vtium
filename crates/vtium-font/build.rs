//! Packs the built-in sets (ADR-1200 D7): `data/unifont.hex` into
//! `OUT_DIR/unifont.vtglass` with the `unifont` feature, and
//! `data/unscii-8.hex` into `OUT_DIR/unscii-8.vtglass` with the `unscii`
//! feature. It also packs the three strikes of vtium's built-in font,
//! "vtium built-in", in every build: each is cut from its source with the
//! list in `data/builtin-chars.txt` (`src/set/cut.rs`), and the list
//! becomes `OUT_DIR/builtin_chars.rs`. Each set is a vitrified set
//! (ADR-1200 D2) written by the set writer's own code, which this script
//! includes, its 1-bit glyph chunks each deflated so the binary carries
//! the font compressed.
//!
//! The hex, BDF and list files are trusted input, checked into the
//! repository: a malformed line stops the build.
//!
//! References:
//! - ADR-1204 D1: the built-in font's list, its strikes and their sources.

#[allow(dead_code)]
#[path = "src/bounds.rs"]
mod bounds;
#[allow(dead_code)]
#[path = "src/reach.rs"]
mod reach;
#[allow(dead_code)]
#[path = "src/sha256.rs"]
mod sha256;
#[allow(dead_code)]
#[path = "src/set"]
mod set {
    pub mod cut;
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
        "src/set/cut.rs",
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
    own(&out);
}

/// Cuts and packs the built-in font's strikes into `out`, and writes its
/// list as `builtin_chars.rs`, the ranges the console's
/// `set::own::holds` reads.
fn own(out: &std::path::Path) {
    const LIST: &str = "data/builtin-chars.txt";
    println!("cargo::rerun-if-changed={LIST}");
    let text = std::fs::read_to_string(LIST).unwrap_or_else(|e| panic!("read {LIST}: {e}"));
    let ranges = set::cut::list(&text).unwrap_or_else(|e| panic!("{LIST}: {e}"));
    let consts: String = ranges
        .iter()
        .map(|(a, b)| format!("    (0x{a:04X}, 0x{b:04X}),\n"))
        .collect();
    let list = format!(
        "/// The ranges `{LIST}` lists.\npub(crate) const LIST: &[(u32, u32)] = &[\n{consts}];\n"
    );
    std::fs::write(out.join("builtin_chars.rs"), list).expect("write the list");
    for strike in &set::cut::STRIKES {
        let read = |path: &str| {
            println!("cargo::rerun-if-changed={path}");
            let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            let sha = sha256::hex(&sha256::Sha256::digest(&bytes));
            (
                String::from_utf8(bytes).expect("a font source is UTF-8"),
                sha,
            )
        };
        let (text, sha) = read(strike.path);
        let own = strike.own.map(read);
        let all = set::cut::read(strike, &text, own.as_ref().map(|(t, _)| t.as_str()))
            .unwrap_or_else(|e| panic!("{}: {e}", strike.path));
        let cut = set::cut::cut(&all, &ranges);
        let mut glyphs = Vec::new();
        let mut map = Vec::new();
        for (cp, glyph) in cut.glyphs {
            map.push((cp, u32::try_from(glyphs.len()).expect("glyphs fit u32")));
            glyphs.push((false, glyph));
        }
        let format = match strike.format {
            set::cut::Format::Hex { .. } => "unifont-hex",
            set::cut::Format::Bdf => "bdf",
        };
        let mut meta = vec![
            ("name", set::cut::SET_NAME.to_owned()),
            ("source", strike.source.to_owned()),
            ("source-sha256", sha),
            ("source-format", format.to_owned()),
            ("license", strike.license.to_owned()),
        ];
        // vtium's own glyphs, a second source with its own license
        if let Some((_, own_sha)) = own {
            meta.extend([
                ("source", set::cut::OWN_SOURCE.to_owned()),
                ("source-sha256", own_sha),
                ("source-format", "unifont-hex".to_owned()),
                ("license", set::cut::OWN_LICENSE.to_owned()),
            ]);
        }
        let bytes = encode_set(&meta, (cut.width, cut.height, cut.baseline), glyphs, map);
        std::fs::write(out.join(strike.file), bytes).expect("write a strike");
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

    let meta = [
        ("name", b.name.to_owned()),
        ("source", b.source.to_owned()),
        ("source-sha256", source_sha),
        ("source-format", "unifont-hex".to_owned()),
        ("license", b.license.to_owned()),
    ];
    encode_set(&meta, (b.width, b.height, b.baseline), glyphs, map)
}

/// A one-strike set of 1-bit `glyphs` in cells of `width` by `height`
/// stood on `baseline`, mapped by `map`, with the provenance `meta`, its
/// name first, as bytes. Its chunks are deflated, and the sprites
/// draw every range.
fn encode_set(
    meta: &[(&str, String)],
    (width, h, baseline): (u16, u16, u16),
    glyphs: Vec<(bool, Vec<u8>)>,
    map: Vec<(u32, u32)>,
) -> Vec<u8> {
    // the lines vtium-font gives a face with no metrics of its own
    // (Decorations::for_height), so a scaled strike keeps them
    let thick = (h / 16).clamp(1, (h / 4).max(1));
    let strike = RawStrike {
        style: 0,
        format: FORMAT_BITS,
        script: 0,
        cell_width: width,
        cell_height: h,
        baseline,
        underline: h - thick,
        underline_thickness: thick,
        strike: (h / 2).min(h - thick),
        strike_thickness: thick,
        overline: 0,
        // built in, the chunks are deflated to fit the binary's budget
        // (ADR-1200 D7)
        compress: true,
        glyphs,
        map,
        clusters: Vec::new(),
        inks: Vec::new(),
    };
    // the name first, then the format and the tool, as every set has them
    let (name, rest) = meta.split_first().expect("a name");
    let meta = [
        (name.0.to_owned(), name.1.clone()),
        ("format".to_owned(), format!("{MAJOR}.{MINOR}")),
        (
            "tool".to_owned(),
            format!("vtium-font {}", env!("CARGO_PKG_VERSION")),
        ),
    ]
    .into_iter()
    .chain(rest.iter().map(|(k, v)| ((*k).to_owned(), v.clone())))
    .collect();
    let set = RawSet {
        meta,
        // the sprites for every range, though unifont covers all six: the
        // sprites line up exactly at every cell size, and the goldens show
        // them; Unscii-8 draws them the same
        policy: [SOURCE_SPRITES; 6],
        strikes: vec![strike],
    };
    encode(&set).unwrap_or_else(|e| panic!("a built-in set: {e}"))
}
