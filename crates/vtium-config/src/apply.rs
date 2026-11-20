//! Saving: the draft a user's save becomes, and `apply DIR`, the
//! privileged step that checks a draft and installs it.
//!
//! A draft is a directory the user owns, under `$XDG_STATE_HOME/vtium/drafts`.
//! It holds the whole edited `vtium.conf`, each frit to write under
//! `fonts/`, and the whole edited `batch.conf`, the list of the active
//! resources ([`crate::batch`]). A draft that installs a font as a system
//! font holds that one file, `system-font/NAME`, and nothing else.
//! `MANIFEST` lists the files, one line a file, tab-separated: `write` or
//! `remove`, the file's name in the draft, and the SHA-256 of the installed
//! file it replaces as it was when the draft was made, or `new`. A last
//! line, `accept` and `no-complete-route`, records that the person
//! confirmed a vtium.conf that wires no complete route, or ran `install
//! --force`.
//!
//! `apply` runs as root through `sudo vtium-config apply DIR`, and takes
//! no option that names a path or a program ([`crate::cli::root_refusal`]).
//! So it writes only these files: `/etc/vtium/vtium.conf`; the batch file
//! vtium reads; the frits, in the frits' directory that the installed
//! vtium.conf names in `font-frits`, else the one the build names, read as
//! the console reads it ([`crate::switch::frits_dir`]); and one font file
//! in `/usr/local/share/fonts`, a folder it creates with mode 0755 when it
//! is missing. It checks vtium.conf with the console the build names,
//! [`VTIUM`], and reads fonts only through the font helper, which runs as
//! `nobody` when `apply` runs as root. `apply`:
//!
//! 1. reads the draft without following a link, takes only regular files
//!    with names it knows, and bounds each file's size, since root is
//!    reading a directory the user owns;
//! 2. refuses the draft, writing nothing, when a file it replaces has
//!    changed since the draft was made, naming the file;
//! 3. checks every file before it writes any: each frit with the helper's
//!    frit reader, strictly; the batch file with vtium's own reader, every
//!    line that the installed batch file does not already hold, and each
//!    resource it activates against the trust rule, as the user `apply`
//!    runs as; the edited vtium.conf, both its parts in one run, with the
//!    console's own `vtium check-config`, so the verdict before writing is
//!    the verdict at the next start; and a system font with the font
//!    helper, which must describe a copy root made of the font's bytes
//!    ([`SystemFont`]). A system font's draft that holds any other file
//!    is refused, so that draft runs no `check-config`;
//! 4. writes the frits, then the batch file, then vtium.conf, so
//!    vtium.conf never names a frit or a resource that is not yet there,
//!    and removes frits last. Each file goes in through
//!    [`vtium_settings::install_file`], in one rename, mode 0644, with the
//!    old one kept as `NAME.bak`. A system font is written the same way,
//!    but the file it replaces is not kept, because fontconfig would read
//!    the copy as one more font;
//! 5. prints one line a file, `wrote PATH`, `removed PATH`, `failed PATH`
//!    with the reason, or `not written PATH` after a failure, and exits 0
//!    when it wrote them all. After it writes a system font, it checks the
//!    font against the trust rule and prints `untrusted PATH` with the
//!    reason when it fails, and exits 1.
//!
//! The draft stays after `apply`, so a user can run it again.
//!
//! References:
//! - ADR-1201 D5: the draft a save becomes, and `apply` as the privileged step.
//! - ADR-1205 D5: the system font's draft, its folder, and the trust check after the write.
//! - ADR-1403 D3: the console's own check judges vtium.conf before `apply` writes it.
//! - ADR-1403 D8: the manifest line that accepts a vtium.conf with no complete route.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{FileType, Mode, OFlags};
use vtium_font::{Sha256, sha256_hex};

use crate::draft::Plan;
use crate::drafts::Verdict;
use crate::frit::{FRIT_BYTES_MAX, Frit, valid_name};
use crate::specimen::Helper;
use crate::tui::{CONF_MAX, Paths};

/// What `apply` says of a file that changed after the draft was made.
/// `install` finds these words in `apply`'s output to choose its exit code.
pub const CHANGED: &str = "changed after the draft was made";

/// Runs an argv with the terminal given back for a password, and says
/// whether it succeeded, with what it printed.
pub type Runner<'a> = &'a mut dyn FnMut(&[OsString]) -> io::Result<(bool, String)>;

/// Asks the person a yes-or-no question: writes it to the writer it is
/// given, reads the answer, and says whether it is yes.
pub type Ask<'a> = &'a mut dyn FnMut(&mut dyn Write, &str) -> bool;

/// The draft's list of files.
pub const MANIFEST: &str = "MANIFEST";

/// Why a change that moves the frits' directory and also writes or
/// removes a frit is refused, by the TUI's save and by `apply`.
pub const FRITS_MOVE_FIRST: &str = "this install changes font-frits and also adds or removes font sets, which would go to the old directory. Install the font-frits change first, then change the font sets.";

/// The manifest line's words that accept a vtium.conf that wires no
/// complete route (ADR-1403 D8).
const ACCEPT_NO_ROUTE: &str = "accept\tno-complete-route";

/// What `apply` prints when it installs a vtium.conf that wires no
/// complete route, because the person confirmed it.
pub const ACCEPTED_NO_ROUTE: &str =
    "The draft's vtium.conf has no complete path. It is installed because you confirmed it.";

/// The most bytes of the manifest.
const MANIFEST_MAX: u64 = 64 * 1024;

/// The console binary `apply` checks configuration files with, set at
/// build time with `VTIUM_PROGRAM`.
pub const VTIUM: &str = match option_env!("VTIUM_PROGRAM") {
    Some(p) => p,
    None => "/usr/bin/vtium",
};

/// Where a draft installs, and the console that checks it: the options
/// `--configdir`, `--frits` and `--vtium` name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Targets {
    pub configdir: PathBuf,
    /// The frits' directory, or why there is none, in which case a draft
    /// that writes or removes a frit is refused and any other installs.
    pub frits: crate::switch::Frits,
    pub vtium: OsString,
}

impl Targets {
    /// The options that name these targets, as `apply` takes them: with no
    /// frits' directory, no `--frits`, so `apply` finds the same reason.
    pub fn options(&self) -> Vec<OsString> {
        let mut options = vec!["--configdir".into(), self.configdir.clone().into()];
        if let Ok(frits) = &self.frits {
            options.extend(["--frits".into(), frits.clone().into()]);
        }
        options.extend(["--vtium".into(), self.vtium.clone()]);
        options
    }
}

/// One file of a draft.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    /// A file to write: its name in the draft, and the hash of the
    /// installed file it replaces, `None` for a new file.
    Write { name: String, was: Option<String> },
    /// A frit to remove, by its name in the draft, and its hash.
    Remove { name: String, was: String },
    /// The person accepts a vtium.conf that wires no complete route.
    AcceptNoRoute,
}

fn hash(bytes: &[u8]) -> String {
    sha256_hex(&Sha256::digest(bytes))
}

/// The draft's name for the batch file.
const BATCH: &str = "batch.conf";

/// The folder in a draft that holds a font to install as a system font.
const SYSTEM_FONT: &str = "system-font";

/// Whether `name` is a name a draft may hold: `vtium.conf`, `batch.conf`,
/// `fonts/NAME.vtfrit` for a frit's name, or `system-font/NAME` for a
/// font file's name.
fn known(name: &str) -> bool {
    name == "vtium.conf"
        || name == BATCH
        || name
            .strip_prefix("fonts/")
            .and_then(|n| n.strip_suffix(".vtfrit"))
            .is_some_and(valid_name)
        || system_font_name(name).is_some()
}

/// The font file's name in the draft's name `system-font/NAME`, when NAME
/// is one a system font may have ([`font_name_refusal`]).
fn system_font_name(name: &str) -> Option<&str> {
    name.strip_prefix(SYSTEM_FONT)
        .and_then(|n| n.strip_prefix('/'))
        .filter(|n| valid_font_name(n))
}

/// Whether `name` may name a font file that "install as a system font"
/// writes, as [`font_name_refusal`] judges it.
pub fn valid_font_name(name: &str) -> bool {
    font_name_refusal(name).is_none()
}

/// Why `name` may not name a font file that "install as a system font"
/// writes, or `None` when it may: a file's name of at most 255 bytes,
/// with no control character, not hidden behind a leading `.`, and with
/// the extension of a font file vtium reads.
pub fn font_name_refusal(name: &str) -> Option<&'static str> {
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Some("this is not the name of a file.");
    }
    if name.len() > 255 {
        return Some("this file's name is longer than 255 bytes, the most a file's name may hold.");
    }
    if name.starts_with('.') {
        return Some(
            "this file's name starts with a dot, which hides a file. Rename the file so that its name starts with another character.",
        );
    }
    if name.chars().any(char::is_control) {
        return Some("this file's name holds a control character. Rename the file without it.");
    }
    if !crate::discover::is_font_file(Path::new(name)) {
        return Some(
            "this file's name does not end like a font file's. vtium-config installs a file whose name ends in .ttf, .otf, .ttc, .otc, .otb or .hex. A PSF or BDF font's name ends in .psf, .psfu or .bdf, which may be followed by .gz, .bz2, .xz or .zst.",
        );
    }
    None
}

/// The packages' fonts folder, which "install as a system font" never
/// takes a font from: `/usr/share/fonts` for the system's directories,
/// and `package-fonts` in the configuration directory for any other, such
/// as a test's.
pub fn package_fonts(targets: &Targets) -> PathBuf {
    if targets.configdir == crate::cli::default_configdir() {
        PathBuf::from(crate::discover::PACKAGE_FONTS)
    } else {
        targets.configdir.join("package-fonts")
    }
}

/// The folder "install as a system font" copies a font into: fontconfig's
/// folder for the administrator's fonts, `/usr/local/share/fonts`, for the
/// system's directories, and `local-fonts` in the configuration
/// directory for any other, such as a test's. It is never
/// `/usr/share/fonts`, which the packages own.
///
/// References:
/// - ADR-1205 D5: the install step copies into `/usr/local/share/fonts`
///   and never into `/usr/share/fonts`.
pub fn local_fonts(targets: &Targets) -> PathBuf {
    if targets.configdir == crate::cli::default_configdir() {
        PathBuf::from(crate::discover::LOCAL_FONTS)
    } else {
        targets.configdir.join("local-fonts")
    }
}

/// The installed path the draft's file `name` replaces, or why a frit
/// has none: there is no frits' directory.
fn target(targets: &Targets, name: &str) -> Result<PathBuf, String> {
    if name == BATCH {
        return Ok(crate::batch::batch_file(targets));
    }
    if let Some(font) = system_font_name(name) {
        return Ok(local_fonts(targets).join(font));
    }
    match name.strip_prefix("fonts/") {
        Some(frit) => targets
            .frits
            .as_ref()
            .map(|dir| dir.join(frit))
            .map_err(Clone::clone),
        None => Ok(targets.configdir.join(name)),
    }
}

/// The draft's name for the installed file `path`, when it is one a draft
/// holds.
fn name_for(targets: &Targets, path: &Path) -> Option<String> {
    if path == crate::batch::batch_file(targets) {
        return Some(BATCH.to_owned());
    }
    if path.parent() == Some(targets.configdir.as_path()) {
        let name = path.file_name()?.to_str()?;
        return (name == "vtium.conf").then(|| name.to_owned());
    }
    if targets
        .frits
        .as_ref()
        .is_ok_and(|dir| path.parent() == Some(dir.as_path()))
    {
        let name = format!("fonts/{}", path.file_name()?.to_str()?);
        return known(&name).then_some(name);
    }
    None
}

/// Writes `plan` as a new draft under `root`, and returns the draft's
/// directory.
pub fn write_draft(root: &Path, plan: &Plan, targets: &Targets) -> io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let dir = new_dir(root)?;
    std::fs::create_dir(dir.join("fonts"))?;
    let mut manifest = String::new();
    for file in &plan.files {
        let name = name_for(targets, &file.path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a file a draft can contain", file.path.display()),
            )
        })?;
        std::fs::write(dir.join(&name), &file.new)?;
        let was = file
            .old
            .as_deref()
            .map_or_else(|| "new".to_owned(), |old| hash(old.as_bytes()));
        manifest.push_str(&format!("write\t{name}\t{was}\n"));
    }
    for removal in &plan.removals {
        let name = name_for(targets, &removal.path)
            .filter(|n| n.starts_with("fonts/"))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{} is not a font set a draft can remove",
                        removal.path.display()
                    ),
                )
            })?;
        let was = hash(&removal.old);
        manifest.push_str(&format!("remove\t{name}\t{was}\n"));
    }
    if plan.no_route {
        manifest.push_str(ACCEPT_NO_ROUTE);
        manifest.push('\n');
    }
    std::fs::write(dir.join(MANIFEST), manifest)?;
    Ok(dir)
}

/// A font to install as a system font: the file the person chose, its
/// bytes as vtium-config read them, and the file of the same name in the
/// folder it goes to, when there is one.
///
/// References:
/// - ADR-1205 D5: the install step copies one font file into
///   `/usr/local/share/fonts` through the privileged step, on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemFont {
    /// The font file the person chose.
    pub source: PathBuf,
    /// Where the install writes it: [`local_fonts`] and the file's name.
    pub target: PathBuf,
    pub bytes: Vec<u8>,
    /// The bytes of the file already at `target`, when there is one.
    pub installed: Option<Vec<u8>>,
}

/// What installing a [`SystemFont`] does to the folder it goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// No file of its name is there, so the install adds it.
    New,
    /// A file of its name holds the same bytes, so the font is already
    /// installed and nothing needs doing.
    Identical,
    /// A file of its name holds other bytes, which the install replaces
    /// only when the person says so.
    Replaces,
}

impl SystemFont {
    /// The font file at `source` as a font for `targets`' system fonts
    /// folder. `Err` says why it cannot be: the file cannot be read, is
    /// not a regular file, is larger than an outline font may be, has a
    /// name a system font may not have, or is a font a package installed,
    /// which is a system font already. The file at the target is read
    /// too, so [`SystemFont::placement`] can compare the two.
    pub fn read(source: &Path, targets: &Targets) -> Result<SystemFont, String> {
        let limit = crate::outline::OUTLINE_BYTES_MAX;
        let name = source
            .file_name()
            .ok_or_else(|| format!("{}: this is not the name of a file.", source.display()))?
            .to_str()
            .ok_or_else(|| {
                format!(
                    "{}: this file's name is not UTF-8 text. Rename the file.",
                    source.display()
                )
            })?;
        if let Some(why) = font_name_refusal(name) {
            return Err(format!("{}: {why}", source.display()));
        }
        // a link to a package's font is the package's font too
        let packages = package_fonts(targets);
        let real = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        let packages_real = std::fs::canonicalize(&packages).unwrap_or(packages.clone());
        if real.starts_with(&packages_real) || source.starts_with(&packages) {
            return Err(format!(
                "{}: a package installed this font in {}, so it is a system font already. Installing it again would only make a duplicate.",
                source.display(),
                packages.display()
            ));
        }
        let target = local_fonts(targets).join(name);
        let bytes = read_regular(source, limit)?
            .ok_or_else(|| format!("{}: there is no such file", source.display()))?;
        let installed = read_regular(&target, limit)?;
        Ok(SystemFont {
            source: source.to_path_buf(),
            target,
            bytes,
            installed,
        })
    }

    /// What the install does: adds the font, finds it already installed,
    /// or replaces another file of its name.
    pub fn placement(&self) -> Placement {
        match &self.installed {
            None => Placement::New,
            Some(old) if *old == self.bytes => Placement::Identical,
            Some(_) => Placement::Replaces,
        }
    }

    /// Where the draft `draft`, written by [`SystemFont::write_draft`],
    /// holds its copy of the font.
    pub fn draft_copy(&self, draft: &Path) -> PathBuf {
        let name = self.target.file_name().unwrap_or_default();
        draft.join(SYSTEM_FONT).join(name)
    }

    /// Writes the font as a new draft under `root`, a draft that holds this
    /// font and nothing else, and returns the draft's directory. The draft
    /// names the installed file it replaces by its hash, so `apply` refuses
    /// it if that file changes before the install.
    pub fn write_draft(&self, root: &Path) -> io::Result<PathBuf> {
        let name = self
            .target
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no font file name"))?;
        std::fs::create_dir_all(root)?;
        let dir = new_dir(root)?;
        std::fs::create_dir(dir.join(SYSTEM_FONT))?;
        std::fs::write(dir.join(SYSTEM_FONT).join(name), &self.bytes)?;
        let was = self
            .installed
            .as_deref()
            .map_or_else(|| "new".to_owned(), hash);
        std::fs::write(
            dir.join(MANIFEST),
            format!("write\t{SYSTEM_FONT}/{name}\t{was}\n"),
        )?;
        Ok(dir)
    }
}

/// The regular file at `path`, at most `limit` bytes, or `None` when
/// nothing is there. A link is followed, since the person reads their own
/// files; anything but a regular file is refused. The file is opened
/// without blocking, so a FIFO named like a font is refused at once
/// rather than waited on.
fn read_regular(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
        Ok(meta) if !meta.is_file() => {
            return Err(format!("{}: this is not a regular file", path.display()));
        }
        Ok(_) => {}
    }
    let file =
        crate::sources::open_regular(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(format!(
            "{}: this file is larger than {limit} bytes, the most a font file may hold",
            path.display()
        ));
    }
    Ok(Some(bytes))
}

/// A new directory under `root`, named for the time it was made.
fn new_dir(root: &Path) -> io::Result<PathBuf> {
    #[expect(
        clippy::disallowed_methods,
        reason = "a draft's directory is named for the time it was made (ADR-1401 D12)"
    )]
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let pid = std::process::id();
    for n in 0..100u32 {
        let dir = root.join(format!("{secs}-{pid}-{n}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free draft directory",
    ))
}

/// How a save is installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// The system's directories: `sudo vtium-config apply DIR`, through
    /// the program named.
    Sudo(OsString),
    /// Directories that are not the system's, as a test or a person
    /// trying the tool names them: `apply` run as the user, with the
    /// options that name them. No privilege is offered for them.
    User,
}

impl Targets {
    /// The system's directories and console: `/etc/vtium`, the frits'
    /// directory its configuration files name in `font-frits`, else the
    /// one the build names ([`crate::switch::frits_dir`]), and the console
    /// the build names.
    pub fn system() -> Targets {
        let configdir = crate::cli::default_configdir();
        Targets {
            frits: crate::switch::frits_dir(&configdir),
            configdir,
            vtium: VTIUM.into(),
        }
    }
}

/// How a save to `targets` is installed, with `sudo` the program that runs
/// a privileged command. The TUI that saves runs as the user (ADR-1201
/// D5). The directories decide: the system's need root to write, and
/// sudo's `apply DIR` takes no option, so it checks with the console it
/// was built with whatever console the user's own check ran.
pub fn route(targets: &Targets, sudo: &OsString) -> Route {
    let system = Targets::system();
    if targets.configdir == system.configdir && targets.frits == system.frits {
        Route::Sudo(sudo.clone())
    } else {
        Route::User
    }
}

/// The argv a save runs, `program` being this binary: `sudo
/// /usr/bin/vtium-config apply DIR` and nothing else for the system's
/// directories, so a sudo rule can name it whole; and the options and
/// `apply DIR` as the user for other directories.
pub fn save_argv(route: &Route, program: &Path, targets: &Targets, draft: &Path) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();
    match route {
        Route::Sudo(sudo) => {
            argv.push(sudo.clone());
            argv.push(program.into());
        }
        Route::User => {
            argv.push(program.into());
            argv.extend(targets.options());
        }
    }
    argv.push("apply".into());
    argv.push(draft.into());
    argv
}

/// The owners a draft and its files may have: root, and the user `sudo`
/// ran `apply` for, by `SUDO_UID`, when `apply` runs as root; root and the
/// user `apply` runs as otherwise.
pub fn trusted_owners(euid: u32, sudo_uid: Option<u32>) -> Vec<u32> {
    match (euid, sudo_uid) {
        (0, Some(uid)) if uid != 0 => vec![0, uid],
        (0, _) => vec![0],
        (uid, _) => vec![0, uid],
    }
}

/// The variables the privileged tools set for the user they ran a
/// command for: sudo's, pkexec's and doas's.
const INVOKING_VARS: [&str; 3] = ["SUDO_UID", "PKEXEC_UID", "DOAS_USER"];

/// The user a privileged tool ran `apply` for: `SUDO_UID`, which sudo
/// sets, `PKEXEC_UID`, which pkexec sets, or `DOAS_USER`, the name doas
/// sets, resolved to its uid by `uid_of`. `var` reads the environment.
/// Each is only as trustworthy as the tool that sets it. Each tool sets
/// its own variable alone, so when more than one is set, one was kept
/// from the user's environment or forged, as doas's `keepenv` keeps a
/// `SUDO_UID` the user set, and the reason is returned.
pub fn invoking_user(
    var: impl Fn(&str) -> Option<String>,
    uid_of: impl Fn(&str) -> Option<u32>,
) -> Result<Option<u32>, String> {
    let set: Vec<(&str, String)> = INVOKING_VARS
        .iter()
        .filter_map(|&name| var(name).map(|value| (name, value)))
        .collect();
    match set.as_slice() {
        [] => Ok(None),
        [("DOAS_USER", name)] => Ok(uid_of(name)),
        [(_, uid)] => Ok(uid.parse::<u32>().ok()),
        more => Err(format!(
            "apply refuses to run: {} are all set, but sudo, pkexec and doas each set only their own, so at least one came from your own environment. Run apply through one of them, with the environment it sets.",
            more.iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(" and ")
        )),
    }
}

/// The uid of the user named `name`, as the system's user database gives
/// it.
fn uid_of(name: &str) -> Option<u32> {
    nix::unistd::User::from_name(name)
        .ok()
        .flatten()
        .map(|u| u.uid.as_raw())
}

/// The owners of this process's drafts, from its effective uid and, as
/// root, the user a privileged tool ran it for.
fn owners() -> Result<Vec<u32>, String> {
    let euid = rustix::process::geteuid().as_raw();
    let invoking = match euid {
        0 => invoking_user(|k| std::env::var(k).ok(), uid_of)?,
        _ => None,
    };
    Ok(trusted_owners(euid, invoking))
}

/// Whether a file owned by `uid` is one of `owners`'.
pub fn owner_trusted(uid: u32, owners: &[u32]) -> bool {
    owners.contains(&uid)
}

/// Refuses `fd`, named `name` in messages, when its owner is not one of
/// `owners`.
fn owned(fd: &OwnedFd, name: &str, owners: &[u32]) -> Result<rustix::fs::Stat, String> {
    let stat = rustix::fs::fstat(fd).map_err(|e| format!("{name}: {e}"))?;
    if !owner_trusted(stat.st_uid, owners) {
        return Err(format!(
            "{name} belongs to user {}, who is neither root nor the user who made the draft",
            stat.st_uid
        ));
    }
    Ok(stat)
}

/// A file of the draft read whole, at most `max` bytes, from the
/// directory `dir`: never through a symbolic link, only a regular file
/// with no second name, owned by one of `owners`, so that root reads no
/// file the user could not have written.
fn read_at(dir: &OwnedFd, name: &str, max: u64, owners: &[u32]) -> Result<Vec<u8>, String> {
    let fd = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|e| format!("{name}: {e} (a draft cannot contain a link)"))?;
    let stat = owned(&fd, name, owners)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
        return Err(format!("{name} is not a regular file"));
    }
    if stat.st_nlink != 1 {
        return Err(format!(
            "{name} has a hard link, and a draft cannot contain a link"
        ));
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{name}: {e}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(format!("{name} is larger than {max} bytes"));
    }
    Ok(bytes)
}

/// The manifest's entries, each line checked.
fn entries(text: &str) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let n = n.saturating_add(1);
        let fields: Vec<&str> = line.split('\t').collect();
        let entry = match fields.as_slice() {
            ["accept", "no-complete-route"] => Entry::AcceptNoRoute,
            ["write", name, was] if known(name) => Entry::Write {
                name: (*name).to_owned(),
                was: (*was != "new").then(|| (*was).to_owned()),
            },
            ["remove", name, was] if known(name) && name.starts_with("fonts/") => Entry::Remove {
                name: (*name).to_owned(),
                was: (*was).to_owned(),
            },
            _ => {
                return Err(format!("{MANIFEST} line {n} is not a valid draft line"));
            }
        };
        out.push(entry);
    }
    Ok(out)
}

/// The installed file at `path` as it is now: its hash, or `None` when
/// there is none.
fn installed(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(hash(&bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// A key's name in a message of the console's check, when it reads as
/// one: lowercase letters, digits and `-`, as vtium.conf's keys are.
fn key_in(message: &str) -> Option<&str> {
    let is_key = |k: &str| {
        (1..=64).contains(&k.len())
            && k.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let before_eq = message
        .split('=')
        .next()
        .filter(|k| is_key(k) && message.contains('='));
    let quoted = message
        .strip_prefix("unknown key \"")
        .and_then(|r| r.split('"').next())
        .filter(|k| is_key(k));
    before_eq.or(quoted)
}

/// What the console's check said of the files `copies` (each the copy's
/// path and the name to give it), as root may say it: each refused line
/// by its file, its line and its key, and no text the file holds, since
/// root read the file for the user.
fn summarize(stderr: &str, copies: &[(PathBuf, &str)]) -> String {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let Some((name, rest)) = copies
            .iter()
            .find_map(|(path, name)| line.strip_prefix(path.to_str()?).map(|rest| (*name, rest)))
        else {
            continue;
        };
        let said = match rest.strip_prefix(':').map(|r| r.split_once(':')) {
            Some(Some((n, message))) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() => {
                match key_in(message.trim_start()) {
                    Some(key) => {
                        format!("{name} line {n}: vtium check-config refused the key {key}")
                    }
                    None => format!("{name} line {n}: vtium check-config refused this line"),
                }
            }
            _ => format!("{name}: vtium check-config refused the file"),
        };
        if !out.contains(&said) {
            out.push(said);
        }
    }
    if out.is_empty() {
        out.push("vtium check-config refused the draft".to_owned());
    }
    out.join("\n")
}

/// Runs the console's `vtium check-config` on the draft's vtium.conf,
/// `file`, both its parts in one run, against the installed configuration
/// directory `configdir`, or on the installed vtium.conf where the draft
/// holds none: what its exit status says of the draft, and what it wrote
/// to standard error. When the check fails, [`summarize`] says that as
/// root may say it.
fn check_config(
    vtium: &OsString,
    configdir: &Path,
    file: Option<&Path>,
) -> Result<(Verdict, String), String> {
    let mut command = std::process::Command::new(vtium);
    command
        .arg("check-config")
        .arg("--configdir")
        .arg(configdir);
    if let Some(file) = file {
        command.arg(file);
    }
    let out = command
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", vtium.to_string_lossy()))?;
    Ok((
        Verdict::of(out.status),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// `apply DIR` with its words, writing to `out` and `err`; returns the
/// exit status.
pub fn command(
    words: &[String],
    targets: &Targets,
    helper: &Helper,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let [dir] = words else {
        let _ = writeln!(
            err,
            "vtium-config: apply takes one draft directory\nTry vtium-config --help."
        );
        return 2;
    };
    match apply(Path::new(dir), targets, helper, out) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}\nNothing was installed.");
            1
        }
    }
}

/// Makes `plan`, the change a command computed (ADR-1201 D2), writing to
/// `out` and `err`; returns the exit status. Run as root, it installs the
/// change at once through the steps `apply` takes, from a draft root makes
/// in a directory of its own. Run as a user, it writes the change as a
/// draft under `paths.drafts` and prints each file the draft writes or
/// removes, the draft's directory, and the command that installs it.
pub fn make(plan: &Plan, paths: &Paths, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if plan.is_empty() {
        let _ = writeln!(out, "unchanged");
        return 0;
    }
    let targets = paths.targets();
    if rustix::process::geteuid().is_root() {
        return match install_now(plan, &targets, &paths.helper, out) {
            Ok(true) => 0,
            Ok(false) => 1,
            Err(why) => {
                let _ = writeln!(err, "vtium-config: {why}\nNothing was installed.");
                1
            }
        };
    }
    let draft = match write_draft(&paths.drafts, plan, &targets) {
        Ok(draft) => draft,
        Err(e) => {
            let _ = writeln!(
                err,
                "vtium-config: the draft under {}: {e}",
                paths.drafts.display()
            );
            return 1;
        }
    };
    for file in &plan.files {
        let _ = writeln!(out, "write\t{}", file.path.display());
    }
    for removal in &plan.removals {
        let _ = writeln!(out, "remove\t{}", removal.path.display());
    }
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
    let argv = save_argv(&route(&targets, &paths.sudo), &program, &targets, &draft);
    let shown: Vec<String> = argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let _ = writeln!(out, "draft\t{}", draft.display());
    let _ = writeln!(out, "run\t{}", shown.join(" "));
    0
}

/// Installs `plan` at once, as root: the draft written into a directory of
/// root's own under `/tmp`, then applied, so every file goes through the
/// checks `apply` makes.
pub fn install_now(
    plan: &Plan,
    targets: &Targets,
    helper: &Helper,
    out: &mut dyn Write,
) -> Result<bool, String> {
    install_under(Scratch::system()?, plan, targets, helper, out)
}

/// [`install_now`] with its draft in `scratch`, which goes when it is
/// dropped, whatever the apply did.
fn install_under(
    scratch: Scratch,
    plan: &Plan,
    targets: &Targets,
    helper: &Helper,
    out: &mut dyn Write,
) -> Result<bool, String> {
    let draft = write_draft(&scratch.path, plan, targets)
        .map_err(|e| format!("the draft under {}: {e}", scratch.path.display()))?;
    apply(&draft, targets, helper, out)
}

/// A copy of `bytes`, named `name`, in a directory of this process's own
/// under the system's temporary directory, as `check` hands a file to the
/// console's check; the directory goes when the copy is dropped.
pub struct Copy {
    scratch: Scratch,
    pub path: PathBuf,
}

impl Copy {
    pub fn new(name: &str, bytes: &[u8]) -> Result<Copy, String> {
        let scratch = Scratch::new(
            &std::env::temp_dir(),
            &mut scratch_name,
            rustix::process::geteuid().as_raw(),
        )?;
        let path = scratch.write(name, bytes)?;
        Ok(Copy { scratch, path })
    }

    /// The directory that holds the copy.
    pub fn dir(&self) -> &Path {
        &self.scratch.path
    }
}

/// What names the frit `name` and so keeps it from being removed, in
/// vtium.conf's text `conf`: the top level's `font-name`, or a look's
/// `font-name` in its `[look NAME]` section (ADR-1403 D5), named with its
/// line; `None` when nothing does. The sections are read as the console
/// reads them, what survives their errors included.
pub fn naming(name: &str, conf: Option<&str>) -> Option<String> {
    let conf = conf?;
    let key = vtium_settings::keys::FONT_NAME.name;
    if vtium_settings::setting(conf, key).as_deref() == Some(name) {
        return Some(format!("{key} in vtium.conf"));
    }
    let config = vtium_settings::sections(conf.as_bytes())?.parsed.config;
    config.looks.iter().find_map(|look| {
        look.settings
            .iter()
            .find(|s| s.key.name == key && s.value == name)
            .map(|s| {
                format!(
                    "`{key}` in `[look {}]`, vtium.conf line {},",
                    look.name, s.line
                )
            })
    })
}

/// A file read from the draft, checked, waiting to be written.
struct Staged {
    target: PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Checks the draft in `dir` and installs it: `Ok(true)` when every file
/// was written, `Ok(false)` when a write failed after the checks, and the
/// reason when the draft was refused before anything was written.
fn apply(
    dir: &Path,
    targets: &Targets,
    helper: &Helper,
    out: &mut dyn Write,
) -> Result<bool, String> {
    // a trailing slash makes the kernel follow a link whatever O_NOFOLLOW
    // says, so the draft's own name is opened, without one
    let named = dir.as_os_str().as_encoded_bytes();
    let end = named
        .iter()
        .rposition(|&b| b != b'/')
        .map_or(1.min(named.len()), |i| i.saturating_add(1));
    let dir = Path::new(std::ffi::OsStr::from_bytes(
        named.get(..end).unwrap_or(named),
    ));
    let draft = rustix::fs::open(
        dir,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("{}: {e}", dir.display()))?;
    let owners = owners()?;
    owned(&draft, &dir.display().to_string(), &owners)?;
    let manifest = read_at(&draft, MANIFEST, MANIFEST_MAX, &owners)?;
    let manifest = String::from_utf8(manifest).map_err(|_| format!("{MANIFEST}: not UTF-8"))?;
    let entries = entries(&manifest)?;
    let mut frits = Vec::new();
    let mut batch = None;
    let mut conf = None;
    let mut removals = Vec::new();
    let mut system_font = None;
    let accept_no_route = entries.contains(&Entry::AcceptNoRoute);
    // a font installed as a system font travels in a draft of its own,
    // never beside a change that activates it
    let fonts = entries
        .iter()
        .filter(|e| matches!(e, Entry::Write { name, .. } if system_font_name(name).is_some()))
        .count();
    if fonts > 0 && (fonts > 1 || entries.len() > 1) {
        return Err(format!(
            "{MANIFEST}: a draft that installs a system font holds that one font and nothing else"
        ));
    }
    for entry in &entries {
        let (name, was) = match entry {
            Entry::Write { name, was } => (name, was.clone()),
            Entry::Remove { name, was } => (name, Some(was.clone())),
            Entry::AcceptNoRoute => continue,
        };
        let path = target(targets, name)?;
        // step 2: a file changed since the draft was made is never lost
        if installed(&path)? != was {
            return Err(format!(
                "{} {CHANGED}, so installing the draft would undo that change. Make your change again in a new draft.",
                path.display()
            ));
        }
        let Entry::Write { .. } = entry else {
            removals.push(path);
            continue;
        };
        if let Some(font) = system_font_name(name) {
            let folder = rustix::fs::openat(
                &draft,
                SYSTEM_FONT,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| format!("{SYSTEM_FONT}: {e}"))?;
            owned(&folder, SYSTEM_FONT, &owners)?;
            let bytes = read_at(&folder, font, crate::outline::OUTLINE_BYTES_MAX, &owners)
                .map_err(|e| format!("{SYSTEM_FONT}/{e}"))?;
            described(helper, name, font, &bytes)?;
            system_font = Some(Staged {
                target: path,
                bytes: Some(bytes),
            });
            continue;
        }
        let bytes = match name.strip_prefix("fonts/") {
            Some(file) => {
                // a set of the catalog's name is never installed beside it
                if let (Ok(frits), Some(stem)) = (&targets.frits, file.strip_suffix(".vtfrit")) {
                    crate::switch::unheld(frits, stem)?;
                }
                let fonts = rustix::fs::openat(
                    &draft,
                    "fonts",
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|e| format!("fonts: {e}"))?;
                owned(&fonts, "fonts", &owners)?;
                let bytes = read_at(
                    &fonts,
                    file,
                    u64::try_from(FRIT_BYTES_MAX).unwrap_or(0),
                    &owners,
                )
                .map_err(|e| format!("fonts/{e}"))?;
                let stem = file.strip_suffix(".vtfrit").unwrap_or(file);
                Frit::parse(&bytes, &path, stem).map_err(|e| match e.line {
                    0 => format!("{name}: not a valid font set"),
                    n => format!("{name} line {n}: not a valid font set line"),
                })?;
                bytes
            }
            None if name == BATCH => {
                let bytes = read_at(
                    &draft,
                    name,
                    u64::try_from(vtium_settings::batch::BATCH_MAX).unwrap_or(0),
                    &owners,
                )?;
                check_batch(&bytes, &path)?;
                bytes
            }
            None => read_at(&draft, name, CONF_MAX, &owners)?,
        };
        let staged = Staged {
            target: path,
            bytes: Some(bytes),
        };
        match name.as_str() {
            "vtium.conf" => conf = Some(staged),
            BATCH => batch = Some(staged),
            _ => frits.push(staged),
        }
    }
    if let Some(font) = system_font {
        return install_system_font(&font, out);
    }
    // a draft that moves the frits' directory writes or removes no frit,
    // whose path was taken from the directory before the move
    if let Some(c) = &conf
        && !(frits.is_empty() && removals.is_empty())
    {
        let key = vtium_settings::keys::FONT_FRITS.name;
        let after = vtium_settings::setting(
            &String::from_utf8_lossy(c.bytes.as_deref().unwrap_or_default()),
            key,
        );
        let before = std::fs::read(targets.configdir.join("vtium.conf"))
            .ok()
            .and_then(|bytes| vtium_settings::setting(&String::from_utf8_lossy(&bytes), key));
        if after != before {
            return Err(FRITS_MOVE_FIRST.to_owned());
        }
    }
    // step 3: the console's own check, on a copy root made, so what is
    // checked is what is written
    if let Some(c) = &conf {
        let scratch = Scratch::system()?;
        let file = scratch.write("vtium.conf", c.bytes.as_deref().unwrap_or_default())?;
        let copies = [
            (file.clone(), "vtium.conf"),
            (
                targets.configdir.join("vtium.conf"),
                "the installed vtium.conf",
            ),
        ];
        let checked = check_config(&targets.vtium, &targets.configdir, Some(&file));
        drop(scratch);
        match checked {
            Ok((Verdict::Passed, _)) => {}
            // check-config's own status for a draft whose only problems
            // are missing fonts: the warnings are shown, and the draft
            // installs
            Ok((Verdict::MissingFonts, said)) => {
                for line in said.lines() {
                    let _ = writeln!(out, "{line}");
                }
            }
            // the one problem the person may accept, and only that one
            // (ADR-1403 D8)
            Ok((Verdict::Failed, said))
                if accept_no_route
                    && crate::drafts::only_no_route(
                        &said.lines().collect::<Vec<_>>(),
                        &file.to_string_lossy(),
                    ) =>
            {
                let _ = writeln!(out, "{ACCEPTED_NO_ROUTE}");
            }
            Ok((Verdict::Failed, said)) | Err(said) => {
                return Err(format!(
                    "vtium check-config refused the draft:\n{}\nTo see the full report, run vtium check-config --configdir {} as yourself.",
                    summarize(&said, &copies),
                    dir.display()
                ));
            }
        }
    }
    // a frit vtium.conf names after the save is never removed
    let conf_text = match &conf {
        Some(c) => String::from_utf8_lossy(c.bytes.as_deref().unwrap_or_default()).into_owned(),
        None => std::fs::read_to_string(targets.configdir.join("vtium.conf")).unwrap_or_default(),
    };
    for path in &removals {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if let Some(by) = naming(stem, Some(&conf_text)) {
            return Err(format!("{} is not removed: {by} uses it", path.display()));
        }
    }
    // step 4: frits, the batch file, vtium.conf, then removals
    let order: Vec<Staged> = frits
        .into_iter()
        .chain(batch)
        .chain(conf)
        .chain(removals.into_iter().map(|target| Staged {
            target,
            bytes: None,
        }))
        .collect();
    let mut failed = false;
    for staged in order {
        let path = staged.target.display();
        if failed {
            let _ = writeln!(out, "not written\t{path}");
            continue;
        }
        let result = match &staged.bytes {
            Some(bytes) => {
                vtium_settings::install_file(&staged.target, bytes, 0o644).map(|()| "wrote")
            }
            None => {
                let mut bak = staged.target.clone().into_os_string();
                bak.push(".bak");
                std::fs::rename(&staged.target, bak).map(|()| "removed")
            }
        };
        match result {
            Ok(done) => {
                let _ = writeln!(out, "{done}\t{path}");
            }
            Err(e) => {
                let _ = writeln!(out, "failed\t{path}\t{e}");
                failed = true;
            }
        }
    }
    Ok(!failed)
}

/// Writes a font that a draft installs as a system font: the system fonts
/// folder made when it is missing, with each folder made given mode 0755
/// whatever the umask says, then the font in one rename,
/// mode 0644, owned by the user `apply` runs as, root through sudo. A
/// file the font replaces is not kept beside it, because fontconfig would
/// read a kept copy as one more font. The font is then checked against
/// the trust rule, so the person learns at once whether vtium can
/// activate it. Prints `wrote PATH`, or `failed PATH` with the reason,
/// and `untrusted PATH` with the reason when the font fails the rule.
///
/// References:
/// - ADR-1205 D5: the copy is root's with mode 0644, and vtium-config
///   checks it against D4's rule afterwards.
fn install_system_font(font: &Staged, out: &mut dyn Write) -> Result<bool, String> {
    let path = &font.target;
    let written = (|| {
        if let Some(folder) = path.parent() {
            make_readable_folders(folder)?;
        }
        let replaces = path.exists();
        vtium_settings::install_file(path, font.bytes.as_deref().unwrap_or_default(), 0o644)?;
        if replaces {
            let mut bak = path.clone().into_os_string();
            bak.push(".bak");
            std::fs::remove_file(bak)?;
        }
        Ok::<(), io::Error>(())
    })();
    if let Err(e) = written {
        let _ = writeln!(out, "failed\t{}\t{e}", path.display());
        return Ok(false);
    }
    let _ = writeln!(out, "wrote\t{}", path.display());
    match crate::batch::trusted(path, rustix::process::geteuid().as_raw()) {
        Ok(()) => Ok(true),
        Err(why) => {
            let _ = writeln!(out, "untrusted\t{}\t{why}", path.display());
            Ok(false)
        }
    }
}

/// Checks the font a draft installs as a system font, the draft's file
/// `name` holding `bytes`, with the font helper, which reads fonts in its
/// sandbox, as the console reads them. The helper reads a copy root made
/// of the bytes `apply` read, named `font` as in the draft, so the file
/// the helper judges is the file that is installed. As root, `apply` runs
/// the helper as `nobody`, as the console does. `Err` says why the font is
/// refused: the helper has no description of the file, gave none, or
/// found no character the file maps to a glyph.
///
/// The user's own check before the draft was made proves nothing here.
/// The user owns the draft and can change its copy after that check, and
/// a sudo rule may let a user run `apply` alone. Root then writes the
/// file into a folder that every user's fontconfig reads, as a font that
/// passes vtium's trust rule, so `apply` judges the bytes it writes.
///
/// References:
/// - ADR-1200 D15: the font helper never runs as root, and the console runs it as `nobody`.
/// - ADR-1205 D5: the install step copies one font file into `/usr/local/share/fonts`.
fn described(helper: &Helper, name: &str, font: &str, bytes: &[u8]) -> Result<(), String> {
    use crate::discover::Described;
    let account = if rustix::process::geteuid().is_root() {
        let passwd = std::fs::read_to_string("/etc/passwd")
            .map_err(|e| format!("{name}: vtium-config cannot read /etc/passwd to find the account the font helper runs as: {e}"))?;
        Some(nobody(&passwd).ok_or_else(|| {
            format!("{name}: /etc/passwd has no user nobody for the font helper to run as, and the font helper never runs as root")
        })?)
    } else {
        None
    };
    let scratch = Scratch::system()?;
    let copy = scratch.write(font, bytes)?;
    scratch.share(font)?;
    let said = crate::discover::describe_as(helper, account, &[copy]);
    drop(scratch);
    // a file that starts like an OpenType font, and holds nothing else of
    // one, reads as a face that maps no character
    let empty = "the file maps no character to a glyph, so vtium could draw nothing from it";
    match said.first() {
        Some(Described::Font(d)) if d.faces.iter().any(|f| f.codepoints > 0) => Ok(()),
        Some(Described::Font(_)) => Err(format!(
            "{name}: vtium-config does not install this file as a system font, because the font helper cannot read it as a font: {empty}"
        )),
        Some(Described::NotAFont(why)) => Err(format!(
            "{name}: vtium-config does not install this file as a system font, because the font helper cannot read it as a font: {why}"
        )),
        Some(Described::Unknown(why)) => Err(format!(
            "{name}: vtium-config does not install this file as a system font, because the font helper gave no description of it: {why}"
        )),
        None => Err(format!(
            "{name}: vtium-config does not install this file as a system font, because the font helper gave no description of it"
        )),
    }
}

/// The uid and gid of the user `nobody` in `passwd`, the text of
/// `/etc/passwd`, which the font helper runs as when `apply` runs as root.
/// A `nobody` with uid or gid 0 is no account to drop to, so it counts as
/// missing.
fn nobody(passwd: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some("nobody")).then_some(())?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        (uid != 0 && gid != 0).then_some((uid, gid))
    })
}

/// Makes `folder` and each missing folder above it, each with mode 0755.
/// `sudo` keeps the person's umask, and a umask of 077 would leave a new
/// folder readable by root alone, so neither the font helper, which
/// never runs as root, nor another user's fontconfig could read the font.
/// Each folder is opened without following a link and its mode set on
/// that descriptor, so the mode lands on the folder just made.
fn make_readable_folders(folder: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut at = folder;
    while !at.is_dir() {
        missing.push(at);
        match at.parent() {
            Some(up) => at = up,
            None => break,
        }
    }
    for dir in missing.into_iter().rev() {
        match std::fs::create_dir(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        let fd = rustix::fs::open(
            dir,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        rustix::fs::fchmod(&fd, Mode::from_raw_mode(0o755))?;
    }
    Ok(())
}

/// Checks the batch file `bytes` that a draft installs at `path` before
/// it is written: vtium's reader takes every line that the installed
/// file does not already hold, and each resource the new file lists that
/// the system does not hold now passes the trust rule as the user `apply`
/// runs as, root through sudo. The system holds what the installed batch
/// file lists, as vtium reads it, or the list of a fresh install when
/// vtium reads no batch file. A line that vtium ignores and that root
/// already wrote in the installed file stays, since vtium-config only
/// keeps it. A resource that stays active is not judged again here, since
/// vtium judges every file when it opens it.
///
/// References:
/// - ADR-1205 D4: the trust rule, and vtium-config writing the batch file through its install.
fn check_batch(bytes: &[u8], path: &Path) -> Result<(), String> {
    use crate::batch::Installed;
    use vtium_settings::batch::Finding;
    let euid = rustix::process::geteuid().as_raw();
    let installed = Installed::read(path, euid);
    let held: Vec<&[u8]> = match &installed {
        Installed::Read(old) => old.split(|&b| b == b'\n').collect(),
        Installed::Missing | Installed::Refused(_) => Vec::new(),
    };
    let read = vtium_settings::batch::Batch::parse(bytes, path);
    for finding in &read.findings {
        let kept = match finding {
            Finding::Line { line, .. } => bytes
                .split(|&b| b == b'\n')
                .nth(line.saturating_sub(1))
                .is_some_and(|raw| held.contains(&raw)),
            Finding::Missing { .. } | Finding::Refused { .. } => false,
        };
        if !kept {
            return Err(finding.to_string());
        }
    }
    let base = installed.base(&crate::batch::fresh());
    for vtium_settings::batch::Entry::Path(resource) in read.batch.entries() {
        if !base.holds(resource) {
            crate::batch::trusted(resource, euid)?;
        }
    }
    Ok(())
}

/// A directory of this process's own for the check's copies: under
/// `/tmp`, whatever `TMPDIR` says, since a kept environment can point
/// `TMPDIR` at a directory the user controls; created new with mode 0700
/// and held open, each copy written through it with O_CREAT, O_EXCL and
/// O_NOFOLLOW.
struct Scratch {
    path: PathBuf,
    dir: OwnedFd,
}

impl Scratch {
    /// The check's directory under `/tmp`, named by [`scratch_name`], so a
    /// local user who makes names under `/tmp` first cannot make every
    /// name apply tries taken.
    fn system() -> Result<Scratch, String> {
        Scratch::new(
            Path::new("/tmp"),
            &mut scratch_name,
            rustix::process::geteuid().as_raw(),
        )
    }

    /// A new directory `vtium-config-apply-NAME` under `base`, each NAME
    /// taken from `names`, held only when it belongs to the uid `owner`; a
    /// name already taken, by anyone, is never reused, and the next name is
    /// tried.
    fn new(base: &Path, names: &mut dyn FnMut() -> String, owner: u32) -> Result<Scratch, String> {
        use std::os::unix::fs::DirBuilderExt;
        for _ in 0..100u32 {
            let path = base.join(format!("vtium-config-apply-{}", names()));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
            // From here the directory is this process's, and `Drop` removes
            // it. One this process makes but cannot hold is removed here;
            // remove_dir takes only an empty directory and follows no link.
            // A panic, which aborts a release build, or a Ctrl-C leaves it
            // behind, empty or holding the copies, at mode 0700, where no
            // other user can read it.
            return match Scratch::hold(&path, owner) {
                Ok(dir) => Ok(Scratch { path, dir }),
                Err(why) => {
                    let _ = std::fs::remove_dir(&path);
                    Err(why)
                }
            };
        }
        Err(format!(
            "no free directory for the check in {}",
            base.display()
        ))
    }

    /// The directory at `path` opened, when it is a directory that belongs
    /// to `owner` and no one else can read or write.
    fn hold(path: &Path, owner: u32) -> Result<OwnedFd, String> {
        let dir = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        let stat = rustix::fs::fstat(&dir).map_err(|e| format!("{}: {e}", path.display()))?;
        if stat.st_uid != owner || stat.st_mode & 0o077 != 0 {
            return Err(format!(
                "{} is not a directory this process made",
                path.display()
            ));
        }
        Ok(dir)
    }

    /// Lets the font helper, which runs as `nobody` when this process is
    /// root, read the copy `name`: the copy becomes mode 0644, and the
    /// directory 0711, so other users can pass through it to a name they
    /// know but cannot list it. No one but this process can write either.
    fn share(&self, name: &str) -> Result<(), String> {
        rustix::fs::chmodat(
            &self.dir,
            name,
            Mode::from_raw_mode(0o644),
            rustix::fs::AtFlags::empty(),
        )
        .map_err(|e| format!("{name}: {e}"))?;
        rustix::fs::fchmod(&self.dir, Mode::from_raw_mode(0o711))
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Writes `bytes` as the new file `name` in the directory.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let fd = rustix::fs::openat(
            &self.dir,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| format!("{name}: {e}"))?;
        File::from(fd)
            .write_all(bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        Ok(self.path.join(name))
    }
}

/// A name for the check's directory that no other user can tell in
/// advance: 16 hex digits of a hash keyed by a fresh `RandomState`, whose
/// keys come from the operating system's random source.
fn scratch_name() -> String {
    use std::hash::BuildHasher;
    format!(
        "{:016x}",
        std::hash::RandomState::new().hash_one(std::process::id())
    )
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;

    /// A fresh directory of a test's own under the system's temporary
    /// directory, standing in for `/tmp`. The test removes it when it drops
    /// the guard, whether the test passes or fails.
    struct Base(PathBuf);

    impl std::ops::Deref for Base {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Base {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn base(test: &str) -> Base {
        let base = std::env::temp_dir().join(format!(
            "vtium-config-scratch-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir(&base).unwrap();
        Base(base)
    }

    /// A line that vtium ignores and that the installed batch file does
    /// not hold is refused: vtium-config never writes one itself, so it
    /// came from somewhere else.
    #[test]
    fn a_new_line_that_vtium_ignores_is_refused() {
        let base = base("check-batch");
        let path = base.join("batch.conf");
        let refused = check_batch(b"# a comment\nfonts/x.ttf\n", &path).unwrap_err();
        assert!(
            refused.ends_with(
                "batch.conf:2: this line does not hold an absolute path, one that starts with /. Because of this, vtium ignores the line."
            ),
            "{refused}"
        );
    }

    /// The targets of a test whose configuration directory is `dir`.
    fn targets_in(dir: &Path) -> Targets {
        Targets {
            configdir: dir.to_path_buf(),
            frits: Err("no frits".to_owned()),
            vtium: "/bin/false".into(),
        }
    }

    /// "Install as a system font" writes into `/usr/local/share/fonts` for
    /// the system's directories, and never under `/usr/share/fonts`, which
    /// the packages own.
    #[test]
    fn a_system_font_goes_to_the_administrators_folder_alone() {
        let system = local_fonts(&Targets::system());
        assert_eq!(system, Path::new("/usr/local/share/fonts"));
        assert!(!system.starts_with("/usr/share/fonts"));
        let base = base("local-fonts");
        assert_eq!(local_fonts(&targets_in(&base)), base.join("local-fonts"));
        assert_eq!(
            target(&targets_in(&base), "system-font/A.ttf").unwrap(),
            base.join("local-fonts/A.ttf")
        );
    }

    #[test]
    fn a_system_font_has_the_name_of_a_font_file() {
        for yes in ["A.ttf", "ter-v16n.psf.gz", "My Font.otf"] {
            assert!(valid_font_name(yes), "{yes}");
            assert!(known(&format!("system-font/{yes}")), "{yes}");
        }
        for no in [
            "",
            ".hidden.ttf",
            "a/b.ttf",
            "notes.txt",
            "a\u{1b}.ttf",
            "fonts.dir",
        ] {
            assert!(!valid_font_name(no), "{no:?}");
        }
        assert!(!known("system-font/../vtium.conf"));
    }

    /// Each name a system font may not have is refused with its own
    /// reason.
    #[test]
    fn a_name_a_system_font_may_not_have_says_why() {
        for (name, why) in [
            (".hidden.ttf", "starts with a dot"),
            ("a\u{1b}[31m.ttf", "holds a control character"),
            ("notes.txt", "does not end like a font file's"),
            ("Font.ttf.gz", "does not end like a font file's"),
            ("a/b.ttf", "not the name of a file"),
        ] {
            let said = font_name_refusal(name).unwrap_or_default();
            assert!(said.contains(why), "{name:?}: {said}");
        }
        let long = format!("{}.ttf", "a".repeat(252));
        assert!(font_name_refusal(&long).unwrap().contains("255 bytes"));
        assert_eq!(font_name_refusal("ter-v16n.psf.gz"), None);
    }

    /// A FIFO named like a font is refused at once: vtium-config never
    /// waits for a writer that may never come.
    #[test]
    fn a_fifo_named_like_a_font_is_refused_at_once() {
        let base = base("system-font-fifo");
        let fifo = base.join("Pipe.ttf");
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::FileType::Fifo,
            Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        let targets = targets_in(&base);
        let (done, read) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(SystemFont::read(&fifo, &targets).map(|_| ()));
        });
        let got = read
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("reading a FIFO waited on a writer");
        assert!(got.unwrap_err().contains("not a regular file"));
    }

    /// A font that a package installed is a system font already, also
    /// through a link to it, so it is never installed a second time.
    #[test]
    fn a_packages_own_font_is_not_installed_again() {
        let base = base("system-font-packaged");
        let packaged = base.join("package-fonts/TTF/Theirs.ttf");
        std::fs::create_dir_all(packaged.parent().unwrap()).unwrap();
        std::fs::write(&packaged, b"OTTO").unwrap();
        let link = base.join("Linked.ttf");
        std::os::unix::fs::symlink(&packaged, &link).unwrap();
        for source in [&packaged, &link] {
            let why = SystemFont::read(source, &targets_in(&base)).unwrap_err();
            assert!(why.contains("a system font already"), "{why}");
        }
        assert_eq!(
            package_fonts(&Targets::system()),
            Path::new("/usr/share/fonts")
        );
    }

    /// `apply` never follows a link inside the draft: neither a
    /// `system-font` folder that is a link, nor a font in it that is one.
    #[test]
    fn a_system_font_that_is_a_link_is_refused() {
        let base = base("system-font-link");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("A.ttf"), b"OTTO").unwrap();
        // the folder is a link
        let dir = base.join("draft-folder");
        std::fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join("system-font")).unwrap();
        std::fs::write(dir.join(MANIFEST), "write\tsystem-font/A.ttf\tnew\n").unwrap();
        let why = apply(&dir, &targets_in(&base), &no_helper(), &mut Vec::new()).unwrap_err();
        assert!(why.starts_with("system-font: "), "{why}");
        // the font is a link
        let dir = base.join("draft-file");
        std::fs::create_dir_all(dir.join("system-font")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("A.ttf"), dir.join("system-font/A.ttf")).unwrap();
        std::fs::write(dir.join(MANIFEST), "write\tsystem-font/A.ttf\tnew\n").unwrap();
        let why = apply(&dir, &targets_in(&base), &no_helper(), &mut Vec::new()).unwrap_err();
        assert!(why.starts_with("system-font/A.ttf"), "{why}");
        assert!(!base.join("local-fonts").exists());
    }

    /// A draft that installs a system font holds that font alone, so
    /// installing a font never activates it or changes vtium.conf.
    #[test]
    fn a_system_font_travels_in_a_draft_of_its_own() {
        let base = base("system-font-alone");
        let dir = base.join("draft");
        std::fs::create_dir_all(dir.join("system-font")).unwrap();
        std::fs::write(dir.join("system-font/A.ttf"), b"OTTO").unwrap();
        std::fs::write(dir.join("vtium.conf"), "font-size = 12\n").unwrap();
        std::fs::write(
            dir.join(MANIFEST),
            "write\tsystem-font/A.ttf\tnew\nwrite\tvtium.conf\tnew\n",
        )
        .unwrap();
        let why = apply(&dir, &targets_in(&base), &no_helper(), &mut Vec::new()).unwrap_err();
        assert!(
            why.contains("holds that one font and nothing else"),
            "{why}"
        );
        assert!(!base.join("local-fonts").exists());
    }

    /// A font helper that cannot start, for the checks that refuse a draft
    /// before any font is read.
    fn no_helper() -> Helper {
        Helper::at("/nonexistent/vtium-config")
    }

    /// `apply` installs only a font the font helper describes, so a font
    /// whose helper cannot start is refused, and nothing is written. The
    /// integration tests drive the real helper.
    #[test]
    fn a_system_font_the_font_helper_cannot_describe_is_refused() {
        let base = base("system-font-text");
        let dir = base.join("draft");
        std::fs::create_dir_all(dir.join("system-font")).unwrap();
        std::fs::write(dir.join("system-font/A.ttf"), b"OTTO").unwrap();
        std::fs::write(dir.join(MANIFEST), "write\tsystem-font/A.ttf\tnew\n").unwrap();
        let why = apply(&dir, &targets_in(&base), &no_helper(), &mut Vec::new()).unwrap_err();
        assert!(
            why.starts_with("system-font/A.ttf: vtium-config does not install this file as a system font, because the font helper gave no description of it: cannot start the font helper /nonexistent/vtium-config: "),
            "{why}"
        );
        assert!(!base.join("local-fonts").exists());
    }

    /// The font helper runs as `nobody` when `apply` runs as root, as
    /// `/etc/passwd` names it, and never as an account with uid or gid 0.
    #[test]
    fn the_font_helper_runs_as_nobody_from_passwd() {
        let passwd =
            "root:x:0:0::/root:/bin/bash\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n";
        assert_eq!(nobody(passwd), Some((65534, 65534)));
        assert_eq!(nobody("root:x:0:0::/root:/bin/bash\n"), None);
        assert_eq!(nobody("nobody:x:0:65534::/:/bin/false\n"), None);
        assert_eq!(nobody("nobody:x:65534:0::/:/bin/false\n"), None);
        assert_eq!(nobody("nobody:x:many:65534::/:/bin/false\n"), None);
    }

    /// The names a test's name source gives, in turn, and then `last`.
    fn names(list: &'static [&'static str]) -> impl FnMut() -> String {
        let mut it = list.iter();
        move || (*it.next().unwrap_or(&"last")).to_owned()
    }

    fn euid() -> u32 {
        rustix::process::geteuid().as_raw()
    }

    /// The entries of `dir`, sorted.
    fn entries_of(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    /// A name a local user made first, as this same user and with the
    /// check's own mode, is never taken as the check's directory: its
    /// files are not the check's, and the next name is tried.
    #[test]
    fn a_name_already_taken_is_never_reused() {
        let base = base("taken");
        let taken = base.join("vtium-config-apply-taken");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&taken)
            .unwrap();
        std::fs::write(taken.join("vtium.conf"), "planted\n").unwrap();
        let scratch = Scratch::new(&base, &mut names(&["taken", "free"]), euid()).unwrap();
        assert_eq!(scratch.path, base.join("vtium-config-apply-free"));
        scratch.write("vtium.conf", b"checked\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(taken.join("vtium.conf")).unwrap(),
            "planted\n"
        );
        drop(scratch);
        assert_eq!(entries_of(&base), ["vtium-config-apply-taken"]);
        // every name taken: the check refuses, and leaves what it found
        let why = Scratch::new(&base, &mut || "taken".to_owned(), euid())
            .err()
            .expect("no free name");
        assert!(why.contains("no free directory"), "{why}");
        assert_eq!(entries_of(&base), ["vtium-config-apply-taken"]);
    }

    /// A directory made but not held, here one whose owner is not the uid
    /// asked for, is removed, so a failed check leaves nothing under
    /// `/tmp`.
    #[test]
    fn a_directory_made_but_not_held_is_removed() {
        let base = base("not-held");
        let why = Scratch::new(&base, &mut names(&["made"]), euid().wrapping_add(1))
            .err()
            .expect("another owner's directory is not held");
        assert!(
            why.contains("is not a directory this process made"),
            "{why}"
        );
        assert!(entries_of(&base).is_empty(), "{:?}", entries_of(&base));
    }

    /// Root's change, installed at once: the draft goes into a directory of
    /// its own and through apply's steps, its files land in the targets,
    /// the directory is gone afterwards, and a failing check writes
    /// nothing. Run as the user, apply takes the user's own draft.
    #[test]
    fn root_s_change_installs_through_a_draft_of_its_own_and_leaves_nothing_behind() {
        use crate::draft::FileEdit;
        let base = base("install-now");
        let etc = base.join("etc");
        let fonts = etc.join("fonts");
        std::fs::create_dir_all(&fonts).unwrap();
        std::fs::write(etc.join("vtium.conf"), "font-size=12\n").unwrap();
        let drafts = base.join("drafts");
        std::fs::create_dir(&drafts).unwrap();
        let targets = |vtium: &str| Targets {
            configdir: etc.clone(),
            frits: Ok(fonts.clone()),
            vtium: vtium.into(),
        };
        let scratch = || Scratch::new(&drafts, &mut scratch_name, euid()).unwrap();
        let plan = |frit: &str, conf: &str| Plan {
            files: vec![
                FileEdit {
                    path: fonts.join("mine.vtfrit"),
                    old: None,
                    new: frit.to_owned(),
                },
                FileEdit {
                    path: etc.join("vtium.conf"),
                    old: Some("font-size=12\n".to_owned()),
                    new: conf.to_owned(),
                },
            ],
            removals: Vec::new(),
            no_route: false,
        };
        // a check that fails, the console's or the frit reader's, writes
        // nothing
        let mut out = Vec::new();
        let refused = install_under(
            scratch(),
            &plan("source=builtin:unifont\n", "font-size=13\n"),
            &targets("/bin/false"),
            &no_helper(),
            &mut out,
        );
        assert!(refused.is_err(), "{refused:?}");
        let refused = install_under(
            scratch(),
            &plan("colour=red\n", "font-size=13\n"),
            &targets("/bin/true"),
            &no_helper(),
            &mut out,
        );
        assert!(refused.is_err(), "{refused:?}");
        assert!(!fonts.join("mine.vtfrit").exists());
        assert_eq!(
            std::fs::read_to_string(etc.join("vtium.conf")).unwrap(),
            "font-size=12\n"
        );
        assert!(entries_of(&drafts).is_empty(), "{:?}", entries_of(&drafts));
        // a set named as a set of the catalog's category folders is not
        // installed, because a name stays unique across every folder
        // (ADR-1204 D9)
        let console = fonts.join("console");
        std::fs::create_dir(&console).unwrap();
        std::fs::write(console.join("mine.vtfrit"), "source=builtin:unifont\n").unwrap();
        let refused = install_under(
            scratch(),
            &plan("source=builtin:unifont\n", "font-size=13\n"),
            &targets("/bin/true"),
            &no_helper(),
            &mut out,
        );
        let why = refused.unwrap_err();
        assert!(why.contains("already installed as"), "{why}");
        assert!(!fonts.join("mine.vtfrit").exists());
        std::fs::remove_dir_all(&console).unwrap();
        // a change every check takes lands in the targets
        let written = install_under(
            scratch(),
            &plan("source=builtin:unifont\n", "font-size=13\n"),
            &targets("/bin/true"),
            &no_helper(),
            &mut out,
        );
        assert_eq!(written, Ok(true), "{}", String::from_utf8_lossy(&out));
        assert_eq!(
            std::fs::read_to_string(fonts.join("mine.vtfrit")).unwrap(),
            "source=builtin:unifont\n"
        );
        assert_eq!(
            std::fs::read_to_string(etc.join("vtium.conf")).unwrap(),
            "font-size=13\n"
        );
        assert!(entries_of(&drafts).is_empty(), "{:?}", entries_of(&drafts));
    }

    /// The system's names cannot be told in advance from the process's id
    /// or a count, so a local user cannot make them all first.
    #[test]
    fn the_system_s_names_are_not_predictable() {
        let pid = std::process::id().to_string();
        let drawn: Vec<String> = (0..8).map(|_| scratch_name()).collect();
        for name in &drawn {
            assert_eq!(name.len(), 16, "{name}");
            assert!(name.bytes().all(|b| b.is_ascii_hexdigit()), "{name}");
            assert!(!name.contains(&pid), "{name}");
        }
        let mut unique = drawn.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), drawn.len(), "{drawn:?}");
    }
}
