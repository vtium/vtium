//! What Cullet Mono is built from and what the build expects of it.
//!
//! References:
//! - ADR-1204 D2, "vtium's own outline Terminus": the sources, the name, the
//!   icon sets, and the sizes and character counts the checks hold the
//!   faces to.
//! - ADR-1204 D8, "Font Logos": why the build leaves the Font Logos icons out.

/// The Terminus (TTF) release archive, as `pins.tsv` names it.
pub const TERMINUS_ARCHIVE: &str = "terminus-ttf-4.49.3.zip";

/// The folder inside the Terminus (TTF) archive that holds its files.
pub const TERMINUS_FOLDER: &str = "terminus-ttf-4.49.3";

/// Nerd Fonts' patcher archive, as `pins.tsv` names it.
pub const PATCHER_ARCHIVE: &str = "FontPatcher.zip";

/// One face of Cullet Mono: the Terminus (TTF) file it is patched from, the
/// name the patcher gives it, and the file the patcher writes.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub source: &'static str,
    pub name: &'static str,
    pub style: &'static str,
    pub output: &'static str,
}

/// The two faces, each patched in a run of its own. The patcher takes the
/// style from the end of the name it is given, so the bold face's name ends
/// in "Bold"; given "Cullet Mono" alone, the bold face would come out named
/// and filed as Regular.
pub const FACES: [Face; 2] = [
    Face {
        source: "TerminusTTF-4.49.3.ttf",
        name: "Cullet Mono",
        style: "Regular",
        output: "CulletMono-Regular.ttf",
    },
    Face {
        source: "TerminusTTF-Bold-4.49.3.ttf",
        name: "Cullet Mono Bold",
        style: "Bold",
        output: "CulletMono-Bold.ttf",
    },
];

/// The font's family name.
pub const FAMILY: &str = "Cullet Mono";

/// The patcher's options. `--mono` makes every icon one cell wide. Every
/// icon-set option of the patcher follows, apart from `--fontlogos`: the
/// Font Logos icons carry no license, so Cullet Mono leaves them out. The
/// patcher adds the Seti-UI, Devicons, heavy angle bracket, box drawing and
/// progress indicator glyphs on its own, without an option.
pub const OPTIONS: [&str; 11] = [
    "--mono",
    "--codicons",
    "--fontawesome",
    "--fontawesomeext",
    "--material",
    "--octicons",
    "--pomicons",
    "--powerline",
    "--powerlineextra",
    "--powersymbols",
    "--weather",
];

/// The glyph sets the patcher reports adding, by the names its log gives
/// them ("Adding 191 Glyphs from Seti-UI + Custom Set"). NOTICES.txt names
/// each of them with its license.
pub const ICON_SETS: [&str; 15] = [
    "Seti-UI + Custom",
    "Heavy Angle Brackets",
    "Box Drawing",
    "Progress Indicators",
    "Devicons",
    "Powerline Symbols",
    "Powerline Extra Symbols",
    "Pomicons",
    "Font Awesome",
    "Font Awesome Extension",
    "Power Symbols",
    "Material",
    "Weather Icons",
    "Octicons",
    "Codicons",
];

/// The date FontForge writes as the `head` table's modification time, in
/// seconds since 1970, passed as `SOURCE_DATE_EPOCH`. Without it, FontForge
/// writes the time of the build, and no two builds match. It is the date of
/// the newest source, the Nerd Fonts commit that the patcher archive is
/// made from (2025-04-24 18:21:16 UTC, which the archive's readme.md names).
pub const SOURCE_DATE_EPOCH: u64 = 1_745_518_876;

/// How many characters each face maps.
pub const CHARACTERS: usize = 11_638;

/// The Font Logos icons' code points, which no face may map.
pub const FONT_LOGOS: std::ops::RangeInclusive<u32> = 0xF300..=0xF385;

/// The Reserved Font Names of Terminus (TTF)'s license. A modified version
/// may not use them as its name; the copyright and license strings may
/// still mention them.
pub const RESERVED_NAMES: [&str; 2] = ["Terminus Font", "Terminus (TTF)"];

/// The `name` table's records that may mention a Reserved Font Name: the
/// copyright notice (0), the license description (13) and the license URL
/// (14).
pub const NAME_IDS_THAT_MAY_MENTION: [u16; 3] = [0, 13, 14];

/// The license files in `notices/` that the build copies verbatim from a
/// pinned archive, and where each comes from in it. The build checks each
/// against the archive, so the notices cannot drift from the sources.
pub const NOTICE_COPIES: [(&str, &str, &str); 9] = [
    (
        "terminus-ttf-COPYING.txt",
        TERMINUS_ARCHIVE,
        "terminus-ttf-4.49.3/COPYING",
    ),
    (
        "codicons-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/codicons/LICENSE.txt",
    ),
    (
        "font-awesome-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/font-awesome/LICENSE.txt",
    ),
    (
        "materialdesign-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/materialdesign/LICENSE",
    ),
    (
        "octicons-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/octicons/LICENSE",
    ),
    (
        "pomicons-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/pomicons/LICENSE",
    ),
    (
        "powerline-extra-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/powerline-extra/LICENSE",
    ),
    (
        "powerline-symbols-LICENSE.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/powerline-symbols/LICENSE.txt",
    ),
    (
        "weather-icons-OFL.txt",
        PATCHER_ARCHIVE,
        "src/glyphs/weather-icons/OFL.txt",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    const NOTICES: &str = include_str!("../notices/NOTICES.txt");

    #[test]
    fn the_options_leave_font_logos_out() {
        assert!(!OPTIONS.contains(&"--fontlogos"));
        assert!(!OPTIONS.contains(&"--complete"));
        assert!(!ICON_SETS.contains(&"Font Logos"));
    }

    #[test]
    fn the_notices_name_every_icon_set_and_license_file() {
        for set in ICON_SETS {
            assert!(NOTICES.contains(set), "NOTICES.txt does not name {set}");
        }
        for (file, _, _) in NOTICE_COPIES {
            assert!(NOTICES.contains(file), "NOTICES.txt does not name {file}");
            assert!(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("notices")
                    .join(file)
                    .is_file(),
                "notices/{file} is missing"
            );
        }
        for name in RESERVED_NAMES {
            assert!(NOTICES.contains(name), "NOTICES.txt does not name {name}");
        }
    }
}
