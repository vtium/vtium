//! The font sets vtium ships for scripts that need room above and below
//! their letters, and the four for Chinese, Japanese and Korean, one for
//! each region's forms of Han (vtium/vtium#204): each one reads as the
//! console and `vtium-config check` read a frit, names its fonts by family
//! alone, so it works wherever a distribution installs them, and states the
//! line spacing and the per-set fit that the frit's comments explain
//! (ADR-1200 D1, ADR-1302 D13).

use std::path::{Path, PathBuf};

use vtium_config::frit::{Frit, Origin};
use vtium_font::spacing::Fit;

/// The directory the frits ship from: the Makefile's `FRIT_DIRS` names
/// it, and `make install-fonts` stages all nine into /etc/vtium/fonts for
/// the vtium-fonts package (ADR-1500 D8).
fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("frits")
}

/// The frit `name`, read from its file as `vtium-config check` reads it.
fn read(name: &str) -> Frit {
    let path = dir().join(format!("{name}.vtfrit"));
    Frit::load(&path).unwrap_or_else(|e| panic!("{e}"))
}

/// Each script frit's name, its fonts' families in its order, and its
/// line spacing in per cent of the text's height.
const SCRIPT_FRITS: [(&str, &[&str], u32); 9] = [
    ("arabic", &["Noto Sans Mono", "Noto Sans Arabic"], 30),
    ("hebrew", &["Noto Sans Mono", "Noto Sans Hebrew"], 10),
    (
        "indic",
        &[
            "Noto Sans Mono",
            "Noto Sans Devanagari",
            "Noto Sans Bengali",
            "Noto Sans Gurmukhi",
            "Noto Sans Gujarati",
            "Noto Sans Oriya",
            "Noto Sans Tamil",
            "Noto Sans Telugu",
            "Noto Sans Kannada",
            "Noto Sans Malayalam",
            "Noto Sans Sinhala",
        ],
        40,
    ),
    (
        "thai-lao",
        &["Noto Sans Mono", "Noto Sans Thai", "Noto Sans Lao"],
        25,
    ),
    ("vietnamese", &["Noto Sans Mono"], 25),
    ("cjk-sc", &["Noto Sans Mono", "Noto Sans CJK SC"], 0),
    ("cjk-tc", &["Noto Sans Mono", "Noto Sans CJK TC"], 0),
    ("cjk-jp", &["Noto Sans Mono", "Noto Sans CJK JP"], 0),
    ("cjk-kr", &["Noto Sans Mono", "Noto Sans CJK KR"], 0),
];

/// Whether `name` is one of the four font sets for Chinese, Japanese and
/// Korean, whose Han, kana and Hangul carry no marks above or below.
fn is_cjk(name: &str) -> bool {
    name.starts_with("cjk-")
}

// ADR-1200 D1: a frit that names its fonts by family works on any machine
// that has the family installed; a path would tie it to one distribution's
// layout. Each frit reads with no warning, under its own name, and states
// no cell and no size, so the automatic rule or vtium.conf's font-size
// sizes it and its line spacing applies (ADR-1302 D13).
#[test]
fn each_script_frit_reads_and_names_its_fonts_by_family() {
    for (name, families, spacing) in SCRIPT_FRITS {
        let frit = read(name);
        assert_eq!(frit.name, name);
        assert_eq!(frit.warnings, [], "{name}");
        let named: Vec<String> = frit
            .sources
            .iter()
            .map(|s| match &s.origin {
                Origin::Font(family) => family.clone(),
                other => panic!("{name}: {other:?} is not a font family"),
            })
            .collect();
        assert_eq!(named, families, "{name}");
        assert_eq!(frit.line_spacing, Some(spacing), "{name}");
        assert_eq!(frit.cell, None, "{name}");
        assert_eq!(frit.size, None, "{name}");
        assert_eq!(frit.license.as_deref(), Some("OFL-1.1"), "{name}");
    }
}

// ADR-1302 D13: the script's own font gives its set `fit=room`, so its marks
// may use the rows line spacing adds. Noto Sans Mono, first in each frit,
// keeps the defaults, so box drawing still fills the taller cell and joins
// from row to row.
#[test]
fn each_script_source_gives_its_marks_room() {
    let marked = SCRIPT_FRITS
        .iter()
        .filter(|f| f.1.len() > 1 && !is_cjk(f.0));
    for (name, families, _) in marked {
        let frit = read(name);
        let (mono, scripts) = frit.sources.split_first().expect("a first source");
        assert_eq!(mono.fits, [], "{name}: {}", families[0]);
        assert_eq!(mono.cover, [], "{name}: {}", families[0]);
        for source in scripts {
            assert_eq!(source.fit(0x0041), Some(Fit::Room), "{}", source.spelling);
            assert_eq!(source.cover, [], "{}", source.spelling);
        }
    }
}

// ADR-1302 D13: Vietnamese stacks two marks on a Latin letter, so its one
// font gives the Latin letters and the combining marks room, and leaves
// box drawing and the rest at their defaults. The four cover= lines
// together hold every codepoint, so the font still draws all it holds.
#[test]
fn the_vietnamese_frit_gives_latin_room_and_keeps_box_drawing_filling() {
    let frit = read("vietnamese");
    let [font] = frit.sources.as_slice() else {
        panic!("one source: {:?}", frit.sources);
    };
    assert_eq!(font.cover, [(0, 0x10_ffff)]);
    for c in [0x41, 0x61, 0xc2, 0x103, 0x1b0, 0x301, 0x323, 0x1ea5, 0x1ed9] {
        assert_eq!(font.fit(c), Some(Fit::Room), "U+{c:04X}");
    }
    for c in [0x391, 0x416, 0x2500, 0x2588, 0x28ff] {
        assert_eq!(font.fit(c), None, "U+{c:04X} keeps its default");
    }
    assert_eq!(Fit::default_for(0x2500, false), Fit::Fill);
}

// ADR-1302 D13: Han, kana and Hangul draw inside the text's height, so each
// CJK set adds no line spacing and states no `fit=` and no `cover=`: its
// region's face draws every character it holds that Noto Sans Mono lacks,
// at the default fit.
#[test]
fn each_cjk_frit_keeps_the_default_fit() {
    for (name, families, _) in SCRIPT_FRITS.iter().filter(|f| is_cjk(f.0)) {
        let frit = read(name);
        assert_eq!(frit.sources.len(), 2, "{name}");
        for (source, family) in frit.sources.iter().zip(families.iter()) {
            assert_eq!(source.fits, [], "{name}: {family}");
            assert_eq!(source.cover, [], "{name}: {family}");
        }
    }
}

/// A font file for the test below: a collection of `faces`, each a `name`
/// table alone holding `(name ID, string)` records in UTF-16 for Windows,
/// US English, which is all the search and the choice of a face read.
fn collection(faces: &[Vec<(u16, String)>]) -> Vec<u8> {
    fn name_table(records: &[(u16, String)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut out = Vec::new();
        let count = u16::try_from(records.len()).expect("few records");
        out.extend(0u16.to_be_bytes());
        out.extend(count.to_be_bytes());
        out.extend((6 + 12 * count).to_be_bytes());
        for (id, text) in records {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            let len = u16::try_from(utf16.len()).expect("a short name");
            let at = u16::try_from(strings.len()).expect("a small table");
            for v in [3u16, 1, 0x0409, *id, len, at] {
                out.extend(v.to_be_bytes());
            }
            strings.extend(utf16);
        }
        out.extend(strings);
        out
    }
    let head = 12 + 4 * faces.len();
    let mut bodies: Vec<Vec<u8>> = Vec::new();
    let mut at = head;
    for records in faces {
        let table = name_table(records);
        let mut body = Vec::new();
        body.extend(0x0001_0000u32.to_be_bytes());
        body.extend([0u8, 1, 0, 16, 0, 0, 0, 0]);
        body.extend(*b"name");
        body.extend(0u32.to_be_bytes());
        body.extend(u32::try_from(at + 28).expect("small").to_be_bytes());
        body.extend(u32::try_from(table.len()).expect("small").to_be_bytes());
        body.extend(table);
        at += body.len();
        bodies.push(body);
    }
    let mut out = Vec::new();
    out.extend(*b"ttcf");
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend(u32::try_from(faces.len()).expect("few").to_be_bytes());
    let mut offset = head;
    for body in &bodies {
        out.extend(u32::try_from(offset).expect("small").to_be_bytes());
        offset += body.len();
    }
    out.extend(bodies.concat());
    out
}

// ADR-1500 D8: a distribution may install Noto Sans CJK as collections,
// one file for each weight with every region's face in it, as Arch's
// noto-fonts-cjk does in /usr/share/fonts/noto-cjk. The glyph helper finds
// each CJK set's family in the Regular collection, not in a heavier one
// that also names the family as its typographic family, and takes that
// region's own face from it, by the same choice the rasteriser makes.
#[test]
fn each_cjk_family_is_found_in_its_collection_and_takes_its_regions_face() {
    let root = std::env::temp_dir().join(format!("vtium-cjk-ttc-{}", std::process::id()));
    let dir = root.join("noto-cjk");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    const REGIONS: [&str; 5] = ["JP", "KR", "SC", "TC", "HK"];
    // the Regular collection, as noto-fonts-cjk 20240730 orders it: the
    // five regions' proportional faces, then their monospaced ones
    let regular: Vec<Vec<(u16, String)>> = REGIONS
        .iter()
        .map(|r| format!("Noto Sans CJK {r}"))
        .chain(REGIONS.iter().map(|r| format!("Noto Sans Mono CJK {r}")))
        .map(|family| vec![(1, family), (2, "Regular".to_owned())])
        .collect();
    // a heavier collection, which sorts first and names each region's
    // family as its typographic family
    let black: Vec<Vec<(u16, String)>> = REGIONS
        .iter()
        .map(|r| {
            vec![
                (1, format!("Noto Sans CJK {r} Black")),
                (2, "Regular".to_owned()),
                (16, format!("Noto Sans CJK {r}")),
                (17, "Black".to_owned()),
            ]
        })
        .collect();
    std::fs::write(dir.join("NotoSansCJK-Black.ttc"), collection(&black)).expect("written");
    std::fs::write(dir.join("NotoSansCJK-Regular.ttc"), collection(&regular)).expect("written");
    let dirs = [root.clone()];
    for (name, families, _) in SCRIPT_FRITS.iter().filter(|f| is_cjk(f.0)) {
        let family = families[1];
        let found = vtium_config::sources::find_outline_family(family, &dirs)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap_or_else(|| panic!("{name}: {family} not found"));
        assert_eq!(
            found.file_name().and_then(|f| f.to_str()),
            Some("NotoSansCJK-Regular.ttc"),
            "{name}"
        );
        let file = std::fs::File::open(&found).expect("opens");
        let faces = vtium_config::names::faces(&file).expect("faces");
        let face = vtium_config::names::choose(&faces, family).expect("a face");
        let region = family.rsplit(' ').next().expect("a region");
        let index = REGIONS.iter().position(|r| *r == region).expect("a region");
        assert_eq!(face.index as usize, index, "{name}");
        assert_eq!(face.families, [family], "{name}");
    }
    std::fs::remove_dir_all(&root).expect("removed");
}
