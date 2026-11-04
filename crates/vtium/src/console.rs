//! `vtium console`: gives one VT, or every VT, to the packaged vtium,
//! `vtium@.service`, and takes them back (ADR-1500 D7).
//!
//! systemd starts the units `getty.target` wants at boot, and logind starts
//! `autovt@ttyN.service` on a VT that nothing holds when the user switches
//! to it. `/etc/systemd/system/autovt@ttyN.service`, a VT's own link, wins
//! over `/etc/systemd/system/autovt@.service`, the template link that every
//! other VT follows. `enable N` enables `vtium@ttyN` and disables the VT's
//! getty; `enable --all` also links the template to vtium and disables the
//! gettys on VTs 2 to 6, which logind then fills with vtium. Before vtium
//! disables a getty or moves the template link, it writes a record of
//! what was there under `/var/lib/vtium/console/`, and `disable` puts back
//! what the records say. It passes by every VT another program holds: a
//! VT that a unit enabled at boot, such as the display manager's, claims,
//! judged from what is on the machine ([`held`]), and a VT a graphical
//! session, a greeter or another console is on now. `fallback N`, which
//! `vtium-getty@.service` runs when vtium on VT N fails, starts the VT's
//! getty only when that start stops no other program ([`fallback`]).
//!
//! The subcommand starts and stops nothing, since either ends every
//! session on the VT, and masks nothing, since a mask is the
//! administrator's. It refuses a VT that the C reference's console holds.
//!
//! Everything it touches is reached through [`Host`]: a root directory,
//! `/` on the running system and a scratch directory in the tests, and a
//! [`Systemd`], which runs `systemctl` and `loginctl` on the running system
//! and is a stand-in in the tests. It reads systemd's links itself, and changes them
//! through `systemctl enable`, `disable`, `add-wants` and `daemon-reload`,
//! but for the links no systemctl verb makes or removes as they were: the
//! template link, a VT's `autovt@ttyN.service` link that `disable` leaves,
//! and a getty's want restored to the unit file it linked to.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod fallback;
mod graphical;
mod held;
mod text;

use held::Holder;

/// The highest VT number the kernel has (`MAX_NR_CONSOLES`).
pub const LAST_VT: u16 = 63;

/// The VTs logind starts consoles on by itself: 1 to `NAutoVTs=`, whose
/// default is 6 (logind.conf(5)).
const LOGIND_VTS: u16 = 6;

/// The packaged vtium's unit, `vtium@.service`, by its template's prefix.
const VTIUM: &str = "vtium";
/// The development install's unit (ADR-1500 D4).
const DEV: &str = "vtium-dev";
/// systemd's getty.
const GETTY: &str = "getty";
/// The C reference's console.
const KMSCON: &str = "kmsconvt";
/// The consoles `vtium console` knows by name, each with rules of its own.
const CONSOLES: [&str; 4] = [VTIUM, DEV, GETTY, KMSCON];

/// systemd's system unit directories that the administrator, the running
/// system or a package installs into, in the order systemd's unit search
/// path reads them (systemd.unit(5)), relative to the root.
/// `lib/systemd/system` is a split-usr host's vendor directory, as on
/// Debian, and on a merged-usr host the same directory as
/// `usr/lib/systemd/system`.
const UNIT_DIRS: [&str; 5] = [
    "etc/systemd/system",
    "run/systemd/system",
    "usr/local/lib/systemd/system",
    "usr/lib/systemd/system",
    "lib/systemd/system",
];
/// Where `systemctl enable` makes its links.
const ETC: &str = "etc/systemd/system";
/// The units systemd starts on the VTs at boot.
const WANTS: &str = "etc/systemd/system/getty.target.wants";
/// logind's reserved VT, `ReserveVT=`, whose default is 6 (logind.conf(5)).
const RESERVED_VT: u16 = 6;

/// The template link's name.
const TEMPLATE: &str = "autovt@.service";
/// The alias a display manager's unit takes when it is enabled.
const DISPLAY_MANAGER: &str = "display-manager.service";
/// What masks a unit.
const MASK: &str = "/dev/null";
/// vtium's records of what it changed.
const RECORDS: &str = "var/lib/vtium/console";
/// A directory only a host booted with systemd has (sd_booted(3)).
const BOOTED: &str = "run/systemd/system";

/// Which VTs a verb acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    All,
    Vt(u16),
}

/// What the subcommand's words ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Enable(Which),
    Disable(Which),
    Status,
    /// Starts VT N's getty in place of a vtium that failed there, unless
    /// that start stops another program.
    Fallback(u16),
}

impl Verb {
    /// The verb and its argument, as the user types them.
    fn words(self) -> String {
        match self {
            Verb::Enable(w) => format!("enable {}", which_word(w)),
            Verb::Disable(w) => format!("disable {}", which_word(w)),
            Verb::Status => "status".to_owned(),
            Verb::Fallback(n) => format!("fallback {n}"),
        }
    }
}

fn which_word(which: Which) -> String {
    match which {
        Which::All => "--all".to_owned(),
        Which::Vt(n) => n.to_string(),
    }
}

/// What the subcommand's words parse to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Verb),
    Help,
}

/// The subcommand `args` names, its words parsed: `None` when the first
/// word is not `console`.
pub fn parse(args: &[OsString]) -> Option<Result<Parsed, String>> {
    if args.first()?.to_str()? != "console" {
        return None;
    }
    Some(parse_words(args.get(1..).unwrap_or_default()))
}

fn parse_words(args: &[OsString]) -> Result<Parsed, String> {
    let words = args
        .iter()
        .map(|w| w.to_str().ok_or_else(|| text::not_utf8(w)))
        .collect::<Result<Vec<_>, _>>()?;
    if words.iter().any(|w| *w == "--help" || *w == "-h") {
        return Ok(Parsed::Help);
    }
    let Some((verb, rest)) = words.split_first() else {
        return Err(text::NO_VERB.to_owned());
    };
    match *verb {
        "status" => match rest.first() {
            Some(extra) => Err(text::extra_word(extra)),
            None => Ok(Parsed::Run(Verb::Status)),
        },
        "fallback" => match rest {
            [] => Err(text::needs_which(verb)),
            [word] => match which(word)? {
                Which::Vt(n) => Ok(Parsed::Run(Verb::Fallback(n))),
                Which::All => Err(text::fallback_one_vt()),
            },
            [_, extra, ..] => Err(text::extra_word(extra)),
        },
        "enable" | "disable" => {
            let which = match rest {
                [] => return Err(text::needs_which(verb)),
                [word] => which(word)?,
                [_, extra, ..] => return Err(text::extra_word(extra)),
            };
            Ok(Parsed::Run(if *verb == "enable" {
                Verb::Enable(which)
            } else {
                Verb::Disable(which)
            }))
        }
        other => Err(text::unknown_verb(other)),
    }
}

fn which(word: &str) -> Result<Which, String> {
    if word == "--all" {
        return Ok(Which::All);
    }
    if word.starts_with('-') {
        return Err(text::unknown_option(word));
    }
    vt_number(word)
        .map(Which::Vt)
        .ok_or_else(|| text::not_a_vt(word))
}

/// `N` or `ttyN`, for N from 1 to [`LAST_VT`].
fn vt_number(value: &str) -> Option<u16> {
    vt_digits(value.strip_prefix("tty").unwrap_or(value))
}

/// N from 1 to [`LAST_VT`], written as the kernel and systemd name the VT:
/// digits only, with no sign and no leading zero.
fn vt_digits(digits: &str) -> Option<u16> {
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| (1..=LAST_VT).contains(n))
}

/// What a `systemctl` run gave back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// Whether it exited with status 0.
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs systemd's `systemctl` and `loginctl` with the arguments it is
/// given.
pub trait Systemd {
    fn systemctl(&mut self, args: &[&str]) -> io::Result<Reply>;
    fn loginctl(&mut self, args: &[&str]) -> io::Result<Reply>;
}

/// The running system's `systemctl` and `loginctl`, found on `PATH`.
pub struct RunningSystemd;

impl RunningSystemd {
    fn run(program: &str, args: &[&str]) -> io::Result<Reply> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()?;
        Ok(Reply {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

impl Systemd for RunningSystemd {
    fn systemctl(&mut self, args: &[&str]) -> io::Result<Reply> {
        RunningSystemd::run("systemctl", args)
    }

    fn loginctl(&mut self, args: &[&str]) -> io::Result<Reply> {
        RunningSystemd::run("loginctl", args)
    }
}

/// What the subcommand runs against.
pub struct Host<'a> {
    /// The system's root directory: `/`, or a scratch directory in a test.
    pub root: PathBuf,
    /// Whether the effective user is root.
    pub root_user: bool,
    pub systemd: &'a mut dyn Systemd,
}

/// A symbolic link's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Link {
    /// Nothing is there.
    #[default]
    Absent,
    /// A link, to this path.
    To(String),
    /// Something that is not a link.
    Other,
}

impl Link {
    fn read(path: &Path) -> Link {
        match std::fs::symlink_metadata(path) {
            Err(_) => Link::Absent,
            Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(path) {
                Ok(to) => Link::To(to.to_string_lossy().into_owned()),
                Err(_) => Link::Other,
            },
            Ok(_) => Link::Other,
        }
    }

    /// The template prefix of the unit the link names, as systemd reads
    /// it from the name of the file it links to: `vtium` for
    /// `/usr/lib/systemd/system/vtium@.service`.
    fn unit(&self) -> Option<&str> {
        match self {
            Link::To(to) => template_prefix(to),
            _ => None,
        }
    }

    fn names(&self, prefix: &str) -> bool {
        self.unit() == Some(prefix)
    }

    fn masks(&self) -> bool {
        *self == Link::To(MASK.to_owned())
    }

    /// The name of the file the link names: `sddm.service`.
    fn file_name(&self) -> Option<&str> {
        match self {
            Link::To(to) => Path::new(to).file_name()?.to_str(),
            _ => None,
        }
    }
}

/// Where a template's unit file is, as systemd finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum UnitFile {
    /// At this path on the running system.
    At(String),
    /// Masked: the first directory that has it links it to `/dev/null`.
    Masked,
    Missing,
}

/// What a VT's record says the VT had before `enable` took it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Record {
    /// No getty was enabled.
    NoGetty,
    /// `getty@ttyN` was enabled, its want linked to this path.
    Getty(String),
    /// The record holds neither, and is taken as no record.
    Unreadable,
}

impl Record {
    fn parse(text: &str) -> Record {
        match text.strip_suffix('\n').unwrap_or(text) {
            "" => Record::NoGetty,
            path if !path.contains('\n') && template_prefix(path) == Some(GETTY) => {
                Record::Getty(path.to_owned())
            }
            _ => Record::Unreadable,
        }
    }
}

/// `PREFIX` of a path whose file name is `PREFIX@.service`.
fn template_prefix(path: &str) -> Option<&str> {
    Path::new(path)
        .file_name()?
        .to_str()?
        .strip_suffix("@.service")
        .filter(|p| !p.is_empty())
}

/// The names whose `NAME.d` directories hold drop-ins for the unit `name`,
/// most specific first, as systemd expands them: `foo-bar-baz.service`,
/// then `foo-bar-.service` and `foo-.service`.
fn drop_in_names(name: &str) -> Vec<String> {
    let mut out = vec![name.to_owned()];
    let mut prefix = name.strip_suffix(".service").unwrap_or_default();
    while let Some((head, _)) = prefix.rsplit_once('-') {
        if head.is_empty() {
            break;
        }
        out.push(format!("{head}-.service"));
        prefix = head;
    }
    out
}

/// `(PREFIX, N)` of a unit named `PREFIX@ttyN.service`.
fn instance(name: &str) -> Option<(&str, u16)> {
    let (prefix, inst) = name.strip_suffix(".service")?.split_once('@')?;
    let n = vt_digits(inst.strip_prefix("tty")?)?;
    (!prefix.is_empty()).then_some((prefix, n))
}

/// What vtium reads of systemd's state and its own records.
#[derive(Debug, Default)]
struct State {
    /// The template link in force: `autovt@.service` in the first unit
    /// directory that has it.
    template: Link,
    /// The template link that vtium manages, in `/etc/systemd/system`.
    template_etc: Link,
    /// Whether a unit directory after `/etc/systemd/system` has the
    /// template link, which is in force without the one in `/etc`.
    template_below: bool,
    /// The units `getty.target` wants on each VT, by prefix, and what each
    /// want links to.
    wants: BTreeMap<u16, BTreeMap<String, Link>>,
    /// The display manager's unit, such as `sddm.service`, when
    /// `display-manager.service` is linked or there and not masked.
    display_manager: Option<String>,
    /// What other programs hold each VT: the units enabled at boot that
    /// claim it, and the graphical sessions, greeters and other consoles on
    /// it now.
    held: BTreeMap<u16, Vec<Holder>>,
    /// The other sessions on each VT, such as a user's login at a getty.
    logins: BTreeMap<u16, Vec<Holder>>,
    /// Why logind could not be asked which sessions run, when it could not.
    sessions_unknown: Option<String>,
    /// Each VT's own `autovt@ttyN.service` link.
    own: BTreeMap<u16, Link>,
    /// The VTs whose `vtium@ttyN` is masked.
    masked: BTreeSet<u16>,
    /// The units running on each VT, by prefix.
    running: BTreeMap<u16, BTreeSet<String>>,
    /// What each VT had before `enable` took it.
    records: BTreeMap<u16, Record>,
    /// The template record: `None` with none; `Some(None)` for a template
    /// that linked nowhere; `Some(Some(path))` for one that linked to path.
    template_record: Option<Option<String>>,
}

impl State {
    fn template(&self) -> &Link {
        &self.template
    }

    fn wanted(&self, n: u16, prefix: &str) -> bool {
        self.wants.get(&n).is_some_and(|w| w.contains_key(prefix))
    }

    fn own(&self, n: u16) -> &Link {
        self.own.get(&n).unwrap_or(&Link::Absent)
    }

    /// Whether logind follows the template link on VT `n`: whether the VT
    /// has no link of its own.
    fn templated(&self, n: u16) -> bool {
        *self.own(n) == Link::Absent
    }

    fn runs(&self, n: u16, prefix: &str) -> bool {
        self.running.get(&n).is_some_and(|r| r.contains(prefix))
    }

    /// Whether `prefix`'s console is enabled, linked or running on VT `n`.
    fn holds(&self, n: u16, prefix: &str) -> bool {
        self.wanted(n, prefix) || self.own(n).names(prefix) || self.runs(n, prefix)
    }

    /// Whether `enable N` is done on VT `n`.
    fn given(&self, n: u16) -> bool {
        self.wanted(n, VTIUM) && self.own(n).names(VTIUM) && !self.wanted(n, GETTY)
    }

    /// Whether anything of `enable N`'s is on VT `n`.
    fn vtium_on(&self, n: u16) -> bool {
        self.wanted(n, VTIUM) || self.own(n).names(VTIUM) || self.records.contains_key(&n)
    }

    /// Every VT anything is enabled, linked, running or recorded on.
    fn vts(&self) -> BTreeSet<u16> {
        let mut vts: BTreeSet<u16> = self.wants.keys().copied().collect();
        vts.extend(self.own.keys());
        vts.extend(self.running.keys());
        vts.extend(self.records.keys());
        vts.extend(&self.masked);
        vts
    }
}

/// Lists a directory's entry names, none when it cannot be read.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Who holds which VT, a VT once for each holder.
type OnVts = Vec<(u16, Holder)>;

/// A step that could not be taken: what to print, on stderr.
struct Stop(String);

type Step<T = ()> = Result<T, Stop>;

/// One run of the subcommand.
struct Run<'h, 'a, 'o> {
    host: &'h mut Host<'a>,
    out: &'o mut dyn Write,
}

/// Runs `verb` against `host`, writing what it does to `out` and what
/// stops it to `err`. Returns the exit status: 0 when it did what it was
/// asked or found it done, 1 when it refused or a step failed.
pub fn run(verb: Verb, host: &mut Host<'_>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut run = Run { host, out };
    match run.verb(verb) {
        Ok(status) => status,
        Err(Stop(why)) => {
            let _ = writeln!(err, "vtium console {}: {why}", verb.words());
            1
        }
    }
}

impl Run<'_, '_, '_> {
    fn path(&self, rel: &str) -> PathBuf {
        self.host.root.join(rel)
    }

    fn say(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
    }

    fn verb(&mut self, verb: Verb) -> Step<i32> {
        if !self.path(BOOTED).is_dir() {
            return Err(Stop(text::NO_SYSTEMD.to_owned()));
        }
        if verb != Verb::Status && !self.host.root_user {
            return Err(Stop(text::needs_root(&verb.words())));
        }
        if let Verb::Fallback(n) = verb {
            return self.fallback(n);
        }
        let unit = self.unit_file(VTIUM);
        let mut state = self.read()?;
        let problem = match unit {
            UnitFile::At(_) => None,
            UnitFile::Masked => Some(text::TEMPLATE_MASKED),
            UnitFile::Missing => Some(text::NOT_INSTALLED),
        };
        if verb == Verb::Status {
            if let Some(problem) = problem {
                self.say(problem);
            }
            self.status(&state);
            return Ok(0);
        }
        let UnitFile::At(unit) = unit else {
            return Err(Stop(problem.unwrap_or_default().to_owned()));
        };
        match verb {
            Verb::Status => Ok(0),
            Verb::Enable(Which::Vt(n)) => {
                self.refuse_kmscon(&state, n)?;
                self.refuse_held(&state, n)?;
                self.refuse_vt(&state, n)?;
                if state.given(n) {
                    self.say(&text::already_given(n));
                    return Ok(0);
                }
                // logind starts autovt@tty6 on every switch to its reserved
                // VT, so vtium enabled there would end a login there at
                // the next switch, unless vtium runs there already and the
                // login is inside it.
                if n == RESERVED_VT
                    && !state.runs(n, VTIUM)
                    && let Some(logins) = state.logins.get(&n)
                {
                    return Err(Stop(text::login_on_reserved_vt(n, logins, false)));
                }
                self.enable_vt(&mut state, n)?;
                self.say(&text::given(n, state.runs(n, VTIUM)));
                Ok(0)
            }
            Verb::Enable(Which::All) => self.enable_all(&mut state, &unit),
            Verb::Disable(Which::Vt(n)) => {
                if !state.vtium_on(n) {
                    // A run whose last step, the reload, failed finds its
                    // links done, so a run that changes nothing reloads.
                    self.reload()?;
                    self.say(&text::not_given(n));
                    return Ok(0);
                }
                let by_hand = self.disable_vt(&mut state, n)?;
                if by_hand {
                    self.reload()?;
                }
                self.say(&text::taken_back(n, state.runs(n, VTIUM)));
                if state.template().names(VTIUM) && !state.wanted(n, GETTY) {
                    return Err(Stop(text::template_still_vtium(n)));
                }
                Ok(0)
            }
            Verb::Disable(Which::All) => self.disable_all(&mut state),
            Verb::Fallback(_) => Ok(0),
        }
    }

    /// Where systemd finds `PREFIX@.service`: in the first unit directory
    /// that has it, unless that directory masks it.
    fn unit_file(&self, prefix: &str) -> UnitFile {
        self.find(&format!("{prefix}@.service"))
    }

    /// Where systemd finds the unit file `file`: in the first unit
    /// directory that has it, unless that directory masks it.
    fn find(&self, file: &str) -> UnitFile {
        for dir in UNIT_DIRS {
            let path = self.path(dir).join(file);
            match Link::read(&path) {
                Link::Absent => {}
                link if link.masks() => return UnitFile::Masked,
                _ if path.is_file() => return UnitFile::At(format!("/{dir}/{file}")),
                _ => {}
            }
        }
        UnitFile::Missing
    }

    /// The template link in each unit directory that has one, in the
    /// search path's order.
    fn templates(&self) -> Vec<Link> {
        UNIT_DIRS
            .iter()
            .map(|dir| Link::read(&self.path(dir).join(TEMPLATE)))
            .filter(|link| *link != Link::Absent)
            .collect()
    }

    fn read(&mut self) -> Step<State> {
        let etc = self.path(ETC);
        let template_etc = Link::read(&etc.join(TEMPLATE));
        let templates = self.templates();
        let below = usize::from(template_etc != Link::Absent);
        let mut state = State {
            template: templates.first().cloned().unwrap_or_default(),
            template_below: templates.len() > below,
            template_etc,
            ..State::default()
        };
        let wants = self.path(WANTS);
        for name in names(&wants) {
            if let Some((prefix, n)) = instance(&name) {
                let link = Link::read(&wants.join(&name));
                state
                    .wants
                    .entry(n)
                    .or_default()
                    .insert(prefix.to_owned(), link);
            }
        }
        state.display_manager = self.display_manager();
        match self.sessions() {
            Ok((graphics, logins)) => {
                for (n, holder) in graphics {
                    state.held.entry(n).or_default().push(holder);
                }
                for (n, holder) in logins {
                    state.logins.entry(n).or_default().push(holder);
                }
            }
            Err(Stop(why)) => state.sessions_unknown = Some(why),
        }
        state.running = self.running()?;
        for (&n, prefixes) in &state.running {
            for prefix in prefixes.iter().filter(|p| !CONSOLES.contains(&p.as_str())) {
                state.held.entry(n).or_default().push(Holder::Console {
                    unit: format!("{prefix}@tty{n}.service"),
                });
            }
        }
        self.judge_boot_units(&mut state);
        for name in names(&etc) {
            match instance(&name) {
                Some(("autovt", n)) => {
                    state.own.insert(n, Link::read(&etc.join(&name)));
                }
                Some((VTIUM, n)) if Link::read(&etc.join(&name)).masks() => {
                    state.masked.insert(n);
                }
                _ => {}
            }
        }
        let records = self.path(RECORDS);
        for name in names(&records) {
            if let Some(n) = name.strip_prefix("tty").and_then(vt_digits) {
                let text = std::fs::read_to_string(records.join(&name)).unwrap_or_default();
                state.records.insert(n, Record::parse(&text));
            }
        }
        state.template_record = std::fs::read_to_string(records.join(TEMPLATE))
            .ok()
            .and_then(|record| {
                let path = record.trim_end_matches('\n');
                if path.is_empty() {
                    Some(None)
                } else {
                    let fits =
                        template_prefix(path).is_some_and(|p| p != VTIUM) && !path.contains('\n');
                    fits.then(|| Some(path.to_owned()))
                }
            });
        Ok(state)
    }

    /// Judges which VTs the units that start at boot hold
    /// ([`held::judge`]), from their unit files and the display manager's
    /// configuration, and adds each to `state`. A unit is a display manager
    /// when `display-manager.service` names it, or when it runs one vtium
    /// knows by name ([`held::is_display_manager`]), as ly's `ly@tty2`,
    /// which is enabled without that alias.
    fn judge_boot_units(&self, state: &mut State) {
        for unit in self.boot_units(state.display_manager.as_deref()) {
            let aliased = state.display_manager.as_deref() == Some(unit.as_str());
            let display_manager = aliased || held::is_display_manager(&unit);
            let texts = self.unit_texts(&unit, aliased);
            let facts = graphical::facts(&unit, texts.iter().map(String::as_str));
            let setting = display_manager
                .then(|| held::config(&self.host.root, &unit, &facts))
                .flatten();
            for (n, reason) in held::judge(&facts, display_manager, setting.as_ref()) {
                state.held.entry(n).or_default().push(Holder::Unit {
                    unit: unit.clone(),
                    display_manager,
                    reason,
                });
            }
        }
    }

    /// The units systemd starts at boot that vtium judges: the display
    /// manager, and each service a `*.wants` or `*.requires` directory of a
    /// unit directory links, but the consoles vtium knows by name, which
    /// have rules of their own, and a masked unit.
    fn boot_units(&self, display_manager: Option<&str>) -> BTreeSet<String> {
        let mut units: BTreeSet<String> = display_manager.map(str::to_owned).into_iter().collect();
        for dir in UNIT_DIRS {
            let dir = self.path(dir);
            for deps in names(&dir)
                .into_iter()
                .filter(|d| d.ends_with(".wants") || d.ends_with(".requires"))
            {
                for name in names(&dir.join(&deps)) {
                    let prefix = name.split_once('@').map(|(p, _)| p);
                    if !name.ends_with(".service")
                        || prefix.is_some_and(|p| CONSOLES.contains(&p) || p == "autovt")
                        || Link::read(&dir.join(&deps).join(&name)).masks()
                        || self.find(&name) == UnitFile::Masked
                    {
                        continue;
                    }
                    units.insert(name);
                }
            }
        }
        units
    }

    /// The unit `display-manager.service` names: the name of the file its
    /// link in `/etc/systemd/system` names, or `display-manager.service`
    /// when it is a file there. None when there is neither, or when the
    /// link or the unit it names is masked.
    fn display_manager(&self) -> Option<String> {
        let name = match Link::read(&self.path(ETC).join(DISPLAY_MANAGER)) {
            Link::Absent => return None,
            link if link.masks() => return None,
            Link::Other => DISPLAY_MANAGER.to_owned(),
            link => link.file_name().unwrap_or(DISPLAY_MANAGER).to_owned(),
        };
        (self.find(&name) != UnitFile::Masked).then_some(name)
    }

    /// A unit's file and its drop-ins, as systemd reads them.
    ///
    /// systemd loads the unit by its name, so the file of that name in the
    /// first unit directory that has one is read, which may be an
    /// administrator's copy in `/etc/systemd/system` rather than the file a
    /// link names (measured); an instance with no file of its own reads its
    /// template's. The drop-ins are each `*.conf` in the directories
    /// `NAME.d` of the unit's own name, then of its template, then, for the
    /// display manager, of its alias `display-manager.service`, with the
    /// prefix directories of each name, such as `display-.service.d`, and
    /// last the directory `service.d` that applies to every service, each
    /// looked for in every unit directory in the search path's order. A
    /// drop-in's file name is read once, from the first of those
    /// directories that has it, and the drop-ins are read in file-name
    /// order (systemd's `unit_file_find_dropin_paths`,
    /// `src/shared/dropin.c`).
    fn unit_texts(&self, name: &str, display_manager: bool) -> Vec<String> {
        let template = name
            .split_once('@')
            .map(|(prefix, _)| format!("{prefix}@.service"));
        let file = match (self.find(name), &template) {
            (UnitFile::Missing, Some(template)) => self.find(template),
            (found, _) => found,
        };
        let mut texts: Vec<String> = match file {
            UnitFile::At(path) => {
                std::fs::read_to_string(self.host.root.join(path.trim_start_matches('/')))
                    .ok()
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        };
        let mut units = vec![name];
        units.extend(template.as_deref());
        if display_manager && name != DISPLAY_MANAGER {
            units.push(DISPLAY_MANAGER);
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        for unit in units {
            for dir in UNIT_DIRS {
                for dropin in drop_in_names(unit) {
                    dirs.push(self.path(dir).join(format!("{dropin}.d")));
                }
            }
        }
        dirs.extend(UNIT_DIRS.iter().map(|dir| self.path(dir).join("service.d")));
        let mut drop_ins: BTreeMap<String, PathBuf> = BTreeMap::new();
        for dir in &dirs {
            for file in names(dir).into_iter().filter(|f| f.ends_with(".conf")) {
                drop_ins
                    .entry(file.clone())
                    .or_insert_with(|| dir.join(file));
            }
        }
        texts.extend(
            drop_ins
                .values()
                .filter_map(|path| std::fs::read_to_string(path).ok()),
        );
        texts
    }

    /// The sessions on a VT now, from logind: the graphical sessions and
    /// greeters, and the others.
    fn sessions(&mut self) -> Step<(OnVts, OnVts)> {
        let list = self.loginctl(&["list-sessions", "--no-legend"])?;
        let ids = graphical::session_ids(&list.stdout);
        if ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut args = vec!["show-session"];
        args.extend(ids.iter().map(String::as_str));
        args.extend([
            "-p", "Name", "-p", "VTNr", "-p", "Service", "-p", "Type", "-p", "Class",
        ]);
        let show = self.loginctl(&args)?;
        Ok((
            graphical::graphical(&show.stdout),
            graphical::logins(&show.stdout),
        ))
    }

    fn loginctl(&mut self, args: &[&str]) -> Step<Reply> {
        let line = args.join(" ");
        let reply = self
            .host
            .systemd
            .loginctl(args)
            .map_err(|e| Stop(text::cannot_ask_logind(&line, &e.to_string())))?;
        if reply.ok {
            Ok(reply)
        } else {
            Err(Stop(text::cannot_ask_logind(
                &line,
                reply.stderr.trim_end(),
            )))
        }
    }

    /// The consoles running on each VT, from the running manager: every
    /// active unit named `PREFIX@ttyN.service`.
    fn running(&mut self) -> Step<BTreeMap<u16, BTreeSet<String>>> {
        let args = [
            "list-units",
            "--plain",
            "--no-legend",
            "--state=active",
            "*@tty*.service",
        ];
        let reply = self.systemctl_quiet(&args)?;
        let mut running: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
        for line in reply.stdout.lines() {
            if let Some((prefix, n)) = line.split_whitespace().next().and_then(instance) {
                running.entry(n).or_default().insert(prefix.to_owned());
            }
        }
        Ok(running)
    }

    /// Runs systemctl and fails the step when it fails, printing nothing.
    fn systemctl_quiet(&mut self, args: &[&str]) -> Step<Reply> {
        let line = args.join(" ");
        let reply = self
            .host
            .systemd
            .systemctl(args)
            .map_err(|e| Stop(text::cannot_run(&line, &e.to_string())))?;
        if reply.ok {
            Ok(reply)
        } else {
            Err(Stop(text::failed(&line, &reply.stderr)))
        }
    }

    /// Runs systemctl, saying so first and passing on what it printed.
    fn systemctl(&mut self, args: &[&str]) -> Step {
        let line = args.join(" ");
        self.say(&text::running(&line));
        let reply = self.systemctl_quiet(args)?;
        for printed in [&reply.stdout, &reply.stderr] {
            let printed = printed.trim_end();
            if !printed.is_empty() {
                let _ = writeln!(self.out, "{printed}");
            }
        }
        Ok(())
    }

    fn reload(&mut self) -> Step {
        self.systemctl(&["daemon-reload"])
    }

    /// Refuses VT `n` while the C reference's console holds it.
    fn refuse_kmscon(&self, state: &State, n: u16) -> Step {
        if state.holds(n, KMSCON) {
            return Err(Stop(text::kmscon_holds(n, state.own(n).names(KMSCON))));
        }
        Ok(())
    }

    /// Refuses VT `n` while another program holds it, or while vtium
    /// cannot tell whether graphics do.
    fn refuse_held(&self, state: &State, n: u16) -> Step {
        if let Some(why) = &state.sessions_unknown {
            return Err(Stop(why.clone()));
        }
        match state.held.get(&n) {
            Some(holders) => Err(Stop(text::graphics_hold(n, holders))),
            None => Ok(()),
        }
    }

    /// Refuses VT `n` for `enable N` when the development install holds
    /// it, vtium's unit is masked there, or its own link names another
    /// unit.
    fn refuse_vt(&self, state: &State, n: u16) -> Step {
        if state.holds(n, DEV) {
            return Err(Stop(text::dev_holds(n)));
        }
        if state.masked.contains(&n) {
            return Err(Stop(text::masked(n)));
        }
        match state.own(n) {
            Link::Absent => Ok(()),
            link if link.names(VTIUM) => Ok(()),
            Link::To(to) => Err(Stop(text::own_link_elsewhere(n, to))),
            _ => Err(Stop(text::own_link_not_link(n))),
        }
    }

    /// Writes the record `name` under the records directory, by a
    /// temporary name renamed into place.
    fn record(&mut self, name: &str, body: &str, what: &str) -> Step {
        let dir = self.path(RECORDS);
        let made = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)));
        let path = dir.join(name);
        let tmp = dir.join(format!(".{name}.vtium-new"));
        let written = made
            .and_then(|()| std::fs::write(&tmp, body))
            .and_then(|()| std::fs::rename(&tmp, &path));
        let shown = format!("/{RECORDS}/{name}");
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(Stop(text::cannot_write(&shown, &e.to_string())));
        }
        self.say(&text::recorded(what, &shown));
        Ok(())
    }

    fn forget(&mut self, name: &str) -> Step {
        let path = self.path(RECORDS).join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Stop(text::cannot_remove(
                &format!("/{RECORDS}/{name}"),
                &e.to_string(),
            ))),
        }
    }

    /// Links `dir/name` to `to`, by a link under a temporary name renamed
    /// into place, or removes it when `to` is `None`.
    fn link(&mut self, dir: &str, name: &str, to: Option<&str>) -> Step {
        let dir_path = self.path(dir);
        let path = dir_path.join(name);
        let shown = format!("/{dir}/{name}");
        match to {
            Some(to) => {
                self.say(&text::linking(&shown, to));
                let tmp = dir_path.join(format!(".{name}.vtium-new"));
                let _ = std::fs::remove_file(&tmp);
                let made = std::fs::create_dir_all(&dir_path)
                    .and_then(|()| symlink(to, &tmp))
                    .and_then(|()| std::fs::rename(&tmp, &path));
                if let Err(e) = made {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(Stop(text::cannot_write(&shown, &e.to_string())));
                }
            }
            None => {
                self.say(&text::removing(&shown));
                std::fs::remove_file(&path)
                    .map_err(|e| Stop(text::cannot_remove(&shown, &e.to_string())))?;
            }
        }
        Ok(())
    }

    /// `enable N`'s changes on VT `n`, once its refusals have passed:
    /// the getty's record, vtium's enable, then the getty's disable, each
    /// skipped when done.
    fn enable_vt(&mut self, state: &mut State, n: u16) -> Step {
        let getty = state.wanted(n, GETTY);
        self.record_vt(state, n)?;
        if !(state.wanted(n, VTIUM) && state.own(n).names(VTIUM)) {
            self.systemctl(&["enable", &format!("vtium@tty{n}.service")])?;
        }
        if getty {
            self.disable_getty(state, n)?;
        }
        Ok(())
    }

    /// Writes VT `n`'s record, unless one is there: the path its getty's
    /// want links to, or nothing when no getty is enabled there.
    fn record_vt(&mut self, state: &mut State, n: u16) -> Step {
        if state.records.contains_key(&n) {
            return Ok(());
        }
        let record = match state.wants.get(&n).and_then(|w| w.get(GETTY)) {
            None => Record::NoGetty,
            Some(Link::To(to)) if template_prefix(to) == Some(GETTY) => Record::Getty(to.clone()),
            // A want that is no link to a getty's unit file: the file
            // systemd finds stands for it.
            Some(_) => match self.unit_file(GETTY) {
                UnitFile::At(path) => Record::Getty(path),
                _ => Record::NoGetty,
            },
        };
        let (body, what) = match &record {
            Record::Getty(to) => (format!("{to}\n"), text::getty_unit(n, to)),
            _ => (String::new(), text::no_getty(n)),
        };
        self.record(&format!("tty{n}"), &body, &what)?;
        state.records.insert(n, record);
        Ok(())
    }

    fn disable_getty(&mut self, state: &mut State, n: u16) -> Step {
        self.systemctl(&["disable", &format!("getty@tty{n}.service")])?;
        if let Some(w) = state.wants.get_mut(&n) {
            w.remove(GETTY);
        }
        Ok(())
    }

    /// `disable N`'s changes on VT `n`: the getty back as the record
    /// found it, vtium's disable, the VT's leftover link, then the record.
    /// Returns whether a link was changed by hand, which the manager has
    /// yet to read.
    fn disable_vt(&mut self, state: &mut State, n: u16) -> Step<bool> {
        let record = state.records.get(&n).cloned();
        let getty = format!("getty@tty{n}.service");
        let mut by_hand = false;
        if !state.wanted(n, GETTY) {
            match &record {
                Some(Record::Getty(to)) => {
                    self.link(WANTS, &getty, Some(to))?;
                    by_hand = true;
                }
                Some(Record::NoGetty) => {}
                // No record, as the earlier hand-typed steps left VT 1:
                // without its getty VT 1 has no login at boot.
                None | Some(Record::Unreadable) if n == 1 => {
                    self.systemctl(&["add-wants", "getty.target", &getty])?;
                }
                None | Some(Record::Unreadable) => {}
            }
        }
        if state.wanted(n, VTIUM) {
            self.systemctl(&["disable", &format!("vtium@tty{n}.service")])?;
        }
        if state.own(n).names(VTIUM) {
            self.link(ETC, &format!("autovt@tty{n}.service"), None)?;
            by_hand = true;
        }
        if record.is_some() {
            self.forget(&format!("tty{n}"))?;
        }
        Ok(by_hand)
    }

    fn enable_all(&mut self, state: &mut State, unit: &str) -> Step<i32> {
        if state.template().names(KMSCON) {
            return Err(Stop(text::KMSCON_TEMPLATE.to_owned()));
        }
        if state.template_etc == Link::Other {
            return Err(Stop(text::TEMPLATE_NOT_LINK.to_owned()));
        }
        if let Some(why) = &state.sessions_unknown {
            return Err(Stop(why.clone()));
        }
        // logind starts autovt@tty6 on every switch to its reserved VT,
        // even while something holds it, so graphics there would meet
        // vtium through the template link, and so would a login, kmscon or
        // the development install there that VT 6's own link does not keep
        // from the template: vtium's Conflicts= stops the console when
        // logind starts vtium there. A getty waiting at its login prompt
        // holds no session, so stopping it ends nothing.
        if let Some(holders) = state.held.get(&RESERVED_VT) {
            return Err(Stop(text::graphics_on_reserved_vt(RESERVED_VT, holders)));
        }
        if state.templated(RESERVED_VT) {
            // A login inside a vtium that runs on VT 6 already is safe: the
            // switch's start of vtium@tty6 finds it active.
            if let Some(logins) = state
                .logins
                .get(&RESERVED_VT)
                .filter(|_| !state.runs(RESERVED_VT, VTIUM))
            {
                return Err(Stop(text::login_on_reserved_vt(RESERVED_VT, logins, true)));
            }
            if state.holds(RESERVED_VT, KMSCON) {
                return Err(Stop(text::kmscon_on_reserved_vt(RESERVED_VT)));
            }
            if state.holds(RESERVED_VT, DEV) {
                return Err(Stop(text::dev_on_reserved_vt(RESERVED_VT)));
            }
        }
        // What each VT keeps, and the VTs whose gettys go.
        let mut kept: Vec<String> = Vec::new();
        let mut gettys: Vec<u16> = Vec::new();
        for (n, holders) in &state.held {
            kept.push(text::kept_graphics(*n, holders));
        }
        for n in state.vts() {
            if state.held.contains_key(&n) {
                continue;
            }
            // kmscon on a VT of its own keeps it: its own link wins over
            // the template.
            if state.holds(n, KMSCON) {
                kept.push(text::kept_kmscon(n));
                continue;
            }
            if state.holds(n, DEV) {
                kept.push(text::kept_dev(n));
                continue;
            }
            // A mask keeps vtium from a VT only where logind does not load
            // vtium through the template link: on VT 1, where logind starts
            // no console, above its VTs, and where an enabled getty holds
            // the VT from boot. On VT 6, logind's reserved VT, it loads
            // vtium at every switch.
            if state.masked.contains(&n) {
                let through_template = state.templated(n)
                    && (n == RESERVED_VT
                        || ((2..=LOGIND_VTS).contains(&n) && !state.wanted(n, GETTY)));
                if through_template {
                    return Err(Stop(text::masked_under_template(n, n == RESERVED_VT)));
                }
                // The mask does not reach autovt@ttyN.service, which the
                // VT's own link still loads as vtium (measured).
                kept.push(if state.own(n).names(VTIUM) {
                    text::kept_masked_own_link(n)
                } else {
                    text::kept_masked(n, state.wanted(n, GETTY))
                });
                continue;
            }
            if n != 1
                && let Some(unit) = state.own(n).unit().filter(|u| *u != VTIUM)
            {
                kept.push(text::kept_own_link(n, unit));
            }
            if n != 1 && state.wanted(n, GETTY) {
                if n <= LOGIND_VTS {
                    gettys.push(n);
                } else {
                    kept.push(text::kept_high_getty(n));
                }
            }
        }
        // VT 1 gets `enable N` unless the development install, kmscon or
        // graphics hold it or vtium is masked there, so it is refused as
        // `enable N` would be.
        let vt1_kept = state.holds(1, DEV)
            || state.holds(1, KMSCON)
            || state.held.contains_key(&1)
            || state.masked.contains(&1);
        if !vt1_kept {
            self.refuse_vt(state, 1)?;
        }
        let template_done = state.template_etc.names(VTIUM);
        let vt1_done = vt1_kept || state.given(1);
        if template_done && vt1_done && gettys.is_empty() {
            // A run whose last step, the reload, failed finds its links
            // done, so a run that changes nothing reloads.
            self.reload()?;
            self.say(text::ALREADY_ALL);
            for line in &kept {
                self.say(line);
            }
            return Ok(0);
        }
        if !template_done {
            if state.template_record.is_none() {
                let body = match &state.template_etc {
                    Link::To(to) => format!("{to}\n"),
                    _ => String::new(),
                };
                self.record(TEMPLATE, &body, text::TEMPLATE_WHAT)?;
            }
            self.link(ETC, TEMPLATE, Some(unit))?;
        }
        if !vt1_done {
            self.enable_vt(state, 1)?;
        }
        for n in gettys {
            self.record_vt(state, n)?;
            self.disable_getty(state, n)?;
        }
        self.reload()?;
        self.say(&text::given_all(!vt1_kept, state.templated(RESERVED_VT)));
        for line in &kept {
            self.say(line);
        }
        Ok(0)
    }

    fn disable_all(&mut self, state: &mut State) -> Step<i32> {
        let vts: Vec<u16> = state
            .vts()
            .into_iter()
            .filter(|&n| state.vtium_on(n))
            .collect();
        let template = state.template_etc.names(VTIUM);
        if vts.is_empty() && !template && state.template_record.is_none() {
            // A run whose last step, the reload, failed finds its links
            // done, so a run that changes nothing reloads.
            self.reload()?;
            self.say(text::NOT_GIVEN_ANY);
            return Ok(0);
        }
        let mut running: Vec<u16> = Vec::new();
        for &n in &vts {
            self.disable_vt(state, n)?;
            if state.runs(n, VTIUM) {
                running.push(n);
            }
        }
        if template {
            // With no record, as after the earlier hand-typed steps, a
            // template link further down the search path, such as Debian's
            // in /lib, is in force once vtium's goes; with none, the getty's
            // is made, as systemd's preset makes it.
            let back = match &state.template_record {
                Some(back) => back.clone(),
                None if state.template_below => None,
                None => match self.unit_file(GETTY) {
                    UnitFile::At(path) => Some(path),
                    _ => None,
                },
            };
            self.link(ETC, TEMPLATE, back.as_deref())?;
        }
        self.forget(TEMPLATE)?;
        self.reload()?;
        self.say(text::TAKEN_BACK_ALL);
        // Every vtium still running, the ones logind started through the
        // template included.
        let mut still: BTreeSet<u16> = running.into_iter().collect();
        still.extend(state.vts().into_iter().filter(|&n| state.runs(n, VTIUM)));
        if !still.is_empty() {
            self.say(&text::still_running(&still.into_iter().collect::<Vec<_>>()));
        }
        Ok(0)
    }

    fn status(&mut self, state: &State) {
        let template = match state.template() {
            Link::Absent => text::template_none(),
            Link::Other => text::template_not_link(),
            link => text::template_names(link.unit().unwrap_or("?")),
        };
        self.say(&template);
        if let Some(dm) = &state.display_manager {
            self.say(&text::status_display_manager(dm));
        }
        if let Some(why) = &state.sessions_unknown {
            self.say(why);
        }
        let mut vts: BTreeSet<u16> = (1..=LOGIND_VTS).collect();
        vts.extend(state.vts());
        vts.extend(state.held.keys());
        let rows: Vec<[String; 4]> = vts
            .into_iter()
            .map(|n| {
                let mut at_boot = state
                    .wants
                    .get(&n)
                    .map(|w| units(w.keys(), n))
                    .unwrap_or_default();
                let mut running_graphics = String::new();
                for holder in state.held.get(&n).into_iter().flatten() {
                    let cell = text::holder_cell(holder);
                    match holder {
                        // An instance getty.target wants shows already.
                        Holder::Unit { .. } if !at_boot.split(", ").any(|c| c == cell) => {
                            at_boot = join(&at_boot, &cell);
                        }
                        Holder::Session { .. } => {
                            running_graphics = join(&running_graphics, &cell);
                        }
                        // A console running there shows in the running
                        // column already.
                        Holder::Unit { .. } | Holder::Console { .. } => {}
                    }
                }
                // logind starts a console by itself only on its VTs, and
                // never on VT 1: to ask whether a VT is busy it opens
                // /dev/tty1 itself, and that open makes VT 1 busy.
                let switched = match state.own(n) {
                    _ if n == 1 || (n > LOGIND_VTS && n != RESERVED_VT) => None,
                    Link::Absent => state.template().unit().map(|u| format!("{u}@tty{n}")),
                    Link::Other => Some(format!("autovt@tty{n}")),
                    link => link.unit().map(|u| format!("{u}@tty{n}")),
                };
                let running = join(
                    &state
                        .running
                        .get(&n)
                        .map(|r| units(r.iter(), n))
                        .unwrap_or_default(),
                    &running_graphics,
                );
                let mut boot = at_boot;
                if state.masked.contains(&n) {
                    boot = join(&boot, &text::masked_cell(n));
                }
                if let Some(record) = state.records.get(&n) {
                    boot = join(&boot, text::recorded_cell(record));
                }
                [n.to_string(), boot, switched.unwrap_or_default(), running]
                    .map(|c| if c.is_empty() { "-".to_owned() } else { c })
            })
            .collect();
        let header = text::STATUS_HEADER.map(str::to_owned);
        let mut widths = header.clone().map(|h| h.len());
        for row in &rows {
            for (w, cell) in widths.iter_mut().zip(row) {
                *w = (*w).max(cell.len());
            }
        }
        for row in std::iter::once(&header).chain(&rows) {
            let mut line = String::new();
            for (i, (cell, w)) in row.iter().zip(widths).enumerate() {
                if i == 3 {
                    line.push_str(cell);
                } else {
                    line.push_str(&format!("{cell:<w$}  "));
                }
            }
            self.say(line.trim_end());
        }
        // Why each VT a unit that starts at boot holds is held.
        for (n, holders) in &state.held {
            let units: Vec<Holder> = holders
                .iter()
                .filter(|h| matches!(h, Holder::Unit { .. }))
                .cloned()
                .collect();
            if !units.is_empty() {
                self.say(&text::status_held(*n, &units));
            }
        }
    }
}

fn units<'p>(prefixes: impl Iterator<Item = &'p String>, n: u16) -> String {
    prefixes
        .map(|p| format!("{p}@tty{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a}, {b}"),
    }
}

/// `vtium console`'s usage.
pub fn usage() -> &'static str {
    text::USAGE
}

/// The subcommand `args` names, run against the running system: `None`
/// when it names none, so the console starts. Its exit status otherwise,
/// 2 for words it cannot take.
pub fn subcommand(args: &[OsString]) -> Option<i32> {
    Some(match parse(args)? {
        Ok(Parsed::Run(verb)) => {
            let mut systemd = RunningSystemd;
            let mut host = Host {
                root: PathBuf::from("/"),
                root_user: rustix::process::geteuid().is_root(),
                systemd: &mut systemd,
            };
            run(verb, &mut host, &mut io::stdout(), &mut io::stderr())
        }
        Ok(Parsed::Help) => {
            print!("{}", usage());
            0
        }
        Err(e) => {
            eprint!("vtium console: {e}\n{}", usage());
            2
        }
    })
}

#[cfg(test)]
mod tests;
