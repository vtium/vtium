//! Saving (ADR-1201 D5): the draft a user's save becomes, and `apply DIR`,
//! the privileged step that checks a draft and installs it.
//!
//! A draft is a directory the user owns, under `$XDG_STATE_HOME/vtium/drafts`:
//! the whole edited `vtium.conf`, each frit to write under
//! `fonts/`, and `MANIFEST`, one line a file, tab-separated: `write` or
//! `remove`, the file's name in the draft, and the SHA-256 of the installed
//! file it replaces as it was when the draft was made, or `new`. A last
//! line, `accept` and `no-complete-route`, records that the person
//! confirmed a vtium.conf that wires no complete route, or ran `install
//! --force` (ADR-1403 D8).
//!
//! `apply`, run as root through `sudo vtium-config apply DIR`, takes no
//! option that names a path or a program ([`crate::cli::root_refusal`]),
//! so it writes only `/etc/vtium/vtium.conf` and the frits in the frits'
//! directory the installed vtium.conf names in `font-frits`, else the one
//! the build names, read as the console reads it
//! ([`crate::switch::frits_dir`]), and checks them with the console the
//! build names, [`VTIUM`]:
//!
//! 1. reads the draft without following a link, takes only regular files
//!    with names it knows, and bounds each file's size, since root is
//!    reading a directory the user owns;
//! 2. refuses the draft, writing nothing, when a file it replaces has
//!    changed since the draft was made, naming the file;
//! 3. checks every file before it writes any: each frit with the helper's
//!    frit reader, strictly, and the edited vtium.conf, both its parts in
//!    one run, with the console's own `vtium check-config`, so the verdict
//!    before writing is the verdict at the next start (ADR-1403 D3);
//! 4. writes the frits, then vtium.conf, so vtium.conf never names a frit
//!    that is not yet there, and removes frits last; each file goes in
//!    through
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
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::fs::{FileType, Mode, OFlags};
use vtium_font::{Sha256, sha256_hex};

use crate::draft::Plan;
use crate::frit::{FRIT_BYTES_MAX, Frit, valid_name};
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

/// Whether `name` is a name a draft may hold: `vtium.conf`, or
/// `fonts/NAME.vtfrit` for a frit's name.
fn known(name: &str) -> bool {
    name == "vtium.conf"
        || name
            .strip_prefix("fonts/")
            .and_then(|n| n.strip_suffix(".vtfrit"))
            .is_some_and(valid_name)
}

/// The installed path the draft's file `name` replaces, or why a frit
/// has none: there is no frits' directory.
fn target(targets: &Targets, name: &str) -> Result<PathBuf, String> {
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
/// holds none; what it wrote to standard error when the check fails,
/// which [`summarize`] says as root may say it.
fn check_config(vtium: &OsString, configdir: &Path, file: Option<&Path>) -> Result<(), String> {
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
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
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
            "vtium-config: apply takes one draft directory\nTry vtium-config --help."
        );
        return 2;
    };
    match apply(Path::new(dir), targets, out) {
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
        return match install_now(plan, &targets, out) {
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
pub fn install_now(plan: &Plan, targets: &Targets, out: &mut dyn Write) -> Result<bool, String> {
    install_under(Scratch::system()?, plan, targets, out)
}

/// [`install_now`] with its draft in `scratch`, which goes when it is
/// dropped, whatever the apply did.
fn install_under(
    scratch: Scratch,
    plan: &Plan,
    targets: &Targets,
    out: &mut dyn Write,
) -> Result<bool, String> {
    let draft = write_draft(&scratch.path, plan, targets)
        .map_err(|e| format!("the draft under {}: {e}", scratch.path.display()))?;
    apply(&draft, targets, out)
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
fn apply(dir: &Path, targets: &Targets, out: &mut dyn Write) -> Result<bool, String> {
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
    let mut conf = None;
    let mut removals = Vec::new();
    let accept_no_route = entries.contains(&Entry::AcceptNoRoute);
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
        let bytes = match name.strip_prefix("fonts/") {
            Some(file) => {
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
            None => read_at(&draft, name, CONF_MAX, &owners)?,
        };
        let staged = Staged {
            target: path,
            bytes: Some(bytes),
        };
        match name.as_str() {
            "vtium.conf" => conf = Some(staged),
            _ => frits.push(staged),
        }
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
            Ok(()) => {}
            // the one problem the person may accept, and only that one
            // (ADR-1403 D8)
            Err(said)
                if accept_no_route
                    && crate::drafts::only_no_route(
                        &said.lines().collect::<Vec<_>>(),
                        &file.to_string_lossy(),
                    ) =>
            {
                let _ = writeln!(out, "{ACCEPTED_NO_ROUTE}");
            }
            Err(said) => {
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
    // step 4: frits, vtium.conf, then removals
    let order: Vec<Staged> = frits
        .into_iter()
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
            &mut out,
        );
        assert!(refused.is_err(), "{refused:?}");
        let refused = install_under(
            scratch(),
            &plan("colour=red\n", "font-size=13\n"),
            &targets("/bin/true"),
            &mut out,
        );
        assert!(refused.is_err(), "{refused:?}");
        assert!(!fonts.join("mine.vtfrit").exists());
        assert_eq!(
            std::fs::read_to_string(etc.join("vtium.conf")).unwrap(),
            "font-size=12\n"
        );
        assert!(entries_of(&drafts).is_empty(), "{:?}", entries_of(&drafts));
        // a change every check takes lands in the targets
        let written = install_under(
            scratch(),
            &plan("source=builtin:unifont\n", "font-size=13\n"),
            &targets("/bin/true"),
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
