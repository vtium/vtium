//! The fonts vtium-config finds on this machine, which the Fonts tab
//! lists and `vtium-config fonts --files` prints.
//!
//! vtium-config looks in three kinds of folder. The system's folders are
//! `/usr/share/fonts` and `/usr/local/share/fonts`, which fontconfig
//! searches for the system's fonts, `/usr/share/vtium/fonts`, where vtium
//! installs its own font files, and kbd's console-font folders. The
//! folders of `font-paths` come next, where vtium.conf sets it. Last come
//! the user's own folders, `~/.local/share/fonts` and `~/.fonts`.
//!
//! [`scan`] lists each font file once, with the folder it was found in,
//! whether it passes the trust rule, and where it stands in the current
//! draft's batch. A font in a user's folder is listed and marked not
//! usable, because the user installed it and the system did not. A font
//! in `/usr/local/share/fonts` that a package also installed in
//! `/usr/share/fonts` is marked as a duplicate, with whether the two
//! files hold the same bytes, so the person removes only a true copy.
//! The scan opens no font. [`describe`] asks the sandboxed font helper
//! what each font is, so the Fonts tab can show a font's family, sizes
//! and scripts without vtium-config parsing it.
//!
//! References:
//! - ADR-1205 D3: the folders, a user's font listed as not usable, and
//!   the helper reading each font.
//! - ADR-1205 D4: the trust mark and the batch state each font carries.
//! - ADR-1205 D5: a font installed in both system folders is a duplicate.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use vtium_settings::cullet::Rule;
use vtium_settings::mounts::Mounts;

use crate::batch::State;
use crate::describe::Description;
use crate::serve::{
    DescribeAsk, FRAME_DESCRIPTION, FRAME_FAILED, FRAME_NO_DESCRIPTION, FRAME_SANDBOX,
    FRAME_WARNING,
};
use crate::specimen::{HELPER_WAIT, Helper};

/// fontconfig's folder for the fonts that packages install.
pub const PACKAGE_FONTS: &str = "/usr/share/fonts";
/// fontconfig's folder for the fonts the machine's administrator installs,
/// where "install as a system font" copies a font.
pub const LOCAL_FONTS: &str = "/usr/local/share/fonts";
/// How deep under a folder the scan looks for fonts.
pub const DEPTH_MAX: usize = crate::sources::FONT_DEPTH_MAX;
/// The most font files one scan lists. A machine with every Noto font
/// installed has about 4,000.
pub const FILES_MAX: usize = 20_000;
/// The most entries the scan reads in one folder.
pub const ENTRIES_MAX: usize = 65_536;
/// The most font files one helper describes before vtium-config starts
/// another, so that no helper reaches its limit of CPU time.
pub const DESCRIBED_PER_HELPER: usize = 256;
/// The extensions of the font files vtium reads, case ignored.
pub const FONT_SUFFIXES: [&str; 9] = [
    "ttf", "otf", "ttc", "otc", "otb", "psf", "psfu", "bdf", "hex",
];
/// The extensions of the font files vtium also reads compressed: PSF and
/// BDF fonts. vtium reads an OpenType font and a `.hex` font only as they
/// are.
pub const COMPRESSIBLE_SUFFIXES: [&str; 3] = ["psf", "psfu", "bdf"];
/// The compressions a PSF or BDF font file may come in, by their suffixes.
pub const COMPRESSED: [&str; 4] = ["gz", "bz2", "xz", "zst"];

/// What kind of folder a font was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// `/usr/share/fonts` or `/usr/local/share/fonts`.
    System,
    /// `/usr/share/vtium/fonts`, vtium's own fonts.
    Vtium,
    /// One of kbd's console-font folders.
    Console,
    /// A folder that `font-paths` names in vtium.conf.
    FontPaths,
    /// The user's own folder, `~/.local/share/fonts` or `~/.fonts`.
    User,
}

impl Place {
    /// The place in the words a listing uses.
    pub const fn words(self) -> &'static str {
        match self {
            Place::System => "system",
            Place::Vtium => "vtium",
            Place::Console => "console fonts",
            Place::FontPaths => "font-paths",
            Place::User => "your folder",
        }
    }
}

/// One folder the scan looks in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub path: PathBuf,
    pub place: Place,
}

/// The folders a scan looks in, in order, and the two system folders
/// that the duplicate mark compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    pub folders: Vec<Folder>,
    /// The packages' folder, `/usr/share/fonts`.
    pub packages: PathBuf,
    /// The administrator's folder, `/usr/local/share/fonts`.
    pub local: PathBuf,
}

impl Folders {
    /// This machine's folders: the system's, then those that `font-paths`
    /// names in `conf`, vtium.conf's text, then the user's under `home`,
    /// with `data_home` for `$XDG_DATA_HOME`.
    pub fn system(conf: Option<&str>, home: Option<&Path>, data_home: Option<&Path>) -> Folders {
        let mut folders: Vec<Folder> = [PACKAGE_FONTS, LOCAL_FONTS]
            .iter()
            .map(|p| Folder {
                path: PathBuf::from(p),
                place: Place::System,
            })
            .collect();
        folders.push(Folder {
            path: PathBuf::from(vtium_settings::batch::PACKAGE_FONTS),
            place: Place::Vtium,
        });
        folders.extend(
            vtium_settings::batch::CONSOLE_FONT_FOLDERS
                .iter()
                .map(|p| Folder {
                    path: PathBuf::from(p),
                    place: Place::Console,
                }),
        );
        folders.extend(font_paths(conf).into_iter().map(|path| Folder {
            path,
            place: Place::FontPaths,
        }));
        let data = data_home
            .map(Path::to_path_buf)
            .or_else(|| home.map(|h| h.join(".local/share")));
        for user in [
            data.map(|d| d.join("fonts")),
            home.map(|h| h.join(".fonts")),
        ]
        .into_iter()
        .flatten()
        {
            folders.push(Folder {
                path: user,
                place: Place::User,
            });
        }
        Folders {
            folders,
            packages: PathBuf::from(PACKAGE_FONTS),
            local: PathBuf::from(LOCAL_FONTS),
        }
    }

    /// The folders of this process's user: [`Folders::system`] with
    /// `$HOME` and `$XDG_DATA_HOME` from the environment, each taken only
    /// as an absolute path.
    pub fn of_user(conf: Option<&str>) -> Folders {
        let absolute = |var: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        Folders::system(
            conf,
            absolute("HOME").as_deref(),
            absolute("XDG_DATA_HOME").as_deref(),
        )
    }
}

/// The folders `font-paths` names at the top level of vtium.conf's text
/// `conf`, separated by `:`, each taken only as an absolute path, as
/// vtium takes them.
pub fn font_paths(conf: Option<&str>) -> Vec<PathBuf> {
    let Some(value) =
        conf.and_then(|c| vtium_settings::setting(c, vtium_settings::keys::FONT_PATHS.name))
    else {
        return Vec::new();
    };
    value
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .collect()
}

/// One font file the scan found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The file, by the path the scan found it at.
    pub path: PathBuf,
    /// The folder it was found under, and what kind of folder that is.
    pub folder: Folder,
    /// Whether the file passes the trust rule, or why it fails, file first,
    /// in the words of `vtium check-config`.
    pub trust: Result<(), String>,
    /// Where the file stands in the current draft's batch.
    pub state: State,
    /// The file of the same name, or of the same bytes, that a package
    /// installed, for a font in the administrator's folder.
    pub duplicate: Option<Duplicate>,
}

/// A file a package installed that a font in the administrator's folder
/// repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    /// The package's file, under `/usr/share/fonts`.
    pub of: PathBuf,
    /// Whether the two files hold the same bytes. A file that only shares
    /// its name may be a newer version of the font that the person
    /// installed on purpose.
    pub same_bytes: bool,
}

impl Duplicate {
    /// The duplicate's note in a listing: the same file, which the person
    /// can remove, or a file of the same name, which may be newer.
    pub fn words(&self) -> String {
        let of = crate::onscreen::path(&self.of);
        if self.same_bytes {
            format!("a package installed the same file as {of}")
        } else {
            format!("a package installed a font file of the same name, {of}")
        }
    }
}

impl Found {
    /// Whether vtium can draw from the font once it is active: false for a
    /// font in a user's own folder, which the person first installs as a
    /// system font.
    pub fn usable(&self) -> bool {
        self.folder.place != Place::User
    }
}

/// Whether `path` names a font file vtium reads, by its extension: one of
/// [`FONT_SUFFIXES`], or one of [`COMPRESSIBLE_SUFFIXES`] followed by a
/// compression's suffix.
pub fn is_font_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let is = |list: &[&str], ext: &str| list.iter().any(|s| s.eq_ignore_ascii_case(ext));
    let mut parts = name.rsplit('.');
    // a name with no dot has no extension
    let (Some(ext), Some(inner)) = (parts.next(), parts.next()) else {
        return false;
    };
    if is(&COMPRESSED, ext) {
        // the inner extension must follow a dot of its own
        return parts.next().is_some() && is(&COMPRESSIBLE_SUFFIXES, inner);
    }
    is(&FONT_SUFFIXES, ext)
}

/// The `source=` value of a font set that draws the file at `path` alone,
/// by its extension: `bdf:` for a BDF font, `hex:` for a `.hex` font,
/// `file:` for a PSF font and `outline:` for an OpenType file. `None` for
/// a path that a font set's `source=` line cannot hold: one that is not
/// UTF-8; one that holds a control character, which would end the line and
/// start another; one that holds a `#`, which a font set reads as the
/// start of a comment; and one that starts or ends with a blank, which a
/// font set drops from the line.
pub fn source_of(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.chars().any(|c| c.is_control() || c == '#') || text.trim() != text {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    let mut parts = name.rsplit('.');
    let mut ext = parts.next()?;
    if crate::discover::COMPRESSED.contains(&ext) {
        ext = parts.next()?;
    }
    let kind = match ext {
        "bdf" => "bdf",
        "hex" => "hex",
        "psf" | "psfu" => "file",
        _ => "outline",
    };
    Some(format!("{kind}:{text}"))
}

/// Every font file under `folder`, at most [`DEPTH_MAX`] folders deep,
/// sorted by path. A folder that is a symbolic link is not entered, so a
/// link cannot lead the scan out of the folder or round in a loop. A file
/// that is a link to a regular file is listed, and the trust rule then
/// says that vtium refuses it.
fn files_under(folder: &Path, budget: &mut usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten().take(ENTRIES_MAX) {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if depth < DEPTH_MAX {
                    pending.push((path, depth.saturating_add(1)));
                }
            } else if is_font_file(&path) && path.is_file() && *budget > 0 {
                *budget = budget.saturating_sub(1);
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Every font file in `folders`, once each, in the folders' order and by
/// path within a folder. A file reached a second time, through another
/// folder or another name, keeps its first place. Each file carries its
/// trust mark under `rule` and `mounts`, its state from `state`, and its
/// duplicate mark. The scan reads folders and file names, and opens no
/// font.
pub fn scan(
    folders: &Folders,
    rule: &Rule,
    mounts: &Mounts,
    state: &dyn Fn(&Path) -> State,
) -> Vec<Found> {
    use std::os::unix::fs::MetadataExt;
    let mut seen = std::collections::BTreeSet::new();
    let mut budget = FILES_MAX;
    let mut out = Vec::new();
    for folder in &folders.folders {
        for path in files_under(&folder.path, &mut budget) {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !seen.insert((meta.dev(), meta.ino())) {
                continue;
            }
            let trust = vtium_settings::cullet::check(&path, rule, mounts)
                .map_err(|refused| refused.to_string());
            out.push(Found {
                state: state(&path),
                trust,
                path,
                folder: folder.clone(),
                duplicate: None,
            });
        }
    }
    mark_duplicates(&mut out, folders);
    out
}

/// Marks each font under `folders.local` that a package installed again
/// under `folders.packages`: a file of the same name, or of the same size
/// and the same bytes. The mark says whether the bytes are the same, and
/// a file of the same bytes is named first.
fn mark_duplicates(found: &mut [Found], folders: &Folders) {
    let packaged: Vec<(PathBuf, u64)> = found
        .iter()
        .filter(|f| f.path.starts_with(&folders.packages))
        .map(|f| {
            let size = std::fs::metadata(&f.path).map_or(0, |m| m.len());
            (f.path.clone(), size)
        })
        .collect();
    for f in found
        .iter_mut()
        .filter(|f| f.path.starts_with(&folders.local))
    {
        let size = std::fs::metadata(&f.path).map_or(u64::MAX, |m| m.len());
        let same = |(p, s): &&(PathBuf, u64)| *s == size && same_bytes(p, &f.path);
        let named = |(p, _): &&(PathBuf, u64)| p.file_name() == f.path.file_name();
        f.duplicate = match packaged.iter().find(same) {
            Some((p, _)) => Some(Duplicate {
                of: p.clone(),
                same_bytes: true,
            }),
            None => packaged.iter().find(named).map(|(p, _)| Duplicate {
                of: p.clone(),
                same_bytes: false,
            }),
        };
    }
}

/// Whether the files at `a` and `b` hold the same bytes, read a piece at a
/// time; false when either cannot be read.
fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(mut a), Ok(mut b)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let (mut x, mut y) = ([0u8; 8192], [0u8; 8192]);
    loop {
        let (Ok(n), Ok(m)) = (read_full(&mut a, &mut x), read_full(&mut b, &mut y)) else {
            return false;
        };
        if x.get(..n) != y.get(..m) {
            return false;
        }
        if n == 0 {
            return true;
        }
    }
}

/// Fills `buf` from `r` as far as `r` goes, and says how far that was.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while let Some(rest) = buf.get_mut(n..).filter(|r| !r.is_empty()) {
        match r.read(rest)? {
            0 => break,
            k => n = n.saturating_add(k),
        }
    }
    Ok(n)
}

/// What the font helper said of one font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Described {
    /// The helper described the file.
    Font(Description),
    /// The helper read the file and it is not a font vtium reads, or the
    /// helper cannot open it: why.
    NotAFont(String),
    /// No answer came: the helper did not start, failed, or did not answer
    /// in time. Nothing is known of the file.
    Unknown(String),
}

/// Asks the font helper at `helper` what each of `paths` is, in order, as
/// [`describe_each`] does, and gives every answer at once.
pub fn describe(helper: &Helper, paths: &[PathBuf]) -> Vec<Described> {
    let mut out = Vec::with_capacity(paths.len());
    describe_each(helper, paths, &mut |d| out.push(d));
    out
}

/// Asks the font helper at `helper` what each of `paths` is, and passes
/// each answer to `each` in order as soon as it arrives. One helper
/// describes at most [`DESCRIBED_PER_HELPER`] files, and each answer is
/// waited for at most [`HELPER_WAIT`]. When a helper fails or stalls on a
/// file, that file is [`Described::Unknown`], with why, and a fresh
/// helper describes the files after it, so one bad file costs only its
/// own description. When the helper cannot start, every file is unknown.
pub fn describe_each(helper: &Helper, paths: &[PathBuf], each: &mut dyn FnMut(Described)) {
    describe_each_as(helper, None, paths, each);
}

/// [`describe`], with each helper run as `account`, a uid and a gid, when
/// one is given, as `apply` runs it as `nobody` when it runs as root. A
/// helper that holds any other id is ended, and its files are unknown.
pub fn describe_as(
    helper: &Helper,
    account: Option<(u32, u32)>,
    paths: &[PathBuf],
) -> Vec<Described> {
    let mut out = Vec::with_capacity(paths.len());
    describe_each_as(helper, account, paths, &mut |d| out.push(d));
    out
}

/// [`describe_each`], with each helper run as `account` when one is given.
fn describe_each_as(
    helper: &Helper,
    account: Option<(u32, u32)>,
    paths: &[PathBuf],
    each: &mut dyn FnMut(Described),
) {
    let mut rest = paths;
    while !rest.is_empty() {
        let chunk = rest.get(..DESCRIBED_PER_HELPER).unwrap_or(rest);
        match describe_in_one(helper, account, chunk, each) {
            // a helper answers at least the file it failed on
            Ok(answered) => rest = rest.get(answered.max(1)..).unwrap_or_default(),
            Err(why) => {
                for _ in rest {
                    each(Described::Unknown(why.clone()));
                }
                return;
            }
        }
    }
}

/// The answer to one description from the helper's frames, passing over
/// its sandbox's report and any warning. A reason the helper gives is
/// kept as [`crate::onscreen::bytes`] gives it, cut short and with no
/// control character, because the helper reads untrusted fonts.
fn answer(from: &mut dyn Read) -> Result<Described, String> {
    use crate::onscreen::bytes;
    loop {
        let (kind, payload) = crate::specimen::frame(from)?;
        match kind {
            FRAME_SANDBOX | FRAME_WARNING => {}
            FRAME_DESCRIPTION => {
                return Description::decode(&payload).map(Described::Font);
            }
            FRAME_NO_DESCRIPTION => {
                return Ok(Described::NotAFont(bytes(&payload)));
            }
            FRAME_FAILED => {
                return Err(format!("the font helper failed: {}", bytes(&payload)));
            }
            other => return Err(format!("the font helper sent a frame of kind {other}")),
        }
    }
}

/// [`describe_each`] of `paths` through one helper, until the helper
/// fails or stalls. Gives how many files it answered, counting the file
/// the helper failed on as answered, or why the helper did not start.
fn describe_in_one(
    helper: &Helper,
    account: Option<(u32, u32)>,
    paths: &[PathBuf],
    each: &mut dyn FnMut(Described),
) -> Result<usize, String> {
    let count = paths.len();
    let paths = paths.to_vec();
    let (mut child, mut stdin, mut stdout) = crate::specimen::start_as(helper, account)?;
    let (arrived, answers) = mpsc::channel();
    // every request is written first, from a thread of its own, so a
    // helper that answers while the requests are still going in never
    // waits on a full pipe
    let writer = std::thread::spawn(move || {
        for path in paths {
            let ask = DescribeAsk { path };
            if stdin.write_all(&ask.encode()).is_err() {
                break;
            }
        }
        let _ = stdin.flush();
    });
    std::thread::spawn(move || {
        for _ in 0..count {
            let got = answer(&mut stdout);
            let failed = got.is_err();
            if arrived.send(got).is_err() || failed {
                break;
            }
        }
    });
    let mut answered = 0;
    while answered < count {
        answered = answered.saturating_add(1);
        match answers.recv_timeout(HELPER_WAIT) {
            Ok(Ok(d)) => each(d),
            Ok(Err(why)) => {
                each(Described::Unknown(why));
                break;
            }
            Err(_) => {
                each(Described::Unknown(
                    "the font helper did not answer in time".to_owned(),
                ));
                break;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = writer.join();
    Ok(answered)
}

/// One file in "install as a system font", as vtium-config shows it
/// before it asks to replace one with the other: its path, size and
/// date, and the family and version of its first face, which the font
/// helper reads.
///
/// References:
/// - ADR-1205 D5, its note of 2026-11-18: on a clash vtium-config shows
///   both files, each with its size, date, family name and version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFacts {
    pub path: PathBuf,
    pub size: u64,
    /// When the file last changed, in seconds since 1970.
    pub modified: u64,
    /// The family and version, or why the helper gave none.
    pub font: Result<(String, Option<String>), String>,
}

impl FileFacts {
    /// The facts of the file at `path`, with what `described` says of it.
    pub fn of(path: &Path, described: &Described) -> FileFacts {
        let meta = std::fs::metadata(path).ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let font = match described {
            Described::Font(d) => d
                .faces
                .first()
                .map(|f| (f.family.clone(), f.version.clone()))
                .ok_or_else(|| "the font helper found no face in it".to_owned()),
            Described::NotAFont(why) | Described::Unknown(why) => Err(why.clone()),
        };
        FileFacts {
            path: path.to_path_buf(),
            size: meta.map_or(0, |m| m.len()),
            modified,
            font,
        }
    }

    /// The facts as one line: `PATH: SIZE bytes, changed DATE, FAMILY
    /// version VERSION`.
    pub fn line(&self) -> String {
        use crate::onscreen::text;
        let font = match &self.font {
            Ok((family, Some(version))) => format!("{}, version {}", text(family), text(version)),
            Ok((family, None)) => format!("{}, no version stated", text(family)),
            Err(why) => format!("the font helper cannot read it: {}", text(why)),
        };
        format!(
            "{}: {} bytes, changed {} UTC, {font}",
            crate::onscreen::path(&self.path),
            self.size,
            crate::report::utc_text(self.modified)
        )
    }
}

/// One font file's row in `fonts --files`, tab-separated: the path, the
/// kind of folder, its state, its trust mark, the family and style of
/// its first face with the count of the others, the sizes it supplies,
/// monospaced or proportional, its scripts, its groups, and a duplicate's
/// note. A font the helper cannot describe says why in place of the five
/// columns that describe it. Each column is passed through
/// [`crate::onscreen::text`], so a file's name or a font's own words
/// print as text and never hold the tab that separates the columns.
pub fn row(found: &Found, described: &Described) -> String {
    let state = if found.usable() {
        found.state.words().to_owned()
    } else {
        "not usable: install it as a system font first".to_owned()
    };
    let trust = match &found.trust {
        Ok(()) => "passes the trust rule".to_owned(),
        Err(why) => why.clone(),
    };
    let mut cols = vec![
        found.path.to_string_lossy().into_owned(),
        found.folder.place.words().to_owned(),
        state,
        trust,
    ];
    match described {
        Described::Font(d) => cols.extend(face_columns(d)),
        Described::NotAFont(why) | Described::Unknown(why) => {
            cols.push(format!("the font helper cannot describe it: {why}"));
            cols.extend(std::iter::repeat_n(String::new(), 4));
        }
    }
    cols.push(
        found
            .duplicate
            .as_ref()
            .map(Duplicate::words)
            .unwrap_or_default(),
    );
    cols.iter()
        .map(|c| crate::onscreen::text(c))
        .collect::<Vec<_>>()
        .join("\t")
}

/// The five columns that describe a font's first face.
fn face_columns(d: &Description) -> Vec<String> {
    let Some(face) = d.faces.first() else {
        return vec![String::new(); 5];
    };
    let mut name = [face.family.as_str(), face.style.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if d.faces.len() > 1 {
        name.push_str(&format!(" and {} more faces", d.faces.len() - 1));
    }
    let strikes: Vec<String> = face
        .strikes
        .iter()
        .map(|s| format!("{}×{}", s.width, s.height))
        .collect();
    let sizes = match (face.outline, strikes.is_empty()) {
        (true, true) => "outline, any size".to_owned(),
        (true, false) => format!("outline, any size, bitmaps {}", strikes.join(" ")),
        (false, _) => format!("bitmap {}", strikes.join(" ")),
    };
    let spacing = if face.monospaced {
        "monospaced"
    } else {
        "proportional"
    };
    let groups: Vec<&str> = face.groups.iter().map(|g| g.name()).collect();
    vec![
        name,
        sizes,
        spacing.to_owned(),
        face.scripts.join(", "),
        groups.join(", "),
    ]
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    fn folders(root: &Path) -> Folders {
        let folder = |p: &str, place| Folder {
            path: root.join(p),
            place,
        };
        Folders {
            folders: vec![
                folder("usr/share/fonts", Place::System),
                folder("usr/local/share/fonts", Place::System),
                folder("kbd", Place::Console),
                folder("home/.local/share/fonts", Place::User),
            ],
            packages: root.join("usr/share/fonts"),
            local: root.join("usr/local/share/fonts"),
        }
    }

    fn put(root: &Path, path: &str, bytes: &[u8]) -> PathBuf {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn found(root: &Path) -> Vec<Found> {
        let mounts = Mounts::parse("", Path::new("/sys"));
        scan(&folders(root), &Rule::running_as(0), &mounts, &|_| {
            State::Inactive
        })
    }

    #[test]
    fn the_scan_lists_each_font_once_with_its_folder() {
        let s = Scratch::new("discover-once");
        let a = put(&s, "usr/share/fonts/TTF/A.ttf", b"a");
        put(&s, "usr/share/fonts/README", b"not a font");
        let b = put(&s, "kbd/ter-v16n.psf.gz", b"b");
        // the same file again, by a second name in a later folder
        std::fs::create_dir_all(s.join("home/.local/share/fonts")).unwrap();
        std::fs::hard_link(&a, s.join("home/.local/share/fonts/A.ttf")).unwrap();
        let mine = put(&s, "home/.local/share/fonts/Mine.otf", b"m");
        let got = found(&s);
        let paths: Vec<&Path> = got.iter().map(|f| f.path.as_path()).collect();
        assert_eq!(paths, [a.as_path(), b.as_path(), mine.as_path()]);
        assert_eq!(got[1].folder.place, Place::Console);
        assert!(got[0].usable() && got[1].usable());
    }

    #[test]
    fn a_font_in_the_users_folder_is_listed_and_not_usable() {
        let s = Scratch::new("discover-user");
        put(&s, "home/.local/share/fonts/Mine.otf", b"m");
        let got = found(&s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].folder.place, Place::User);
        assert!(!got[0].usable());
        let row = row(&got[0], &Described::Unknown("x".to_owned()));
        assert!(
            row.contains("not usable: install it as a system font first"),
            "{row}"
        );
    }

    #[test]
    fn a_local_font_that_a_package_also_installed_is_a_duplicate() {
        let s = Scratch::new("discover-dup");
        let packaged = put(&s, "usr/share/fonts/misc/Same.ttf", b"one");
        put(&s, "usr/local/share/fonts/Same.ttf", b"two");
        let copied = put(&s, "usr/share/fonts/x/Packaged.ttf", b"bytes");
        put(&s, "usr/local/share/fonts/Renamed.ttf", b"bytes");
        put(&s, "usr/local/share/fonts/Alone.ttf", b"alone");
        let identical = put(&s, "usr/share/fonts/y/Twin.ttf", b"twin");
        put(&s, "usr/local/share/fonts/Twin.ttf", b"twin");
        let got = found(&s);
        let dup = |name: &str| {
            got.iter()
                .find(|f| f.path.ends_with(name) && f.path.starts_with(s.join("usr/local")))
                .unwrap()
                .duplicate
                .clone()
        };
        let same = |of: &PathBuf| Duplicate {
            of: of.clone(),
            same_bytes: true,
        };
        // a file that only shares its name may be a newer version
        let only_named = dup("Same.ttf").unwrap();
        assert_eq!(
            only_named,
            Duplicate {
                of: packaged.clone(),
                same_bytes: false
            }
        );
        assert_eq!(
            only_named.words(),
            format!(
                "a package installed a font file of the same name, {}",
                packaged.display()
            )
        );
        assert_eq!(dup("Renamed.ttf"), Some(same(&copied)));
        assert_eq!(dup("Twin.ttf"), Some(same(&identical)));
        assert_eq!(
            same(&identical).words(),
            format!(
                "a package installed the same file as {}",
                identical.display()
            )
        );
        assert_eq!(dup("Alone.ttf"), None);
        // a package's own file is never the duplicate
        assert!(
            got.iter()
                .filter(|f| f.path.starts_with(s.join("usr/share")))
                .all(|f| f.duplicate.is_none())
        );
    }

    #[test]
    fn a_font_file_is_known_by_its_extension() {
        for yes in [
            "a.ttf",
            "b.OTF",
            "c.psf.gz",
            "d.psfu",
            "e.bdf.gz",
            "f.hex",
            "g.ttc",
            "h.otb",
            "i.psfu.ZST",
            "j.bdf.bz2",
            "k.psf.xz",
        ] {
            assert!(is_font_file(Path::new(yes)), "{yes}");
        }
        // vtium reads an OpenType or .hex font only uncompressed
        for no in [
            "fonts.dir",
            "a.pcf.gz",
            "ttf",
            "README",
            "x.gz",
            "a.cp.gz",
            "psf.gz",
            "b.ttf.gz",
            "c.otf.zst",
            "f.hex.gz",
            "g.ttc.xz",
        ] {
            assert!(!is_font_file(Path::new(no)), "{no}");
        }
    }

    #[test]
    fn font_paths_gives_its_absolute_folders_and_the_users_come_last() {
        let f = Folders::system(
            Some("font-paths = /opt/fonts:relative:/srv/f\n"),
            Some(Path::new("/home/u")),
            None,
        );
        let named: Vec<(&Path, Place)> = f
            .folders
            .iter()
            .map(|f| (f.path.as_path(), f.place))
            .filter(|(_, p)| matches!(p, Place::FontPaths | Place::User))
            .collect();
        assert_eq!(
            named,
            [
                (Path::new("/opt/fonts"), Place::FontPaths),
                (Path::new("/srv/f"), Place::FontPaths),
                (Path::new("/home/u/.local/share/fonts"), Place::User),
                (Path::new("/home/u/.fonts"), Place::User),
            ]
        );
        assert_eq!(f.folders[0].path, Path::new(PACKAGE_FONTS));
    }

    /// A stand-in font helper that answers its first request, that the
    /// file is no font, and then stops on the second, as a helper that a
    /// hostile file kills does.
    fn dying_helper(dir: &Path) -> Helper {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("helper.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '\\023\\000\\000\\000\\003\\000\\000\\000one'\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Helper::at(script)
    }

    /// One file that kills the helper leaves only that file unknown: a
    /// fresh helper describes the files after it.
    #[test]
    fn a_file_that_stops_the_helper_leaves_the_rest_described() {
        let s = Scratch::new("discover-dying");
        let paths: Vec<PathBuf> = ["/a.ttf", "/b.ttf", "/c.ttf", "/d.ttf"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let got = describe(&dying_helper(&s), &paths);
        let one = Described::NotAFont("one".to_owned());
        let stopped = Described::Unknown("the font helper stopped without answering".to_owned());
        // each fresh helper answers one file and stops on the next
        assert_eq!(got, [one.clone(), stopped.clone(), one, stopped]);
    }

    /// The answers reach the caller one at a time, in order, as each
    /// arrives, so a listing can print a row before the next file is read.
    #[test]
    fn each_answer_is_passed_on_as_it_arrives() {
        let s = Scratch::new("discover-each");
        let paths = vec![PathBuf::from("/a.ttf"), PathBuf::from("/b.ttf")];
        let mut seen = Vec::new();
        describe_each(&dying_helper(&s), &paths, &mut |d| seen.push(d));
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], Described::NotAFont("one".to_owned()));
    }

    /// A file's name, the trust rule's verdict and the helper's reason
    /// print as text: an escape sequence in any of them reaches the
    /// terminal as U+FFFD and the characters after it, and no column
    /// holds a tab.
    #[test]
    fn a_row_shows_control_characters_as_text() {
        use std::os::unix::ffi::OsStrExt;
        let s = Scratch::new("discover-escape");
        let name = std::ffi::OsStr::from_bytes(b"\x1b[31mred\x1b[0m.ttf");
        let path = s.join("home/.local/share/fonts").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"m").unwrap();
        let mut got = found(&s);
        assert_eq!(got.len(), 1);
        got[0].trust = Err(format!(
            "{} is \x1b]0;title\x07 refused\tnow",
            path.display()
        ));
        let row = row(
            &got[0],
            &Described::NotAFont("bad \x1b[2J font\nhere".to_owned()),
        );
        assert!(
            !row.contains('\x1b') && !row.contains('\x07') && !row.contains('\n'),
            "{row:?}"
        );
        assert_eq!(row.matches('\t').count(), 9, "{row:?}");
        assert!(row.contains("\u{fffd}[31mred\u{fffd}[0m.ttf"), "{row:?}");
        assert!(row.contains("bad \u{fffd}[2J font\u{fffd}here"), "{row:?}");
    }
}
