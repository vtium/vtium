//! The command line (ADR-1200 D4): non-interactive and idempotent. Each
//! command that builds or installs prints one line a set (`built`,
//! `current`, `installed` or `failed`, the name and the reason), and exits
//! 0 when every set succeeded or was current, 1 when any failed and 2 on a
//! usage error. A command that writes the default store refuses to run
//! without root, naming `sudo`.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use rustix::process::geteuid;
use vtium_font::set::{ManifestRow, Set, Store, VERSION, builtin};
use vtium_font::{SET_BYTES_MAX, sha256_hex};

use crate::build::build_key;
use crate::child::Parser;
use crate::frit::{Frit, RESERVED, valid_name};
use crate::sources::{
    Consoles, FONT_DIRS, Host, KBD_DIRS, KernelConsoles, Resolved, SourceError, resolve,
};
use crate::store::{StoreWriter, default_frits, default_store, manifest, write_atomic};

const USAGE: &str = "\
Usage: vtium-vitrify [OPTION]... COMMAND [ARG]...

Builds vitrified font sets from frits and keeps vtium's font store.

Commands:
  create [--force] NAME|FRIT...      build frits into the store (root)
  create [--force] --all             build every frit in the frits directory (root)
  create [--force] --kbd NAME | --file PATH | --vconsole | --console VT
                                     write the one-source frit and build it (root)
  create --out FILE FRIT             build a portable set into FILE (any user)
  install FILE...                    check portable sets and add them to the store (root)
  list [--json]                      list the store's sets and the built-ins
  verify [NAME...]                   run the console's check on the store's sets
  remove NAME...                     drop sets from the store; their frits stay (root)

Options:
  --store DIR           a store the invoking user owns, in place of the system's
  --frits DIR           the frits directory, in place of the system's
  --kbd-dir DIR         a kbd data directory, in place of the build's list (repeatable)
  --font-dir DIR        an outline font directory, in place of the build's list (repeatable)
  --vconsole-conf FILE  vconsole.conf, in place of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, in place of /proc/cmdline
  --help, --version
";

/// What the command line asked for.
#[derive(Debug, Default)]
struct Options {
    store: Option<PathBuf>,
    frits: Option<PathBuf>,
    kbd_dirs: Vec<PathBuf>,
    font_dirs: Vec<PathBuf>,
    vconsole_conf: Option<PathBuf>,
    kernel_cmdline: Option<PathBuf>,
    force: bool,
    all: bool,
    json: bool,
    out: Option<PathBuf>,
    shortcut: Option<Shortcut>,
    words: Vec<String>,
}

/// `create`'s one-source forms.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shortcut {
    Kbd(String),
    File(PathBuf),
    Vconsole,
    Console(u16),
}

/// A usage error's message.
struct Usage(String);

fn parse_args(args: &[OsString]) -> Result<Options, Usage> {
    let mut o = Options::default();
    let mut it = args.iter();
    let text = |a: &OsString| {
        a.to_str()
            .map(str::to_owned)
            .ok_or_else(|| Usage(format!("{} is not UTF-8", a.to_string_lossy())))
    };
    while let Some(arg) = it.next() {
        let arg = text(arg)?;
        let mut value = |name: &str| {
            it.next()
                .ok_or_else(|| Usage(format!("{name} takes a value")))
                .and_then(text)
        };
        let shortcut = |o: &mut Options, s: Shortcut| {
            if o.shortcut.replace(s).is_some() {
                return Err(Usage(
                    "give one of --kbd, --file, --vconsole and --console".to_owned(),
                ));
            }
            Ok(())
        };
        match arg.as_str() {
            "--store" => o.store = Some(value("--store")?.into()),
            "--frits" => o.frits = Some(value("--frits")?.into()),
            "--kbd-dir" => o.kbd_dirs.push(value("--kbd-dir")?.into()),
            "--font-dir" => o.font_dirs.push(value("--font-dir")?.into()),
            "--vconsole-conf" => o.vconsole_conf = Some(value("--vconsole-conf")?.into()),
            "--kernel-cmdline" => o.kernel_cmdline = Some(value("--kernel-cmdline")?.into()),
            "--out" => o.out = Some(value("--out")?.into()),
            "--force" => o.force = true,
            "--all" => o.all = true,
            "--json" => o.json = true,
            "--kbd" => {
                let name = value("--kbd")?;
                shortcut(&mut o, Shortcut::Kbd(name))?;
            }
            "--file" => {
                let path = value("--file")?;
                shortcut(&mut o, Shortcut::File(path.into()))?;
            }
            "--vconsole" => shortcut(&mut o, Shortcut::Vconsole)?,
            "--console" => {
                let vt = value("--console")?;
                let vt = vt
                    .parse()
                    .map_err(|_| Usage(format!("--console takes a VT's number, not {vt}")))?;
                shortcut(&mut o, Shortcut::Console(vt))?;
            }
            "--" => {
                for rest in it.by_ref() {
                    o.words.push(text(rest)?);
                }
            }
            a if a.starts_with('-') && a != "-" && !matches!(a, "--help" | "--version") => {
                return Err(Usage(format!("unknown option {a}")));
            }
            _ => o.words.push(arg),
        }
    }
    Ok(o)
}

/// Runs the command line `args` (the program's name left out), writing to
/// `out` and `err`; returns the exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let o = match parse_args(args) {
        Ok(o) => o,
        Err(Usage(why)) => {
            let _ = writeln!(err, "vtium-vitrify: {why}\nTry vtium-vitrify --help.");
            return 2;
        }
    };
    let (command, rest) = match o.words.split_first() {
        Some((c, rest)) => (c.as_str(), rest.to_vec()),
        None => ("", Vec::new()),
    };
    let mut cli = Cli { o: &o, out, err };
    match command {
        "--help" => cli.say(USAGE.trim_end()),
        "--version" => cli.say(concat!("vtium-vitrify ", env!("CARGO_PKG_VERSION"))),
        "create" => cli.create(&rest),
        "install" => cli.install(&rest),
        "list" => cli.list(&rest),
        "verify" => cli.verify(&rest),
        "remove" => cli.remove(&rest),
        "" => cli.usage("no command"),
        other => cli.usage(&format!("unknown command {other}")),
    }
}

struct Cli<'a> {
    o: &'a Options,
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
}

/// One set's outcome, as its line reports it.
enum Outcome {
    Built,
    Current,
    /// Built, or current, with these sources left out as missing.
    BuiltPartial(Vec<String>),
    CurrentPartial(Vec<String>),
    Installed,
    Removed,
    Verified,
    Failed(String),
}

impl Cli<'_> {
    fn say(self, text: &str) -> u8 {
        let _ = writeln!(self.out, "{text}");
        0
    }

    fn usage(&mut self, why: &str) -> u8 {
        let _ = writeln!(self.err, "vtium-vitrify: {why}\nTry vtium-vitrify --help.");
        2
    }

    fn fail(&mut self, why: &str) -> u8 {
        let _ = writeln!(self.err, "vtium-vitrify: {why}");
        1
    }

    /// Prints `name`'s line; true when it did not fail.
    fn report(&mut self, name: &str, outcome: Outcome) -> bool {
        let word = match &outcome {
            Outcome::Built => "built",
            Outcome::Current => "current",
            Outcome::Installed => "installed",
            Outcome::Removed => "removed",
            Outcome::Verified => "verified",
            Outcome::BuiltPartial(left) | Outcome::CurrentPartial(left) => {
                let word = if matches!(outcome, Outcome::BuiltPartial(_)) {
                    "built-partial"
                } else {
                    "current-partial"
                };
                let _ = writeln!(self.out, "{word}\t{name}\tleft out: {}", left.join(", "));
                return true;
            }
            Outcome::Failed(why) => {
                let _ = writeln!(self.out, "failed\t{name}\t{why}");
                return false;
            }
        };
        let _ = writeln!(self.out, "{word}\t{name}");
        true
    }

    /// The store path and the owner its check expects: the system's,
    /// owned by root, or `--store`'s, owned by the invoking user.
    fn store_path(&self) -> (PathBuf, u32) {
        match &self.o.store {
            Some(p) => (p.clone(), geteuid().as_raw()),
            None => (default_store(), 0),
        }
    }

    /// `None` when the user may write the store; else the refusal's
    /// message.
    fn refuse_without_root(&self, command: &str) -> Option<String> {
        (self.o.store.is_none() && !geteuid().is_root()).then(|| {
            format!(
                "{command} writes the store {} and needs root: run it with sudo vtium-vitrify {command} ...",
                default_store().display()
            )
        })
    }

    fn frits_dir(&self) -> PathBuf {
        self.o.frits.clone().unwrap_or_else(default_frits)
    }

    fn host<'c>(&self, consoles: &'c dyn Consoles) -> Host<'c> {
        Host {
            kbd_dirs: if self.o.kbd_dirs.is_empty() {
                KBD_DIRS.iter().map(PathBuf::from).collect()
            } else {
                self.o.kbd_dirs.clone()
            },
            font_dirs: if self.o.font_dirs.is_empty() {
                default_font_dirs()
            } else {
                self.o.font_dirs.clone()
            },
            vconsole_conf: self
                .o
                .vconsole_conf
                .clone()
                .unwrap_or_else(|| "/etc/vconsole.conf".into()),
            kernel_cmdline: self
                .o
                .kernel_cmdline
                .clone()
                .unwrap_or_else(|| "/proc/cmdline".into()),
            consoles,
        }
    }

    /// A frit named on the command line: a path when it holds a `/`,
    /// else `<frits>/<name>.vtfrit`.
    fn frit_path(&self, word: &str) -> PathBuf {
        if word.contains('/') {
            PathBuf::from(word)
        } else {
            self.frits_dir().join(format!("{word}.vtfrit"))
        }
    }

    fn create(mut self, words: &[String]) -> u8 {
        let o = self.o;
        if let Some(out) = &o.out {
            if o.all || o.shortcut.is_some() || words.len() != 1 {
                return self.usage("create --out FILE takes one FRIT");
            }
            let path = self.frit_path(words.first().map_or("", String::as_str));
            return self.create_out(&path, out);
        }
        let targets: Vec<PathBuf> =
            match (&o.shortcut, o.all, words.is_empty()) {
                (Some(_), false, true) | (None, true, true) => Vec::new(),
                (None, false, false) => words.iter().map(|w| self.frit_path(w)).collect(),
                _ => return self.usage(
                    "create takes frits, --all, or one of --kbd, --file, --vconsole and --console",
                ),
            };
        if let Some(why) = self.refuse_without_root("create") {
            return self.fail(&why);
        }
        let targets = if let Some(s) = &o.shortcut {
            match self.write_shortcut(s) {
                Ok(path) => vec![path],
                Err(why) => return self.fail(&why),
            }
        } else if o.all {
            match frits_in(&self.frits_dir()) {
                Ok(t) => t,
                Err(why) => return self.fail(&why),
            }
        } else {
            targets
        };
        let (path, uid) = self.store_path();
        let writer = match StoreWriter::open(&path, uid) {
            Ok(w) => w,
            Err(why) => return self.fail(&why),
        };
        let parser = match Parser::new() {
            Ok(p) => p,
            Err(why) => return self.fail(&why),
        };
        let consoles = KernelConsoles;
        let host = self.host(&consoles);
        let mut ok = true;
        for target in targets {
            let (name, outcome) =
                build_into(&writer, &target, &host, &parser, o.force, &mut *self.err);
            ok &= self.report(&name, outcome);
        }
        u8::from(!ok)
    }

    fn create_out(&mut self, frit: &Path, out: &Path) -> u8 {
        let consoles = KernelConsoles;
        let host = self.host(&consoles);
        let built = Parser::new().and_then(|parser| {
            let frit = Frit::load(frit).map_err(|e| e.to_string())?;
            warn(&mut *self.err, &frit);
            let read = read_sources(&frit, &host)?;
            warn_all(&mut *self.err, &read.warnings);
            let bytes = parse_and_assemble(&frit, &read, &parser)?;
            write_atomic(out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
            Ok((frit.name.clone(), read.outcome(&frit, false)))
        });
        match built {
            Ok((name, outcome)) => u8::from(!self.report(&name, outcome)),
            Err(why) => {
                let name = frit_name(frit);
                u8::from(!self.report(&name, Outcome::Failed(why)))
            }
        }
    }

    /// Writes the one-source frit a shortcut implies, refusing to
    /// overwrite a different frit of that name; returns its path.
    fn write_shortcut(&self, s: &Shortcut) -> Result<PathBuf, String> {
        let (name, source) = match s {
            Shortcut::Kbd(name) => (font_name(name), format!("kbd:{name}")),
            Shortcut::File(path) => {
                if !path.is_absolute() {
                    return Err("--file takes an absolute path".to_owned());
                }
                (
                    font_name(&path.to_string_lossy()),
                    format!("file:{}", path.display()),
                )
            }
            Shortcut::Vconsole => {
                let consoles = KernelConsoles;
                let host = self.host(&consoles);
                let text = |path: &Path| {
                    crate::sources::read_regular(path, crate::sources::CONFIG_BYTES_MAX)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default()
                };
                let (conf, cmdline) = (text(&host.vconsole_conf), text(&host.kernel_cmdline));
                let font = crate::sources::vconsole(&conf, &cmdline)
                    .0
                    .ok_or("vconsole.conf names no FONT=")?;
                (font_name(&font), "vconsole".to_owned())
            }
            Shortcut::Console(vt) => ("console".to_owned(), format!("console:{vt}")),
        };
        if !valid_name(&name) || RESERVED.contains(&name.as_str()) {
            return Err(format!("{name:?} is not a name a set may take"));
        }
        let text = format!("source={source}\n");
        let dir = self.frits_dir();
        let path = dir.join(format!("{name}.vtfrit"));
        if std::fs::symlink_metadata(&path).is_ok() {
            let old = crate::sources::read_regular(&path, crate::frit::FRIT_BYTES_MAX as u64)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if old == text.as_bytes() {
                return Ok(path);
            }
            return Err(format!(
                "{} holds a different frit: build it by name, or remove it first",
                path.display()
            ));
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        write_atomic(&path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    fn install(mut self, files: &[String]) -> u8 {
        if files.is_empty() || self.o.out.is_some() || self.o.all || self.o.shortcut.is_some() {
            return self.usage("install takes one or more set files");
        }
        if let Some(why) = self.refuse_without_root("install") {
            return self.fail(&why);
        }
        let (path, uid) = self.store_path();
        let writer = match StoreWriter::open(&path, uid) {
            Ok(w) => w,
            Err(why) => return self.fail(&why),
        };
        let mut ok = true;
        for file in files {
            let (name, outcome) = install_one(&writer, Path::new(file));
            ok &= self.report(&name, outcome);
        }
        u8::from(!ok)
    }

    fn remove(mut self, names: &[String]) -> u8 {
        if names.is_empty() {
            return self.usage("remove takes one or more set names");
        }
        if let Some(why) = self.refuse_without_root("remove") {
            return self.fail(&why);
        }
        let (path, uid) = self.store_path();
        let writer = match StoreWriter::open(&path, uid) {
            Ok(w) => w,
            Err(why) => return self.fail(&why),
        };
        let mut ok = true;
        for name in names {
            let outcome = match writer.remove(name) {
                Ok(true) => Outcome::Removed,
                Ok(false) => Outcome::Failed("the store holds no set of that name".to_owned()),
                Err(why) => Outcome::Failed(why),
            };
            ok &= self.report(name, outcome);
        }
        u8::from(!ok)
    }

    /// The store, opened and checked, or `None` when it does not exist.
    fn open_store(&self) -> Result<Option<Store>, String> {
        let (path, uid) = self.store_path();
        if !path.exists() {
            return Ok(None);
        }
        Store::open(&path, uid).map(Some).map_err(|e| e.to_string())
    }

    fn list(mut self, words: &[String]) -> u8 {
        if !words.is_empty() {
            return self.usage("list takes no names");
        }
        let mut rows: Vec<ListRow> = Vec::new();
        let mut status = 0;
        match self.open_store().and_then(|s| {
            s.map(|s| manifest(&s).map_err(|e| e.to_string()))
                .transpose()
        }) {
            Ok(Some(m)) => rows.extend(m.rows().iter().map(ListRow::from_row)),
            Ok(None) => {}
            Err(why) => status = self.fail(&why),
        }
        for name in RESERVED {
            if let Some(set) = builtin(name) {
                rows.push(ListRow::builtin(name, &set));
            }
        }
        let text = if self.o.json { json(&rows) } else { tsv(&rows) };
        let _ = self.out.write_all(text.as_bytes());
        status
    }

    fn verify(mut self, names: &[String]) -> u8 {
        let store = match self.open_store() {
            Ok(Some(s)) => s,
            Ok(None) if names.is_empty() => return 0,
            Ok(None) => return self.fail("the store does not exist"),
            Err(why) => return self.fail(&why),
        };
        let m = match manifest(&store) {
            Ok(m) => m,
            Err(e) => return self.fail(&e.to_string()),
        };
        let rows: Vec<(String, Option<&ManifestRow>)> = if names.is_empty() {
            m.rows().iter().map(|r| (r.name.clone(), Some(r))).collect()
        } else {
            names.iter().map(|n| (n.clone(), m.row(n))).collect()
        };
        let mut ok = true;
        for (name, row) in rows {
            let outcome = match row {
                None => Outcome::Failed("the store holds no set of that name".to_owned()),
                Some(row) => match store
                    .load_row(row)
                    .map_err(|e| e.to_string())
                    .and_then(|set| set.check_chunks().map_err(|e| e.to_string()))
                {
                    Ok(()) => Outcome::Verified,
                    Err(why) => Outcome::Failed(why),
                },
            };
            ok &= self.report(&name, outcome);
        }
        u8::from(!ok)
    }
}

/// The frits in `dir`, sorted by name.
fn frits_in(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "vtfrit"))
        .collect();
    out.sort();
    Ok(out)
}

/// The name a set from `path` would take when its frit cannot be read:
/// the file's stem.
fn frit_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy())
        .map(|n| n.strip_suffix(".vtfrit").unwrap_or(&n).to_owned())
        .unwrap_or_default()
}

/// A font's name for a set: its last path component without the font and
/// compression suffixes kbd tries.
fn font_name(spelling: &str) -> String {
    let mut name = spelling.rsplit('/').next().unwrap_or(spelling).to_owned();
    for z in [".gz", ".bz2", ".xz", ".zst"] {
        if let Some(n) = name.strip_suffix(z) {
            name = n.to_owned();
        }
    }
    for s in [".psfu", ".psf", ".cp", ".fnt"] {
        if let Some(n) = name.strip_suffix(s) {
            name = n.to_owned();
        }
    }
    name
}

/// Every source of `frit` read, `None` for one missing on this machine,
/// the build key, and a warning for each one missing.
struct Read {
    resolved: Vec<Option<Resolved>>,
    key: [u8; 32],
    warnings: Vec<String>,
}

impl Read {
    /// What a build from these sources reports: `current` when the store
    /// already held it, partial when a source was left out.
    fn outcome(&self, frit: &Frit, current: bool) -> Outcome {
        let left: Vec<String> = frit
            .sources
            .iter()
            .zip(&self.resolved)
            .filter(|(_, r)| r.is_none())
            .map(|(s, _)| s.spelling.clone())
            .collect();
        match (current, left.is_empty()) {
            (false, true) => Outcome::Built,
            (true, true) => Outcome::Current,
            (false, false) => Outcome::BuiltPartial(left),
            (true, false) => Outcome::CurrentPartial(left),
        }
    }
}

/// Reads every source of `frit` and makes its build key. A source missing
/// on this machine is left out with a warning; a source that fails, or a
/// frit with every source missing, fails the set.
fn read_sources(frit: &Frit, host: &Host<'_>) -> Result<Read, String> {
    let mut resolved = Vec::new();
    let mut warnings = Vec::new();
    for s in &frit.sources {
        match resolve(s, host) {
            Ok(r) => resolved.push(Some(r)),
            Err(SourceError::Missing(why)) => {
                warnings.push(format!(
                    "{}:{}: source={} is left out: {why}",
                    frit.file.display(),
                    s.line,
                    s.spelling
                ));
                resolved.push(None);
            }
            Err(SourceError::Failed(why)) => return Err(format!("source={}: {why}", s.spelling)),
        }
    }
    if resolved.iter().all(Option::is_none) {
        return Err(format!(
            "no source of the frit is on this machine: {}",
            warnings.join("; ")
        ));
    }
    let key = build_key(frit, &resolved);
    Ok(Read {
        resolved,
        key,
        warnings,
    })
}

/// Parses each source found in the child and assembles the set's bytes.
fn parse_and_assemble(frit: &Frit, read: &Read, parser: &Parser) -> Result<Vec<u8>, String> {
    crate::build::parse_and_assemble(frit, &read.resolved, &read.key, parser)
}

/// The directories installed outline fonts are found in: the system's,
/// and for a user who is not root, that user's own, as fontconfig's
/// default configuration adds `$XDG_DATA_HOME/fonts`.
fn default_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = FONT_DIRS.iter().map(PathBuf::from).collect();
    if !geteuid().is_root() {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|d| d.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
        dirs.extend(data.map(|d| d.join("fonts")));
    }
    dirs
}

/// Writes each of `frit`'s warnings to `err`.
fn warn(err: &mut dyn Write, frit: &Frit) {
    let lines: Vec<String> = frit.warnings.iter().map(ToString::to_string).collect();
    warn_all(err, &lines);
}

fn warn_all(err: &mut dyn Write, warnings: &[String]) {
    for w in warnings {
        let _ = writeln!(err, "vtium-vitrify: warning: {w}");
    }
}

/// Builds the frit at `path` into the store, unless its build key is
/// the manifest's already.
fn build_into(
    writer: &StoreWriter,
    path: &Path,
    host: &Host<'_>,
    parser: &Parser,
    force: bool,
    err: &mut dyn Write,
) -> (String, Outcome) {
    let frit = match Frit::load(path) {
        Ok(r) => r,
        Err(e) => return (frit_name(path), Outcome::Failed(e.to_string())),
    };
    warn(err, &frit);
    let name = frit.name.clone();
    let built = (|| {
        let read = read_sources(&frit, host)?;
        warn_all(err, &read.warnings);
        let key = read.key;
        let manifest = writer.manifest()?;
        if !force && manifest.row(&name).is_some_and(|r| r.build_key == key) {
            return Ok(read.outcome(&frit, true));
        }
        let bytes = parse_and_assemble(&frit, &read, parser)?;
        let set = Set::from_bytes(bytes.clone()).map_err(|e| e.to_string())?;
        // the first source found, as the set's provenance names it first
        // and `install` reads it from there
        let first = frit
            .sources
            .iter()
            .zip(&read.resolved)
            .find_map(|(s, r)| r.as_ref().map(|_| s.spelling.clone()))
            .unwrap_or_default();
        writer.put(row_for(&set, key, first), &bytes)?;
        Ok(read.outcome(&frit, false))
    })();
    (name, built.unwrap_or_else(Outcome::Failed))
}

/// The manifest row of `set`.
fn row_for(set: &Set, build_key: [u8; 32], source: String) -> ManifestRow {
    ManifestRow {
        name: set.name().to_owned(),
        sha256: set.sha256().unwrap_or_default(),
        build_key,
        version: set.version(),
        cells: set.cells(),
        source,
    }
}

/// Checks the portable set at `file` and adds it to the store (ADR-1200
/// D12): its format version, every bound, every chunk's hash and the
/// whole file's hash, which becomes the row's, with the build key read
/// from its provenance.
fn install_one(writer: &StoreWriter, file: &Path) -> (String, Outcome) {
    let label = file.display().to_string();
    let checked = (|| {
        let bytes = crate::sources::read_regular(file, u64::from(SET_BYTES_MAX))?;
        let set = Set::from_bytes(bytes.clone()).map_err(|e| e.to_string())?;
        set.check_chunks().map_err(|e| e.to_string())?;
        let name = set.name().to_owned();
        if !valid_name(&name) || RESERVED.contains(&name.as_str()) {
            return Err(format!("{name:?} is not a name a set may take"));
        }
        let key = set
            .meta_values("build-key")
            .next()
            .and_then(crate::frit::parse_sha256)
            .ok_or_else(|| "its provenance holds no build-key".to_owned())?;
        let source = set
            .meta_values("source")
            .next()
            .filter(|s| !s.is_empty() && !s.contains(['\t', '\r', '\n']))
            .ok_or_else(|| "its provenance names no source".to_owned())?
            .to_owned();
        if set.cells().is_empty() {
            return Err("the set has no strike".to_owned());
        }
        let manifest = writer.manifest()?;
        let row = row_for(&set, key, source);
        if manifest.row(&name) == Some(&row) {
            return Ok((name, Outcome::Current));
        }
        writer.put(row, &bytes)?;
        Ok((name, Outcome::Installed))
    })();
    checked.unwrap_or_else(|why| (label, Outcome::Failed(why)))
}

/// One line of `list`.
struct ListRow {
    name: String,
    kind: &'static str,
    sha256: Option<String>,
    build_key: Option<String>,
    format: String,
    cells: String,
    source: String,
}

impl ListRow {
    fn from_row(r: &ManifestRow) -> ListRow {
        ListRow {
            name: r.name.clone(),
            kind: "built",
            sha256: Some(sha256_hex(&r.sha256)),
            build_key: Some(sha256_hex(&r.build_key)),
            format: format!("{}.{}", r.version.0, r.version.1),
            cells: cells(&r.cells),
            source: r.source.clone(),
        }
    }

    fn builtin(name: &str, set: &Set) -> ListRow {
        let (major, minor) = VERSION;
        ListRow {
            name: name.to_owned(),
            kind: "built-in",
            sha256: None,
            build_key: None,
            format: format!("{major}.{minor}"),
            cells: cells(&set.cells()),
            source: format!("builtin:{name}"),
        }
    }
}

fn cells(cells: &[vtium_font::Cell]) -> String {
    cells
        .iter()
        .map(|c| format!("{}x{}", c.width, c.height))
        .collect::<Vec<_>>()
        .join(",")
}

fn tsv(rows: &[ListRow]) -> String {
    let mut s = String::from("name\tkind\tsha256\tbuild-key\tformat\tcells\tsource\n");
    for r in rows {
        s += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.name,
            r.kind,
            r.sha256.as_deref().unwrap_or("-"),
            r.build_key.as_deref().unwrap_or("-"),
            r.format,
            r.cells,
            r.source
        );
    }
    s
}

/// `s` as a JSON string.
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            c if (c as u32) < 0x20 => out += &format!("\\u{:04x}", c as u32),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json(rows: &[ListRow]) -> String {
    let opt = |v: &Option<String>| v.as_deref().map_or("null".to_owned(), json_string);
    let items: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "{{\"name\":{},\"kind\":{},\"sha256\":{},\"build-key\":{},\"format\":{},\"cells\":[{}],\"source\":{}}}",
                json_string(&r.name),
                json_string(r.kind),
                opt(&r.sha256),
                opt(&r.build_key),
                json_string(&r.format),
                r.cells
                    .split(',')
                    .filter(|c| !c.is_empty())
                    .map(json_string)
                    .collect::<Vec<_>>()
                    .join(","),
                json_string(&r.source)
            )
        })
        .collect();
    format!("[{}]\n", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_name_drops_its_suffixes() {
        assert_eq!(font_name("ter-v16n"), "ter-v16n");
        assert_eq!(
            font_name("/usr/share/kbd/consolefonts/lat2-16.psfu.gz"),
            "lat2-16"
        );
        assert_eq!(font_name("x.psf.zst"), "x");
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\u{1}"), "\"a\\\"b\\\\c\\u0001\"");
    }
}
