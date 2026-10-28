//! The outline fonts the desktop set is built from, and the set itself.
//!
//! vtium ships no third-party font bytes, so the outline profiles' fonts
//! are pinned here by URL and SHA-256 and fetched into a cache outside the
//! tree, as tier 3 pins its guest images: `$XDG_CACHE_HOME/vtium/specimens/fonts`.
//! The desktop set is built from them by vtium-vitrify itself, `create
//! --out` on `frits/desktop.vtfrit` with the cache as its only font
//! directory, into a directory the run creates beside the cache, and read back with
//! vtium-font's set reader. A profile whose set cannot be built, for want
//! of the tool or of a font, is skipped, and says why.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use vtium_font::set::Set;
use vtium_font::{Sha256, sha256_hex};

/// One pinned font: its file in the cache, where it comes from, and the
/// SHA-256 of its bytes, which the frit's `sha256=` line repeats.
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

/// The desktop set, built by `vitrify` from the fonts in `fonts`, or why
/// it is not.
pub fn desktop(vitrify: Option<&Path>, fonts: &Path) -> Result<Arc<Set>, String> {
    let vitrify = vitrify.ok_or("no vtium-vitrify given, as `make specimens` gives it")?;
    if let Some(why) = FONTS.iter().find_map(|f| fault(fonts, f)) {
        return Err(format!(
            "{why}; `make specimens-fonts` fetches the pinned fonts"
        ));
    }
    // beside the font cache, in a directory this run creates itself: an
    // existing one, whoever made it, is refused rather than written into
    let parent = fonts.parent().unwrap_or(fonts);
    let dir = parent.join(format!("build-{}", std::process::id()));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let built = build(vitrify, fonts, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    built
}

fn build(vitrify: &Path, fonts: &Path, dir: &Path) -> Result<Arc<Set>, String> {
    let frit = dir.join("desktop.vtfrit");
    let out = dir.join("desktop.vtglass");
    std::fs::write(&frit, FRIT).map_err(|e| format!("{}: {e}", frit.display()))?;
    let run = Command::new(vitrify)
        .args(["create", "--no-banner", "--font-dir"])
        .arg(fonts)
        .arg("--out")
        .arg(&out)
        .arg(&frit)
        .output()
        .map_err(|e| format!("{}: {e}", vitrify.display()))?;
    let stdout = String::from_utf8_lossy(&run.stdout);
    // a set built from only some of its sources is not the profile's set
    if !run.status.success() || stdout.trim_end() != "built\tdesktop" {
        return Err(format!(
            "vtium-vitrify did not build the whole set: {}{}",
            stdout.trim_end(),
            String::from_utf8_lossy(&run.stderr).trim_end()
        ));
    }
    let bytes = std::fs::read(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    Set::from_bytes(bytes)
        .map(Arc::new)
        .map_err(|e| format!("the desktop set: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The frit pins each source to the bytes the cache is checked against,
    // in the same order.
    #[test]
    fn the_frit_pins_the_fonts_this_table_fetches() {
        let pins: Vec<&str> = FRIT
            .lines()
            .filter_map(|l| l.strip_prefix("sha256="))
            .collect();
        let table: Vec<&str> = FONTS.iter().map(|f| f.sha256).collect();
        assert_eq!(pins, table);
    }

    // A cache holding a font of other bytes builds no set, before
    // vtium-vitrify is run: the profiles drawn from it go undrawn.
    #[test]
    fn a_changed_font_in_the_cache_builds_no_set() {
        let dir =
            std::env::temp_dir().join(format!("vtium-specimens-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in &FONTS {
            std::fs::write(dir.join(f.file), b"not the font").unwrap();
        }
        let why = desktop(Some(Path::new("/nonexistent/vtium-vitrify")), &dir).unwrap_err();
        assert!(why.contains("has SHA-256"), "{why}");
        std::fs::remove_dir_all(&dir).unwrap();
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
