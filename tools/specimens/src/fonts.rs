//! The outline fonts the desktop frit names, and its glyphs.
//!
//! vtium ships no third-party font bytes, so the outline profiles' fonts
//! are pinned here by URL and SHA-256 and fetched into a cache outside the
//! tree, as tier 3 pins its guest images: `$XDG_CACHE_HOME/vtium/specimens/fonts`.
//! The glyphs of `frits/desktop.vtfrit` are generated from them by
//! vtium-vitrify's engine, the one the helper runs, in this process, with
//! the cache as its only font directory, at each outline profile's cell. A
//! profile whose glyphs cannot be generated, for want of a font, is
//! skipped, and says why.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use vtium_font::set::Set;
use vtium_font::{Cell, Sha256, sha256_hex};

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
    for font in &FONTS {
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

/// The desktop frit's glyphs at each of `cells`, generated from the fonts
/// in `fonts`, or why they are not.
pub fn desktop(fonts: &Path, cells: &[Cell]) -> Result<Vec<(Cell, Arc<Set>)>, String> {
    if let Some(why) = FONTS.iter().find_map(|f| fault(fonts, f)) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    generate("desktop", FRIT, fonts, cells)
}

/// The glyphs of the frit `name`, `text`, at each of `cells`, as
/// vtium-vitrify's engine, the one the helper runs (ADR-1301 D2), draws
/// every codepoint the frit gives, `fonts` its only font directory. A cell
/// at which a source gives nothing, or fails as it draws, is refused: the
/// glyphs of only some of the frit's sources are not the profile's.
fn generate(
    name: &str,
    text: &str,
    fonts: &Path,
    cells: &[Cell],
) -> Result<Vec<(Cell, Arc<Set>)>, String> {
    use vtium_vitrify::serve::{Engine, FritText, Paths};
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
            let set = Set::from_bytes(bytes).map_err(|e| format!("the frit {name}: {e:?}"))?;
            Ok((cell, Arc::new(set)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // A cache holding a font of other bytes generates no glyphs, before the
    // engine reads it: the profiles drawn from it go undrawn.
    #[test]
    fn a_changed_font_in_the_cache_builds_no_set() {
        let dir =
            std::env::temp_dir().join(format!("vtium-specimens-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in &FONTS {
            std::fs::write(dir.join(f.file), b"not the font").unwrap();
        }
        let why = desktop(&dir, &[]).unwrap_err();
        assert!(why.contains("has SHA-256"), "{why}");
        std::fs::remove_dir_all(&dir).unwrap();
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
        )
        .unwrap();
        assert_eq!(sets.iter().map(|(c, _)| *c).collect::<Vec<_>>(), cells);
        for (cell, set) in sets {
            let face = vtium_font::set::SetFace::new(set, cell.height).unwrap();
            assert_eq!(face.cell(), cell);
            assert!(face.has_glyph('A'));
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
        let why = generate("t", frit, Path::new("/nonexistent"), &[cell]).unwrap_err();
        assert!(why.contains("source=builtin:unifont"), "{why}");
    }

    #[test]
    fn a_font_absent_or_changed_is_a_fault() {
        let dir =
            std::env::temp_dir().join(format!("vtium-specimens-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
