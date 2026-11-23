//! The outline fonts tier 2 draws text with, and the console's face over
//! their glyphs (ADR-1503 D9).
//!
//! Tier 2 draws its outline screens from a test font set (a frit, vtium's
//! font set file) that names one open font for each script, in the frit's
//! fallback order: DejaVu Sans Mono with its Regular, Bold, Oblique and
//! Bold Oblique faces, then Noto Sans Hebrew and Noto Sans Arabic, then
//! Noto Sans CJK, then Noto Color Emoji. DejaVu Sans Mono comes first, so
//! its cell and its size are the font set's, and its `cover=` lines leave
//! out the Hebrew and Arabic blocks, which the two Noto fonts' `cover=`
//! lines take, so each of those scripts draws from its own font (ADR-1004
//! D14). vtium ships no third-party font bytes, so each font is pinned here by its URL
//! and SHA-256 and fetched into a cache outside the tree,
//! `$XDG_CACHE_HOME/vtium/tier2/fonts`, as the specimens fetch theirs.
//! DejaVu Sans Mono's four faces come from its release archive, which
//! `fetch` downloads, checks and unpacks with `tar`, and then removes.
//!
//! A screen draws as the console draws a frit: vtium's own face for a frit
//! (`vtium::font::Chosen`), in the cell the frit's first source gives at
//! the height drawn, holding the glyphs vtium-config's engine, the one the
//! helper runs (ADR-1301 D2), generates for the characters and clusters the
//! screen shows in each style, and the built-in font drawing what no source
//! of the frit holds. vtium does no system font search, and neither does
//! tier 2: the cache is the engine's only font directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use vtium::deferred::{Shown, lock};
use vtium::font::{Chosen, DisplayFace};
use vtium::helper::{FritText, OwnCell};
use vtium_font::set::{Set, SetFace};
use vtium_font::{Cell, Face, Sha256, sha256_hex};

/// The archive a font is unpacked from, pinned by its URL and SHA-256.
#[derive(Clone, Copy, Debug)]
pub struct Archive {
    /// The name `fetch` downloads it under, in the cache.
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

/// Where a pinned font comes from.
#[derive(Clone, Copy, Debug)]
pub enum Origin {
    /// The font file itself, at this URL.
    Url(&'static str),
    /// A member of a `.tar.bz2` or `.zip` archive, at this path in it.
    Member(&'static Archive, &'static str),
}

/// The style a frit's source draws, its `style=` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FritStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FritStyle {
    /// The value of the source's `style=` line; `None` for regular, which
    /// a frit writes no line for.
    fn line(self) -> Option<&'static str> {
        match self {
            FritStyle::Regular => None,
            FritStyle::Bold => Some("bold"),
            FritStyle::Italic => Some("italic"),
            FritStyle::BoldItalic => Some("bold-italic"),
        }
    }
}

/// One pinned font: its file in the cache, the SHA-256 of its bytes, where
/// it comes from, the style the frit names it for, the frit's `cover=`
/// lines for it, none for the whole font, and whether it is the color
/// emoji font, which the `emoji-mono` screen's frit leaves out.
#[derive(Clone, Copy, Debug)]
pub struct Pinned {
    pub file: &'static str,
    pub sha256: &'static str,
    pub origin: Origin,
    pub style: FritStyle,
    pub cover: &'static [&'static str],
    pub color: bool,
}

/// The Hebrew blocks: Hebrew, and the Hebrew presentation forms of
/// Alphabetic Presentation Forms.
const HEBREW: [&str; 2] = ["U+0590-U+05FF", "U+FB1D-U+FB4F"];

/// The Arabic blocks: Arabic, Arabic Supplement, Arabic Extended-B and -A,
/// Arabic Presentation Forms-A and -B, Arabic Extended-C and the Arabic
/// Mathematical Alphabetic Symbols.
const ARABIC: [&str; 8] = [
    "U+0600-U+06FF",
    "U+0750-U+077F",
    "U+0870-U+089F",
    "U+08A0-U+08FF",
    "U+FB50-U+FDFF",
    "U+FE70-U+FEFF",
    "U+10EC0-U+10EFF",
    "U+1EE00-U+1EEFF",
];

/// Every codepoint but the [`HEBREW`] and [`ARABIC`] blocks: DejaVu Sans
/// Mono's `cover=` lines, so the two Noto fonts after it draw those
/// scripts.
const NOT_HEBREW_OR_ARABIC: [&str; 8] = [
    "U+0000-U+058F",
    "U+0700-U+074F",
    "U+0780-U+086F",
    "U+0900-U+FB1C",
    "U+FE00-U+FE6F",
    "U+FF00-U+10EBF",
    "U+10F00-U+1EDFF",
    "U+1EF00-U+10FFFF",
];

/// DejaVu Sans Mono's release 2.37, the TrueType fonts' archive.
pub const DEJAVU: Archive = Archive {
    file: "dejavu-fonts-ttf-2.37.tar.bz2",
    url: "https://github.com/dejavu-fonts/dejavu-fonts/releases/download/version_2_37/dejavu-fonts-ttf-2.37.tar.bz2",
    sha256: "fa9ca4d13871dd122f61258a80d01751d603b4d3ee14095d65453b4e846e17d7",
};

/// Noto Sans Arabic from the Noto project's release of September 2024,
/// which draws the Arabic blocks in both of tier 2's frits.
const NOTO_SANS_ARABIC: Pinned = Pinned {
    file: "NotoSansArabic-Regular.ttf",
    sha256: "c270782ee478c685f95509e724125729e1284c82ebfbe414cc67926f29449e1d",
    origin: Origin::Url(
        "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansArabic/hinted/ttf/NotoSansArabic-Regular.ttf",
    ),
    style: FritStyle::Regular,
    cover: &ARABIC,
    color: false,
};

/// Noto Sans Hebrew, which draws the Hebrew blocks in the test frit.
const NOTO_SANS_HEBREW: Pinned = Pinned {
    file: "NotoSansHebrew-Regular.ttf",
    sha256: "cdefaf8efd47045f6820928eba84db5bed7557539328952b5f828315485e02ee",
    origin: Origin::Url(
        "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf",
    ),
    style: FritStyle::Regular,
    cover: &HEBREW,
    color: false,
};

/// Noto Sans Devanagari, in the scripts frit.
const NOTO_SANS_DEVANAGARI: Pinned = Pinned {
    file: "NotoSansDevanagari-Regular.ttf",
    sha256: "306b53ecfb182a504dd8a7446093c316387d2fd8dc350d0792ed1753fe0996cd",
    origin: Origin::Url(
        "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansDevanagari/hinted/ttf/NotoSansDevanagari-Regular.ttf",
    ),
    style: FritStyle::Regular,
    cover: &[],
    color: false,
};

/// Noto Sans Bengali, in the scripts frit.
const NOTO_SANS_BENGALI: Pinned = Pinned {
    file: "NotoSansBengali-Regular.ttf",
    sha256: "9f8b4dd091f19b111d24ea18daae81bea8684cc67de17ea1acd797e144bf20ba",
    origin: Origin::Url(
        "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansBengali/hinted/ttf/NotoSansBengali-Regular.ttf",
    ),
    style: FritStyle::Regular,
    cover: &[],
    color: false,
};

/// Iosevka Term 34.9.1's TrueType release archive, a zip.
pub const IOSEVKA: Archive = Archive {
    file: "PkgTTF-IosevkaTerm-34.9.1.zip",
    url: "https://github.com/be5invis/Iosevka/releases/download/v34.9.1/PkgTTF-IosevkaTerm-34.9.1.zip",
    sha256: "3bd57a66a7f1e429a131d3bc35f4d0f3a8eb722f2fe8f59f595e424bda9a33c9",
};

/// The scripts frit's fonts, in its order: Iosevka Term 34.9.1, whose
/// cell at 40 pixels is 16x40, half an em wide, then Noto Sans
/// Devanagari, Noto Sans Bengali and Noto Sans Arabic from the Noto
/// project's release of September 2024, which draw the scripts Iosevka
/// Term lacks. These are the fonts the showcase's scripts scene draws
/// Hindi, Bengali and Arabic with, as its `showcase-iosevka` frit names
/// them, and the files the showcase pins.
pub const SCRIPT_FONTS: [Pinned; 4] = [
    Pinned {
        file: "IosevkaTerm-Regular.ttf",
        sha256: "430ad4f56c6cd82e4c954efb6dd4b3e2b4cc2b2b1bd4f8143f3d3800096f32b4",
        origin: Origin::Member(&IOSEVKA, "IosevkaTerm-Regular.ttf"),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    NOTO_SANS_DEVANAGARI,
    NOTO_SANS_BENGALI,
    NOTO_SANS_ARABIC,
];

/// The height in pixels the scripts frit draws at, which gives Iosevka
/// Term its own 16x40 cell, the showcase's.
pub const SCRIPTS_HEIGHT: u32 = 40;

/// A font set vtium ships for a script that needs room above and below its
/// letters (vtium/vtium#204): its name and its text, as
/// `crates/vtium-config/frits/scripts/` holds it. It names its fonts by family, and the
/// engine finds each family in the cache, its only font directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShippedFrit {
    pub name: &'static str,
    pub text: &'static str,
}

/// The catalog's script set `name`, read from the `scripts/` category
/// folder as vtium-config ships it.
macro_rules! shipped {
    ($name:literal) => {
        ShippedFrit {
            name: $name,
            text: include_str!(concat!(
                "../../../crates/vtium-config/frits/scripts/",
                $name,
                ".vtfrit"
            )),
        }
    };
}

/// The text of the catalog's script layer `name`, as vtium-config ships
/// it. The layers every set shares after its script, Nerd Fonts' icons,
/// color emoji and GNU Unifont, are left out: tier 2 pins none of their
/// fonts, and the `frit-` screens judge the script sets' own fonts.
fn script_layer(name: &str) -> Option<&'static str> {
    macro_rules! layer {
        ($name:literal) => {
            include_str!(concat!(
                "../../../crates/vtium-config/frits/layers/",
                $name,
                ".vtlayer"
            ))
        };
    }
    Some(match name {
        "arabic" => layer!("arabic"),
        "hebrew" => layer!("hebrew"),
        "indic" => layer!("indic"),
        "thai-lao" => layer!("thai-lao"),
        "cjk-sc" => layer!("cjk-sc"),
        "cjk-tc" => layer!("cjk-tc"),
        "cjk-jp" => layer!("cjk-jp"),
        "cjk-kr" => layer!("cjk-kr"),
        _ => return None,
    })
}

impl ShippedFrit {
    /// The set's text with its script layer in place, as the console hands
    /// a set to the helper.
    pub fn spliced(&self) -> String {
        vtium_font::set::layer::expand(self.text, &mut |name| {
            Ok(script_layer(name).map(str::to_owned))
        })
        .text
    }
}

/// The shipped font sets the `frit-` screens draw, in their screens' order.
/// Of the four CJK sets, the three whose Han forms differ from the
/// Simplified Chinese ones that the `cjk` and `wide-deco` screens draw have
/// a screen each.
pub const SHIPPED_FRITS: [ShippedFrit; 8] = [
    shipped!("noto-arabic"),
    shipped!("noto-hebrew"),
    shipped!("noto-indic"),
    shipped!("noto-thai-lao"),
    shipped!("noto-vietnamese"),
    shipped!("noto-cjk-jp"),
    shipped!("noto-cjk-tc"),
    shipped!("noto-cjk-kr"),
];

/// Every font the shipped font sets name, each the file of its family in
/// the Noto project's release of September 2024: Noto Sans Mono, which
/// sets every one of their cells, then each script's font, then the
/// regional faces of Noto Sans CJK 2.004, one file for each region, from
/// the release the test frit's Noto Sans CJK SC comes from.
pub const SHIPPED_FONTS: [Pinned; 18] = [
    Pinned {
        file: "NotoSansMono-Regular.ttf",
        sha256: "65b5e2b2c4a1fba9ae8be1f026cb35b03dcb8886d9b2a4147054fde12f7e767d",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansMono/hinted/ttf/NotoSansMono-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    NOTO_SANS_ARABIC,
    NOTO_SANS_HEBREW,
    NOTO_SANS_DEVANAGARI,
    NOTO_SANS_BENGALI,
    Pinned {
        file: "NotoSansGurmukhi-Regular.ttf",
        sha256: "658d0207da305a1411c539a8b0bbeda64d4146e54fb4827facddb890b6b90d74",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansGurmukhi/hinted/ttf/NotoSansGurmukhi-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansGujarati-Regular.ttf",
        sha256: "9b5a7aaeeb649a2e75a49d8b006a1f87db1b61c0df3b001609f4e0725d88dbf6",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansGujarati/hinted/ttf/NotoSansGujarati-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansOriya-Regular.ttf",
        sha256: "38ac5eae2e1666558b88c63bf584349d6f924cca6b3b8867811546734afeb251",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansOriya/hinted/ttf/NotoSansOriya-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansTamil-Regular.ttf",
        sha256: "3c0a186feb3c63c7f6d63e1511dcdc144e745ae09b98e217c83f3e317974f6f9",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansTamil/hinted/ttf/NotoSansTamil-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansTelugu-Regular.ttf",
        sha256: "b274780b69d1d23fe84b55e809a152cb2ac5306d33864b1f87622f6971871aae",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansTelugu/hinted/ttf/NotoSansTelugu-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansKannada-Regular.ttf",
        sha256: "f1a39a97a40486a345b1930169cb111098a4eaf40a88eb4303d2c0b8e7ceeeb5",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansKannada/hinted/ttf/NotoSansKannada-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansMalayalam-Regular.ttf",
        sha256: "c08de7fa8d032a5d6a4d120fb82c78cec60b362a4e73fa26360d89759ff2a7f9",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansMalayalam/hinted/ttf/NotoSansMalayalam-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansSinhala-Regular.ttf",
        sha256: "493a1c9cb7151783dac21456e45ad1bec554b3361b830734dcdaf6304421dfe6",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansSinhala/hinted/ttf/NotoSansSinhala-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansThai-Regular.ttf",
        sha256: "61cf814eec46b294d6ea4401ac295d0cecd5207bd2331dcc5a15e7301d30ee44",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansThai/hinted/ttf/NotoSansThai-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansLao-Regular.ttf",
        sha256: "0a86e5e1ccfe34ca78c43fac6829dc751b42bcc469272a9a55325aae587bfbe7",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansLao/hinted/ttf/NotoSansLao-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansCJKjp-Regular.otf",
        sha256: "68a3fc98800b2a27b371f2fb79991daf3633bd89309d4ffaa6946fd587f375b5",
        origin: Origin::Url(
            "https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/OTF/Japanese/NotoSansCJKjp-Regular.otf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansCJKtc-Regular.otf",
        sha256: "dce08bd4fd91aa8aa76ed8fea4b694c2dfb8550f67871e326843212ddbeb88b4",
        origin: Origin::Url(
            "https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/OTF/TraditionalChinese/NotoSansCJKtc-Regular.otf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoSansCJKkr-Regular.otf",
        sha256: "6bcb2a0703aa137e874fc2dffa85f6c21ba9a67fa329e81b8c801663af7e992a",
        origin: Origin::Url(
            "https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/OTF/Korean/NotoSansCJKkr-Regular.otf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
];

/// The height in pixels the `frit-` screens draw the shipped font sets
/// at: the text's height, which each set's line spacing then makes taller.
pub const SHIPPED_HEIGHT: u32 = 32;

/// The test frit's fonts, in its order: DejaVu Sans Mono 2.37's four faces,
/// Noto Sans Hebrew and Noto Sans Arabic from the Noto project's release of
/// September 2024, Noto Sans CJK SC 2.004, which holds the Chinese,
/// Japanese and Korean characters, and Noto Color Emoji 2.051, whose glyphs
/// are CBDT bitmaps. Noto Sans CJK SC and Noto Color Emoji are the files
/// the showcase and the specimens pin.
pub const FONTS: [Pinned; 8] = [
    Pinned {
        file: "DejaVuSansMono.ttf",
        sha256: "b4a6c3e4faab8773f4ff761d56451646409f29abedd68f05d38c2df667d3c582",
        origin: Origin::Member(&DEJAVU, "dejavu-fonts-ttf-2.37/ttf/DejaVuSansMono.ttf"),
        style: FritStyle::Regular,
        cover: &NOT_HEBREW_OR_ARABIC,
        color: false,
    },
    Pinned {
        file: "DejaVuSansMono-Bold.ttf",
        sha256: "bce60f1b4421acd9ea51ba6623d7024ecbe6817a953e3654df62a5e6bdf8f769",
        origin: Origin::Member(&DEJAVU, "dejavu-fonts-ttf-2.37/ttf/DejaVuSansMono-Bold.ttf"),
        style: FritStyle::Bold,
        cover: &NOT_HEBREW_OR_ARABIC,
        color: false,
    },
    Pinned {
        file: "DejaVuSansMono-Oblique.ttf",
        sha256: "742097840c541870e8d6dc5c9b37bb1ceeea6c0dedd1d475faf903ef9df734b0",
        origin: Origin::Member(
            &DEJAVU,
            "dejavu-fonts-ttf-2.37/ttf/DejaVuSansMono-Oblique.ttf",
        ),
        style: FritStyle::Italic,
        cover: &NOT_HEBREW_OR_ARABIC,
        color: false,
    },
    Pinned {
        file: "DejaVuSansMono-BoldOblique.ttf",
        sha256: "91713a71d550bba22c2a6b2bb2a9ad8f9a159e12e4e9f0a5b2677998ba21213e",
        origin: Origin::Member(
            &DEJAVU,
            "dejavu-fonts-ttf-2.37/ttf/DejaVuSansMono-BoldOblique.ttf",
        ),
        style: FritStyle::BoldItalic,
        cover: &NOT_HEBREW_OR_ARABIC,
        color: false,
    },
    NOTO_SANS_HEBREW,
    NOTO_SANS_ARABIC,
    Pinned {
        file: "NotoSansCJKsc-Regular.otf",
        sha256: "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b",
        origin: Origin::Url(
            "https://github.com/notofonts/noto-cjk/raw/Sans2.004/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: false,
    },
    Pinned {
        file: "NotoColorEmoji.ttf",
        sha256: "72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b",
        origin: Origin::Url(
            "https://github.com/googlefonts/noto-emoji/raw/v2.051/fonts/NotoColorEmoji.ttf",
        ),
        style: FritStyle::Regular,
        cover: &[],
        color: true,
    },
];

/// The height in pixels the outline screens draw at, which gives DejaVu
/// Sans Mono its own 9x16 cell.
pub const HEIGHT: u32 = 16;

/// The locale the built-in font is chosen for: one that is not Chinese,
/// Japanese or Korean, so a cell of 16 pixels draws unifont, as it does at
/// every height from 16.
const LOCALE: &str = "C";

/// The cache the pinned fonts are fetched into and read from:
/// `VTIUM_TIER2_FONTS` when it is set, else
/// `$XDG_CACHE_HOME/vtium/tier2/fonts`.
pub fn cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VTIUM_TIER2_FONTS").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("vtium/tier2/fonts")
}

/// The SHA-256 of `path`'s bytes, or why it cannot be read.
fn digest(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(sha256_hex(&Sha256::digest(&bytes)))
}

/// Why `file` in `dir` is not the pinned bytes `sha256`, absent included;
/// `None` when it is.
pub(crate) fn fault(dir: &Path, file: &str, sha256: &str) -> Option<String> {
    match digest(&dir.join(file)) {
        Err(e) => Some(format!("{file} is not in {}: {e}", dir.display())),
        Ok(got) if got != sha256 => Some(format!("{file} has SHA-256 {got}, not {sha256}")),
        Ok(_) => None,
    }
}

/// Why the fonts in `dir` cannot be drawn from: the first pinned font that
/// is absent or not its pinned bytes, with the command that fetches them.
pub fn check(dir: &Path) -> Result<(), String> {
    match FONTS.iter().find_map(|f| fault(dir, f.file, f.sha256)) {
        Some(why) => Err(format!(
            "{why}; `make tier2-fonts` fetches the pinned fonts"
        )),
        None => Ok(()),
    }
}

/// Why the scripts frit's fonts in `dir` cannot be drawn from, as
/// [`check`] says it for the test frit's.
pub fn check_scripts(dir: &Path) -> Result<(), String> {
    match SCRIPT_FONTS
        .iter()
        .find_map(|f| fault(dir, f.file, f.sha256))
    {
        Some(why) => Err(format!(
            "{why}; `make tier2-fonts` fetches the pinned fonts"
        )),
        None => Ok(()),
    }
}

/// Why the shipped font sets' fonts in `dir` cannot be drawn from, as
/// [`check`] says it for the test frit's.
pub fn check_shipped(dir: &Path) -> Result<(), String> {
    match SHIPPED_FONTS
        .iter()
        .find_map(|f| fault(dir, f.file, f.sha256))
    {
        Some(why) => Err(format!(
            "{why}; `make tier2-fonts` fetches the pinned fonts"
        )),
        None => Ok(()),
    }
}

/// The scripts frit's text, its sources the fonts in `dir`: each of
/// [`SCRIPT_FONTS`] named by its path, with its `cover=` lines.
pub fn scripts_frit_text(dir: &Path) -> String {
    let mut text = String::new();
    for font in &SCRIPT_FONTS {
        text.push_str(&format!(
            "source=outline:{}\n",
            dir.join(font.file).display()
        ));
        for cover in font.cover {
            text.push_str(&format!("cover={cover}\n"));
        }
    }
    text
}

/// Downloads `url` into `dir` as `file` with curl, through a `.part` file,
/// so a download that stops leaves no file of that name.
pub(crate) fn download(dir: &Path, file: &str, url: &str) -> Result<(), String> {
    let part = dir.join(format!("{file}.part"));
    let status = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("curl {url}: {status}"));
    }
    let path = dir.join(file);
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `member` of the archive `archive` to `dir` as `file`, through a
/// `.part` file: with `unzip` for a `.zip` archive, and with `tar` for a
/// `.tar.bz2` one.
fn unpack(archive: &Path, member: &str, dir: &Path, file: &str) -> Result<(), String> {
    let part = dir.join(format!("{file}.part"));
    let out = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let zip = archive.extension().is_some_and(|e| e == "zip");
    let (tool, args) = if zip {
        ("unzip", ["-p"])
    } else {
        ("tar", ["-xjOf"])
    };
    let status = Command::new(tool)
        .args(args)
        .arg(archive)
        .arg(member)
        .stdout(Stdio::from(out))
        .status()
        .map_err(|e| format!("{tool}: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{tool} {} {member}: {status}", archive.display()));
    }
    let path = dir.join(file);
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Fetches each of `fonts` that `dir` lacks or holds with other bytes, and
/// checks every one against its SHA-256. A font is downloaded with curl,
/// or unpacked from its archive, which is downloaded and checked first and
/// removed once every font it holds is in place. A file that does not
/// match its SHA-256 is removed, and the fetch fails naming it.
pub fn fetch(dir: &Path, fonts: &[Pinned]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut archives: Vec<&Archive> = Vec::new();
    for font in fonts {
        if fault(dir, font.file, font.sha256).is_none() {
            continue;
        }
        match font.origin {
            Origin::Url(url) => download(dir, font.file, url)?,
            Origin::Member(archive, member) => {
                if fault(dir, archive.file, archive.sha256).is_some() {
                    download(dir, archive.file, archive.url)?;
                }
                if let Some(why) = fault(dir, archive.file, archive.sha256) {
                    let _ = std::fs::remove_file(dir.join(archive.file));
                    return Err(why);
                }
                unpack(&dir.join(archive.file), member, dir, font.file)?;
                if !archives.iter().any(|a| a.file == archive.file) {
                    archives.push(archive);
                }
            }
        }
        if let Some(why) = fault(dir, font.file, font.sha256) {
            let _ = std::fs::remove_file(dir.join(font.file));
            return Err(why);
        }
    }
    for archive in archives {
        let _ = std::fs::remove_file(dir.join(archive.file));
    }
    Ok(())
}

/// The test frit's text, its sources the fonts in `dir`: every pinned
/// font, less the color emoji font unless `color`, each named by its path
/// with its `style=` line and its `cover=` lines, and `line-spacing=` with
/// `spacing` when it is not 0 (ADR-1302 D13).
pub fn frit_text(dir: &Path, color: bool, spacing: u32) -> String {
    let mut text = String::new();
    for font in FONTS.iter().filter(|f| color || !f.color) {
        text.push_str(&format!(
            "source=outline:{}\n",
            dir.join(font.file).display()
        ));
        if let Some(style) = font.style.line() {
            text.push_str(&format!("style={style}\n"));
        }
        for cover in font.cover {
            text.push_str(&format!("cover={cover}\n"));
        }
    }
    if spacing > 0 {
        text.push_str(&format!("line-spacing={spacing}\n"));
    }
    text
}

/// The bitmap-first frit's text, its outline sources the fonts in `dir`.
/// The built-in GNU Unifont comes first, limited to ASCII by its `cover=`
/// line, so it is a bitmap first source that sets the 8x16 cell. Each
/// regular font of the test frit follows with its `cover=` lines, the
/// color emoji font included, and with a `render=smooth` line under it
/// when `smooth` is set. Without that line, each outline source draws
/// 1-bit, because the first source is a bitmap font.
///
/// References:
/// - ADR-1200 D1, the `render=` row: the default behind a bitmap first
///   source and its override.
pub fn bitmap_frit_text(dir: &Path, smooth: bool) -> String {
    let mut text = String::from("source=builtin:unifont\ncover=ascii\n");
    for font in FONTS.iter().filter(|f| f.style.line().is_none()) {
        text.push_str(&format!(
            "source=outline:{}\n",
            dir.join(font.file).display()
        ));
        for cover in font.cover {
            text.push_str(&format!("cover={cover}\n"));
        }
        if smooth {
            text.push_str("render=smooth\n");
        }
    }
    text
}

/// The test frit's name: `tier2-outline`, or `tier2-outline-mono` without
/// the color emoji font.
fn frit_name(color: bool) -> &'static str {
    if color {
        "tier2-outline"
    } else {
        "tier2-outline-mono"
    }
}

/// `(first, count)` runs of `chars`, which are sorted and each once.
fn runs(chars: &[char]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for c in chars.iter().map(|&c| u32::from(c)) {
        match runs.last_mut() {
            Some((first, n)) if first.checked_add(*n) == Some(c) => *n += 1,
            _ => runs.push((c, 1)),
        }
    }
    runs
}

/// The test frit, in the fonts in `dir`, less the color emoji font unless
/// `color`, its rows made taller by `spacing` per cent of line spacing
/// (ADR-1302 D13), as the console holds it at [`HEIGHT`] once the helper
/// has answered for what the screen shows in each style, which `shown`
/// reads in the frit's whole cell: the cell the frit's first source gives
/// at that height, taller by the rows the spacing adds, and in the
/// console's store of generated glyphs, the glyphs the engine generates
/// for each style's characters and clusters at that cell and its added
/// rows, as the helper answers the console's asks for them.
/// [`Chosen::face`] then draws through that store, and in the built-in
/// font what no source holds.
pub fn chosen(
    dir: &Path,
    (color, spacing): (bool, u32),
    shown: impl FnOnce(Cell) -> Result<Vec<Shown>, String>,
) -> Result<Chosen, String> {
    check(dir)?;
    let text = frit_text(dir, color, spacing);
    chosen_frit((frit_name(color), text), dir, HEIGHT, shown)
}

/// The frit `name` of `text`, whose fonts are in `dir`, as the console
/// holds it at `height` pixels once the helper has answered for what the
/// screen shows in each style, as [`chosen`] gives the test frit. Where
/// the screen shows nothing the helper draws, the store also holds the
/// first answer of the helper's fill, which brings the font's own lines.
pub fn chosen_frit(
    (name, text): (&str, String),
    dir: &Path,
    height: u32,
    shown: impl FnOnce(Cell) -> Result<Vec<Shown>, String>,
) -> Result<Chosen, String> {
    use vtium_config::serve::{Engine, FritText as EngineFrit, OwnCell as EngineCell, Paths};
    use vtium_font::spacing::SpacedCell;
    let paths = Paths {
        font_dirs: vec![dir.to_path_buf()],
        ..Paths::default()
    };
    let engine = Engine::load(
        &EngineFrit {
            name: name.to_owned(),
            text: text.clone(),
        },
        &paths,
        None,
    )?;
    if !engine.left_out().is_empty() {
        let why: Vec<String> = engine
            .left_out()
            .iter()
            .map(|l| format!("source={}: {}", l.spelling, l.why))
            .collect();
        return Err(format!("the frit {name} left out {}", why.join("; ")));
    }
    // the cell the frit's first source gives, as the helper answers the
    // console's cell query: a bitmap font's own cell, or an outline font's
    // at `height`
    let own = match engine.own_cell(height)? {
        EngineCell::Outline(cell) => OwnCell::Outline(cell),
        EngineCell::Bitmap(cell) => OwnCell::Bitmap(cell),
    };
    let mut chosen = Chosen::frit(
        FritText {
            name: name.to_owned(),
            text,
        },
        false,
        None,
    )?;
    if !chosen.wants_own_cell(height, false) {
        return Err(format!(
            "the frit {name} asks for no cell at {height} pixels"
        ));
    }
    chosen.set_own_cell(own, false);
    // the cell and the rows its spacing adds, as the console reads them
    // from a display's face to ask the helper at
    let at = {
        let face = face_at(&chosen, height)?;
        SpacedCell {
            cell: face.cell(),
            band: face.band(),
        }
    };
    let Chosen::Frit { generated, .. } = &chosen else {
        return Err(format!("the frit {name} is not taken as a frit"));
    };
    // whether a set of normal glyphs is held, which gives the font's lines
    let mut lines = false;
    for one in &shown(at.cell)? {
        let clusters: Vec<Vec<u32>> = one
            .clusters
            .iter()
            .map(|t| t.iter().map(|&c| u32::from(c)).collect())
            .collect();
        if let Some(doubled) = one.doubled {
            add_doubled(
                &engine,
                generated,
                (at, one.variant, doubled),
                (&one.chars, &clusters),
            )
            .map_err(|why| format!("the frit {name}: {why}"))?;
            continue;
        }
        let added = hold(
            &engine,
            generated,
            (at, one.variant),
            (&one.chars, &clusters),
        )
        .map_err(|why| format!("the frit {name}: {why}"))?;
        lines |= added && one.variant.script == vtium_font::Script::Baseline;
    }
    // the console starts the helper's fill at each cell it draws at, and
    // the fill's first answer holds regular glyphs, printable Latin and
    // box drawing first, whatever the screen shows; those glyphs bring the
    // font's own lines, so a screen that shows no character, such as a
    // line of underlined spaces, draws its lines where the font puts them.
    // The fill's glyphs change no pixel of a screen whose asks hold a set,
    // so tier 2 holds the fill's first answer only where they hold none.
    if !lines {
        let fill: Vec<char> = vtium_config::serve::LATIN_AND_BOX
            .iter()
            .flat_map(|&(a, b)| a..=b)
            .filter_map(char::from_u32)
            .collect();
        hold(
            &engine,
            generated,
            (at, vtium_font::Variant::default()),
            (&fill, &[]),
        )
        .map_err(|why| format!("the frit {name}: {why}"))?;
    }
    Ok(chosen)
}

/// Adds to the console's store `generated` the glyphs of `chars` and of
/// `clusters` that the engine draws for the cell `at` in `variant`, as the
/// helper answers an ask and the console takes the answer: whether the
/// store took a set, which it does not when the engine draws none.
fn hold(
    engine: &vtium_config::serve::Engine,
    generated: &vtium::deferred::Shared,
    (at, variant): (vtium_font::spacing::SpacedCell, vtium_font::Variant),
    (chars, clusters): (&[char], &[Vec<u32>]),
) -> Result<bool, String> {
    let want: Vec<u32> = chars.iter().map(|&c| u32::from(c)).collect();
    let Some(bytes) = engine.draw_variant(at, variant, &want, (clusters, true))? else {
        return Ok(false);
    };
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let set = Arc::new(Set::from_bytes(bytes).map_err(|e| format!("{e:?}"))?);
    let face = SetFace::new(Arc::clone(&set), at.cell.height)
        .ok_or_else(|| format!("no strike at {}", at.cell.height))?;
    // the codepoints the set maps in the variant, as the helper's frame
    // names them
    let held: Vec<char> = chars
        .iter()
        .copied()
        .filter(|&c| face.draw_variant(&[c], variant).is_some())
        .collect();
    let mut store = lock(generated);
    let fresh = store
        .fresh((at, variant), set, &runs(&held), len)
        .ok_or("the store refused its glyphs")?;
    Ok(store.add(fresh).is_some())
}

/// Adds to the console's store `generated` the glyphs of `chars` and of
/// `clusters` that the engine draws at the double size `doubled` for the
/// cell `at` in `variant`, as the helper answers a double ask and the
/// console takes the answer. The engine refuses a superscript or subscript and a cell
/// whose double passes the largest cell, as the helper does, and the set
/// must hold one cell, the one the double size gives `at`, as the console
/// checks.
fn add_doubled(
    engine: &vtium_config::serve::Engine,
    generated: &vtium::deferred::Shared,
    (at, variant, doubled): (
        vtium_font::spacing::SpacedCell,
        vtium_font::Variant,
        vtium_font::Doubled,
    ),
    (chars, clusters): (&[char], &[Vec<u32>]),
) -> Result<(), String> {
    let mut want: Vec<u32> = chars.iter().map(|&c| u32::from(c)).collect();
    want.sort_unstable();
    want.dedup();
    let Some(bytes) = engine.draw_doubled_clusters(at, (variant, doubled), &want, clusters)? else {
        return Ok(());
    };
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let set = Arc::new(Set::from_bytes(bytes).map_err(|e| format!("{e:?}"))?);
    let drawn = doubled
        .cell(at.cell)
        .ok_or("a double size past the largest cell")?;
    if set.cells() != [drawn] {
        return Err(format!(
            "the glyphs at {doubled:?} are not at the cell {}x{}",
            drawn.width, drawn.height
        ));
    }
    let face = SetFace::new(Arc::clone(&set), drawn.height)
        .ok_or_else(|| format!("no strike at {}", drawn.height))?;
    let held: Vec<char> = chars
        .iter()
        .copied()
        .filter(|&c| face.draw_variant(&[c], variant).is_some())
        .collect();
    let mut store = lock(generated);
    let fresh = store
        .fresh_doubled((at, variant, doubled), set, &runs(&held), len)
        .ok_or("the store refused the glyphs at a double size")?;
    store.add(fresh);
    Ok(())
}

/// The face `chosen` draws through at [`HEIGHT`], as a display of the
/// console's takes it.
pub fn face(chosen: &Chosen) -> Result<DisplayFace, String> {
    face_at(chosen, HEIGHT)
}

/// The face `chosen` draws through at `height` pixels, as a display of
/// the console's takes it.
pub fn face_at(chosen: &Chosen, height: u32) -> Result<DisplayFace, String> {
    chosen
        .face(height, LOCALE)
        .ok_or_else(|| format!("no face at {height} pixels"))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// A directory of a test's own, removed when the test ends, whether it
    /// passes or fails.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir =
                std::env::temp_dir().join(format!("vtium-tier2-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sha(bytes: &[u8]) -> String {
        sha256_hex(&Sha256::digest(bytes))
    }

    // ADR-1503 D9, ADR-1004 D14: the frit names DejaVu Sans Mono's four
    // faces by style, each covering all but the Hebrew and Arabic blocks,
    // then Noto Sans Hebrew and Noto Sans Arabic, each covering its own
    // blocks, then Noto Sans CJK, then Noto Color Emoji, and the emoji-mono
    // screen's frit is the same without the color emoji font; ADR-1302
    // D13: a spaced screen's frit ends with its line-spacing= line
    #[test]
    fn the_frit_names_each_font_in_order_with_its_style_and_cover() {
        let dir = Path::new("/c");
        assert_eq!(
            frit_text(dir, true, 0),
            "source=outline:/c/DejaVuSansMono.ttf\ncover=U+0000-U+058F\ncover=U+0700-U+074F\ncover=U+0780-U+086F\ncover=U+0900-U+FB1C\ncover=U+FE00-U+FE6F\ncover=U+FF00-U+10EBF\ncover=U+10F00-U+1EDFF\ncover=U+1EF00-U+10FFFF\n\
             source=outline:/c/DejaVuSansMono-Bold.ttf\nstyle=bold\ncover=U+0000-U+058F\ncover=U+0700-U+074F\ncover=U+0780-U+086F\ncover=U+0900-U+FB1C\ncover=U+FE00-U+FE6F\ncover=U+FF00-U+10EBF\ncover=U+10F00-U+1EDFF\ncover=U+1EF00-U+10FFFF\n\
             source=outline:/c/DejaVuSansMono-Oblique.ttf\nstyle=italic\ncover=U+0000-U+058F\ncover=U+0700-U+074F\ncover=U+0780-U+086F\ncover=U+0900-U+FB1C\ncover=U+FE00-U+FE6F\ncover=U+FF00-U+10EBF\ncover=U+10F00-U+1EDFF\ncover=U+1EF00-U+10FFFF\n\
             source=outline:/c/DejaVuSansMono-BoldOblique.ttf\nstyle=bold-italic\ncover=U+0000-U+058F\ncover=U+0700-U+074F\ncover=U+0780-U+086F\ncover=U+0900-U+FB1C\ncover=U+FE00-U+FE6F\ncover=U+FF00-U+10EBF\ncover=U+10F00-U+1EDFF\ncover=U+1EF00-U+10FFFF\n\
             source=outline:/c/NotoSansHebrew-Regular.ttf\ncover=U+0590-U+05FF\ncover=U+FB1D-U+FB4F\n\
             source=outline:/c/NotoSansArabic-Regular.ttf\ncover=U+0600-U+06FF\ncover=U+0750-U+077F\ncover=U+0870-U+089F\ncover=U+08A0-U+08FF\ncover=U+FB50-U+FDFF\ncover=U+FE70-U+FEFF\ncover=U+10EC0-U+10EFF\ncover=U+1EE00-U+1EEFF\n\
             source=outline:/c/NotoSansCJKsc-Regular.otf\n\
             source=outline:/c/NotoColorEmoji.ttf\n"
        );
        assert_eq!(
            frit_text(dir, false, 0),
            frit_text(dir, true, 0).replace("source=outline:/c/NotoColorEmoji.ttf\n", "")
        );
        assert_eq!(
            frit_text(dir, true, 30),
            format!("{}line-spacing=30\n", frit_text(dir, true, 0))
        );
    }

    // a font absent or of other bytes is refused, naming the font and the
    // command that fetches the fonts
    #[test]
    fn a_font_absent_or_of_other_bytes_is_refused() {
        let s = Scratch::new("check");
        let e = check(&s.0).unwrap_err();
        assert!(e.starts_with("DejaVuSansMono.ttf is not in"), "{e}");
        assert!(
            e.ends_with("`make tier2-fonts` fetches the pinned fonts"),
            "{e}"
        );
        std::fs::write(s.0.join("DejaVuSansMono.ttf"), b"not the font").unwrap();
        let e = check(&s.0).unwrap_err();
        assert!(
            e.starts_with(&format!(
                "DejaVuSansMono.ttf has SHA-256 {}, not {}",
                sha(b"not the font"),
                FONTS[0].sha256
            )),
            "{e}"
        );
    }

    // ADR-1004 D14: DejaVu Sans Mono's cover= lines and the two Noto fonts'
    // share every codepoint out between them, each once, so Hebrew and
    // Arabic draw from their own fonts and everything else from DejaVu Sans
    // Mono, which sets the cell as the first source
    #[test]
    fn dejavu_and_the_noto_fonts_share_every_codepoint_out_once() {
        let range = |r: &str| {
            let (a, b) = r.split_once('-').unwrap();
            let point = |p: &str| u32::from_str_radix(p.strip_prefix("U+").unwrap(), 16).unwrap();
            (point(a), point(b))
        };
        let mut all: Vec<(u32, u32)> = NOT_HEBREW_OR_ARABIC
            .iter()
            .chain(&HEBREW)
            .chain(&ARABIC)
            .map(|r| range(r))
            .collect();
        all.sort_unstable();
        let mut next = 0;
        for (a, b) in all {
            assert_eq!(a, next, "a gap or an overlap before U+{a:04X}");
            next = b + 1;
        }
        assert_eq!(next, 0x11_0000);
        let first = FONTS.first().unwrap();
        assert_eq!(
            first.file, "DejaVuSansMono.ttf",
            "DejaVu Sans Mono sets the cell"
        );
        let text = frit_text(Path::new("/f"), true, 0);
        assert!(
            text.contains("source=outline:/f/NotoSansArabic-Regular.ttf\ncover=U+0600-U+06FF\n")
        );
    }

    // fetch takes a font the cache holds with its pinned bytes as it is,
    // and removes one that does not match after it is fetched; here the
    // URL cannot be reached, so the fetch fails and nothing of that name
    // is left
    #[test]
    fn fetch_keeps_a_pinned_font_and_leaves_no_file_that_fails() {
        let s = Scratch::new("fetch");
        let held = Pinned {
            file: "held.ttf",
            sha256: "",
            origin: Origin::Url("file:///nowhere"),
            style: FritStyle::Regular,
            cover: &[],
            color: false,
        };
        std::fs::write(s.0.join("held.ttf"), b"held").unwrap();
        let held = Pinned {
            sha256: Box::leak(sha(b"held").into_boxed_str()),
            ..held
        };
        fetch(&s.0, &[held]).unwrap();
        assert_eq!(std::fs::read(s.0.join("held.ttf")).unwrap(), b"held");
        let gone = Pinned {
            file: "gone.ttf",
            origin: Origin::Url("file:///nowhere/at/all"),
            ..held
        };
        assert!(fetch(&s.0, &[gone]).is_err());
        assert!(!s.0.join("gone.ttf").exists());
        assert!(!s.0.join("gone.ttf.part").exists());
    }

    // a font unpacked from its archive is checked against its own SHA-256,
    // and the archive, checked first, is removed once the font is in place
    #[test]
    fn a_font_is_unpacked_from_its_checked_archive() {
        let s = Scratch::new("unpack");
        let src = s.0.join("src");
        std::fs::create_dir_all(src.join("pkg")).unwrap();
        std::fs::write(src.join("pkg/f.ttf"), b"face").unwrap();
        let tar = s.0.join("a.tar.bz2");
        let ok = Command::new("tar")
            .arg("-cjf")
            .arg(&tar)
            .arg("-C")
            .arg(&src)
            .arg("pkg/f.ttf")
            .status()
            .unwrap();
        assert!(ok.success());
        let bytes = std::fs::read(&tar).unwrap();
        let cache = s.0.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("a.tar.bz2"), &bytes).unwrap();
        let archive: &'static Archive = Box::leak(Box::new(Archive {
            file: "a.tar.bz2",
            url: "file:///nowhere",
            sha256: Box::leak(sha(&bytes).into_boxed_str()),
        }));
        let font = Pinned {
            file: "f.ttf",
            sha256: Box::leak(sha(b"face").into_boxed_str()),
            origin: Origin::Member(archive, "pkg/f.ttf"),
            style: FritStyle::Regular,
            cover: &[],
            color: false,
        };
        fetch(&cache, &[font]).unwrap();
        assert_eq!(std::fs::read(cache.join("f.ttf")).unwrap(), b"face");
        assert!(!cache.join("a.tar.bz2").exists());
        // a member of other bytes than its pin is refused and removed
        std::fs::write(cache.join("a.tar.bz2"), &bytes).unwrap();
        std::fs::remove_file(cache.join("f.ttf")).unwrap();
        let wrong = Pinned {
            sha256: "0000",
            ..font
        };
        let e = fetch(&cache, &[wrong]).unwrap_err();
        assert!(e.contains("f.ttf has SHA-256"), "{e}");
        assert!(!cache.join("f.ttf").exists());
    }

    // a font in a zip archive, as Iosevka Term's release ships it, is
    // unpacked with unzip and checked against its own SHA-256, and the
    // archive is removed once the font is in place
    #[test]
    fn a_font_is_unpacked_from_its_checked_zip_archive() {
        let s = Scratch::new("unzip");
        let src = s.0.join("src");
        std::fs::create_dir_all(src.join("pkg")).unwrap();
        std::fs::write(src.join("pkg/f.ttf"), b"face").unwrap();
        let zip = s.0.join("a.zip");
        let ok = Command::new("zip")
            .arg("-q")
            .arg(&zip)
            .arg("pkg/f.ttf")
            .current_dir(&src)
            .status()
            .unwrap();
        assert!(ok.success());
        let bytes = std::fs::read(&zip).unwrap();
        let cache = s.0.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("a.zip"), &bytes).unwrap();
        let archive: &'static Archive = Box::leak(Box::new(Archive {
            file: "a.zip",
            url: "file:///nowhere",
            sha256: Box::leak(sha(&bytes).into_boxed_str()),
        }));
        let font = Pinned {
            file: "f.ttf",
            sha256: Box::leak(sha(b"face").into_boxed_str()),
            origin: Origin::Member(archive, "pkg/f.ttf"),
            style: FritStyle::Regular,
            cover: &[],
            color: false,
        };
        fetch(&cache, &[font]).unwrap();
        assert_eq!(std::fs::read(cache.join("f.ttf")).unwrap(), b"face");
        assert!(!cache.join("a.zip").exists());
    }

    // the scripts frit names Iosevka Term first, so its cell and size are
    // the frit's, then the three Noto fonts for the scripts it lacks, each
    // over the whole font but Noto Sans Arabic, which keeps its Arabic
    // blocks, and no style lines
    #[test]
    fn the_scripts_frit_names_iosevka_term_then_the_noto_fonts() {
        let text = scripts_frit_text(Path::new("/c"));
        let sources: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("source=outline:/c/"))
            .collect();
        assert_eq!(
            sources,
            [
                "IosevkaTerm-Regular.ttf",
                "NotoSansDevanagari-Regular.ttf",
                "NotoSansBengali-Regular.ttf",
                "NotoSansArabic-Regular.ttf",
            ]
        );
        assert!(!text.contains("style="));
        assert!(text.ends_with(
            "source=outline:/c/NotoSansArabic-Regular.ttf\ncover=U+0600-U+06FF\ncover=U+0750-U+077F\ncover=U+0870-U+089F\ncover=U+08A0-U+08FF\ncover=U+FB50-U+FDFF\ncover=U+FE70-U+FEFF\ncover=U+10EC0-U+10EFF\ncover=U+1EE00-U+1EEFF\n"
        ));
        assert_eq!(
            SCRIPT_FONTS[3].sha256, FONTS[5].sha256,
            "the same Noto Sans Arabic as the test frit"
        );
    }

    // each shipped font set names its fonts by family, and tier 2 pins
    // exactly the files of those families, so every `frit-` screen finds
    // each of its fonts in the cache and none is left out
    #[test]
    fn the_shipped_fonts_are_the_families_the_shipped_frits_name() {
        use std::collections::BTreeSet;
        use vtium_config::frit::{Frit, Origin};
        let mut named = BTreeSet::new();
        for frit in SHIPPED_FRITS {
            let parsed = Frit::parse_spliced(
                frit.spliced().as_bytes(),
                Path::new(frit.name),
                frit.name,
                Path::new("layers"),
            )
            .unwrap();
            assert_eq!(parsed.name, frit.name);
            for source in parsed.sources {
                let Origin::Font(family) = source.origin else {
                    panic!("{}: {} is no family", frit.name, source.spelling);
                };
                named.insert(format!("{}-regular", family.replace(' ', "")).to_lowercase());
            }
        }
        // a file is its family's when its name, less its extension and
        // with case ignored, is the family's without blanks and `-Regular`
        let pinned: BTreeSet<String> = SHIPPED_FONTS
            .iter()
            .map(|f| {
                let stem = f.file.rsplit_once('.').map_or(f.file, |(stem, _)| stem);
                stem.to_lowercase()
            })
            .collect();
        assert_eq!(pinned, named);
        assert_eq!(pinned.len(), SHIPPED_FONTS.len(), "each font pinned once");
    }

    /// The ink test font with an underline of its own, 40 units below the
    /// baseline and 150 thick, written into `dir`: its frit's text.
    fn underlined_ink(dir: &Path) -> String {
        let font = vtium_testfont::ttf::Font {
            underline: Some((-40, 150)),
            ..crate::inkfont::font()
        };
        let path = dir.join("underlined-ink.ttf");
        std::fs::write(&path, font.build()).unwrap();
        format!("source=outline:{}\n", path.display())
    }

    /// The lines of the frit `text` in `dir` at 16 pixels, as the console
    /// holds it once the helper has answered for `chars` in the regular
    /// style.
    fn lines(dir: &Path, text: &str, chars: &[char]) -> vtium_font::Decorations {
        let shown = vec![Shown {
            chars: chars.to_vec(),
            ..Shown::default()
        }];
        let chosen = chosen_frit(("underlined", text.to_owned()), dir, 16, |_| Ok(shown)).unwrap();
        face_at(&chosen, 16).unwrap().decorations()
    }

    // vtium/vtium#462: the console starts the helper's fill at every cell
    // it draws at, and the fill's first answer brings the font's lines
    // whatever the screen shows, so a screen of underlined spaces draws its
    // line in the font's row, as a screen of letters does
    #[test]
    fn a_screen_that_shows_no_character_takes_the_fonts_own_lines() {
        let scratch = Scratch::new("own-lines");
        let text = underlined_ink(&scratch.0);
        let letters = lines(&scratch.0, &text, &['A']);
        assert_ne!(
            letters,
            vtium_font::Decorations::for_height(16),
            "the test font's underline is its own"
        );
        assert_eq!(lines(&scratch.0, &text, &[]), letters);
    }

    #[test]
    fn runs_join_consecutive_codepoints() {
        assert_eq!(
            runs(&['a', 'b', 'c', 'x', 'z']),
            [(0x61, 3), (0x78, 1), (0x7a, 1)]
        );
        assert!(runs(&[]).is_empty());
    }
}
