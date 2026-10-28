//! Frits (ADR-1200 D1): `/etc/vtium/fonts/<name>.vtfrit`, in
//! kmscon.conf's line syntax, read strictly. One `key=value` a line, blanks
//! around the key and the value dropped, `#` to the line's end a comment.
//! An unknown key, a malformed line, a value out of range or a key this
//! build does not build yet refuses the whole frit, naming the file and
//! the line, since `create` runs unattended and a half-read frit builds
//! the wrong set.

use std::fmt;
use std::path::{Path, PathBuf};

/// The most bytes a frit holds.
pub const FRIT_BYTES_MAX: usize = 64 * 1024;
/// The most bytes one line holds.
pub const FRIT_LINE_MAX: usize = 4096;
/// The most sources a frit names.
pub const FRIT_SOURCES_MAX: usize = 16;

/// Set names a frit may not take: the built-in sets'.
pub const RESERVED: [&str; 2] = ["unifont", "unscii-8"];

/// Why a frit was refused: its file, the line from 1 (0 for the whole
/// file) and the reason.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FritError {
    pub file: PathBuf,
    pub line: usize,
    pub why: String,
}

impl fmt::Display for FritError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file.display(), self.why)
        } else {
            write!(f, "{}:{}: {}", self.file.display(), self.line, self.why)
        }
    }
}

impl std::error::Error for FritError {}

/// Where a source's glyphs come from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Origin {
    /// A kbd console font by name, found as `setfont` finds it.
    Kbd(String),
    /// A font by family, matched on the machine that builds the set, as
    /// fontconfig matches a family: case and blanks ignored.
    Font(String),
    /// A font file by its absolute path.
    File(PathBuf),
    /// `FONT=` and `FONT_UNIMAP=` from vconsole.conf and the kernel's
    /// command line.
    Vconsole,
    /// The font the kernel holds for a VT.
    Console(u16),
    /// A built-in set: `unifont` or `unscii-8`.
    Builtin(String),
}

/// A range of codepoints a source is limited to, both ends included.
pub type Cover = (u32, u32);

/// One `source=` line and the keys that follow it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Source {
    /// The value as the frit spelled it.
    pub spelling: String,
    pub origin: Origin,
    /// `unimap=`: a kbd unimap name or an absolute path.
    pub unimap: Option<String>,
    /// `cover=`: empty for every glyph the source holds.
    pub cover: Vec<Cover>,
    /// `package=`: the package that installs the font, named when the
    /// font is missing.
    pub package: Option<String>,
    /// `sha256=`: the font file's SHA-256; a file with other bytes is
    /// taken as missing.
    pub sha256: Option<[u8; 32]>,
    /// The `source=` line, from 1.
    pub line: usize,
}

impl Source {
    /// Whether `cover=` lets the source give `c`.
    pub fn covers(&self, c: u32) -> bool {
        self.cover.is_empty() || self.cover.iter().any(|&(a, b)| (a..=b).contains(&c))
    }
}

/// A parsed frit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frit {
    pub name: String,
    pub sources: Vec<Source>,
    /// `compress=`: deflate the strikes that gain (ADR-1200 D13).
    pub compress: bool,
    pub license: Option<String>,
    /// The frit's bytes, verbatim, for the provenance and the build key.
    pub text: String,
    /// The file the frit was read from, for messages.
    pub file: PathBuf,
    /// What builds but does not travel: each line that names a path on
    /// the building machine, which another machine may not have.
    pub warnings: Vec<FritError>,
}

/// Whether `name` is a set name: 1 to 64 characters of `A-Z a-z 0-9 . _
/// + -`, not starting with `.` (ADR-1200 D1).
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

impl Frit {
    /// Reads and parses the frit at `path`.
    pub fn load(path: &Path) -> Result<Frit, FritError> {
        let err = |why: String| FritError {
            file: path.to_owned(),
            line: 0,
            why,
        };
        let bytes = crate::sources::read_regular(path, FRIT_BYTES_MAX as u64).map_err(err)?;
        let stem = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n))
            .unwrap_or_default();
        Frit::parse(&bytes, path, stem)
    }

    /// Parses a frit's bytes; `file` names it in errors and `stem` is the
    /// set's name when the frit gives none.
    pub fn parse(bytes: &[u8], file: &Path, stem: &str) -> Result<Frit, FritError> {
        let err = |line: usize, why: &str| FritError {
            file: file.to_owned(),
            line,
            why: why.to_owned(),
        };
        if bytes.len() > FRIT_BYTES_MAX {
            return Err(err(0, "larger than FRIT_BYTES_MAX"));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| err(0, "not UTF-8"))?;
        let mut name = None;
        let mut sources: Vec<Source> = Vec::new();
        let mut compress = None;
        let mut license = None;
        let mut warnings = Vec::new();
        let not_portable = |line: usize, key: &str| FritError {
            file: file.to_owned(),
            line,
            why: format!("{key} names a path on this machine, so the frit is not portable"),
        };
        for (line, n) in text.split('\n').zip(1usize..) {
            if line.len() > FRIT_LINE_MAX {
                return Err(err(n, "longer than FRIT_LINE_MAX"));
            }
            if line.contains('\0') {
                return Err(err(n, "holds a NUL byte"));
            }
            let body = line.split('#').next().unwrap_or_default().trim();
            if body.is_empty() {
                continue;
            }
            let Some((key, value)) = body.split_once('=') else {
                return Err(err(n, "not key=value"));
            };
            let (key, value) = (key.trim(), value.trim());
            if value.is_empty() {
                return Err(err(n, &format!("{key}= has no value")));
            }
            if value.chars().any(char::is_control) {
                return Err(err(n, &format!("{key}= holds a control character")));
            }
            let once = |seen: bool| {
                if seen {
                    Err(err(n, &format!("{key}= given twice")))
                } else {
                    Ok(())
                }
            };
            let current = |sources: &mut Vec<Source>| -> Result<usize, FritError> {
                sources
                    .len()
                    .checked_sub(1)
                    .ok_or_else(|| err(n, &format!("{key}= before any source=")))
            };
            match key {
                "name" => {
                    once(name.is_some())?;
                    name = Some(value.to_owned());
                }
                "source" => {
                    if sources.len() >= FRIT_SOURCES_MAX {
                        return Err(err(n, "more sources than FRIT_SOURCES_MAX"));
                    }
                    let origin = origin(value).map_err(|why| err(n, &why))?;
                    if matches!(origin, Origin::File(_)) {
                        warnings.push(not_portable(n, "source=file:"));
                    }
                    sources.push(Source {
                        spelling: value.to_owned(),
                        origin,
                        unimap: None,
                        cover: Vec::new(),
                        package: None,
                        sha256: None,
                        line: n,
                    });
                }
                "unimap" => {
                    let i = current(&mut sources)?;
                    let source = sources.get_mut(i).ok_or_else(|| err(n, "no source"))?;
                    once(source.unimap.is_some())?;
                    if matches!(source.origin, Origin::Builtin(_) | Origin::Console(_)) {
                        return Err(err(n, "unimap= for a source that carries its own map"));
                    }
                    if value.contains('/') && !value.starts_with('/') {
                        return Err(err(n, "unimap= is a kbd unimap name or an absolute path"));
                    }
                    if value.starts_with('/') {
                        warnings.push(not_portable(n, "unimap="));
                    }
                    source.unimap = Some(value.to_owned());
                }
                "package" | "sha256" => {
                    let i = current(&mut sources)?;
                    let source = sources.get_mut(i).ok_or_else(|| err(n, "no source"))?;
                    if !matches!(
                        source.origin,
                        Origin::Kbd(_) | Origin::File(_) | Origin::Font(_)
                    ) {
                        return Err(err(
                            n,
                            &format!("{key}= is for a kbd:, file: or font: source"),
                        ));
                    }
                    if key == "package" {
                        once(source.package.is_some())?;
                        source.package = Some(value.to_owned());
                    } else {
                        once(source.sha256.is_some())?;
                        source.sha256 = Some(
                            parse_sha256(value)
                                .ok_or_else(|| err(n, "sha256= is 64 lower-case hex digits"))?,
                        );
                    }
                }
                "cover" => {
                    let i = current(&mut sources)?;
                    let range = cover(value).map_err(|why| err(n, &why))?;
                    if let Some(source) = sources.get_mut(i) {
                        source.cover.push(range);
                    }
                }
                "style" => {
                    current(&mut sources)?;
                    if value != "regular" {
                        return Err(err(
                            n,
                            "style= other than regular is not built yet (vtium/vtium#125's later slices)",
                        ));
                    }
                }
                "compress" => {
                    once(compress.is_some())?;
                    compress = Some(match value {
                        "yes" => true,
                        "no" => false,
                        _ => return Err(err(n, "compress= is yes or no")),
                    });
                }
                "license" => {
                    once(license.is_some())?;
                    license = Some(value.to_owned());
                }
                "size" | "cp-size" => {
                    return Err(err(
                        n,
                        &format!(
                            "{key}= is for outline and CPI sources, not built yet (vtium/vtium#125's later slices)"
                        ),
                    ));
                }
                k if k == "glyph-source" || k.starts_with("glyph-source-") => {
                    return Err(err(
                        n,
                        "glyph-source= is not built yet; the set takes the sprite policy its coverage gives",
                    ));
                }
                _ => return Err(err(n, &format!("unknown key {key}"))),
            }
        }
        let name = name.unwrap_or_else(|| stem.to_owned());
        if !valid_name(&name) {
            return Err(err(
                0,
                &format!(
                    "{name:?} is not a set name: 1 to 64 of A-Z a-z 0-9 . _ + -, not starting with ."
                ),
            ));
        }
        if RESERVED.contains(&name.as_str()) {
            return Err(err(0, &format!("{name} is a built-in set's name")));
        }
        if sources.is_empty() {
            return Err(err(0, "no source="));
        }
        Ok(Frit {
            name,
            sources,
            compress: compress.unwrap_or(true),
            license,
            text: text.to_owned(),
            file: file.to_owned(),
            warnings,
        })
    }
}

/// A `source=` value's origin.
fn origin(value: &str) -> Result<Origin, String> {
    if value == "vconsole" {
        return Ok(Origin::Vconsole);
    }
    let Some((kind, rest)) = value.split_once(':') else {
        return Err(format!(
            "source={value} is none of font:, kbd:, file:, vconsole, console: or builtin:"
        ));
    };
    match kind {
        "font" => {
            if rest.trim().is_empty() || rest.contains('/') {
                return Err("font: takes a family name, with no /".to_owned());
            }
            Ok(Origin::Font(rest.to_owned()))
        }
        "kbd" if !rest.is_empty() && !rest.contains('\0') => {
            if rest.split('/').any(|c| c == "..") {
                return Err("a kbd: name holds ..".to_owned());
            }
            Ok(Origin::Kbd(rest.to_owned()))
        }
        "file" => {
            let path = PathBuf::from(rest);
            if !path.is_absolute() {
                return Err("file: takes an absolute path".to_owned());
            }
            Ok(Origin::File(path))
        }
        "console" => rest
            .parse::<u16>()
            .ok()
            .filter(|&vt| (1..=63).contains(&vt) && !rest.starts_with('0'))
            .map(Origin::Console)
            .ok_or_else(|| "console: takes a VT's number, 1 to 63".to_owned()),
        "builtin" if RESERVED.contains(&rest) => Ok(Origin::Builtin(rest.to_owned())),
        "builtin" => Err("builtin: is unifont or unscii-8".to_owned()),
        "bdf" | "outline" => Err(format!(
            "{kind}: sources are not built yet (vtium/vtium#125's later slices)"
        )),
        _ => Err(format!(
            "source={value} is none of font:, kbd:, file:, vconsole, console: or builtin:"
        )),
    }
}

/// 64 lower-case hex digits as the 32 bytes they spell.
pub fn parse_sha256(s: &str) -> Option<[u8; 32]> {
    let digits = s.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut out = [0u8; 32];
    for (byte, pair) in out.iter_mut().zip(digits.chunks(2)) {
        let [hi, lo] = pair else { return None };
        *byte = (nibble(*hi)? << 4) | nibble(*lo)?;
    }
    Some(out)
}

/// A `cover=` value: `ascii`, `latin-1` or `U+XXXX-U+YYYY`.
fn cover(value: &str) -> Result<Cover, String> {
    match value {
        "ascii" => return Ok((0, 0x7f)),
        "latin-1" => return Ok((0, 0xff)),
        "gb2312" | "gbk" | "big5" | "jis-x-0208" | "ks-x-1001" => {
            return Err(format!(
                "cover={value} is not built yet (vtium/vtium#125's later slices); give a range"
            ));
        }
        _ => {}
    }
    let point = |s: &str| {
        let hex = s.strip_prefix("U+")?;
        if !(4..=6).contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16)
            .ok()
            .filter(|&c| c <= 0x10_ffff)
    };
    let (a, b) = match value.split_once('-') {
        Some((a, b)) => (point(a), point(b)),
        None => (point(value), point(value)),
    };
    match (a, b) {
        (Some(a), Some(b)) if a <= b => Ok((a, b)),
        _ => Err(format!(
            "cover={value} is none of ascii, latin-1, U+XXXX or U+XXXX-U+YYYY"
        )),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Frit, FritError> {
        Frit::parse(text.as_bytes(), Path::new("/r/t.vtfrit"), "t")
    }

    fn refusal(text: &str) -> (usize, String) {
        let e = parse(text).expect_err("refused");
        (e.line, e.why)
    }

    #[test]
    fn every_key_is_read() {
        let r = parse(
            "# a comment\n\
             name = term-16 # trailing comment\n\
             source=kbd:ter-v16n\n\
             unimap=lat1u\n\
             cover=ascii\n\
             cover=U+2500-U+257F\n\
             style=regular\n\
             source=file:/usr/share/fonts/x.psf.gz\n\
             source=vconsole\n\
             source=console:3\n\
             source=builtin:unifont\n\
             source=font:Terminus 16\n\
             style=regular\n\
             package=terminus-font\n\
             sha256=00000000000000000000000000000000000000000000000000000000000000ff\n\
             compress=no\n\
             license=OFL-1.1\n",
        )
        .expect("parses");
        assert_eq!(r.name, "term-16");
        assert!(!r.compress);
        assert_eq!(r.license.as_deref(), Some("OFL-1.1"));
        let origins: Vec<&Origin> = r.sources.iter().map(|s| &s.origin).collect();
        assert_eq!(
            origins,
            [
                &Origin::Kbd("ter-v16n".into()),
                &Origin::File("/usr/share/fonts/x.psf.gz".into()),
                &Origin::Vconsole,
                &Origin::Console(3),
                &Origin::Builtin("unifont".into()),
                &Origin::Font("Terminus 16".into()),
            ]
        );
        let font = &r.sources[5];
        assert_eq!(font.package.as_deref(), Some("terminus-font"));
        let mut sha = [0u8; 32];
        sha[31] = 0xff;
        assert_eq!((font.sha256, font.line), (Some(sha), 12));
        assert_eq!(r.sources[0].unimap.as_deref(), Some("lat1u"));
        assert_eq!(r.sources[0].cover, [(0, 0x7f), (0x2500, 0x257f)]);
        assert!(r.sources[0].covers(0x41) && !r.sources[0].covers(0xe9));
        assert!(r.sources[1].covers(0xe9));
    }

    #[test]
    fn the_name_defaults_to_the_stem_and_compress_to_yes() {
        let r = parse("source=kbd:x\n").expect("parses");
        assert_eq!((r.name.as_str(), r.compress), ("t", true));
        assert_eq!(r.text, "source=kbd:x\n");
    }

    #[test]
    fn each_refusal_names_its_line() {
        for (text, line, why) in [
            ("source=kbd:x\nbogus=1\n", 2, "unknown key bogus"),
            ("source=kbd:x\njust words\n", 2, "not key=value"),
            ("unimap=x\n", 1, "before any source="),
            ("cover=ascii\n", 1, "before any source="),
            ("source=kbd:x\ncompress=maybe\n", 2, "yes or no"),
            ("source=kbd:x\nname=a\nname=b\n", 3, "given twice"),
            ("source=file:relative.psf\n", 1, "absolute path"),
            ("source=console:0\n", 1, "1 to 63"),
            ("source=console:64\n", 1, "1 to 63"),
            ("source=builtin:terminus\n", 1, "unifont or unscii-8"),
            ("source=bdf:/x.bdf\n", 1, "not built yet"),
            ("source=outline:/x.ttf\n", 1, "not built yet"),
            ("source=x\n", 1, "none of"),
            ("source=kbd:../x\n", 1, ".."),
            ("source=kbd:x\nsize=24\n", 2, "not built yet"),
            ("source=kbd:x\ncp-size=8\n", 2, "not built yet"),
            ("source=kbd:x\nstyle=bold\n", 2, "not built yet"),
            ("source=kbd:x\nglyph-source=font\n", 2, "not built yet"),
            ("source=kbd:x\ncover=gbk\n", 2, "not built yet"),
            ("source=kbd:x\ncover=U+9FFF-U+4E00\n", 2, "none of"),
            ("source=kbd:x\ncover=U+41\n", 2, "none of"),
            ("source=font: \n", 1, "family name"),
            ("source=font:a/b\n", 1, "family name"),
            ("package=x\n", 1, "before any source="),
            (
                "source=builtin:unifont\npackage=x\n",
                2,
                "kbd:, file: or font:",
            ),
            ("source=font:x\nsha256=ABC\n", 2, "64 lower-case hex"),
            ("source=font:x\npackage=a\npackage=b\n", 3, "given twice"),
            ("source=kbd:x\nunimap=a/b\n", 2, "absolute path"),
            ("source=builtin:unifont\nunimap=x\n", 2, "its own map"),
            ("source=kbd:x\nlicense=\n", 2, "has no value"),
            ("source=file:/a\tb.psf\n", 1, "control character"),
            ("name=unifont\nsource=kbd:x\n", 0, "built-in"),
            ("name=.hidden\nsource=kbd:x\n", 0, "not a set name"),
            ("name=a b\nsource=kbd:x\n", 0, "not a set name"),
            ("# nothing\n", 0, "no source="),
        ] {
            let (l, w) = refusal(text);
            assert_eq!(l, line, "{text:?}: {w}");
            assert!(w.contains(why), "{text:?}: {w}");
        }
    }

    #[test]
    fn a_line_past_its_bound_and_a_nul_are_refused() {
        let long = format!("source=kbd:{}\n", "x".repeat(FRIT_LINE_MAX));
        assert_eq!(refusal(&long).0, 1);
        assert_eq!(refusal("source=kbd:x\0\n").0, 1);
        let many = "source=kbd:x\n".repeat(FRIT_SOURCES_MAX + 1);
        assert_eq!(refusal(&many).0, FRIT_SOURCES_MAX + 1);
        let e = Frit::parse(&[0xff], Path::new("/r/t.vtfrit"), "t").expect_err("refused");
        assert_eq!(e.why, "not UTF-8");
    }

    #[test]
    fn the_error_names_the_file_and_line() {
        let e = parse("source=kbd:x\nbogus=1\n").expect_err("refused");
        assert_eq!(e.to_string(), "/r/t.vtfrit:2: unknown key bogus");
    }
}
