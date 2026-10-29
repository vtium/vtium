//! Finding and reading a frit's sources, as the helper's account, before
//! anything is parsed (ADR-1200 D1, D8, D14): kbd names resolved as
//! `setfont` resolves them, outline families found among the installed
//! fonts, files, vconsole.conf with the kernel's command line over it, the
//! kernel's own font, and the built-in fonts. Each source becomes the
//! bytes the parser is handed, or the glyphs vtium-vitrify reads itself: a
//! built-in font's, and a BDF font's through [`crate::bdf`].

use std::path::{Path, PathBuf};

use vtium_font::PSF_BYTES_MAX;
use vtium_platform::vt::font::{ConsoleFont, ConsoleTty};

use crate::child::Request;
use crate::font::Font;
use crate::frit::{Origin, Source};
use crate::outline::{OUTLINE_BYTES_MAX, OutlineRequest, family_key as outline_key};

/// The kbd data directories the build names, in the order they are
/// searched: kbd's own, Debian's console-setup and the Red Hat family's.
pub const KBD_DIRS: [&str; 3] = ["/usr/share/kbd", "/usr/share", "/usr/lib/kbd"];

/// The suffixes `setfont` tries after a font's name, in order
/// (`src/libkfont/context.c`).
pub const FONT_SUFFIXES: [&str; 5] = ["", ".psfu", ".psf", ".cp", ".fnt"];
/// The suffixes it tries after a unimap's name.
pub const UNIMAP_SUFFIXES: [&str; 3] = ["", ".uni", ".sfm"];
/// The compressions it tries after each, in order (`kbdfile.c`).
pub const COMPRESSED: [&str; 5] = ["", ".gz", ".bz2", ".xz", ".zst"];

/// The directories installed outline fonts are found in, as fontconfig's
/// default configuration names them for the system; `create` run as a
/// user also searches that user's own (`Host::font_dirs`).
pub const FONT_DIRS: [&str; 2] = ["/usr/share/fonts", "/usr/local/share/fonts"];
/// The extensions an outline font file carries, case ignored.
pub const OUTLINE_SUFFIXES: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// How deep under a font directory an outline family is looked for.
pub const FONT_DEPTH_MAX: usize = 8;

/// The most bytes vconsole.conf or the kernel's command line holds.
pub(crate) const CONFIG_BYTES_MAX: u64 = 64 * 1024;

/// What the host gives a build: where kbd's data is, where vconsole.conf
/// and the kernel's command line are, and the kernel's VT fonts.
pub struct Host<'a> {
    pub kbd_dirs: Vec<PathBuf>,
    /// Where installed outline fonts are found, in order.
    pub font_dirs: Vec<PathBuf>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
    pub consoles: &'a dyn Consoles,
}

/// The kernel's VT fonts: [`KernelConsoles`] reads them, a test stands in.
pub trait Consoles {
    fn font(&self, vt: u16) -> Result<ConsoleFont, String>;
}

/// The kernel's VTs, through `/dev/tty<N>` and vtium-platform's ioctls.
pub struct KernelConsoles;

impl Consoles for KernelConsoles {
    fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
        let path = PathBuf::from(format!("/dev/tty{vt}"));
        let tty = ConsoleTty::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        vtium_platform::vt::font::read(&tty).map_err(|e| match e {
            vtium_platform::vt::font::FontError::Graphics => {
                format!("VT {vt} is in graphics mode, as vtium's own VT is: name a VT in text mode")
            }
            e => format!("VT {vt}: {e}"),
        })
    }
}

/// A source read: what the parser is to be given, or glyphs already read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A PSF font, the font of a CPI file `cp-size=` took, or the
    /// kernel's, with the maps it may take.
    Font(Request),
    /// An outline font, its cells and coverage filled in when the cell is
    /// known.
    Outline(OutlineRequest),
    /// A bitmap font's glyphs at its own cell, read here: a built-in
    /// font's, or a BDF font's.
    Builtin(Font),
}

/// A source ready to draw from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub input: Input,
}

/// Reads `path`, at most `limit` bytes: opened once, without blocking on
/// a FIFO and never as a controlling terminal, and refused unless that
/// descriptor is a regular file. The error leaves the path to the caller.
pub(crate) fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let flags = rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::NOCTTY;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits() as i32)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err("not a regular file".to_owned());
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("larger than {limit} bytes"));
    }
    Ok(bytes)
}

/// [`read_regular`], its error naming `path`.
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    read_regular(path, limit).map_err(|e| format!("{}: {e}", path.display()))
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// Finds `name` in `dir` as kbd's `findfile_in_dir` does at depth 0: a name
/// with a `/` looks in the subdirectory its first component names; any
/// other is `name` followed by the first of `suffixes` for which a file
/// exists plain or compressed, plain first.
fn find_in_dir(name: &str, dir: &Path, suffixes: &[&str]) -> Option<PathBuf> {
    if let Some((sub, rest)) = name.split_once('/') {
        let sub = dir.join(sub);
        return if sub.is_dir() {
            find_in_dir(rest, &sub, suffixes)
        } else {
            None
        };
    }
    suffixes.iter().find_map(|suffix| {
        COMPRESSED.iter().find_map(|z| {
            let path = dir.join(format!("{name}{suffix}{z}"));
            is_file(&path).then_some(path)
        })
    })
}

/// Finds a kbd font or unimap as `setfont` finds it (`kbdfile_find`): an
/// absolute path as given, then with each suffix, each plain then
/// compressed; any other name in `<data>/<sub>/` of each data directory in
/// turn. Unlike `setfont`, a relative name is never looked for in the
/// working directory, which an unattended `create` does not choose.
pub fn find(name: &str, data_dirs: &[PathBuf], sub: &str, suffixes: &[&str]) -> Option<PathBuf> {
    if name.starts_with('/') {
        return suffixes.iter().find_map(|suffix| {
            COMPRESSED.iter().find_map(|z| {
                let path = PathBuf::from(format!("{name}{suffix}{z}"));
                is_file(&path).then_some(path)
            })
        });
    }
    data_dirs
        .iter()
        .find_map(|d| find_in_dir(name, &d.join(sub), suffixes))
}

/// The values of `keys` in an environment-style file: `KEY=value` lines,
/// `#` and `;` comment lines, a value in single or double quotes taken
/// without them, a later line over an earlier one.
fn env_values(text: &str, keys: &[&str]) -> Vec<Option<String>> {
    let mut out = vec![None; keys.len()];
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if let Some(i) = keys.iter().position(|&key| key == k.trim()) {
            let v = v.trim();
            let v = ['"', '\'']
                .iter()
                .find_map(|&q| v.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
                .unwrap_or(v);
            if let Some(slot) = out.get_mut(i) {
                *slot = Some(v.to_owned()).filter(|v| !v.is_empty());
            }
        }
    }
    out
}

/// `FONT=` and `FONT_UNIMAP=` as systemd-vconsole-setup reads them:
/// vconsole.conf, then the kernel command line's `vconsole.font=` and
/// `vconsole.font_unimap=` over it.
pub fn vconsole(conf: &str, cmdline: &str) -> (Option<String>, Option<String>) {
    let mut values = env_values(conf, &["FONT", "FONT_UNIMAP"]).into_iter();
    let (mut font, mut unimap) = (values.next().flatten(), values.next().flatten());
    for word in cmdline.split_whitespace() {
        if let Some(v) = word.strip_prefix("vconsole.font=") {
            font = Some(v.to_owned()).filter(|v| !v.is_empty());
        } else if let Some(v) = word.strip_prefix("vconsole.font_unimap=") {
            unimap = Some(v.to_owned()).filter(|v| !v.is_empty());
        }
    }
    (font, unimap)
}

/// A PSF2 font of the kernel's glyphs, with its map as the table (ADR-1200
/// D8): a VT's font is vitrified as a PSF with that map.
pub fn console_psf(font: &ConsoleFont) -> Vec<u8> {
    let row = font.width.div_ceil(8);
    let has_table = !font.unimap.is_empty();
    let mut out = Vec::with_capacity(32usize.saturating_add(font.data.len()));
    for v in [
        0x864a_b572u32,
        0,
        32,
        u32::from(has_table),
        font.charcount,
        row.saturating_mul(font.height),
        font.height,
        font.width,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&font.data);
    if has_table {
        let mut by_glyph: Vec<Vec<char>> = vec![Vec::new(); font.charcount as usize];
        for pair in &font.unimap {
            if let (Some(glyph), Some(c)) = (
                by_glyph.get_mut(usize::from(pair.fontpos)),
                char::from_u32(u32::from(pair.unicode)),
            ) {
                glyph.push(c);
            }
        }
        for chars in by_glyph {
            let mut buf = [0u8; 4];
            for c in chars {
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out.push(0xff);
        }
    }
    out
}

/// Why a source gave no font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The font is not on this machine.
    Missing(String),
    /// The source is there and cannot be read.
    Failed(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Missing(why) | SourceError::Failed(why) => f.write_str(why),
        }
    }
}

impl From<String> for SourceError {
    fn from(why: String) -> SourceError {
        SourceError::Failed(why)
    }
}

/// Lowercase with blanks dropped, as fontconfig compares family names.
fn family_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// `name` less the compression and then the font suffix kbd tries.
fn font_stem(name: &str) -> &str {
    let name = COMPRESSED
        .iter()
        .filter(|z| !z.is_empty())
        .find_map(|z| name.strip_suffix(z))
        .unwrap_or(name);
    FONT_SUFFIXES
        .iter()
        .filter(|s| !s.is_empty())
        .find_map(|s| name.strip_suffix(s))
        .unwrap_or(name)
}

/// The most entries a `consolefonts/` directory holds for a family's
/// scan: Arch's kbd ships about 300 fonts.
pub const FAMILY_ENTRIES_MAX: usize = 65536;

/// Finds a font by family (`font:`): a kbd console font whose name is the
/// family as `setfont` would find it, else the first, in each data
/// directory's sorted `consolefonts/` in turn, whose name less its
/// suffixes equals the family with case and blanks ignored, as fontconfig
/// compares families. Outline and BDF families are matched once those
/// formats are read (vtium/vtium#23). A directory past
/// `FAMILY_ENTRIES_MAX` entries fails the search.
pub fn find_family(family: &str, data_dirs: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    find_family_capped(family, data_dirs, FAMILY_ENTRIES_MAX)
}

fn find_family_capped(
    family: &str,
    data_dirs: &[PathBuf],
    cap: usize,
) -> Result<Option<PathBuf>, String> {
    if let Some(p) = find(family, data_dirs, "consolefonts", &FONT_SUFFIXES) {
        return Ok(Some(p));
    }
    let key = family_key(family);
    for d in data_dirs {
        let dir = d.join("consolefonts");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names = Vec::new();
        for entry in entries.flatten() {
            if names.len() >= cap {
                return Err(format!("{} holds more than {cap} entries", dir.display()));
            }
            names.push(entry.path());
        }
        names.sort();
        let found = names.into_iter().find(|p| {
            is_file(p)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| family_key(font_stem(n)) == key)
        });
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// Finds an outline font by family (`font:`) under `dirs`, as installed
/// fonts are named: a TrueType or OpenType file or collection whose name,
/// less its extension, is the family, or the family and `Regular`, with
/// case, blanks, `-` and `_` ignored, the plain name first, then in the
/// order the directories are given and by path. The parser checks the
/// family the file's name table gives. A directory past
/// `FAMILY_ENTRIES_MAX` entries fails the search; directories deeper than
/// `FONT_DEPTH_MAX` and links to directories are not entered. A file
/// found is read only if, its links resolved, it stays under `dirs`.
pub fn find_outline_family(family: &str, dirs: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let key = outline_key(family);
    let regular = format!("{key}regular");
    let mut found: Option<(u8, usize, PathBuf)> = None;
    for (order, root) in dirs.iter().enumerate() {
        let mut pending = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for (n, entry) in entries.flatten().enumerate() {
                if n >= FAMILY_ENTRIES_MAX {
                    return Err(format!(
                        "{} holds more than {FAMILY_ENTRIES_MAX} entries",
                        dir.display()
                    ));
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if depth < FONT_DEPTH_MAX {
                        pending.push((path, depth.saturating_add(1)));
                    }
                    continue;
                }
                let (Some(stem), Some(ext)) = (
                    path.file_stem().and_then(|s| s.to_str()),
                    path.extension().and_then(|s| s.to_str()),
                ) else {
                    continue;
                };
                if !OUTLINE_SUFFIXES.iter().any(|s| s.eq_ignore_ascii_case(ext)) {
                    continue;
                }
                let stem = outline_key(stem);
                let rank = if stem == key {
                    0
                } else if stem == regular {
                    1
                } else {
                    continue;
                };
                let better = found
                    .as_ref()
                    .is_none_or(|(r, o, p)| (rank, order, &path) < (*r, *o, p));
                if better && is_file(&path) {
                    found = Some((rank, order, path));
                }
            }
        }
    }
    found
        .map(|(_, _, path)| {
            let real =
                std::fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let inside = dirs
                .iter()
                .any(|d| std::fs::canonicalize(d).is_ok_and(|root| real.starts_with(root)));
            if inside {
                Ok(real)
            } else {
                Err(format!(
                    "{} leaves the font directories through a link",
                    path.display()
                ))
            }
        })
        .transpose()
}

/// `path`, found by name under `<data>/<sub>/`, resolved through its links
/// and refused unless it stays under that directory of one of
/// `data_dirs`, so a link out of kbd's directories reads nothing.
fn contained(path: &Path, data_dirs: &[PathBuf], sub: &str) -> Result<PathBuf, String> {
    let real = std::fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let inside = data_dirs
        .iter()
        .any(|d| std::fs::canonicalize(d.join(sub)).is_ok_and(|root| real.starts_with(root)));
    if inside {
        Ok(real)
    } else {
        Err(format!(
            "{} leaves the kbd data directories through a link",
            path.display()
        ))
    }
}

/// Reads `source` for the parser.
pub fn resolve(source: &Source, host: &Host<'_>) -> Result<Resolved, SourceError> {
    let limit = u64::from(PSF_BYTES_MAX);
    let dirs = || {
        host.kbd_dirs
            .iter()
            .map(|d| d.join("consolefonts").display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    // a name found in kbd's directories is read only from inside them
    let inside = |name: &str, path: PathBuf, sub: &str| {
        if name.starts_with('/') {
            Ok(path)
        } else {
            contained(&path, &host.kbd_dirs, sub)
        }
    };
    let find_font = |name: &str| -> Result<PathBuf, SourceError> {
        let path = find(name, &host.kbd_dirs, "consolefonts", &FONT_SUFFIXES)
            .ok_or_else(|| SourceError::Missing(format!("no kbd font {name} in {}", dirs())))?;
        Ok(inside(name, path, "consolefonts")?)
    };
    let read_unimap = |name: &str| -> Result<Vec<u8>, String> {
        let path = find(name, &host.kbd_dirs, "unimaps", &UNIMAP_SUFFIXES)
            .ok_or_else(|| format!("no kbd unimap {name}"))?;
        read_bounded(&inside(name, path, "unimaps")?, limit)
    };
    let def_uni = || {
        find("def.uni", &host.kbd_dirs, "unimaps", &UNIMAP_SUFFIXES)
            .map(|p| read_bounded(&contained(&p, &host.kbd_dirs, "unimaps")?, limit))
            .transpose()
    };
    let not_here =
        |path: &Path| SourceError::Missing(format!("{}: not on this machine", path.display()));
    let (font, unimap_name) = match &source.origin {
        Origin::Builtin(name) => return Ok(bitmap(Font::from_builtin(name)?)),
        Origin::Bdf(path) if !path.exists() => return Err(not_here(path)),
        Origin::Bdf(path) => {
            return Ok(bitmap(crate::bdf::font(&read_bounded(path, limit)?)?));
        }
        Origin::Kbd(name) => (
            read_bounded(&find_font(name)?, limit)?,
            source.unimap.clone(),
        ),
        Origin::Font(family) => match find_family(family, &host.kbd_dirs)? {
            Some(path) => {
                let path = contained(&path, &host.kbd_dirs, "consolefonts")?;
                (read_bounded(&path, limit)?, source.unimap.clone())
            }
            None => {
                let path = find_outline_family(family, &host.font_dirs)?.ok_or_else(|| {
                    let fonts: Vec<String> = host
                        .font_dirs
                        .iter()
                        .map(|d| d.display().to_string())
                        .collect();
                    SourceError::Missing(format!(
                        "no font of family {family}: no kbd console font of that name in {}, \
                         and no outline font named for it in {}",
                        dirs(),
                        fonts.join(", ")
                    ))
                })?;
                if source.cp_size.is_some() {
                    return Err(SourceError::Failed(format!(
                        "cp-size= is for a CPI font, and {} is an outline font",
                        path.display()
                    )));
                }
                return Ok(outline(read_bounded(&path, OUTLINE_BYTES_MAX)?, family));
            }
        },
        Origin::Outline(path) if !path.exists() => return Err(not_here(path)),
        Origin::Outline(path) => {
            return Ok(outline(read_bounded(path, OUTLINE_BYTES_MAX)?, ""));
        }
        Origin::File(path) if !path.exists() => return Err(not_here(path)),
        Origin::File(path) => (read_bounded(path, limit)?, source.unimap.clone()),
        Origin::Vconsole => {
            let conf = match read_bounded(&host.vconsole_conf, CONFIG_BYTES_MAX) {
                Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                Err(_) if !host.vconsole_conf.exists() => String::new(),
                Err(e) => return Err(e.into()),
            };
            let cmdline = read_bounded(&host.kernel_cmdline, CONFIG_BYTES_MAX)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let (font, unimap) = vconsole(&conf, &cmdline);
            let font = font.ok_or_else(|| {
                format!(
                    "{} names no FONT= and the kernel command line no vconsole.font=, \
                     so the console keeps the kernel's font: use console:VT",
                    host.vconsole_conf.display()
                )
            })?;
            (
                read_bounded(&find_font(&font)?, limit)?,
                source.unimap.clone().or(unimap),
            )
        }
        Origin::Console(vt) => (console_psf(&host.consoles.font(*vt)?), None),
    };
    let font = match source.cp_size {
        Some(height) => crate::decode::cpi(&font, height)?,
        None => font,
    };
    let unimap = unimap_name.as_deref().map(read_unimap).transpose()?;
    let def_uni = if unimap.is_none() { def_uni()? } else { None };
    Ok(Resolved {
        input: Input::Font(Request {
            font,
            unimap,
            def_uni,
        }),
    })
}

/// A bitmap font's glyphs, read here.
fn bitmap(font: Font) -> Resolved {
    Resolved {
        input: Input::Builtin(font),
    }
}

/// An outline font's bytes as the parser's input: the family the parser
/// is to find in it, the cells and coverage filled in once the cell is
/// known.
fn outline(font: Vec<u8>, family: &str) -> Resolved {
    Resolved {
        input: Input::Outline(OutlineRequest {
            font: font.into(),
            family: family.to_owned(),
            cells: Vec::new(),
            cover: Vec::new(),
        }),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use std::fs;

    use vtium_platform::vt::font::UniPair;

    use super::*;

    struct Tree(PathBuf);

    impl Tree {
        fn new(test: &str) -> Tree {
            let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/tmp"))
                .join("vitrify-sources")
                .join(test);
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Tree(dir)
        }

        fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    fn found(tree: &Tree, name: &str, dirs: &[&str]) -> Option<String> {
        let dirs: Vec<PathBuf> = dirs.iter().map(|d| tree.0.join(d)).collect();
        find(name, &dirs, "consolefonts", &FONT_SUFFIXES)
            .map(|p| p.strip_prefix(&tree.0).unwrap().display().to_string())
    }

    #[test]
    fn kbd_names_resolve_in_setfonts_order() {
        let t = Tree::new("order");
        t.file("a/consolefonts/ter.psf.gz", b"x");
        t.file("a/consolefonts/ter.psfu.zst", b"x");
        t.file("a/consolefonts/lat.psf", b"x");
        t.file("a/consolefonts/lat.psf.gz", b"x");
        t.file("a/consolefonts/exact", b"x");
        t.file("a/consolefonts/exact.psfu", b"x");
        t.file("b/consolefonts/ter.psfu", b"x");
        t.file("b/consolefonts/only-b.fnt.xz", b"x");
        t.file("b/consolefonts/sub/deep.cp.bz2", b"x");
        fs::create_dir_all(t.0.join("a/consolefonts/dir.psfu")).unwrap();
        // the first directory holding the name wins, then the first suffix
        // with a file, compressed or not, then plain before compressed
        assert_eq!(
            found(&t, "ter", &["a", "b"]).as_deref(),
            Some("a/consolefonts/ter.psfu.zst")
        );
        assert_eq!(
            found(&t, "ter", &["b", "a"]).as_deref(),
            Some("b/consolefonts/ter.psfu")
        );
        assert_eq!(
            found(&t, "lat", &["a"]).as_deref(),
            Some("a/consolefonts/lat.psf")
        );
        assert_eq!(
            found(&t, "exact", &["a"]).as_deref(),
            Some("a/consolefonts/exact")
        );
        assert_eq!(
            found(&t, "only-b", &["a", "b"]).as_deref(),
            Some("b/consolefonts/only-b.fnt.xz")
        );
        assert_eq!(
            found(&t, "sub/deep", &["a", "b"]).as_deref(),
            Some("b/consolefonts/sub/deep.cp.bz2")
        );
        // a directory is not a font, and a missing name finds nothing
        assert_eq!(found(&t, "dir", &["a"]), None);
        assert_eq!(found(&t, "missing", &["a", "b"]), None);
        // an absolute path takes the suffixes too
        let abs = t.0.join("a/consolefonts/ter");
        let p = find(abs.to_str().unwrap(), &[], "consolefonts", &FONT_SUFFIXES).unwrap();
        assert_eq!(p, t.0.join("a/consolefonts/ter.psfu.zst"));
    }

    #[test]
    fn a_relative_name_is_never_read_from_the_working_directory() {
        let t = Tree::new("cwd");
        // the test's working directory is the crate's, which holds Cargo.toml
        assert!(Path::new("Cargo.toml").exists());
        assert_eq!(found(&t, "Cargo.toml", &["a"]), None);
    }

    #[test]
    fn vconsole_reads_the_conf_with_the_cmdline_over_it() {
        let conf = "# comment\nKEYMAP=us\nFONT=\"ter-v16n\"\nFONT_UNIMAP='lat1u'\n";
        assert_eq!(
            vconsole(conf, "quiet\n"),
            (Some("ter-v16n".into()), Some("lat1u".into()))
        );
        assert_eq!(
            vconsole(
                conf,
                "root=/dev/sda vconsole.font=lat2-16 vconsole.font_unimap=x\n"
            ),
            (Some("lat2-16".into()), Some("x".into()))
        );
        assert_eq!(vconsole("FONT=\n", ""), (None, None));
        assert_eq!(vconsole("", "vconsole.keymap=us"), (None, None));
    }

    #[test]
    fn a_console_font_becomes_a_psf_with_its_map() {
        let font = ConsoleFont {
            width: 8,
            height: 2,
            charcount: 3,
            data: vec![1, 1, 2, 2, 3, 3],
            unimap: vec![
                UniPair {
                    unicode: 0x41,
                    fontpos: 1,
                },
                UniPair {
                    unicode: 0xc0,
                    fontpos: 1,
                },
                UniPair {
                    unicode: 0x2500,
                    fontpos: 2,
                },
                UniPair {
                    unicode: 0x42,
                    fontpos: 9,
                },
            ],
        };
        let psf = vtium_font::Psf::parse(&console_psf(&font)).expect("a PSF");
        assert_eq!((psf.glyph_count(), psf.glyph_size()), (3, (8, 2)));
        assert_eq!(psf.glyph_index('A'), Some(1));
        assert_eq!(psf.glyph_index('À'), Some(1));
        assert_eq!(psf.glyph_index('─'), Some(2));
        assert_eq!(psf.glyph_index('B'), None);
    }

    struct Graphics;

    impl Consoles for Graphics {
        fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
            Err(format!("VT {vt} is in graphics mode"))
        }
    }

    fn host<'a>(t: &Tree, consoles: &'a dyn Consoles) -> Host<'a> {
        Host {
            kbd_dirs: vec![t.0.join("kbd")],
            font_dirs: vec![t.0.join("fonts")],
            vconsole_conf: t.0.join("vconsole.conf"),
            kernel_cmdline: t.0.join("cmdline"),
            consoles,
        }
    }

    fn source(spelling: &str, origin: Origin, unimap: Option<&str>) -> Source {
        Source {
            spelling: spelling.into(),
            origin,
            unimap: unimap.map(Into::into),
            cover: Vec::new(),
            cp_size: None,
            line: 1,
        }
    }

    /// The request a source resolves to.
    fn request(r: Resolved) -> Request {
        match r.input {
            Input::Font(request) => request,
            other => panic!("not a font: {other:?}"),
        }
    }

    #[test]
    fn each_source_reads_its_bytes() {
        let t = Tree::new("resolve");
        t.file("kbd/consolefonts/f.psfu.gz", b"font bytes");
        t.file("kbd/unimaps/lat1u.uni", b"0x41 U+0041\n");
        t.file("kbd/unimaps/def.uni", b"0x00 U+0041\n");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = request(resolve(&source("kbd:f", Origin::Kbd("f".into()), None), &h).unwrap());
        assert_eq!(
            (r.font.as_slice(), r.unimap, r.def_uni.as_deref()),
            (&b"font bytes"[..], None, Some(&b"0x00 U+0041\n"[..]))
        );
        let lat1u = source("kbd:f", Origin::Kbd("f".into()), Some("lat1u"));
        let r = request(resolve(&lat1u, &h).unwrap());
        // def.uni is not read when the frit names a unimap
        assert_eq!(
            (r.unimap.as_deref(), r.def_uni),
            (Some(&b"0x41 U+0041\n"[..]), None)
        );
        // a font not here is missing
        let e = resolve(&source("kbd:g", Origin::Kbd("g".into()), None), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Missing(w) if w.starts_with("no kbd font g in ")),
            "{e:?}"
        );
        let e = resolve(
            &source("file:/nope.psf", Origin::File("/nope.psf".into()), None),
            &h,
        )
        .unwrap_err();
        assert!(matches!(e, SourceError::Missing(_)), "{e:?}");
        // a VT that cannot give its font fails the set
        let e = resolve(&source("console:3", Origin::Console(3), None), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Failed(w) if w.contains("graphics mode")),
            "{e:?}"
        );
    }

    /// A stand-in kernel holding an 8x4 font of 256 glyphs for VT 3.
    struct Kernel;

    impl Consoles for Kernel {
        fn font(&self, vt: u16) -> Result<ConsoleFont, String> {
            if vt != 3 {
                return Err(format!("VT {vt}: no such VT"));
            }
            Ok(ConsoleFont {
                width: 8,
                height: 4,
                charcount: 256,
                data: (0..256u32 * 4).map(|i| (i / 4) as u8).collect(),
                unimap: (0x20..0x7f)
                    .map(|c| UniPair {
                        unicode: c,
                        fontpos: c,
                    })
                    .chain([UniPair {
                        unicode: 0x2588,
                        fontpos: 0xdb,
                    }])
                    .collect(),
            })
        }
    }

    #[test]
    fn the_kernels_font_parses_as_a_psf_with_its_map() {
        let t = Tree::new("console");
        let consoles = Kernel;
        let h = host(&t, &consoles);
        let request = request(resolve(&source("console:3", Origin::Console(3), None), &h).unwrap());
        let g = crate::decode::font(&request.font, None, None).expect("parses");
        assert_eq!(
            (g.width, g.height, g.count, g.format.as_str()),
            (8, 4, 256, "psf2+table")
        );
        assert_eq!(g.map.len(), 0x7f - 0x20 + 1);
        assert!(g.map.contains(&(0x2588, 0xdb)));
        assert_eq!(g.glyph(0xdb), Some(&[0xdbu8; 4][..]));
        // the kernel's answer for another VT names it
        let e = resolve(&source("console:4", Origin::Console(4), None), &h).unwrap_err();
        assert_eq!(e, SourceError::Failed("VT 4: no such VT".into()));
    }

    #[test]
    fn a_builtin_reads_as_its_glyphs() {
        let t = Tree::new("builtin");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = resolve(
            &source("builtin:unscii-8", Origin::Builtin("unscii-8".into()), None),
            &h,
        )
        .unwrap();
        let font = crate::font::Font::from_builtin("unscii-8").unwrap();
        assert_eq!(r.input, Input::Builtin(font));
    }

    // ADR-1200 D1: cp-size= hands the parser the CPI font of that height
    #[test]
    fn cp_size_hands_the_parser_the_cpi_font_of_its_height() {
        let t = Tree::new("cp-size");
        let cp = crate::decode::tests::cpi_file(&[16, 14, 8]);
        t.file(
            "kbd/consolefonts/437.cp.gz",
            &crate::decode::tests::gzip(&cp),
        );
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let mut s = source("kbd:437", Origin::Kbd("437".into()), None);
        s.cp_size = Some(14);
        let r = request(resolve(&s, &h).unwrap());
        assert_eq!(r.font, crate::decode::cpi(&cp, 14).unwrap());
        let psf = vtium_font::Psf::parse(&r.font).expect("a PSF");
        assert_eq!(psf.glyph_size(), (8, 14));
        // a PSF with cp-size= fails its source
        t.file(
            "kbd/consolefonts/f.psf",
            &crate::decode::tests::psf2(8, 16, 4, None),
        );
        let mut s = source("kbd:f", Origin::Kbd("f".into()), None);
        s.cp_size = Some(16);
        let e = resolve(&s, &h).unwrap_err();
        assert!(e.to_string().contains("cp-size= is for a CPI font"), "{e}");
    }

    // ADR-1200 D1, D14: a bdf: source is read by vitrify's own reader
    #[test]
    fn a_bdf_source_reads_as_its_glyphs() {
        let t = Tree::new("bdf");
        let path = t.file(
            "fonts/t.bdf.xz",
            &crate::decode::tests::xz(crate::bdf::tests::bdf("").as_bytes()),
        );
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = resolve(&source("bdf:", Origin::Bdf(path), None), &h).unwrap();
        let Input::Builtin(font) = r.input else {
            panic!("not glyphs: {:?}", r.input)
        };
        assert_eq!(
            (font.cell.width, font.cell.height, font.format.as_str()),
            (4, 6, "bdf+xz")
        );
        let gone = t.0.join("fonts/gone.bdf");
        let e = resolve(&source("bdf:", Origin::Bdf(gone), None), &h).unwrap_err();
        assert!(matches!(e, SourceError::Missing(_)), "{e}");
        let bad = t.file("fonts/bad.bdf", b"STARTFONT 2.1\nCHARS 1\n");
        let e = resolve(&source("bdf:", Origin::Bdf(bad), None), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Failed(w) if w.contains("CHARSET_REGISTRY")),
            "{e}"
        );
    }

    #[test]
    fn a_family_scan_past_its_entry_cap_fails() {
        let t = Tree::new("cap");
        for i in 0..4 {
            t.file(&format!("a/consolefonts/f{i}.psf"), b"x");
        }
        let dirs = [t.0.join("a")];
        assert!(find_family_capped("f3", &dirs, 4).unwrap().is_some());
        assert!(find_family_capped("F 3", &dirs, 3).is_err());
    }

    #[test]
    fn a_family_matches_a_kbd_font_with_case_and_blanks_ignored() {
        let t = Tree::new("family");
        t.file("a/consolefonts/README", b"x");
        t.file("a/consolefonts/Lat2-Terminus16.psfu.gz", b"x");
        t.file("b/consolefonts/ter-v16n.psf.zst", b"x");
        t.file("b/consolefonts/ter-v16b.psf.gz", b"x");
        let dirs = [t.0.join("a"), t.0.join("b")];
        let found = |family: &str| {
            find_family(family, &dirs)
                .unwrap()
                .map(|p| p.strip_prefix(&t.0).unwrap().display().to_string())
        };
        assert_eq!(
            found("Lat2-Terminus16").as_deref(),
            Some("a/consolefonts/Lat2-Terminus16.psfu.gz")
        );
        assert_eq!(
            found("lat2-terminus 16").as_deref(),
            Some("a/consolefonts/Lat2-Terminus16.psfu.gz")
        );
        assert_eq!(
            found("TER-V16N").as_deref(),
            Some("b/consolefonts/ter-v16n.psf.zst")
        );
        assert_eq!(found("ter-v16"), None);
        assert_eq!(found("Terminus"), None);
    }

    #[test]
    fn an_outline_family_is_found_among_installed_fonts() {
        let t = Tree::new("outline-family");
        t.file("a/TTF/JetBrainsMono-Bold.ttf", b"x");
        t.file("a/TTF/JetBrainsMono-Regular.ttf", b"x");
        t.file("a/noto/NotoColorEmoji.ttf", b"x");
        t.file("a/noto/NotoColorEmoji.txt", b"x");
        t.file("b/SymbolsNerdFontMono-Regular.TTF", b"x");
        t.file("b/deep/1/2/3/4/5/6/7/8/9/Hidden.ttf", b"x");
        t.file("b/NotoColorEmoji.ttf", b"x");
        let dirs = [t.0.join("a"), t.0.join("b")];
        let found = |family: &str| {
            find_outline_family(family, &dirs).unwrap().map(|p| {
                p.strip_prefix(fs::canonicalize(&t.0).unwrap())
                    .unwrap()
                    .display()
                    .to_string()
            })
        };
        assert_eq!(
            found("JetBrains Mono").as_deref(),
            Some("a/TTF/JetBrainsMono-Regular.ttf")
        );
        // the first directory given wins
        assert_eq!(
            found("noto color emoji").as_deref(),
            Some("a/noto/NotoColorEmoji.ttf")
        );
        assert_eq!(
            found("Symbols Nerd Font Mono").as_deref(),
            Some("b/SymbolsNerdFontMono-Regular.TTF")
        );
        // a style other than regular, a family's prefix, too deep
        assert_eq!(
            found("JetBrains Mono Bold").as_deref(),
            Some("a/TTF/JetBrainsMono-Bold.ttf")
        );
        assert_eq!(found("JetBrains"), None);
        assert_eq!(found("Hidden"), None);
    }

    #[test]
    fn an_outline_font_linked_out_of_the_font_directories_is_refused() {
        let t = Tree::new("outline-link");
        let outside = t.file("outside/Secret.ttf", b"x");
        fs::create_dir_all(t.0.join("fonts")).unwrap();
        std::os::unix::fs::symlink(&outside, t.0.join("fonts/Secret.ttf")).unwrap();
        let e = find_outline_family("Secret", &[t.0.join("fonts")]).unwrap_err();
        assert!(e.contains("leaves the font directories"), "{e}");
    }

    #[test]
    fn a_font_source_takes_an_outline_family_when_kbd_has_none() {
        let t = Tree::new("outline-resolve");
        t.file("fonts/Test-Regular.otf", b"outline bytes");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = resolve(&source("font:Test", Origin::Font("Test".into()), None), &h).unwrap();
        let Input::Outline(req) = &r.input else {
            panic!("not an outline: {:?}", r.input);
        };
        assert_eq!(
            (&req.font[..], req.family.as_str()),
            (&b"outline bytes"[..], "Test")
        );
        let e = resolve(
            &source("font:Absent", Origin::Font("Absent".into()), None),
            &h,
        )
        .unwrap_err();
        assert!(
            matches!(e, SourceError::Missing(ref why) if why.contains("no outline font")),
            "{e}"
        );
        let path = t.0.join("fonts/Test-Regular.otf");
        let r = resolve(&source("outline:", Origin::Outline(path), None), &h).unwrap();
        assert!(matches!(&r.input, Input::Outline(req) if req.family.is_empty()));
        let gone = t.0.join("fonts/Gone.ttf");
        let e = resolve(&source("outline:", Origin::Outline(gone), None), &h).unwrap_err();
        assert!(matches!(e, SourceError::Missing(_)), "{e}");
    }

    #[test]
    fn vconsole_resolves_its_font_as_a_kbd_name() {
        let t = Tree::new("vconsole");
        t.file("kbd/consolefonts/lat2-16.psfu.gz", b"lat2");
        t.file("kbd/consolefonts/ter-v16n.psf.gz", b"ter");
        t.file("kbd/unimaps/x.uni", b"0x41 U+0041\n");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let vc = source("vconsole", Origin::Vconsole, None);
        let e = resolve(&vc, &h).unwrap_err().to_string();
        assert!(e.contains("names no FONT="), "{e}");
        t.file("vconsole.conf", b"FONT=ter-v16n\n");
        assert_eq!(request(resolve(&vc, &h).unwrap()).font, b"ter");
        t.file("cmdline", b"vconsole.font=lat2-16 vconsole.font_unimap=x\n");
        let r = request(resolve(&vc, &h).unwrap());
        assert_eq!(r.font, b"lat2");
        assert_eq!(r.unimap.as_deref(), Some(&b"0x41 U+0041\n"[..]));
    }
}
