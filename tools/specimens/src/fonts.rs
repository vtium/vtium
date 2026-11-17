//! The fonts the catalog sets jetbrains-mono and tiny5 name, and their glyphs.
//!
//! vtium ships no third-party font bytes, so the profiles' fonts are
//! pinned here by URL and SHA-256 and fetched into a cache outside the
//! tree, as tier 3 pins its guest images: `$XDG_CACHE_HOME/vtium/specimens/fonts`.
//! The glyphs of the catalog set `jetbrains-mono` are generated from the outline
//! fonts by vtium-config's engine, the one the helper runs, in this
//! process, with the cache as its only font directory, at each outline
//! profile's cell. The glyphs of the shipped frit `tiny5` are generated the
//! same way from Tiny5 Mono's BDF file in the cache, its `source=` line
//! pointed there. Both sets add the catalog's layers, the fallback layer
//! `unifont` among them, whose BDF file the cache also holds, so the
//! specimens show each set as it looks once the fonts it names are
//! installed. A profile whose glyphs cannot be generated, for want of a
//! font, is skipped, and says why.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use vtium_font::set::Set;
use vtium_font::{Cell, Sha256, sha256_hex};

use crate::draw::{CellGlyphs, StyleClusters};

/// One pinned font: its file in the cache, where it comes from, and the
/// SHA-256 of its bytes. The set names the font's version, which the URL
/// holds. A font published gzipped has `gzipped`, the SHA-256 of the file
/// the URL serves, and the cache holds it decompressed, the form the set
/// names.
#[derive(Clone, Copy, Debug)]
pub struct Pinned {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub gzipped: Option<&'static str>,
}

/// The outline fonts of the set jetbrains-mono and its layers, in the
/// set's order.
pub const FONTS: [Pinned; 3] = [
    Pinned {
        file: "JetBrainsMono-Regular.ttf",
        url: "https://github.com/JetBrains/JetBrainsMono/raw/v2.304/fonts/ttf/JetBrainsMono-Regular.ttf",
        sha256: "a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f",
        gzipped: None,
    },
    Pinned {
        file: "SymbolsNerdFontMono-Regular.ttf",
        url: "https://github.com/ryanoasis/nerd-fonts/raw/v3.4.0/patched-fonts/NerdFontsSymbolsOnly/SymbolsNerdFontMono-Regular.ttf",
        sha256: "f0f624d9b474bea1662cf7e862d44aebe1ae1f6c7f9cb7a0ca5d0e5ac9561c60",
        gzipped: None,
    },
    Pinned {
        file: "NotoColorEmoji.ttf",
        url: "https://github.com/googlefonts/noto-emoji/raw/v2.051/fonts/NotoColorEmoji.ttf",
        sha256: "72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b",
        gzipped: None,
    },
];

/// The catalog set the outline profiles draw, as vtium-config ships it in
/// its category folder.
pub const FRIT: &str =
    include_str!("../../../crates/vtium-config/frits/code/jetbrains-mono.vtfrit");

/// GNU Unifont 17.0.03's BDF file, the file the catalog's fallback layer
/// `unifont` names. Unifoundry publishes it gzipped, at 1,259,456 bytes,
/// and the font helper reads a gzipped font of 1 MiB at most, so the layer
/// names the decompressed file, which the cache holds.
///
/// References:
/// - ADR-1204 D2: the Unifont version and where vtium installs the file.
pub const UNIFONT: Pinned = Pinned {
    file: "unifont-17.0.03.bdf",
    url: "https://unifoundry.com/pub/unifont/unifont-17.0.03/font-builds/unifont-17.0.03.bdf.gz",
    sha256: "3dbf3ed14e63f69dcc36663092269b312494429ce5a4ddc3adab042b5a0909c2",
    gzipped: Some("30d50302daca631e2cf454d9747117d98c9f35c0b18af4f67d2fa5e116992033"),
};

/// The path the layer `unifont` names its BDF file at.
const UNIFONT_SOURCE: &str = "source=bdf:/usr/share/vtium/fonts/unifont-17.0.03.bdf";

/// Unifont's character cell. A set whose cell is smaller gets no glyph
/// from the layer `unifont`, and the console draws those characters in its
/// built-in font instead.
const UNIFONT_CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// The text of the catalog's layer `name` that the specimens' sets add,
/// as vtium-config ships it.
///
/// References:
/// - ADR-1204 D11: the specimens draw the catalog sets from the catalog's
///   folders.
fn layer_text(name: &str) -> Option<&'static str> {
    Some(match name {
        "nerd-symbols" => {
            include_str!("../../../crates/vtium-config/frits/layers/nerd-symbols.vtlayer")
        }
        "noto-emoji" => {
            include_str!("../../../crates/vtium-config/frits/layers/noto-emoji.vtlayer")
        }
        "nerd-symbols-small" => {
            include_str!("../../../crates/vtium-config/frits/layers/nerd-symbols-small.vtlayer")
        }
        "noto-emoji-small" => {
            include_str!("../../../crates/vtium-config/frits/layers/noto-emoji-small.vtlayer")
        }
        "unifont" => include_str!("../../../crates/vtium-config/frits/layers/unifont.vtlayer"),
        _ => return None,
    })
}

/// The set `text` with its layers in place, as the console hands a set to
/// the helper.
pub fn spliced(text: &str) -> String {
    vtium_font::set::layer::expand(text, &mut |name| Ok(layer_text(name).map(str::to_owned))).text
}

/// The set `text` with every layer in place but `unifont`: the sources
/// the helper draws from at a cell smaller than Unifont's.
fn spliced_without_unifont(text: &str) -> String {
    vtium_font::set::layer::expand(text, &mut |name| {
        Ok((name != "unifont")
            .then(|| layer_text(name))
            .flatten()
            .map(str::to_owned))
    })
    .text
}

/// The set `text` with its layers in place and the layer `unifont`'s
/// `source=` line pointed at the cache `fonts`, or why not: the spliced
/// set must name Unifont's file exactly once, on the line the layer ships.
fn spliced_in(text: &str, fonts: &Path) -> Result<String, String> {
    let text = spliced(text);
    if text.lines().filter(|l| *l == UNIFONT_SOURCE).count() != 1 {
        return Err(format!("the set names no line {UNIFONT_SOURCE}"));
    }
    let here = format!("source=bdf:{}", fonts.join(UNIFONT.file).display());
    Ok(text.replace(UNIFONT_SOURCE, &here))
}

/// Tiny5 Mono's BDF file, at the commit and with the SHA-256 the shipped
/// frit `tiny5` names in its comment.
pub const TINY5: Pinned = Pinned {
    file: "Tiny5Mono.bdf",
    url: "https://github.com/Gissio/font_Tiny5Mono/raw/7d30f9e887fdb29e0b951e92493822afba5f70a7/fonts/bdf/Tiny5Mono.bdf",
    sha256: "33a935687277817534ddcc1decf586d977cee211550f611ba4bb4365e99755d3",
    gzipped: None,
};

/// The shipped frit `tiny5`, as vtium-config ships it.
pub const TINY5_FRIT: &str = include_str!("../../../crates/vtium-config/frits/small/tiny5.vtfrit");

/// The path the shipped frit `tiny5` names its BDF file at.
const TINY5_SOURCE: &str = "source=bdf:/usr/local/share/fonts/misc/Tiny5Mono.bdf";

/// Every font `fetch` keeps in the cache.
fn pinned() -> impl Iterator<Item = &'static Pinned> {
    FONTS.iter().chain([&TINY5, &UNIFONT])
}

/// The cache the pinned fonts are fetched into.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("vtium/specimens/fonts")
}

/// Why `font` in `dir` cannot be used: absent, unreadable, or not the
/// pinned bytes. `None` when it is the pinned font.
fn fault(dir: &Path, font: &Pinned) -> Option<String> {
    match std::fs::read(dir.join(font.file)) {
        Err(e) => Some(format!("{} is not in {}: {e}", font.file, dir.display())),
        Ok(bytes) => {
            let got = sha256_hex(&Sha256::digest(&bytes));
            (got != font.sha256)
                .then(|| format!("{} has SHA-256 {got}, not {}", font.file, font.sha256))
        }
    }
}

/// Fetches each pinned font `dir` lacks with curl, and checks every one
/// against its SHA-256; a fetched file that does not match is removed. A
/// gzipped font is checked against its gzipped SHA-256 first, then
/// decompressed with gzip into the file the cache holds.
pub fn fetch(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for font in pinned() {
        let path = dir.join(font.file);
        if !path.exists() {
            let part = dir.join(format!("{}.part", font.file));
            let status = Command::new("curl")
                .args(["-fL", "--retry", "3", "-o"])
                .arg(&part)
                .arg(font.url)
                .status()
                .map_err(|e| format!("curl: {e}"))?;
            if !status.success() {
                return Err(format!("curl {}: {status}", font.url));
            }
            if let Some(gzipped) = font.gzipped {
                let bytes = std::fs::read(&part).map_err(|e| format!("{}: {e}", part.display()))?;
                let got = sha256_hex(&Sha256::digest(&bytes));
                if got != gzipped {
                    let _ = std::fs::remove_file(&part);
                    return Err(format!("{} has SHA-256 {got}, not {gzipped}", font.url));
                }
                let out = Command::new("gzip")
                    .arg("-dc")
                    .arg(&part)
                    .output()
                    .map_err(|e| format!("gzip: {e}"))?;
                let _ = std::fs::remove_file(&part);
                if !out.status.success() {
                    return Err(format!("gzip -dc {}: {}", font.url, out.status));
                }
                std::fs::write(&part, &out.stdout)
                    .map_err(|e| format!("{}: {e}", part.display()))?;
            }
            std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(why) = fault(dir, font) {
            let _ = std::fs::remove_file(&path);
            return Err(why);
        }
    }
    Ok(())
}

/// The set jetbrains-mono's glyphs at each of `cells`, and `clusters` drawn
/// whole in each style ([`crate::draw::clusters_shown`]), generated from the fonts in `fonts`, or why they
/// are not.
pub fn desktop(
    fonts: &Path,
    cells: &[Cell],
    clusters: &[StyleClusters],
) -> Result<Vec<CellGlyphs>, String> {
    if let Some(why) = FONTS.iter().chain([&UNIFONT]).find_map(|f| fault(fonts, f)) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    generate(
        "jetbrains-mono",
        &spliced_in(FRIT, fonts)?,
        fonts,
        cells,
        clusters,
    )
}

/// The shipped frit `tiny5`'s glyphs at each of `cells`, and `clusters`
/// drawn whole in each style, generated from Tiny5 Mono's BDF file in
/// `fonts`, or why they are not: the frit's one `source=` line and the
/// layer `unifont`'s are pointed at the cache's copies, and nothing else in
/// the frit changes.
///
/// Unifont's 8×16 glyphs do not fit a cell smaller than that, such as
/// Tiny5 Mono's own 6×9, so at such a cell the helper gets no glyph from
/// the layer `unifont`, and the console draws those characters in its
/// built-in font. These glyphs then come from the set's other sources
/// alone, which is what the helper hands the console at that cell.
pub fn tiny5(
    fonts: &Path,
    cells: &[Cell],
    clusters: &[StyleClusters],
) -> Result<Vec<CellGlyphs>, String> {
    if let Some(why) = [&TINY5, &UNIFONT].into_iter().find_map(|f| fault(fonts, f)) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    if TINY5_FRIT.lines().filter(|l| *l == TINY5_SOURCE).count() != 1 {
        return Err(format!("the frit tiny5 has no line {TINY5_SOURCE}"));
    }
    let here = format!("source=bdf:{}", fonts.join(TINY5.file).display());
    let whole = spliced_in(TINY5_FRIT, fonts)?.replace(TINY5_SOURCE, &here);
    let small = spliced_without_unifont(TINY5_FRIT).replace(TINY5_SOURCE, &here);
    let mut out = Vec::new();
    for &cell in cells {
        let fits = cell.width >= UNIFONT_CELL.width && cell.height >= UNIFONT_CELL.height;
        let text = if fits { &whole } else { &small };
        out.extend(generate("tiny5", text, fonts, &[cell], clusters)?);
    }
    Ok(out)
}

/// The glyphs of the frit `name`, `text`, at each of `cells`, as
/// vtium-config's engine, the one the helper runs (ADR-1301 D2), draws
/// every codepoint the frit gives, as its fill does, and each of
/// `clusters` whole in its style, as it answers the console's asks for
/// them (ADR-1200 D21), `fonts` its only font directory. A cell at which a
/// source gives nothing, or fails as it draws, is refused: the glyphs of
/// only some of the frit's sources are not the profile's. A style none of
/// whose clusters the frit draws gives no set.
pub(crate) fn generate(
    name: &str,
    text: &str,
    fonts: &Path,
    cells: &[Cell],
    clusters: &[StyleClusters],
) -> Result<Vec<CellGlyphs>, String> {
    use vtium_config::serve::{Engine, FritText, Paths};
    let frit = FritText {
        name: name.to_owned(),
        text: text.to_owned(),
    };
    let paths = Paths {
        font_dirs: vec![fonts.to_path_buf()],
        ..Paths::default()
    };
    let engine = Engine::load(&frit, &paths, None)?;
    cells
        .iter()
        .map(|&cell| {
            let bytes = engine.draw(cell, engine.coverage())?;
            // none_at names each source that gave nothing at the cell
            let why = engine.none_at(cell);
            let bytes = match bytes {
                Some(bytes) if !why.contains("source=") => bytes,
                _ => return Err(format!("the frit {name}: {why}")),
            };
            let read = |bytes| {
                Set::from_bytes(bytes)
                    .map(Arc::new)
                    .map_err(|e| format!("the frit {name}: {e:?}"))
            };
            let set = read(bytes)?;
            let mut whole = Vec::new();
            for (style, keys) in clusters {
                if let Some(bytes) = engine.draw_style(cell, *style, &[], (keys, true))? {
                    whole.push((*style, read(bytes)?));
                }
            }
            Ok(CellGlyphs {
                cell,
                set,
                clusters: whole,
                frit: (name.to_owned(), text.to_owned()),
                codepoints: engine.coverage().to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of a test's own, `vtium-specimens-<name>-<pid>` in the
    /// temporary directory, removed when the test ends, whether it passes
    /// or fails.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir =
                std::env::temp_dir().join(format!("vtium-specimens-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl std::ops::Deref for Scratch {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    use vtium_font::Face;

    // The set and its layers name the version of each font this table
    // fetches, in the same order, the version the table's URL holds.
    #[test]
    fn the_frit_names_the_fonts_this_table_fetches() {
        let versions: Vec<String> = spliced(FRIT)
            .lines()
            .filter_map(|l| l.strip_prefix("version="))
            .map(str::to_owned)
            .collect();
        let fetched: Vec<&Pinned> = FONTS.iter().chain([&UNIFONT]).collect();
        assert_eq!(versions.len(), fetched.len(), "{versions:?}");
        for (font, version) in fetched.iter().zip(&versions) {
            assert!(
                font.url.contains(&format!("/v{version}/"))
                    || font.url.contains(&format!("/unifont-{version}/")),
                "{} {version}",
                font.url
            );
        }
    }

    // The layer unifont names, on its one source= line, the file this table
    // fetches, with the SHA-256 the cache's copy has.
    #[test]
    fn the_unifont_layer_names_the_file_this_table_fetches() {
        let layer = layer_text("unifont").unwrap();
        let sources: Vec<&str> = layer.lines().filter(|l| l.starts_with("source=")).collect();
        assert_eq!(sources, [UNIFONT_SOURCE]);
        assert!(UNIFONT_SOURCE.ends_with(&format!("/{}", UNIFONT.file)));
        assert!(layer.contains(&format!("sha256={}", UNIFONT.sha256)));
        // both sets the specimens draw end with the layer
        for set in [FRIT, TINY5_FRIT] {
            assert!(set.lines().any(|l| l == "layer=unifont"));
        }
    }

    // A cell Unifont's 8×16 glyphs fit draws from every layer; a smaller
    // one draws from every layer but unifont, as the helper does.
    #[test]
    fn a_cell_smaller_than_unifonts_draws_without_its_layer() {
        let dir = Scratch::new("small");
        let whole = spliced_in(TINY5_FRIT, &dir).unwrap();
        let unifont = format!("source=bdf:{}", dir.join(UNIFONT.file).display());
        assert!(whole.lines().any(|l| l == unifont), "{whole}");
        let small = spliced_without_unifont(TINY5_FRIT);
        assert!(!small.contains("unifont-17.0.03"), "{small}");
        assert!(small.contains("source=font:Noto Color Emoji"), "{small}");
    }

    // The shipped tiny5 frit names, in its comment, the commit and the
    // SHA-256 of the BDF file this table fetches, and its one source is the
    // line `tiny5` points at the cache.
    #[test]
    fn the_tiny5_frit_names_the_file_this_table_fetches() {
        assert!(TINY5_FRIT.contains(&format!("# {}.", TINY5.sha256)));
        let commit = TINY5.url.split('/').nth(6).unwrap();
        assert_eq!(commit.len(), 40, "{commit}");
        assert!(TINY5_FRIT.contains(commit));
        let sources: Vec<&str> = TINY5_FRIT
            .lines()
            .filter(|l| l.starts_with("source="))
            .collect();
        assert_eq!(sources, [TINY5_SOURCE]);
    }

    // A cache holding a font of other bytes generates no glyphs, before the
    // engine reads it: the profiles drawn from it go undrawn.
    #[test]
    fn a_changed_font_in_the_cache_builds_no_set() {
        let dir = Scratch::new("cache");
        for f in pinned() {
            std::fs::write(dir.join(f.file), b"not the font").unwrap();
        }
        let why = desktop(&dir, &[], &[]).unwrap_err();
        assert!(why.contains("has SHA-256"), "{why}");
        let why = tiny5(&dir, &[], &[]).unwrap_err();
        assert!(why.contains("Tiny5Mono.bdf has SHA-256"), "{why}");
    }

    // ADR-1301 D2: the frit's glyphs come from the engine the helper runs,
    // one set at each cell asked, drawn in that cell.
    #[test]
    fn a_frits_glyphs_are_generated_at_each_cell() {
        let cells = [
            Cell {
                width: 8,
                height: 8,
            },
            Cell {
                width: 12,
                height: 24,
            },
        ];
        let sets = generate(
            "t",
            "source=builtin:unscii-8\n",
            Path::new("/nonexistent"),
            &cells,
            &[],
        )
        .unwrap();
        assert_eq!(sets.iter().map(|g| g.cell).collect::<Vec<_>>(), cells);
        for g in sets {
            let face = vtium_font::set::SetFace::new(g.set, g.cell.height).unwrap();
            assert_eq!(face.cell(), g.cell);
            assert!(face.has_glyph('A'));
            assert!(g.clusters.is_empty());
        }
    }

    // A source that gives nothing at a cell, as unifont's 8x16 in an 8x8
    // cell, leaves the frit's glyphs there short of the frit: refused.
    #[test]
    fn a_source_left_out_at_a_cell_is_refused() {
        let cell = Cell {
            width: 8,
            height: 8,
        };
        let frit = "source=builtin:unscii-8\nsource=builtin:unifont\n";
        let why = generate("t", frit, Path::new("/nonexistent"), &[cell], &[]).unwrap_err();
        assert!(why.contains("source=builtin:unifont"), "{why}");
    }

    #[test]
    fn a_font_absent_or_changed_is_a_fault() {
        let dir = Scratch::new("fonts");
        let font = Pinned {
            file: "x.ttf",
            url: "",
            // the SHA-256 of "x"
            sha256: "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881",
            gzipped: None,
        };
        assert!(fault(&dir, &font).unwrap().contains("is not in"));
        std::fs::write(dir.join("x.ttf"), b"y").unwrap();
        assert!(fault(&dir, &font).unwrap().contains("has SHA-256"));
        std::fs::write(dir.join("x.ttf"), b"x").unwrap();
        assert_eq!(fault(&dir, &font), None);
    }
}
