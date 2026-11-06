//! The fonts the desktop frit and the tiny5 frit name, and their glyphs.
//!
//! vtium ships no third-party font bytes, so the profiles' fonts are
//! pinned here by URL and SHA-256 and fetched into a cache outside the
//! tree, as tier 3 pins its guest images: `$XDG_CACHE_HOME/vtium/specimens/fonts`.
//! The glyphs of `frits/desktop.vtfrit` are generated from the outline
//! fonts by vtium-config's engine, the one the helper runs, in this
//! process, with the cache as its only font directory, at each outline
//! profile's cell. The glyphs of the shipped frit `tiny5` are generated the
//! same way from Tiny5 Mono's BDF file in the cache, its `source=` line
//! pointed there. A profile whose glyphs cannot be generated, for want of
//! a font, is skipped, and says why.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use vtium_font::set::Set;
use vtium_font::{Cell, Sha256, sha256_hex};

use crate::draw::{CellGlyphs, StyleClusters};

/// One pinned font: its file in the cache, where it comes from, and the
/// SHA-256 of its bytes, which the frit's comment above its source repeats.
#[derive(Clone, Copy, Debug)]
pub struct Pinned {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

/// The desktop set's outline fonts, in the frit's order.
pub const FONTS: [Pinned; 3] = [
    Pinned {
        file: "JetBrainsMono-Regular.ttf",
        url: "https://github.com/JetBrains/JetBrainsMono/raw/v2.304/fonts/ttf/JetBrainsMono-Regular.ttf",
        sha256: "a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f",
    },
    Pinned {
        file: "SymbolsNerdFontMono-Regular.ttf",
        url: "https://github.com/ryanoasis/nerd-fonts/raw/v3.4.0/patched-fonts/NerdFontsSymbolsOnly/SymbolsNerdFontMono-Regular.ttf",
        sha256: "f0f624d9b474bea1662cf7e862d44aebe1ae1f6c7f9cb7a0ca5d0e5ac9561c60",
    },
    Pinned {
        file: "NotoColorEmoji.ttf",
        url: "https://github.com/googlefonts/noto-emoji/raw/v2.051/fonts/NotoColorEmoji.ttf",
        sha256: "72a635cb3d2f3524c51620cdde406b217204e8a6a06c6a096ff8ed4b5fd6e27b",
    },
];

/// The desktop set's frit.
pub const FRIT: &str = include_str!("../frits/desktop.vtfrit");

/// Tiny5 Mono's BDF file, at the commit and with the SHA-256 the shipped
/// frit `tiny5` names in its comment.
pub const TINY5: Pinned = Pinned {
    file: "Tiny5Mono.bdf",
    url: "https://github.com/Gissio/font_Tiny5Mono/raw/7d30f9e887fdb29e0b951e92493822afba5f70a7/fonts/bdf/Tiny5Mono.bdf",
    sha256: "33a935687277817534ddcc1decf586d977cee211550f611ba4bb4365e99755d3",
};

/// The shipped frit `tiny5`, as vtium-config ships it.
pub const TINY5_FRIT: &str = include_str!("../../../crates/vtium-config/frits/tiny5.vtfrit");

/// The path the shipped frit `tiny5` names its BDF file at.
const TINY5_SOURCE: &str = "source=bdf:/usr/local/share/fonts/misc/Tiny5Mono.bdf";

/// Every font `fetch` keeps in the cache.
fn pinned() -> impl Iterator<Item = &'static Pinned> {
    FONTS.iter().chain(std::iter::once(&TINY5))
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
/// against its SHA-256; a fetched file that does not match is removed.
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
            std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        if let Some(why) = fault(dir, font) {
            let _ = std::fs::remove_file(&path);
            return Err(why);
        }
    }
    Ok(())
}

/// The desktop frit's glyphs at each of `cells`, and `clusters` drawn
/// whole in each style ([`crate::draw::clusters_shown`]), generated from the fonts in `fonts`, or why they
/// are not.
pub fn desktop(
    fonts: &Path,
    cells: &[Cell],
    clusters: &[StyleClusters],
) -> Result<Vec<CellGlyphs>, String> {
    if let Some(why) = FONTS.iter().find_map(|f| fault(fonts, f)) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    generate("desktop", FRIT, fonts, cells, clusters)
}

/// The shipped frit `tiny5`'s glyphs at each of `cells`, and `clusters`
/// drawn whole in each style, generated from Tiny5 Mono's BDF file in
/// `fonts`, or why they are not: the frit's one `source=` line is pointed
/// at the cache's copy, and nothing else in the frit changes.
pub fn tiny5(
    fonts: &Path,
    cells: &[Cell],
    clusters: &[StyleClusters],
) -> Result<Vec<CellGlyphs>, String> {
    if let Some(why) = fault(fonts, &TINY5) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    if TINY5_FRIT.lines().filter(|l| *l == TINY5_SOURCE).count() != 1 {
        return Err(format!("the frit tiny5 has no line {TINY5_SOURCE}"));
    }
    let here = format!("source=bdf:{}", fonts.join(TINY5.file).display());
    generate(
        "tiny5",
        &TINY5_FRIT.replace(TINY5_SOURCE, &here),
        fonts,
        cells,
        clusters,
    )
}

/// The glyphs of the frit `name`, `text`, at each of `cells`, as
/// vtium-config's engine, the one the helper runs (ADR-1301 D2), draws
/// every codepoint the frit gives, as its fill does, and each of
/// `clusters` whole in its style, as it answers the console's asks for
/// them (ADR-1200 D21), `fonts` its only font directory. A cell at which a
/// source gives nothing, or fails as it draws, is refused: the glyphs of
/// only some of the frit's sources are not the profile's. A style none of
/// whose clusters the frit draws gives no set.
fn generate(
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
                if let Some(bytes) = engine.draw_style(cell, *style, &[], keys)? {
                    whole.push((*style, read(bytes)?));
                }
            }
            Ok(CellGlyphs {
                cell,
                set,
                clusters: whole,
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

    // The frit names each source's SHA-256, the bytes the cache is checked
    // against, in the same order.
    #[test]
    fn the_frit_names_the_fonts_this_table_fetches() {
        let pins: Vec<&str> = FRIT
            .lines()
            .filter_map(|l| l.strip_prefix("# "))
            .filter(|l| l.len() == 65 && l.ends_with('.'))
            .map(|l| &l[..64])
            .collect();
        let table: Vec<&str> = FONTS.iter().map(|f| f.sha256).collect();
        assert_eq!(pins, table);
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
        };
        assert!(fault(&dir, &font).unwrap().contains("is not in"));
        std::fs::write(dir.join("x.ttf"), b"y").unwrap();
        assert!(fault(&dir, &font).unwrap().contains("has SHA-256"));
        std::fs::write(dir.join("x.ttf"), b"x").unwrap();
        assert_eq!(fault(&dir, &font), None);
    }
}
