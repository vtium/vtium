//! The merged symbol test font: a short outline font of Powerline
//! separators and icons in the Private Use Area, built in memory, which
//! the `glyph-fit` screens merge behind DejaVu Sans Mono with `cover=`.
//!
//! A symbol font made for merging lays its symbols out between its own
//! ascent and descent, and the font helper draws a merged symbol at the
//! size at which that span fills the main font's cell. This font is
//! short: its ascent and descent span 0.8 em, where DejaVu Sans Mono's
//! span about 1.16 em. At DejaVu Sans Mono's size its separators would
//! cover only about 70 per cent of the cell's height, so the screens show
//! at a glance whether the helper fits the symbols to the cell: each
//! separator runs from the cell's top row to its bottom row.
//!
//! The font has 1000 units to the em, an ascent of 700 and a descent of
//! 100. Every glyph advances 500 units, which is narrower than DejaVu
//! Sans Mono's cell at the fitted size, so no symbol is drawn smaller to
//! fit its cell's width. The glyphs:
//!
//! - U+E0B0 and U+E0B2, the right-pointing and left-pointing solid
//!   triangles that Powerline puts between segments, from the descent to
//!   the ascent;
//! - U+E0B1, a thin bar from the descent to the ascent, as a Powerline
//!   thin separator runs;
//! - U+F015, a house, and U+F07B, a folder, the icons a prompt shows,
//!   in the middle of the span.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use vtium_testfont::ttf::{Font, Glyph};

/// A glyph advancing 500 units with `contours`, each clockwise.
fn glyph(contours: Vec<Vec<(i16, i16)>>) -> Glyph {
    Glyph {
        advance: 500,
        contours,
    }
}

/// The font's bytes.
pub fn bytes() -> Vec<u8> {
    let all = [
        (0xe0b0, glyph(vec![vec![(0, -100), (0, 700), (500, 300)]])),
        (0xe0b2, glyph(vec![vec![(500, -100), (0, 300), (500, 700)]])),
        (
            0xe0b1,
            glyph(vec![vec![(200, -100), (200, 700), (300, 700), (300, -100)]]),
        ),
        (
            0xf015,
            glyph(vec![
                vec![(250, 550), (450, 350), (50, 350)],
                vec![(100, 50), (100, 350), (400, 350), (400, 50)],
            ]),
        ),
        (
            0xf07b,
            glyph(vec![
                vec![(50, 50), (50, 450), (450, 450), (450, 50)],
                vec![(50, 450), (50, 520), (220, 520), (250, 450)],
            ]),
        ),
    ];
    let mut glyphs = vec![Glyph::empty(500)];
    let mut cmap = Vec::new();
    for (c, g) in all {
        let id = u16::try_from(glyphs.len()).expect("fewer than 65536 glyphs");
        glyphs.push(g);
        cmap.push((c, id));
    }
    cmap.sort_unstable();
    Font {
        family: "vtium tier2 symbols".to_owned(),
        upem: 1000,
        ascent: 700,
        descent: -100,
        glyphs,
        cmap,
        colr: Vec::new(),
        palette: Vec::new(),
        cbdt: None,
        gsub: Vec::new(),
        gpos: Vec::new(),
        underline: None,
        os2: None,
        empty_gpos: false,
    }
    .build()
}

/// The directory the font is kept in: `symbols` beside the outline
/// fonts' cache ([`crate::fonts::cache_dir`]), where each run finds the
/// font's file or writes it once. The file's name holds the start of its
/// SHA-256, so a run never reads a font built from other code, and the
/// file is written under a name of the process's own and renamed into
/// place, so two runs at once never read a half-written font.
pub fn dir() -> Result<&'static Path, String> {
    static DIR: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = crate::fonts::cache_dir().with_file_name("symbols");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let font = bytes();
        let path = dir.join(file_name(&font));
        if std::fs::read(&path).ok().as_deref() != Some(font.as_slice()) {
            let part = dir.join(format!("{}.{}", file_name(&font), std::process::id()));
            std::fs::write(&part, &font).map_err(|e| format!("{}: {e}", part.display()))?;
            std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(dir)
    })
    .as_ref()
    .map(PathBuf::as_path)
    .map_err(Clone::clone)
}

/// The name of the file that holds `font`: the start of its SHA-256 in
/// it.
fn file_name(font: &[u8]) -> String {
    let sha = vtium_font::sha256_hex(&vtium_font::Sha256::digest(font));
    format!("vtium-tier2-symbols-{}.ttf", sha.get(..16).unwrap_or(&sha))
}

/// The merged frit: DejaVu Sans Mono, the first source of the test frit
/// of outline fonts in `fetched`, with no `cover=` line, then the symbol
/// font in `symbols`, limited by `cover=` to the Private Use Area, then
/// `extra`, lines of the screen's own such as `fit=` lines, and
/// `line-spacing=` when `spacing` is not 0. DejaVu Sans Mono states no
/// `cover=` line, so a `cover=` line in `extra` names the set that a
/// character DejaVu Sans Mono lacks belongs to.
pub fn frit_text(fetched: &Path, symbols: &Path, spacing: u32, extra: &str) -> String {
    let all = crate::fonts::frit_text(fetched, false, 0);
    let mut text = all.lines().next().unwrap_or_default().to_owned();
    text.push('\n');
    text.push_str(&format!(
        "source=outline:{}\ncover=U+E000-U+F8FF\n",
        symbols.join(file_name(&bytes())).display()
    ));
    text.push_str(extra);
    if spacing > 0 {
        text.push_str(&format!("line-spacing={spacing}\n"));
    }
    text
}
