//! Finding and reading a frit's sources, as the helper's account, before
//! anything is parsed (ADR-1200 D1, D8, D14): kbd names resolved as
//! `setfont` resolves them, outline families found among the installed
//! fonts, files, vconsole.conf with the kernel's command line over it, the
//! kernel's own font, and the built-in fonts. Each source becomes the
//! bytes the parser is handed, or the glyphs vtium-config reads itself: a
//! built-in font's, and a BDF font's through [`crate::bdf`].
//!
//! The console's font helper draws only from the active fonts ([`Gate`]).
//! A font file counts only while the batch lists it, and vtium opens it
//! through the trust rule: root owns the file and every folder above it,
//! neither its group nor other users can write any of them, none of them
//! is a symbolic link, and the filesystem that holds it stays mounted. A
//! file that fails counts as missing, and its characters draw from the
//! next source. The built-in fonts are always active.
//!
//! References:
//! - ADR-1205 D4: the batch, the trust rule, and a refused file counting
//!   as missing.
//! - ADR-1204 D7: a missing font's characters draw from the next source.

use std::path::{Path, PathBuf};

use vtium_font::{PSF_BYTES_MAX, Sha256, sha256_hex};
use vtium_platform::vt::font::ConsoleFont;
use vtium_settings::cullet::{self, Check};

use crate::child::Request;
use crate::font::Font;
use crate::frit::{Hinting, Origin, Source};
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
/// default configuration names them for the system. vtium-config's
/// `--font-dir` names other directories in their place
/// (`Host::font_dirs`); no user's own font directory is searched.
pub const FONT_DIRS: [&str; 2] = ["/usr/share/fonts", "/usr/local/share/fonts"];
/// The extensions an outline font file carries, case ignored.
pub const OUTLINE_SUFFIXES: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// How deep under a font directory an outline family is looked for.
pub const FONT_DEPTH_MAX: usize = 8;

/// The first two bytes of a gzip member (RFC 1952 2.3.1).
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

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
    /// Which font files the sources may read.
    pub gate: &'a Gate,
}

/// Which font files a frit's sources may read.
#[derive(Clone, Debug)]
pub enum Gate {
    /// Any font file a frit names. vtium-config's own previews read fonts
    /// this way in vtium-config's window, and so do the tests.
    Open,
    /// The active fonts alone, as the console's font helper draws: the
    /// files the batch lists that pass the trust rule, through the gate
    /// every resource passes. Its mount table is `None`, so the helper
    /// reads the system's table at each open, and a filesystem mounted
    /// since counts as it is now.
    Active(cullet::Gate),
}

/// Why a file the batch does not list is refused, as what follows
/// "because" in [`SourceError::Refused`], and what follows a refused font
/// file's path in that reason.
pub use vtium_settings::batch::{MISSING_BECAUSE, NOT_ACTIVE};

impl Gate {
    /// Whether a search may take `path` as a candidate: any file through
    /// an open gate, and only a file the batch lists through an active
    /// one. A search never opens a file the batch does not list.
    fn lists(&self, path: &Path) -> bool {
        match self {
            Gate::Open => true,
            Gate::Active(active) => active.holds(path),
        }
    }

    /// Whether the gate reads the active fonts alone.
    fn active(&self) -> bool {
        matches!(self, Gate::Active(_))
    }

    /// Opens the font file at `path` for reading. Through an open gate it
    /// opens any regular file. Through an active gate the batch must list
    /// `path`, and the trust rule must pass on the descriptor this returns,
    /// so the file read is the file checked. A file that fails is
    /// [`SourceError::Refused`], which counts as missing.
    ///
    /// References:
    /// - ADR-1205 D4: a font draws only while the batch holds it and the
    ///   file passes the rule, checked on the file opened.
    fn open(&self, path: &Path) -> Result<std::fs::File, SourceError> {
        let Gate::Active(active) = self else {
            return open_regular(path)
                .map_err(|e| SourceError::Failed(format!("{}: {e}", path.display())));
        };
        match active.open(path) {
            Ok(opened) => Ok(opened.file),
            Err(refused) if refused.check == Check::Missing => Err(SourceError::Missing(format!(
                "{} is not on this machine",
                path.display()
            ))),
            Err(refused) if refused.check == Check::Inactive => Err(SourceError::Refused {
                path: refused.path,
                because: NOT_ACTIVE.to_owned(),
            }),
            Err(refused) => Err(SourceError::Refused {
                path: refused.path,
                because: refused.check.to_string(),
            }),
        }
    }

    /// Reads the font file at `path`, at most `limit` bytes, opened by
    /// [`Gate::open`].
    pub(crate) fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, SourceError> {
        let file = self.open(path)?;
        read_limited(file, limit)
            .map_err(|e| SourceError::Failed(format!("{}: {e}", path.display())))
    }
}

/// The kernel's VT fonts, as the console hands them to the helper; a test
/// stands in.
pub trait Consoles {
    fn font(&self, vt: u16) -> Result<ConsoleFont, String>;
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
    /// A BDF font's bytes, read here and parsed by [`crate::bdf`] when the
    /// frit loads, where the other untrusted fonts are.
    Bdf(Vec<u8>),
    /// A `.hex` font's bytes, uncompressed, read here and parsed by
    /// [`crate::hex`] when the frit loads, as a BDF font's are.
    Hex(Vec<u8>),
    /// A bitmap font's glyphs at its own cell, already read: a built-in
    /// font's.
    Bitmap(Font),
}

/// Opens `path` for reading: once, without blocking on a FIFO and never as
/// a controlling terminal, and refused unless that descriptor is a regular
/// file. The error leaves the path to the caller.
pub fn open_regular(path: &Path) -> Result<std::fs::File, String> {
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
    Ok(file)
}

/// Reads `path`, at most `limit` bytes, opened by [`open_regular`]. The
/// error leaves the path to the caller.
pub(crate) fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    read_limited(open_regular(path)?, limit)
}

/// Reads `file` to its end, at most `limit` bytes. The error leaves the
/// path to the caller.
fn read_limited(file: std::fs::File, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
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
    /// The font file is here, and the console's font helper may not draw
    /// from it, so it counts as missing: the batch does not list it, or it
    /// fails the trust rule.
    Refused {
        /// The font file.
        path: PathBuf,
        /// Why, as what follows "because", ending with a period.
        because: String,
    },
    /// The source is there and cannot be read.
    Failed(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Missing(why) | SourceError::Failed(why) => f.write_str(why),
            SourceError::Refused { path, because } => {
                write!(f, "{}{MISSING_BECAUSE}{because}", path.display())
            }
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
    find_family_capped(family, data_dirs, FAMILY_ENTRIES_MAX, &Gate::Open)
}

/// [`find_family`], taking only the files `gate` lists.
fn find_family_capped(
    family: &str,
    data_dirs: &[PathBuf],
    cap: usize,
    gate: &Gate,
) -> Result<Option<PathBuf>, String> {
    if let Some(p) =
        find(family, data_dirs, "consolefonts", &FONT_SUFFIXES).filter(|p| gate.lists(p))
    {
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
                return Err(format!("{} has more than {cap} entries", dir.display()));
            }
            names.push(entry.path());
        }
        names.sort();
        let found = names.into_iter().find(|p| {
            is_file(p)
                && gate.lists(p)
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

/// The most outline font files one `font:` search opens to read their
/// family (ADR-1500 D8); a search that would open more fails.
pub const FAMILY_FILES_MAX: usize = 4096;

/// Finds an outline font by family (`font:`) under `dirs`: a TrueType or
/// OpenType file or collection with a face of that family, as its name
/// table stores it, typographic or legacy, compared with case, blanks, `-`
/// and `_` ignored (ADR-1500 D8). Reading a name table costs a file open,
/// so the search takes two passes. It first takes the files whose name,
/// less its extension, is the family, or the family and `Regular`, the
/// plain name first, then in the order the directories are given and by
/// path, and returns the first whose name table confirms the family. Only
/// when none does, it opens every other file in that same order and
/// returns the first with a Regular face of the family, one whose
/// subfamily is `Regular`, else the first with any face of it. A face is
/// of the family by [`crate::names::matches`], the predicate the
/// rasteriser's choice of a face uses too. A search that would open more
/// than [`FAMILY_FILES_MAX`] files fails, even when it has found a face of
/// the family in another style, and its message names that file. A directory past `FAMILY_ENTRIES_MAX`
/// entries fails the search; directories deeper than `FONT_DEPTH_MAX` and
/// links to directories are not entered. A file is opened only if, its
/// links resolved, it stays under `dirs`.
pub fn find_outline_family(family: &str, dirs: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    find_outline_family_capped(family, dirs, FAMILY_FILES_MAX, &Gate::Open, &mut None)
}

/// [`find_outline_family`] among the files `gate` lists. A file the gate
/// does not list is never opened, and counts toward no bound. Through an
/// active gate, each candidate is opened through the gate, with no link
/// followed, and its faces are read from the file the gate checked. A
/// candidate the gate refuses is skipped, so it cannot hide a later one
/// that passes. The first refusal of a file named for the family goes into
/// `refused` for the message. A refusal of a file the search opens to read
/// its families says nothing of the family, so the search drops it.
fn find_outline_family_capped(
    family: &str,
    dirs: &[PathBuf],
    cap: usize,
    gate: &Gate,
    refused: &mut Option<SourceError>,
) -> Result<Option<PathBuf>, String> {
    let key = outline_key(family);
    let regular = format!("{key}regular");
    let files = outline_files(dirs, gate)?;
    let over = |first: Option<&PathBuf>| match first {
        None => format!(
            "no font of family {family} among the first {cap} outline font files, \
             the most one search opens"
        ),
        Some(path) => format!(
            "no Regular face of family {family} among the first {cap} outline font files, \
             the most one search opens; {} holds a face of it in another style",
            path.display()
        ),
    };
    let roots: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| std::fs::canonicalize(d).ok())
        .collect();
    let mut opened = 0usize;
    // what the search learns of the candidate `path`: its faces, with the
    // path the search returns for it, or why it is skipped. A refusal is
    // kept, to report when no file matches, only where `named` holds: the
    // file's name names the family. A refusal of any other file comes
    // before its faces are read, so it says nothing of the family.
    let mut look = |path: &PathBuf, named: bool| -> Look {
        let real = if gate.active() {
            path.clone()
        } else {
            let Some(real) = std::fs::canonicalize(path)
                .ok()
                .filter(|real| roots.iter().any(|r| real.starts_with(r)))
            else {
                return Look::Outside;
            };
            real
        };
        if opened >= cap {
            return Look::Over;
        }
        opened += 1;
        let file = if gate.active() {
            match gate.open(path) {
                Ok(file) => Some(file),
                Err(e) => {
                    if named {
                        refused.get_or_insert(e);
                    }
                    return Look::Refused;
                }
            }
        } else {
            open_regular(&real).ok()
        };
        let faces = file
            .and_then(|f| crate::names::faces(&f).ok())
            .unwrap_or_default();
        Look::Faces(real, faces)
    };
    // whether a face is of the family, and whether one such is Regular
    let declares = |faces: &[crate::names::Face]| {
        faces
            .iter()
            .filter(|f| crate::names::matches(f, family))
            .map(|f| f.regular)
            .max()
    };
    // first pass: the files named for the family
    let mut named: Vec<(u8, usize, &PathBuf)> = files
        .iter()
        .filter_map(|(order, path)| {
            let stem = outline_key(path.file_stem()?.to_str()?);
            let rank = if stem == key {
                0
            } else if stem == regular {
                1
            } else {
                return None;
            };
            Some((rank, *order, path))
        })
        .collect();
    named.sort();
    for (_, _, path) in &named {
        match look(path, true) {
            Look::Faces(found, faces) if declares(&faces).is_some() => return Ok(Some(found)),
            Look::Faces(..) | Look::Refused => {}
            Look::Over => return Err(over(None)),
            Look::Outside => {
                return Err(format!(
                    "{} is a link that leads out of the font directories",
                    path.display()
                ));
            }
        }
    }
    // second pass: every other file, by the family its name table stores
    let mut first = None;
    for (_, path) in &files {
        if named.iter().any(|(_, _, p)| p == &path) {
            continue;
        }
        match look(path, false) {
            Look::Faces(found, faces) => match declares(&faces) {
                Some(true) => return Ok(Some(found)),
                Some(false) if first.is_none() => first = Some(found),
                _ => {}
            },
            Look::Over => return Err(over(first.as_ref())),
            Look::Outside | Look::Refused => {}
        }
    }
    Ok(first)
}

/// What an outline family search learns of one candidate file.
enum Look {
    /// Its faces, and the path the search returns for it: through an
    /// active gate the path the batch lists, else the file with its links
    /// resolved.
    Faces(PathBuf, Vec<crate::names::Face>),
    /// The active gate refused it.
    Refused,
    /// Its links lead out of the font folders.
    Outside,
    /// The search has opened as many files as it may.
    Over,
}

/// The outline font file under `dirs` whose file name, less its extension,
/// is `family`, or `family` and `Regular`, with case, blanks, `-` and `_`
/// ignored: the first, as [`find_outline_family`]'s first pass orders
/// them. Only the names in the folders are read, and no file is opened.
fn outline_named(family: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let key = outline_key(family);
    let regular = format!("{key}regular");
    let files = outline_files(dirs, &Gate::Open).ok()?;
    let mut named: Vec<(u8, usize, PathBuf)> = files
        .into_iter()
        .filter_map(|(order, path)| {
            let stem = outline_key(path.file_stem()?.to_str()?);
            let rank = if stem == key {
                0
            } else if stem == regular {
                1
            } else {
                return None;
            };
            Some((rank, order, path))
        })
        .collect();
    named.sort();
    named.into_iter().next().map(|(_, _, path)| path)
}

/// The family of an installed outline font under `dirs` that covers the
/// most of `letters`, which the font set leaves to the squeeze (ADR-1401
/// D7): a face's first family name that prints as itself (`printable`),
/// among its Regular faces first; a face with no such name is left out. A face
/// covers a letter its character map gives a glyph for. The search opens
/// at most [`FAMILY_FILES_MAX`] files, in the order of
/// [`find_outline_family`], and a file holds at most
/// [`OUTLINE_BYTES_MAX`] bytes; files past either bound are left out.
/// `None` when no face covers any of `letters`.
pub fn font_covering(letters: &[char], dirs: &[PathBuf]) -> Option<String> {
    font_covering_in(letters, dirs, &Gate::Open)
}

/// [`font_covering`] among the files `gate` lets the sources read, so the
/// family it names is one a `source=font:` line can draw.
pub fn font_covering_in(letters: &[char], dirs: &[PathBuf], gate: &Gate) -> Option<String> {
    if letters.is_empty() {
        return None;
    }
    let files = outline_files(dirs, gate).ok()?;
    let mut best: Option<(usize, bool, String)> = None;
    for (_, path) in files.iter().take(FAMILY_FILES_MAX) {
        let Ok(data) = gate.read(path, OUTLINE_BYTES_MAX) else {
            continue;
        };
        let Ok(faces) = crate::names::faces(data.as_slice()) else {
            continue;
        };
        let Ok(fonts) = crate::names::swash_fonts(&data) else {
            continue;
        };
        for face in faces {
            let Some(font) = fonts.get(face.index as usize) else {
                continue;
            };
            // The font file is user-writable, so its family name can hold
            // ESC, a newline or a bidirectional override. Such a name would
            // print control sequences into check-config's output and break
            // the source= line that the note suggests, so the search skips
            // the name.
            let Some(family) = face.families.iter().find(|f| printable(f)) else {
                continue;
            };
            let map = font.charmap();
            let covered = letters.iter().filter(|&&c| map.map(c) != 0).count();
            let better = best
                .as_ref()
                .is_none_or(|(n, regular, _)| (covered, face.regular) > (*n, *regular));
            if covered > 0 && better {
                best = Some((covered, face.regular, family.clone()));
            }
        }
    }
    best.map(|(_, _, family)| family)
}

/// Whether a family name prints as itself on a terminal and in a
/// `source=font:` line: not empty, and with no control character (Unicode's
/// Cc), and no character that turns text around or hides (the bidirectional
/// overrides, isolates and marks, zero-width characters and the byte order
/// mark).
fn printable(name: &str) -> bool {
    let hidden = |c: char| {
        matches!(
            c,
            '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'
        )
    };
    !name.trim().is_empty() && !name.chars().any(|c| c.is_control() || hidden(c))
}

/// Every TrueType or OpenType file or collection under `dirs` that `gate`
/// lists, with the place of its directory in `dirs`, in that order and
/// then by path.
fn outline_files(dirs: &[PathBuf], gate: &Gate) -> Result<Vec<(usize, PathBuf)>, String> {
    let mut files = Vec::new();
    for (order, root) in dirs.iter().enumerate() {
        let mut pending = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for (n, entry) in entries.flatten().enumerate() {
                if n >= FAMILY_ENTRIES_MAX {
                    return Err(format!(
                        "{} has more than {FAMILY_ENTRIES_MAX} entries",
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
                let outline = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|ext| {
                        OUTLINE_SUFFIXES.iter().any(|s| s.eq_ignore_ascii_case(ext))
                    });
                if outline && gate.lists(&path) && is_file(&path) {
                    files.push((order, path));
                }
            }
        }
    }
    files.sort();
    Ok(files)
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
            "{} is a link that leads out of the kbd data directories",
            path.display()
        ))
    }
}

/// Why a `font:` source found no font of `family`, whose kbd console fonts
/// were looked for in `kbd`. Through an active gate the search took the
/// active fonts alone, so vtium looks again in the same folders for a font
/// file named for the family, by its file name alone, as a kbd console font
/// is found. A file found that
/// way is refused as not active, and the refusal names it, so the person
/// learns which file to activate. That second look opens no file: a font
/// the batch does not list is never parsed, so a file anyone could have
/// written never reaches the parser, even in the helper's sandbox.
///
/// References:
/// - ADR-1205 D4: a font the batch does not hold counts as missing, and
///   vtium names the file and the check it failed.
fn family_missing(family: &str, host: &Host<'_>, kbd: &str) -> SourceError {
    if host.gate.active() {
        let inactive = find_family(family, &host.kbd_dirs)
            .ok()
            .flatten()
            .or_else(|| outline_named(family, &host.font_dirs));
        if let Some(path) = inactive {
            return SourceError::Refused {
                path,
                because: NOT_ACTIVE.to_owned(),
            };
        }
    }
    let fonts: Vec<String> = host
        .font_dirs
        .iter()
        .map(|d| d.display().to_string())
        .collect();
    SourceError::Missing(format!(
        "no font of family {family}: no kbd console font of that name in {kbd}, \
         and no outline font of that family in {}",
        fonts.join(", ")
    ))
}

/// Reads `source` for the parser.
pub fn resolve(source: &Source, host: &Host<'_>) -> Result<Input, SourceError> {
    read_source(source, host, false).map(|(input, _)| input)
}

/// [`resolve`], with the SHA-256 of the file that `source` names when its
/// `sha256=` line asks for one, as lowercase hexadecimal digits. The hash
/// is of the bytes this read takes from the file, as the file is stored,
/// so the font that is parsed is the font that is checked, and the file
/// is read once. `None` when the source asks for no hash, or names no
/// file of its own: a `font:`, `kbd:` or `vconsole` source, a console's
/// font, or a built-in font.
///
/// References:
/// - ADR-1204 D7: `sha256=` is the hash of the file a source names by its
///   path.
pub fn resolve_hashed(
    source: &Source,
    host: &Host<'_>,
) -> Result<(Input, Option<String>), SourceError> {
    read_source(source, host, source.needs.sha256.is_some())
}

/// [`resolve`], which also hashes the file a path source names when
/// `hash` is true.
fn read_source(
    source: &Source,
    host: &Host<'_>,
    hash: bool,
) -> Result<(Input, Option<String>), SourceError> {
    let limit = u64::from(PSF_BYTES_MAX);
    let digest = |bytes: &[u8]| hash.then(|| sha256_hex(&Sha256::digest(bytes)));
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
    let gate = host.gate;
    // a kbd font is found as setfont finds it; an active gate then opens
    // the file found, never the file a link there leads to, and refuses a
    // link
    let find_font = |name: &str| -> Result<PathBuf, SourceError> {
        let path = find(name, &host.kbd_dirs, "consolefonts", &FONT_SUFFIXES)
            .ok_or_else(|| SourceError::Missing(format!("no kbd font {name} in {}", dirs())))?;
        if gate.active() {
            return Ok(path);
        }
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
        |path: &Path| SourceError::Missing(format!("{} is not on this machine", path.display()));
    // the hash of a file: source's file, before cp-size= takes one height
    // out of it
    let mut found = None;
    let (font, unimap_name) = match &source.origin {
        Origin::Builtin(name) => return Ok((Input::Bitmap(Font::from_builtin(name)?), None)),
        Origin::Bdf(path) if !path.exists() => return Err(not_here(path)),
        Origin::Bdf(path) => {
            let bytes = gate.read(path, limit)?;
            let found = digest(&bytes);
            return Ok((Input::Bdf(bytes), found));
        }
        Origin::Hex(path) if !path.exists() => return Err(not_here(path)),
        Origin::Hex(path) => {
            let bytes = gate.read(path, limit)?;
            // a .hex font is read as it is installed, uncompressed
            if bytes.starts_with(&GZIP_MAGIC) {
                return Err(SourceError::Failed(format!(
                    "{} is gzipped. vtium reads a .hex font uncompressed, so decompress the file with gunzip and name the file that gives.",
                    path.display()
                )));
            }
            let found = digest(&bytes);
            return Ok((Input::Hex(bytes), found));
        }
        Origin::Kbd(name) => (gate.read(&find_font(name)?, limit)?, source.unimap.clone()),
        Origin::Font(family) => {
            match find_family_capped(family, &host.kbd_dirs, FAMILY_ENTRIES_MAX, gate)? {
                Some(path) => {
                    let path = if gate.active() {
                        path
                    } else {
                        contained(&path, &host.kbd_dirs, "consolefonts")?
                    };
                    (gate.read(&path, limit)?, source.unimap.clone())
                }
                None => {
                    let mut refused = None;
                    let found = find_outline_family_capped(
                        family,
                        &host.font_dirs,
                        FAMILY_FILES_MAX,
                        gate,
                        &mut refused,
                    )?;
                    let Some(path) = found else {
                        return Err(
                            refused.unwrap_or_else(|| family_missing(family, host, &dirs()))
                        );
                    };
                    if source.cp_size.is_some() {
                        return Err(SourceError::Failed(format!(
                            "cp-size= is for a CPI font, and {} is an outline font",
                            path.display()
                        )));
                    }
                    return Ok((outline(gate.read(&path, OUTLINE_BYTES_MAX)?, family), None));
                }
            }
        }
        Origin::Outline(path) if !path.exists() => return Err(not_here(path)),
        Origin::Outline(path) => {
            let bytes = gate.read(path, OUTLINE_BYTES_MAX)?;
            let found = digest(&bytes);
            return Ok((outline(bytes, ""), found));
        }
        Origin::File(path) if !path.exists() => return Err(not_here(path)),
        Origin::File(path) => {
            let bytes = gate.read(path, limit)?;
            found = digest(&bytes);
            (bytes, source.unimap.clone())
        }
        Origin::Vconsole => {
            let conf = match read_bounded(&host.vconsole_conf, CONFIG_BYTES_MAX) {
                Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                Err(_) if !host.vconsole_conf.exists() => String::new(),
                Err(e) => return Err(e.into()),
            };
            let cmdline = read_bounded(&host.kernel_cmdline, CONFIG_BYTES_MAX)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let named = vtium_settings::batch::vconsole(&conf, &cmdline);
            let font = named.font.ok_or_else(|| {
                format!(
                    "{} names no FONT= and the kernel command line no vconsole.font=, \
                     so the console keeps the kernel's font: use console:VT",
                    host.vconsole_conf.display()
                )
            })?;
            (
                gate.read(&find_font(&font)?, limit)?,
                source.unimap.clone().or(named.unimap),
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
    Ok((
        Input::Font(Request {
            font,
            unimap,
            def_uni,
        }),
        found,
    ))
}

/// A smaller size of a source's font as [`smaller`] reads it: the line and
/// the spelling of the `script-source=` that names it, `None` for a CPI
/// source's other height, and the font read for the parser.
pub type Smaller = (Option<(usize, String)>, Result<Input, SourceError>);

/// The smaller sizes of `source`'s font that its superscript and
/// subscript glyphs may come from (ADR-1202 D3, D12), read for the parser,
/// each with the spelling and the line its messages name, in the order
/// the engine names them: a CPI source's other heights, each read as
/// `cp-size=` reads one, then each font `script-source=` names, which
/// takes the source's `unimap=` (the parser keeps the font's own Unicode
/// table where it has one). A CPI height the file does not hold is left
/// out. A `script-source=` font comes with its line and its spelling, and
/// a CPI height with none. Each entry's error says why its font cannot be
/// read.
pub fn smaller(source: &Source, host: &Host<'_>) -> Vec<Smaller> {
    let mut out = Vec::new();
    if let Some(own) = source.cp_size {
        for height in crate::frit::CP_SIZES.into_iter().filter(|&h| h != own) {
            let other = Source {
                cp_size: Some(height),
                script_sources: Vec::new(),
                strikes: Vec::new(),
                ..source.clone()
            };
            if let Ok(input) = resolve(&other, host) {
                out.push((None, Ok(input)));
            }
        }
    }
    for named in &source.script_sources {
        let small = Source {
            spelling: named.spelling.clone(),
            origin: named.origin.clone(),
            unimap: source.unimap.clone(),
            cover: Vec::new(),
            cp_size: None,
            script_sources: Vec::new(),
            strikes: Vec::new(),
            style: source.style,
            fits: Vec::new(),
            layer: source.layer.clone(),
            line: named.line,
            needs: crate::frit::Needs::default(),
        };
        out.push((
            Some((named.line, named.spelling.clone())),
            resolve(&small, host),
        ));
    }
    out
}

/// An outline font's bytes as the parser's input: the family the parser
/// is to find in it, the cells and coverage filled in once the cell is
/// known.
fn outline(font: Vec<u8>, family: &str) -> Input {
    Input::Outline(OutlineRequest {
        cluster_width: Default::default(),
        font: font.into(),
        family: family.to_owned(),
        cells: Vec::new(),
        cover: Vec::new(),
        clusters: Vec::new(),
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::default(),
        synth: vtium_font::Style::default(),
        script: vtium_font::Script::Baseline,
        fit: false,
        double_width: false,
    })
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
    use vtium_settings::batch::Batch;
    use vtium_settings::cullet::Rule;
    use vtium_settings::mounts::Mounts;

    use super::*;

    /// A directory of a test's own, removed when the test drops it.
    struct Tree(PathBuf);

    impl Tree {
        /// A fresh tree for `test`, named with the test process's id, so
        /// two test runs at once never remove each other's tree.
        fn new(test: &str) -> Tree {
            let dir = std::env::temp_dir().join(format!(
                "vtium-config-sources-{test}-{}",
                std::process::id()
            ));
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

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
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
            gate: &Gate::Open,
        }
    }

    fn source(spelling: &str, origin: Origin, unimap: Option<&str>) -> Source {
        Source {
            spelling: spelling.into(),
            origin,
            unimap: unimap.map(Into::into),
            cover: Vec::new(),
            cp_size: None,
            script_sources: Vec::new(),
            strikes: Vec::new(),
            style: vtium_font::Style::default(),
            fits: Vec::new(),
            layer: None,
            line: 1,
            needs: crate::frit::Needs::default(),
        }
    }

    /// The request a source resolves to.
    fn request(r: Input) -> Request {
        match r {
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
                return Err(format!("there is no VT {vt}"));
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
        assert_eq!(e, SourceError::Failed("there is no VT 4".into()));
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
        assert_eq!(r, Input::Bitmap(font));
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

    // ADR-1200 D1, D14: a bdf: source is read by vtium-config's own reader
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
        let Input::Bdf(bytes) = r else {
            panic!("not a BDF font's bytes: {r:?}")
        };
        let font = crate::bdf::font(&bytes).unwrap();
        assert_eq!(
            (font.cell.width, font.cell.height, font.format.as_str()),
            (4, 6, "bdf+xz")
        );
        let gone = t.0.join("fonts/gone.bdf");
        let e = resolve(&source("bdf:", Origin::Bdf(gone), None), &h).unwrap_err();
        assert!(matches!(e, SourceError::Missing(_)), "{e}");
        let bad = t.file("fonts/bad.bdf", b"STARTFONT 2.1\nCHARS 1\n");
        // reading the bytes does not parse them: the parse is the
        // sandboxed side's
        let r = resolve(&source("bdf:", Origin::Bdf(bad), None), &h).unwrap();
        assert!(matches!(r, Input::Bdf(ref b) if b.starts_with(b"STARTFONT")));
    }

    // A source with a sha256= line gets the SHA-256 of the bytes its read
    // took from the file, as the file is stored, for each source kind
    // that names a file. A source with no sha256= line, and a source that
    // names no file of its own, gets no hash.
    //
    // References:
    // - ADR-1204 D7: sha256= is the hash of the file a source names.
    #[test]
    fn a_source_with_a_sha256_line_is_hashed_from_the_bytes_it_reads() {
        let t = Tree::new("hashed");
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let hash_of = |bytes: &[u8]| sha256_hex(&Sha256::digest(bytes));
        let bdf = crate::decode::tests::xz(crate::bdf::tests::bdf("").as_bytes());
        let hex = format!("0041:{}\n", "00".repeat(16)).into_bytes();
        let psf = crate::decode::tests::psf2(8, 16, 4, None);
        let ttf = vtium_testfont::ttf::test_mono().build();
        let cases = [
            (Origin::Bdf(t.file("fonts/t.bdf.xz", &bdf)), hash_of(&bdf)),
            (Origin::Hex(t.file("fonts/u.hex", &hex)), hash_of(&hex)),
            (Origin::File(t.file("fonts/f.psf", &psf)), hash_of(&psf)),
            (Origin::Outline(t.file("fonts/m.ttf", &ttf)), hash_of(&ttf)),
        ];
        for (origin, hash) in cases {
            let mut s = source("x", origin.clone(), None);
            assert_eq!(resolve_hashed(&s, &h).unwrap().1, None, "{origin:?}");
            s.needs.sha256 = Some("0".repeat(64));
            assert_eq!(resolve_hashed(&s, &h).unwrap().1, Some(hash), "{origin:?}");
        }
        t.file("kbd/consolefonts/k.psf", &psf);
        let mut s = source("kbd:k", Origin::Kbd("k".into()), None);
        s.needs.sha256 = Some("0".repeat(64));
        assert_eq!(resolve_hashed(&s, &h).unwrap().1, None);
    }

    // A hex: source reads its file's bytes as they are, for the sandboxed
    // side to parse. A file that is not here is missing, a gzipped file is
    // refused, because vtium reads a .hex font uncompressed, and a file
    // past the 16 MiB that bounds every bitmap file is refused.
    //
    // References:
    // - ADR-1204 D5: the hex: source, uncompressed, up to 16 MiB.
    #[test]
    fn a_hex_source_reads_its_file_uncompressed() {
        let t = Tree::new("hex");
        let text = format!("0041:{}\n", "00".repeat(16));
        let path = t.file("fonts/u.hex", text.as_bytes());
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let hex = |p: PathBuf| source("hex:", Origin::Hex(p), None);
        let r = resolve(&hex(path), &h).unwrap();
        assert_eq!(r, Input::Hex(text.clone().into_bytes()));
        let gone = t.0.join("fonts/gone.hex");
        let e = resolve(&hex(gone), &h).unwrap_err();
        assert!(matches!(e, SourceError::Missing(_)), "{e}");
        let gz = t.file(
            "fonts/u.hex.gz",
            &crate::decode::tests::gzip(text.as_bytes()),
        );
        let e = resolve(&hex(gz), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Failed(why) if why.contains("is gzipped") && why.contains("uncompressed")),
            "{e}"
        );
        let big = t.file(
            "fonts/big.hex",
            &vec![b'0'; usize::try_from(PSF_BYTES_MAX).unwrap() + 1],
        );
        let e = resolve(&hex(big), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Failed(why) if why.contains("larger than")),
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
        assert!(
            find_family_capped("f3", &dirs, 4, &Gate::Open)
                .unwrap()
                .is_some()
        );
        assert!(find_family_capped("F 3", &dirs, 3, &Gate::Open).is_err());
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

    /// An outline font file holding one face of `family` in `style`.
    fn outline_font(family: &str, style: &str) -> Vec<u8> {
        crate::names::sfnt(&[&[(1, family), (2, style)]])
    }

    fn found_outline(t: &Tree, family: &str, dirs: &[&str]) -> Result<Option<String>, String> {
        let dirs: Vec<PathBuf> = dirs.iter().map(|d| t.0.join(d)).collect();
        let root = fs::canonicalize(&t.0).unwrap();
        find_outline_family(family, &dirs)
            .map(|p| p.map(|p| p.strip_prefix(&root).unwrap().display().to_string()))
    }

    #[test]
    fn an_outline_family_is_found_by_its_file_name_and_confirmed_by_its_name_table() {
        let t = Tree::new("outline-family");
        t.file(
            "a/TTF/JetBrainsMono-Bold.ttf",
            &outline_font("JetBrains Mono Bold", "Regular"),
        );
        t.file(
            "a/TTF/JetBrainsMono-Regular.ttf",
            &outline_font("JetBrains Mono", "Regular"),
        );
        t.file(
            "a/noto/NotoColorEmoji.ttf",
            &outline_font("Noto Color Emoji", "Regular"),
        );
        t.file("a/noto/NotoColorEmoji.txt", b"x");
        t.file(
            "b/SymbolsNerdFontMono-Regular.TTF",
            &outline_font("Symbols Nerd Font Mono", "Regular"),
        );
        t.file(
            "b/deep/1/2/3/4/5/6/7/8/9/Hidden.ttf",
            &outline_font("Hidden", "Regular"),
        );
        t.file(
            "b/NotoColorEmoji.ttf",
            &outline_font("Noto Color Emoji", "Regular"),
        );
        let found = |family: &str| found_outline(&t, family, &["a", "b"]).unwrap();
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
        assert_eq!(
            found("JetBrains Mono Bold").as_deref(),
            Some("a/TTF/JetBrainsMono-Bold.ttf")
        );
        // a family's prefix, too deep
        assert_eq!(found("JetBrains"), None);
        assert_eq!(found("Hidden"), None);
    }

    // ADR-1500 D8: the family is the one the font stores, so a file of
    // another name is found, a variable font's included, and a file named
    // for a family it does not hold is not taken.
    #[test]
    fn an_outline_family_is_found_by_the_family_its_name_table_stores() {
        let t = Tree::new("outline-name-table");
        t.file(
            "a/JetBrainsMono[wght].ttf",
            &outline_font("JetBrains Mono", "Regular"),
        );
        t.file(
            "a/Misnamed-Regular.ttf",
            &outline_font("Something Else", "Regular"),
        );
        t.file(
            "a/x/terminus.ttf",
            &outline_font("Terminus (TTF)", "Regular"),
        );
        let found = |family: &str| found_outline(&t, family, &["a"]).unwrap();
        assert_eq!(
            found("JetBrains Mono").as_deref(),
            Some("a/JetBrainsMono[wght].ttf")
        );
        assert_eq!(
            found("jetbrainsmono").as_deref(),
            Some("a/JetBrainsMono[wght].ttf")
        );
        assert_eq!(found("Misnamed"), None);
        assert_eq!(
            found("Something Else").as_deref(),
            Some("a/Misnamed-Regular.ttf")
        );
        assert_eq!(found("Terminus (TTF)").as_deref(), Some("a/x/terminus.ttf"));
    }

    // ADR-1500 D8: of several files of a family, the Regular face wins,
    // then the directory listed first, then the path that sorts first.
    #[test]
    fn of_several_files_of_a_family_the_regular_face_wins_then_the_directory_then_the_path() {
        let t = Tree::new("outline-choice");
        let heavy = crate::names::sfnt(&[&[
            (1, "Fam ExtraBold"),
            (2, "Regular"),
            (16, "Fam"),
            (17, "ExtraBold"),
        ]]);
        t.file("a/fam-extrabold.ttf", &heavy);
        t.file("a/fam-italic.otf", &outline_font("Fam", "Italic"));
        let found = |family: &str, dirs: &[&str]| found_outline(&t, family, dirs).unwrap();
        // no Regular face: the first by directory and path
        assert_eq!(found("Fam", &["a"]).as_deref(), Some("a/fam-extrabold.ttf"));
        t.file("b/z.ttf", &outline_font("Fam", "Regular"));
        assert_eq!(found("Fam", &["a", "b"]).as_deref(), Some("b/z.ttf"));
        t.file("a/y.ttf", &outline_font("Fam", "Regular"));
        t.file("a/sub/a.ttf", &outline_font("Fam", "Regular"));
        assert_eq!(found("Fam", &["b", "a"]).as_deref(), Some("b/z.ttf"));
        assert_eq!(found("Fam", &["a", "b"]).as_deref(), Some("a/sub/a.ttf"));
        // the heavy face's legacy family names it alone
        assert_eq!(
            found("Fam ExtraBold", &["a", "b"]).as_deref(),
            Some("a/fam-extrabold.ttf")
        );
    }

    #[test]
    fn a_family_in_a_collection_is_found() {
        let t = Tree::new("outline-collection");
        let both = crate::names::sfnt(&[
            &[(1, "Alpha"), (2, "Regular")],
            &[(1, "Beta"), (2, "Regular")],
        ]);
        t.file("a/pair.ttc", &both);
        assert_eq!(
            found_outline(&t, "Beta", &["a"]).unwrap().as_deref(),
            Some("a/pair.ttc")
        );
    }

    // ADR-1500 D8: a search opens at most its cap of files, and one that
    // reaches the cap fails, naming the cap and the family.
    #[test]
    fn a_family_search_past_its_file_cap_fails() {
        let t = Tree::new("outline-cap");
        for i in 0..3 {
            t.file(
                &format!("a/f{i}.ttf"),
                &outline_font(&format!("F{i}"), "Regular"),
            );
        }
        let dirs = [t.0.join("a")];
        assert!(
            find_outline_family_capped("Absent", &dirs, 3, &Gate::Open, &mut None)
                .unwrap()
                .is_none()
        );
        let e = find_outline_family_capped("Absent", &dirs, 2, &Gate::Open, &mut None).unwrap_err();
        assert!(e.contains("Absent") && e.contains('2'), "{e}");
        // a match by file name opens one file
        assert!(
            find_outline_family_capped("F2", &dirs, 1, &Gate::Open, &mut None)
                .unwrap()
                .is_some()
        );
        assert_eq!(FAMILY_FILES_MAX, 4096);
    }

    // ADR-1500 D8: a search that reaches its cap fails even with a face
    // of the family in another style in hand, and names that file.
    #[test]
    fn a_family_search_at_its_cap_names_a_face_of_another_style_it_found() {
        let t = Tree::new("outline-cap-other-style");
        t.file("a/a.ttf", &outline_font("Fam", "Bold"));
        t.file("a/b.ttf", &outline_font("Other", "Regular"));
        t.file("a/c.ttf", &outline_font("Fam", "Regular"));
        let dirs = [t.0.join("a")];
        let e = find_outline_family_capped("Fam", &dirs, 2, &Gate::Open, &mut None).unwrap_err();
        assert!(
            e.contains("no Regular face of family Fam") && e.contains("a.ttf"),
            "{e}"
        );
        assert!(
            find_outline_family_capped("Fam", &dirs, 3, &Gate::Open, &mut None)
                .unwrap()
                .unwrap()
                .ends_with("c.ttf")
        );
    }

    // The search and the rasteriser decide by one predicate, so a family
    // stored only in another language is found and then taken.
    #[test]
    fn a_family_in_another_language_is_found_and_taken() {
        let t = Tree::new("outline-localized");
        let font = crate::names::sfnt_in(&[&[(1, 0x0804, "文鼎测试楷"), (2, 0x0804, "Regular")]]);
        t.file("a/gkai00mp.ttf", &font);
        assert_eq!(
            found_outline(&t, "文鼎测试楷", &["a"]).unwrap().as_deref(),
            Some("a/gkai00mp.ttf")
        );
    }

    // A FIFO under the font directories, named like a font, is neither
    // opened as a font nor waited on.
    #[test]
    fn a_fifo_named_like_a_font_does_not_hold_up_the_search() {
        let t = Tree::new("outline-fifo");
        fs::create_dir_all(t.0.join("a")).unwrap();
        let fifo = t.0.join("a/Fam-Regular.ttf");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o644),
            0,
        )
        .unwrap();
        t.file("a/real.ttf", &outline_font("Fam", "Regular"));
        let (send, got) = std::sync::mpsc::channel();
        let dirs = [t.0.join("a")];
        std::thread::spawn(move || {
            let _ = send.send((
                find_outline_family("Fam", &dirs).map(|p| p.map(|p| p.ends_with("real.ttf"))),
                open_regular(&fifo).map(|_| ()),
            ));
        });
        // bounded only so a hang fails the test instead of stalling it
        let (search, opened) = got
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the search or the open waited on the FIFO");
        assert_eq!(search, Ok(Some(true)));
        assert_eq!(opened, Err("not a regular file".to_owned()));
    }

    #[test]
    fn an_outline_font_linked_out_of_the_font_directories_is_refused() {
        let t = Tree::new("outline-link");
        let outside = t.file("outside/Secret.ttf", &outline_font("Secret", "Regular"));
        fs::create_dir_all(t.0.join("fonts")).unwrap();
        std::os::unix::fs::symlink(&outside, t.0.join("fonts/Secret.ttf")).unwrap();
        let e = find_outline_family("Secret", &[t.0.join("fonts")]).unwrap_err();
        assert!(e.contains("leads out of the font directories"), "{e}");
        // a link of another name is not opened to read its family
        let t = Tree::new("outline-link-other");
        let outside = t.file("outside/Other.ttf", &outline_font("Secret", "Regular"));
        fs::create_dir_all(t.0.join("fonts")).unwrap();
        std::os::unix::fs::symlink(&outside, t.0.join("fonts/link.ttf")).unwrap();
        assert_eq!(
            find_outline_family("Secret", &[t.0.join("fonts")]).unwrap(),
            None
        );
    }

    #[test]
    fn a_font_source_takes_an_outline_family_when_kbd_has_none() {
        let t = Tree::new("outline-resolve");
        let font = outline_font("Test", "Regular");
        t.file("fonts/Test-Regular.otf", &font);
        let consoles = Graphics;
        let h = host(&t, &consoles);
        let r = resolve(&source("font:Test", Origin::Font("Test".into()), None), &h).unwrap();
        let Input::Outline(req) = &r else {
            panic!("not an outline: {r:?}");
        };
        assert_eq!((&req.font[..], req.family.as_str()), (&font[..], "Test"));
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
        assert!(matches!(&r, Input::Outline(req) if req.family.is_empty()));
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

    /// A gate that lets through the files `paths` names, under the rule of
    /// the test's own account, its folder check stopping at the tree, which
    /// sits in the system's temporary folder that every account can write.
    fn active(t: &Tree, paths: &[&Path]) -> Gate {
        use vtium_settings::batch::Entry;
        let me = rustix::process::geteuid().as_raw();
        Gate::Active(cullet::Gate::new(
            Rule::running_as(me).below(&t.0),
            None,
            Batch::new(paths.iter().map(|p| Entry::Path(p.to_path_buf())).collect()),
        ))
    }

    fn host_through<'a>(t: &Tree, consoles: &'a dyn Consoles, gate: &'a Gate) -> Host<'a> {
        Host {
            gate,
            ..host(t, consoles)
        }
    }

    /// Why `origin` reads no font through `gate`, as the log shows it.
    fn refused(t: &Tree, gate: &Gate, origin: Origin) -> SourceError {
        let consoles = Graphics;
        let h = host_through(t, &consoles, gate);
        resolve(&source("s", origin, None), &h).unwrap_err()
    }

    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    // Through an active gate, a font file the batch lists and
    // that passes the trust rule reads as it does through an open gate
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn an_active_font_that_passes_the_rule_reads() {
        let t = Tree::new("gate-pass");
        let font = t.file("fonts/f.psf", b"font bytes");
        let unimap = t.file("kbd/unimaps/def.uni", b"0x00 U+0041\n");
        set_mode(&font, 0o644);
        set_mode(&unimap, 0o644);
        let gate = active(&t, &[&font]);
        let consoles = Graphics;
        let h = host_through(&t, &consoles, &gate);
        let r = request(resolve(&source("f", Origin::File(font.clone()), None), &h).unwrap());
        assert_eq!(r.font, b"font bytes");
    }

    // A font file the batch does not list is not active, and
    // counts as missing, whatever else it passes
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_font_the_batch_does_not_list_is_refused() {
        let t = Tree::new("gate-unlisted");
        let font = t.file("fonts/f.psf", b"font bytes");
        let other = t.file("fonts/g.psf", b"other");
        let e = refused(&t, &active(&t, &[&other]), Origin::File(font.clone()));
        assert_eq!(
            e,
            SourceError::Refused {
                path: font.clone(),
                because: NOT_ACTIVE.to_owned()
            }
        );
        assert_eq!(
            e.to_string(),
            format!(
                "{}: vtium treats this file as missing, because it is not among the active fonts.",
                font.display()
            )
        );
        // an open gate, as vtium-config's own previews read, reads it
        let consoles = Graphics;
        let h = host(&t, &consoles);
        assert!(resolve(&source("f", Origin::File(font), None), &h).is_ok());
    }

    // A font file its group can write fails the rule, even
    // when the batch lists it, and the refusal names the file and the mode
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_group_writable_font_is_refused() {
        let t = Tree::new("gate-writable");
        let font = t.file("fonts/f.bdf", b"STARTFONT 2.1\n");
        set_mode(&font, 0o664);
        let e = refused(&t, &active(&t, &[&font]), Origin::Bdf(font.clone()));
        let SourceError::Refused { path, because } = &e else {
            panic!("not refused: {e:?}");
        };
        assert_eq!(path, &font);
        assert!(because.contains("664"), "{because}");
        // the same file, which its group cannot write, passes
        set_mode(&font, 0o644);
        let consoles = Graphics;
        let gate = active(&t, &[&font]);
        let h = host_through(&t, &consoles, &gate);
        assert!(resolve(&source("f", Origin::Bdf(font), None), &h).is_ok());
    }

    // A font reached through a symbolic link fails the rule,
    // the link as the file and a link as a folder on the way, even when the
    // batch lists the link's path
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_font_through_a_symbolic_link_is_refused() {
        let t = Tree::new("gate-link");
        let real = t.file("fonts/real.ttf", b"outline");
        let link = t.0.join("fonts/link.ttf");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let e = refused(&t, &active(&t, &[&link]), Origin::Outline(link.clone()));
        assert!(
            matches!(&e, SourceError::Refused { path, because }
                if path == &link && because.contains("is a symbolic link")),
            "{e:?}"
        );
        std::os::unix::fs::symlink(t.0.join("fonts"), t.0.join("linked")).unwrap();
        let through = t.0.join("linked/real.ttf");
        let e = refused(
            &t,
            &active(&t, &[&through]),
            Origin::Outline(through.clone()),
        );
        assert!(
            matches!(&e, SourceError::Refused { because, .. }
                if because.contains(&format!("{} is a symbolic link", t.0.join("linked").display()))),
            "{e:?}"
        );
    }

    // The gate judges the filesystem that holds the file by
    // the mount table it is given, and refuses a network filesystem
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_font_on_a_network_filesystem_is_refused() {
        let t = Tree::new("gate-nfs");
        let font = t.file("fonts/f.psf", b"font bytes");
        let me = rustix::process::geteuid().as_raw();
        // the system's mount table, each filesystem's type made nfs4, so
        // the line that holds the tree's folder says nfs4
        let table: String = fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .map(|line| {
                let (head, tail) = line.split_once(" - ").unwrap();
                let rest = tail.split_once(' ').map_or("", |(_, rest)| rest);
                format!("{head} - nfs4 {rest}\n")
            })
            .collect();
        let gate = Gate::Active(cullet::Gate::new(
            Rule::running_as(me).below(&t.0),
            Some(Mounts::parse(table, Path::new("/nonexistent-sys"))),
            Batch::new(vec![vtium_settings::batch::Entry::Path(font.clone())]),
        ));
        let e = refused(&t, &gate, Origin::File(font));
        assert!(
            matches!(&e, SourceError::Refused { because, .. } if because.contains("nfs4")),
            "{e:?}"
        );
    }

    // A font in a user's own folder, which
    // `font-paths` names, is no active font. The batch does not list it,
    // so a family search through an active gate never takes it; and listed,
    // it fails the rule, because root does not own the folder
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    // - ADR-1200 D25: `font-paths` names folders of a user's own fonts.
    #[test]
    fn a_font_in_a_users_folder_that_font_paths_names_is_not_active() {
        use vtium_settings::Trust;
        use vtium_settings::batch::Entry;
        let t = Tree::new("gate-user-folder");
        let font = t.file(
            "home/me/.local/share/fonts/Fam-Regular.ttf",
            &outline_font("Fam", "Regular"),
        );
        let consoles = Graphics;
        let family = || source("font:Fam", Origin::Font("Fam".into()), None);
        let mut h = host(&t, &consoles);
        h.font_dirs = vec![t.0.join("home/me/.local/share/fonts")];
        // an open gate finds it
        assert!(resolve(&family(), &h).is_ok());
        // an active gate whose batch does not list it names the file, which
        // is not active
        let gate = active(&t, &[]);
        h.gate = &gate;
        assert_eq!(
            resolve(&family(), &h).unwrap_err(),
            SourceError::Refused {
                path: font.clone(),
                because: NOT_ACTIVE.to_owned()
            }
        );
        // listed, under the rule of a console that runs as root, the
        // user's folder fails the owner check
        let gate = Gate::Active(cullet::Gate::new(
            Rule::new(Trust::ROOT).below(&t.0),
            None,
            Batch::new(vec![Entry::Path(font.clone())]),
        ));
        h.gate = &gate;
        let e = resolve(&family(), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Refused { path, because }
                if path == &font && because.contains("belongs to")),
            "{e:?}"
        );
    }

    // A family search through an active gate takes only the
    // fonts the batch lists, so an active font of the family draws even
    // when a font that is not active comes first in the search's order;
    // a family the machine lacks is missing, as through an open gate
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_family_search_takes_the_active_font_of_the_family() {
        let t = Tree::new("gate-family");
        // the first file differs from the listed one by its style's name
        t.file("a/Fam-Regular.ttf", &outline_font("Fam", "Book"));
        let listed = t.file("b/Fam-Regular.ttf", &outline_font("Fam", "Regular"));
        let consoles = Graphics;
        let gate = active(&t, &[&listed]);
        let mut h = host_through(&t, &consoles, &gate);
        h.font_dirs = vec![t.0.join("a"), t.0.join("b")];
        let family = |name: &str| source("font:x", Origin::Font(name.into()), None);
        let Input::Outline(read) = resolve(&family("Fam"), &h).unwrap() else {
            panic!("not an outline font");
        };
        assert_eq!(&read.font[..], &fs::read(&listed).unwrap()[..]);
        let e = resolve(&family("Gone"), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Missing(w) if w.starts_with("no font of family Gone:")),
            "{e:?}"
        );
    }

    // A family search through an active gate blames a refused file only
    // when the file's name names the family. Here the batch lists b.ttf, of
    // family B, and the gate refuses it because root does not own its
    // folder. A search for family C, which the machine lacks, opens b.ttf in
    // its second pass, which reads families from the files' name tables,
    // and the gate refuses the file before any face is read. That refusal
    // says nothing of family C, so the search reports the family missing.
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_missing_family_is_missing_whatever_file_of_another_family_the_gate_refuses() {
        use vtium_settings::Trust;
        use vtium_settings::batch::Entry;
        let t = Tree::new("gate-family-blame");
        let other = t.file("a/b.ttf", &outline_font("B", "Regular"));
        let consoles = Graphics;
        let gate = Gate::Active(cullet::Gate::new(
            Rule::new(Trust::ROOT).below(&t.0),
            None,
            Batch::new(vec![Entry::Path(other.clone())]),
        ));
        let mut h = host_through(&t, &consoles, &gate);
        h.font_dirs = vec![t.0.join("a")];
        let family = |name: &str| source("font:x", Origin::Font(name.into()), None);
        // the file named for its family is refused, with why
        let e = resolve(&family("B"), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Refused { path, .. } if path == &other),
            "{e:?}"
        );
        // a family the machine lacks is missing
        let e = resolve(&family("C"), &h).unwrap_err();
        assert!(
            matches!(&e, SourceError::Missing(w) if w.starts_with("no font of family C:")),
            "{e:?}"
        );
    }

    // A kbd font, found as setfont finds it, draws only while
    // the batch lists the file found
    //
    // References:
    // - ADR-1205 D4: a font draws only while the batch lists it and it passes the trust rule.
    #[test]
    fn a_kbd_font_draws_only_while_the_batch_lists_it() {
        let t = Tree::new("gate-kbd");
        let font = t.file("kbd/consolefonts/ter-v16n.psf.gz", b"ter");
        t.file("vconsole.conf", b"FONT=ter-v16n\n");
        let consoles = Graphics;
        for origin in [Origin::Kbd("ter-v16n".into()), Origin::Vconsole] {
            let e = refused(&t, &active(&t, &[]), origin.clone());
            assert_eq!(
                e,
                SourceError::Refused {
                    path: font.clone(),
                    because: NOT_ACTIVE.to_owned()
                },
                "{origin:?}"
            );
            let gate = active(&t, &[&font]);
            let h = host_through(&t, &consoles, &gate);
            assert_eq!(
                request(resolve(&source("s", origin, None), &h).unwrap()).font,
                b"ter"
            );
        }
        // a kbd name that finds a link, here to a font in the same folder,
        // reads nothing, even when the batch lists both files
        let link = t.0.join("kbd/consolefonts/alias.psf.gz");
        std::os::unix::fs::symlink(&font, &link).unwrap();
        let e = refused(
            &t,
            &active(&t, &[&font, &link]),
            Origin::Kbd("alias".into()),
        );
        assert!(
            matches!(&e, SourceError::Refused { path, because }
                if path == &link && because.contains("is a symbolic link")),
            "{e:?}"
        );
    }
}
