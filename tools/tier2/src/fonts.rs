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
    /// A member of a `.tar.bz2` archive, at this path in it.
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
    Pinned {
        file: "NotoSansHebrew-Regular.ttf",
        sha256: "cdefaf8efd47045f6820928eba84db5bed7557539328952b5f828315485e02ee",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansHebrew/hinted/ttf/NotoSansHebrew-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &HEBREW,
        color: false,
    },
    Pinned {
        file: "NotoSansArabic-Regular.ttf",
        sha256: "c270782ee478c685f95509e724125729e1284c82ebfbe414cc67926f29449e1d",
        origin: Origin::Url(
            "https://github.com/notofonts/notofonts.github.io/raw/noto-monthly-release-24.9.1/fonts/NotoSansArabic/hinted/ttf/NotoSansArabic-Regular.ttf",
        ),
        style: FritStyle::Regular,
        cover: &ARABIC,
        color: false,
    },
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
fn fault(dir: &Path, file: &str, sha256: &str) -> Option<String> {
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

/// Downloads `url` into `dir` as `file` with curl, through a `.part` file,
/// so a download that stops leaves no file of that name.
fn download(dir: &Path, file: &str, url: &str) -> Result<(), String> {
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

/// Writes `member` of the `.tar.bz2` archive `archive` to `dir` as `file`,
/// with `tar`, through a `.part` file.
fn unpack(archive: &Path, member: &str, dir: &Path, file: &str) -> Result<(), String> {
    let part = dir.join(format!("{file}.part"));
    let out = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let status = Command::new("tar")
        .arg("-xjOf")
        .arg(archive)
        .arg(member)
        .stdout(Stdio::from(out))
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("tar {} {member}: {status}", archive.display()));
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
/// screen shows in each style, as [`chosen`] gives the test frit.
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
    let cell = match engine.own_cell(height)? {
        EngineCell::Outline(cell) => cell,
        EngineCell::Bitmap(cell) => {
            return Err(format!(
                "the frit {name}'s first source is a bitmap font of cell {}x{}",
                cell.width, cell.height
            ));
        }
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
    chosen.set_own_cell(OwnCell::Outline(cell), false);
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
    for one in &shown(at.cell)? {
        let want: Vec<u32> = one.chars.iter().map(|&c| u32::from(c)).collect();
        let clusters: Vec<Vec<u32>> = one
            .clusters
            .iter()
            .map(|t| t.iter().map(|&c| u32::from(c)).collect())
            .collect();
        let Some(bytes) = engine.draw_style(at, one.style, &want, &clusters)? else {
            continue;
        };
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let set = Arc::new(Set::from_bytes(bytes).map_err(|e| format!("the frit {name}: {e:?}"))?);
        let face = SetFace::new(Arc::clone(&set), at.cell.height)
            .ok_or_else(|| format!("the frit {name}: no strike at {}", at.cell.height))?;
        // the codepoints the set maps in the style, as the helper's frame
        // names them
        let held: Vec<char> = one
            .chars
            .iter()
            .copied()
            .filter(|&c| face.draw(&[c], one.style).is_some())
            .collect();
        let mut store = lock(generated);
        let fresh = store
            .fresh((at, one.style), set, &runs(&held), len)
            .ok_or_else(|| format!("the frit {name}: the store refused its glyphs"))?;
        store.add(fresh);
    }
    Ok(chosen)
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

    #[test]
    fn runs_join_consecutive_codepoints() {
        assert_eq!(
            runs(&['a', 'b', 'c', 'x', 'z']),
            [(0x61, 3), (0x78, 1), (0x7a, 1)]
        );
        assert!(runs(&[]).is_empty());
    }
}
