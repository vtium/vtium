//! Saving (ADR-1201 D5): the draft a user's save becomes, and `apply DIR`,
//! the privileged step that checks a draft and installs it.
//!
//! A draft is a directory the user owns, under `$XDG_STATE_HOME/vtium/drafts`:
//! the whole edited `vtium.conf` and `heads.conf`, each frit to write under
//! `fonts/`, and `MANIFEST`, one line a file, tab-separated: `write` or
//! `remove`, the file's name in the draft, and the SHA-256 of the installed
//! file it replaces as it was when the draft was made, or `new`.
//!
//! `apply`, run as root through `sudo vtium-config apply DIR`:
//!
//! 1. reads the draft without following a link, takes only regular files
//!    with names it knows, and bounds each file's size, since root is
//!    reading a directory the user owns;
//! 2. refuses the draft, writing nothing, when a file it replaces has
//!    changed since the draft was made, naming the file;
//! 3. checks every file before it writes any: each frit with the helper's
//!    frit reader, strictly, and the edited vtium.conf and head file with
//!    the console's own `vtium check-config`, so the verdict before writing
//!    is the verdict at the next start;
//! 4. writes the frits, then the head file, then vtium.conf, so neither
//!    configuration file names a frit that is not yet there, and removes
//!    frits last; each file goes in through
//!    [`vtium_settings::install_file`], in one rename, mode 0644, the old
//!    one kept as `NAME.bak`;
//! 5. prints one line a file, `wrote PATH`, `removed PATH`, `failed PATH`
//!    with the reason, or `not written PATH` after a failure, and exits 0
//!    when it wrote them all.
//!
//! The draft stays after `apply`, so a user can run it again.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::{FileType, Mode, OFlags};
use vtium_font::{Sha256, sha256_hex};

use crate::draft::Plan;
use crate::frit::{FRIT_BYTES_MAX, Frit, valid_name};
use crate::tui::{CONF_MAX, Paths};

/// Runs an argv with the terminal given back for a password, and says
/// whether it succeeded, with what it printed.
pub type Runner<'a> = &'a mut dyn FnMut(&[OsString]) -> io::Result<(bool, String)>;

/// What the TUI calls to save: writes `plan`'s draft and runs the
/// privileged step through the runner; the files written, or why none
/// was, with what the step printed.
pub type Saver = Box<dyn FnMut(&Plan, Runner<'_>) -> Result<usize, String>>;

/// The draft's list of files.
pub const MANIFEST: &str = "MANIFEST";

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
    pub frits: PathBuf,
    pub vtium: OsString,
}

impl Targets {
    /// The options that name these targets, as `apply` takes them.
    pub fn options(&self) -> Vec<OsString> {
        vec![
            "--configdir".into(),
            self.configdir.clone().into(),
            "--frits".into(),
            self.frits.clone().into(),
            "--vtium".into(),
            self.vtium.clone(),
        ]
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
}

fn hash(bytes: &[u8]) -> String {
    sha256_hex(&Sha256::digest(bytes))
}

/// Whether `name` is a name a draft may hold: `vtium.conf`, `heads.conf`,
/// or `fonts/NAME.vtfrit` for a frit's name.
fn known(name: &str) -> bool {
    matches!(name, "vtium.conf" | "heads.conf")
        || name
            .strip_prefix("fonts/")
            .and_then(|n| n.strip_suffix(".vtfrit"))
            .is_some_and(valid_name)
}

/// The installed path the draft's file `name` replaces.
fn target(targets: &Targets, name: &str) -> PathBuf {
    match name.strip_prefix("fonts/") {
        Some(frit) => targets.frits.join(frit),
        None => targets.configdir.join(name),
    }
}

/// The draft's name for the installed file `path`, when it is one a draft
/// holds.
fn name_for(targets: &Targets, path: &Path) -> Option<String> {
    if path.parent() == Some(targets.configdir.as_path()) {
        let name = path.file_name()?.to_str()?;
        return matches!(name, "vtium.conf" | "heads.conf").then(|| name.to_owned());
    }
    if path.parent() == Some(targets.frits.as_path()) {
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
                format!("{} is no file a draft holds", file.path.display()),
            )
        })?;
        std::fs::write(dir.join(&name), &file.new)?;
        let was = file
            .old
            .as_deref()
            .map_or_else(|| "new".to_owned(), |old| hash(old.as_bytes()));
        manifest.push_str(&format!("write\t{name}\t{was}\n"));
    }
    std::fs::write(dir.join(MANIFEST), manifest)?;
    Ok(dir)
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

/// The argv of the privileged step: `sudo vtium-config [OPTIONS] apply
/// DIR`, or without `sudo` when the TUI already runs as root.
pub fn privileged_argv(
    sudo: &OsString,
    program: &Path,
    targets: &Targets,
    draft: &Path,
    root: bool,
) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();
    if !root {
        argv.push(sudo.clone());
    }
    argv.push(program.into());
    argv.push(crate::banner::OFF.into());
    argv.extend(targets.options());
    argv.push("apply".into());
    argv.push(draft.into());
    argv
}

/// The TUI's saver for `paths`: the draft under `paths.drafts`, then the
/// privileged step on it.
pub fn saver(paths: Paths) -> Saver {
    Box::new(move |plan, run| {
        let targets = paths.targets();
        let draft = write_draft(&paths.drafts, plan, &targets)
            .map_err(|e| format!("the draft under {}: {e}", paths.drafts.display()))?;
        let program = std::env::current_exe().map_err(|e| e.to_string())?;
        let root = rustix::process::geteuid().is_root();
        let argv = privileged_argv(&paths.sudo, &program, &targets, &draft, root);
        match run(&argv) {
            Ok((true, _)) => Ok(plan.files.len()),
            Ok((false, said)) => Err(format!(
                "the command failed, and the draft stays in {}.\n{}",
                draft.display(),
                said.trim_end()
            )),
            Err(e) => Err(format!("{}: {e}", paths.sudo.to_string_lossy())),
        }
    })
}

/// A file of the draft read whole, at most `max` bytes, from the
/// directory `dir`, never through a link and only a regular file.
fn read_at(dir: &OwnedFd, name: &str, max: u64) -> Result<Vec<u8>, String> {
    let fd = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|e| format!("{name}: {e}"))?;
    let stat = rustix::fs::fstat(&fd).map_err(|e| format!("{name}: {e}"))?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
        return Err(format!("{name}: not a regular file"));
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{name}: {e}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(format!("{name}: larger than {max} bytes"));
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
            ["write", name, was] if known(name) => Entry::Write {
                name: (*name).to_owned(),
                was: (*was != "new").then(|| (*was).to_owned()),
            },
            ["remove", name, was] if known(name) && name.starts_with("fonts/") => Entry::Remove {
                name: (*name).to_owned(),
                was: (*was).to_owned(),
            },
            _ => {
                return Err(format!(
                    "{MANIFEST}:{n}: not a line a draft holds: {line:?}"
                ));
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

/// Runs the console's `vtium check-config` on the draft's configuration:
/// vtium.conf in `confdir`, and the head file `heads` in the head file's
/// place; the problems it names when the check fails.
fn check_config(vtium: &OsString, confdir: &Path, heads: Option<&Path>) -> Result<(), String> {
    let mut command = std::process::Command::new(vtium);
    command.arg("check-config").arg("--configdir").arg(confdir);
    if let Some(heads) = heads {
        command.arg(heads);
    }
    let out = command
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", vtium.to_string_lossy()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim_end().to_owned())
    }
}

/// `apply DIR` with its words, writing to `out` and `err`; returns the
/// exit status.
pub fn command(
    words: &[String],
    targets: &Targets,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let [dir] = words else {
        let _ = writeln!(
            err,
            "vtium-config: apply takes one draft's directory\nTry vtium-config --help."
        );
        return 2;
    };
    match apply(Path::new(dir), targets, out) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}\nNothing was written.");
            1
        }
    }
}

/// A file read from the draft, checked, waiting to be written.
struct Staged {
    target: PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Checks the draft in `dir` and installs it: `Ok(true)` when every file
/// was written, `Ok(false)` when a write failed after the checks, and the
/// reason when the draft was refused before anything was written.
fn apply(dir: &Path, targets: &Targets, out: &mut dyn Write) -> Result<bool, String> {
    let draft = rustix::fs::open(
        dir,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("{}: {e}", dir.display()))?;
    let manifest = read_at(&draft, MANIFEST, MANIFEST_MAX)?;
    let manifest = String::from_utf8(manifest).map_err(|_| format!("{MANIFEST}: not UTF-8"))?;
    let entries = entries(&manifest)?;
    let mut frits = Vec::new();
    let mut heads = None;
    let mut conf = None;
    let mut removals = Vec::new();
    for entry in &entries {
        let (name, was) = match entry {
            Entry::Write { name, was } => (name, was.clone()),
            Entry::Remove { name, was } => (name, Some(was.clone())),
        };
        let path = target(targets, name);
        // step 2: a file changed since the draft was made is never lost
        if installed(&path)? != was {
            return Err(format!(
                "{} changed since the draft was made, so the draft would undo that change. Make the change again in a new draft.",
                path.display()
            ));
        }
        let Entry::Write { .. } = entry else {
            removals.push(path);
            continue;
        };
        let bytes = match name.strip_prefix("fonts/") {
            Some(file) => {
                let fonts = rustix::fs::openat(
                    &draft,
                    "fonts",
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|e| format!("fonts: {e}"))?;
                let bytes = read_at(&fonts, file, u64::try_from(FRIT_BYTES_MAX).unwrap_or(0))?;
                let stem = file.strip_suffix(".vtfrit").unwrap_or(file);
                Frit::parse(&bytes, &path, stem).map_err(|e| e.to_string())?;
                bytes
            }
            None => read_at(&draft, name, CONF_MAX)?,
        };
        let staged = Staged {
            target: path,
            bytes: Some(bytes),
        };
        match name.as_str() {
            "heads.conf" => heads = Some(staged),
            "vtium.conf" => conf = Some(staged),
            _ => frits.push(staged),
        }
    }
    // step 3: the console's own check, on copies root made, so what is
    // checked is what is written
    if heads.is_some() || conf.is_some() {
        let scratch = tempdir()?;
        let confdir = match &conf {
            Some(c) => {
                std::fs::write(
                    scratch.join("vtium.conf"),
                    c.bytes.as_deref().unwrap_or_default(),
                )
                .map_err(|e| e.to_string())?;
                scratch.clone()
            }
            None => targets.configdir.clone(),
        };
        let heads_file = match &heads {
            Some(h) => {
                let file = scratch.join("heads.conf");
                std::fs::write(&file, h.bytes.as_deref().unwrap_or_default())
                    .map_err(|e| e.to_string())?;
                Some(file)
            }
            None => Some(targets.configdir.join("heads.conf")).filter(|p| p.exists()),
        };
        let checked = check_config(&targets.vtium, &confdir, heads_file.as_deref());
        let _ = std::fs::remove_dir_all(&scratch);
        checked.map_err(|why| format!("the console's check refused the draft:\n{why}"))?;
    }
    // a frit vtium.conf names after the save is never removed
    let conf_text = match &conf {
        Some(c) => String::from_utf8_lossy(c.bytes.as_deref().unwrap_or_default()).into_owned(),
        None => std::fs::read_to_string(targets.configdir.join("vtium.conf")).unwrap_or_default(),
    };
    if let Some(named) = vtium_settings::setting(&conf_text, "font-name")
        && let Some(path) = removals
            .iter()
            .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(named.as_str()))
    {
        return Err(format!(
            "{} is the frit vtium.conf's font-name names, so it is not removed",
            path.display()
        ));
    }
    // step 4: frits, the head file, vtium.conf, then removals
    let order: Vec<Staged> = frits
        .into_iter()
        .chain(heads)
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

/// A new directory of root's own for the check's copies.
fn tempdir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    for n in 0..100u32 {
        let dir = base.join(format!("vtium-config-apply-{}-{n}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
                return Ok(dir);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        }
    }
    Err("no free directory for the check".to_owned())
}
