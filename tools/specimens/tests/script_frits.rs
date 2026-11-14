//! The font sets vtium ships for scripts that need room above and below
//! their letters (vtium/vtium#204): each one reads as the console and
//! `vtium-config check` read a frit, names its fonts by family alone, so
//! it works wherever a distribution installs them, and states the line
//! spacing and the per-set fit that the frit's comments explain (ADR-1200
//! D1, ADR-1302 D13).

use std::path::{Path, PathBuf};

use vtium_config::frit::{Frit, Origin};
use vtium_font::spacing::Fit;

/// The directory the frits ship from: the Makefile's `FRIT_DIRS` names
/// it, so `make install FRITS=...` installs them into /etc/vtium/fonts.
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
const SCRIPT_FRITS: [(&str, &[&str], u32); 5] = [
    ("arabic", &["Noto Sans Mono", "Noto Sans Arabic"], 50),
    ("hebrew", &["Noto Sans Mono", "Noto Sans Hebrew"], 20),
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
        30,
    ),
    ("vietnamese", &["Noto Sans Mono"], 25),
];

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
    for (name, families, _) in SCRIPT_FRITS.iter().filter(|f| f.1.len() > 1) {
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
