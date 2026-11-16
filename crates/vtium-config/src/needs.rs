//! The glyph helper's match of each font it finds against what the frit
//! says the source needs: `check=`, `version=` and `sha256=`.
//!
//! A font that lacks a character its `check=` names is a different font of
//! a similar name, such as the plain font in place of its Nerd Fonts
//! build, so the helper leaves it out as it leaves out a font this machine
//! does not have, and the next source gives its characters. A font of a
//! lower version than `version=`, or a file whose SHA-256 is not the one
//! `sha256=` gives, still draws: the helper reports it in its answer to a
//! sources query, and vtium-config names it to the person.
//!
//! The console logs one line for each font that is missing, once a boot.
//! The helper sends that line as one of the frit's warnings, which the
//! console already logs once a boot each.
//!
//! References:
//! - ADR-1204 D7: what vtium does with each of the four keys, and the
//!   console's line for a missing font.

use std::path::Path;

use vtium_font::{PSF_BYTES_MAX, Sha256, sha256_hex};

use crate::frit::{Cover, Origin, Source, Version};
use crate::outline::OUTLINE_BYTES_MAX;
use crate::sources::Input;

/// How a font the helper found differs from what its source needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Differ {
    /// The font lacks this character, which `check=` names, so the helper
    /// left it out as missing.
    Lacks(u32),
    /// The font states this version, lower than the one `version=` asks
    /// for. It still draws.
    Older(String),
    /// The file's SHA-256 is this one, not the one `sha256=` gives. It
    /// still draws.
    OtherHash(String),
}

/// A font that differs from what its source needs, as a sources query
/// answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Differs {
    /// The source's line in the frit's own text: for a layer's source, the
    /// frit's `layer=` line that adds it, as [`crate::serve::LeftOut`]
    /// names a line.
    pub line: usize,
    /// The source's value as the frit spells it, which tells two sources
    /// of one layer apart.
    pub spelling: String,
    pub what: Differ,
}

impl Differs {
    /// The line a sources query answers for it: `lacks`, `older` or
    /// `sha256`, a tab, the source's line, a tab, its spelling, a tab, and
    /// the character as `U+XXXX`, the font's version or the file's
    /// SHA-256. A reader of the answer that knows only the `missing` and
    /// `refused` lines skips these.
    pub fn line_text(&self) -> String {
        let (kind, value) = match &self.what {
            Differ::Lacks(c) => ("lacks", format!("U+{c:04X}")),
            Differ::Older(found) => ("older", found.clone()),
            Differ::OtherHash(found) => ("sha256", found.clone()),
        };
        let clean = |t: &str| t.replace(['\t', '\n'], " ");
        format!(
            "{kind}\t{}\t{}\t{}",
            self.line,
            clean(&self.spelling),
            clean(&value)
        )
    }

    /// The font a sources query's line `text` names, `None` for a line
    /// that is not one.
    pub fn parse(text: &str) -> Option<Differs> {
        let mut f = text.splitn(4, '\t');
        let kind = f.next()?;
        let line = f.next()?.parse().ok()?;
        let spelling = f.next()?.to_owned();
        let value = f.next()?;
        let what = match kind {
            "lacks" => Differ::Lacks(
                value
                    .strip_prefix("U+")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())?,
            ),
            "older" => Differ::Older(value.to_owned()),
            "sha256" => Differ::OtherHash(value.to_owned()),
            _ => return None,
        };
        Some(Differs {
            line,
            spelling,
            what,
        })
    }
}

/// The first character of `check` that a font does not hold, where
/// `count(a, b)` counts the characters from `a` to `b` that the font
/// holds; `None` when it holds them all.
pub fn lacking(check: &[Cover], count: impl Fn(u32, u32) -> usize) -> Option<u32> {
    check.iter().find_map(|&(a, b)| {
        let want = usize::try_from(b.saturating_sub(a)).map_or(usize::MAX, |n| n.saturating_add(1));
        if count(a, b) >= want {
            return None;
        }
        (a..=b).find(|&c| count(c, c) == 0)
    })
}

/// The version string a font read from `input` states: an outline font's
/// version from its name table, in the face of the family the source
/// names, and a BDF font's `FONT_VERSION` property. `None` for a font that
/// states none, such as a PSF font, which has no place for one.
pub fn version_of(input: &Input) -> Option<String> {
    match input {
        Input::Outline(req) => crate::serve::outline_version(&req.font, &req.family),
        Input::Bdf(bytes) => crate::bdf::version(bytes),
        Input::Font(_) | Input::Bitmap(_) => None,
    }
}

/// The SHA-256 of the file a path source names, as lowercase hexadecimal
/// digits, read again with the bound the helper reads it with; `None` for
/// a source that names no file, or a file that no longer reads.
pub fn sha256_of(origin: &Origin) -> Option<String> {
    let (path, limit): (&Path, u64) = match origin {
        Origin::Outline(path) => (path, OUTLINE_BYTES_MAX),
        Origin::File(path) | Origin::Bdf(path) => (path, u64::from(PSF_BYTES_MAX)),
        _ => return None,
    };
    let bytes = crate::sources::read_regular(path, limit).ok()?;
    Some(sha256_hex(&Sha256::digest(&bytes)))
}

/// How the font that `source` names differs from what the source needs,
/// once the helper has read it: its version, `found` as the font states
/// it, against `version=`, and its file's SHA-256 against `sha256=`. A
/// font that states no version is not compared, because nothing says it
/// is older. `line` is the source's line in the frit's own text.
pub fn differences(source: &Source, found: Option<&str>, line: usize) -> Vec<Differs> {
    let mut out = Vec::new();
    let differs = |what| Differs {
        line,
        spelling: source.spelling.clone(),
        what,
    };
    if let Some(wanted) = &source.needs.version
        && let Some(found) = found.and_then(Version::in_string)
        && &found < wanted
    {
        out.push(differs(Differ::Older(found.spelling)));
    }
    if let Some(wanted) = &source.needs.sha256
        && let Some(found) = sha256_of(&source.origin)
        && &found != wanted
    {
        out.push(differs(Differ::OtherHash(found)));
    }
    out
}

/// The line the console logs, once a boot, for a font that the frit `set`
/// needs and that this machine lacks: the set, the font and the version
/// the set asks for. `lacks` is the character a font of that name lacks,
/// where one is here but is a different font. The console puts "the frit"
/// before the line. `None` for a source that names no font of its own,
/// such as `vconsole`, whose absence the console reports elsewhere.
pub fn missing_line(set: &str, source: &Source, lacks: Option<u32>) -> Option<String> {
    let font = source.font_name()?;
    let font = match &source.needs.version {
        Some(version) => format!("{font} {} or later", version.spelling),
        None => font,
    };
    let from = source
        .layer
        .as_ref()
        .map(|layer| format!(" from the layer {layer}"))
        .unwrap_or_default();
    Some(match lacks {
        None => format!(
            "{set} needs the font {font}{from}, which is not on this machine, so vtium draws its characters from the next font that has them"
        ),
        Some(c) => format!(
            "{set} needs the font {font}{from}, and the font of that name on this machine lacks U+{c:04X}, so vtium counts it as missing and draws its characters from the next font that has them"
        ),
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::frit::Frit;

    fn sources(text: &str) -> Vec<Source> {
        Frit::parse(text.as_bytes(), Path::new("t"), "t")
            .expect("parses")
            .sources
    }

    // The answer's lines read back as the fonts they name, and a reader
    // that knows only the older lines finds none of them.
    #[test]
    fn a_difference_reads_back_from_its_line() {
        for what in [
            Differ::Lacks(0xe0b0),
            Differ::Older("2.242".into()),
            Differ::OtherHash("ab".repeat(32)),
        ] {
            let d = Differs {
                line: 4,
                spelling: "font:A\tB".into(),
                what,
            };
            let text = d.line_text();
            let back = Differs::parse(&text).expect("reads");
            assert_eq!(back.line, 4);
            assert_eq!(back.spelling, "font:A B");
            assert_eq!(back.what, d.what);
            assert_eq!(crate::serve::LeftOut::parse(&text), None);
        }
        assert_eq!(Differs::parse("missing\t1\tfont:A\twhy"), None);
        assert_eq!(Differs::parse("lacks\t1\tfont:A\tE0B0"), None);
    }

    // A font lacks a character of check= when any range of it is short,
    // and the first one it lacks is named.
    #[test]
    fn lacking_names_the_first_character_a_font_lacks() {
        let held = [0x41u32, 0x42, 0xe0b0, 0xe0b2];
        let count = |a: u32, b: u32| held.iter().filter(|&&c| (a..=b).contains(&c)).count();
        assert_eq!(lacking(&[], count), None);
        assert_eq!(lacking(&[(0x41, 0x42), (0xe0b0, 0xe0b0)], count), None);
        assert_eq!(
            lacking(&[(0x41, 0x42), (0xe0b0, 0xe0b2)], count),
            Some(0xe0b1)
        );
        assert_eq!(lacking(&[(0x43, 0x43)], count), Some(0x43));
        assert_eq!(lacking(&[(0, 0x10_ffff)], count), Some(0));
    }

    // A lower version and another hash each give a difference; a font
    // that states no version, or a higher or equal one, gives none.
    #[test]
    fn a_lower_version_and_another_hash_are_differences() {
        let dir = crate::scratch::Scratch::new("needs-differences");
        let file = dir.join("f.bdf");
        std::fs::write(&file, b"font bytes").expect("a file");
        let hash = sha256_hex(&Sha256::digest(b"font bytes"));
        let s = sources(&format!(
            "source=bdf:{}\nversion=2.304\nsha256={hash}\nsource=bdf:{}\nsha256={}\n",
            file.display(),
            file.display(),
            "0".repeat(64)
        ));
        let line = 1;
        assert_eq!(differences(&s[0], Some("Version 2.304"), line), []);
        assert_eq!(differences(&s[0], Some("Version 2.310"), line), []);
        assert_eq!(differences(&s[0], None, line), []);
        assert_eq!(
            differences(&s[0], Some("Version 2.242; ttfautohint"), line),
            [Differs {
                line,
                spelling: format!("bdf:{}", file.display()),
                what: Differ::Older("2.242".into()),
            }]
        );
        assert_eq!(
            differences(&s[1], None, 4),
            [Differs {
                line: 4,
                spelling: format!("bdf:{}", file.display()),
                what: Differ::OtherHash(hash),
            }]
        );
    }

    // The console's line names the set, the font and the version, and
    // the layer a font comes from.
    #[test]
    fn the_missing_line_names_the_set_the_font_and_its_version() {
        let s = sources(
            "source=font:JetBrains Mono\nversion=2.304\nsource=kbd:ter-v16n\nsource=vconsole\n",
        );
        assert_eq!(
            missing_line("desktop", &s[0], None).as_deref(),
            Some(
                "desktop needs the font JetBrains Mono 2.304 or later, which is not on this machine, so vtium draws its characters from the next font that has them"
            )
        );
        assert_eq!(
            missing_line("desktop", &s[1], Some(0xe0b0)).as_deref(),
            Some(
                "desktop needs the font ter-v16n, and the font of that name on this machine lacks U+E0B0, so vtium counts it as missing and draws its characters from the next font that has them"
            )
        );
        assert_eq!(missing_line("desktop", &s[2], None), None);
        let mut layered = s[0].clone();
        layered.layer = Some("nerd-symbols".into());
        assert!(
            missing_line("t", &layered, None)
                .expect("a line")
                .starts_with("t needs the font JetBrains Mono 2.304 or later from the layer nerd-symbols, which")
        );
    }
}
