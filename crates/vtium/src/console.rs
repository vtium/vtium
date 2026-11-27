//! `vtium console`: gives one VT, or every VT, to the packaged vtium, and
//! takes them back (ADR-1500 D7). vtium runs on a VT in one of two modes
//! (ADR-1400 D3): direct mode through `vtium@.service`, and seat mode
//! through `vtium-seat@.service`. `enable` takes the mode as `--direct` or
//! `--seat`, and moves a VT that has the other mode to the one asked for.
//! Without a mode it changes nothing and gives advice (`advice`).
//!
//! systemd starts the units `getty.target` wants at boot, and logind starts
//! `autovt@ttyN.service` on a VT that nothing holds when the user switches
//! to it. `/etc/systemd/system/autovt@ttyN.service`, a VT's own link, wins
//! over `/etc/systemd/system/autovt@.service`, the template link that every
//! other VT follows. `enable N` enables the mode's unit on VT N, such as
//! `vtium@ttyN`, and disables the VT's getty; `enable --all` also links the
//! template to the mode's unit and does `enable N` on each free VT from 1 to
//! 6. So systemd starts vtium at boot on each VT it claims, and vtium holds
//! each one open from boot, where a display manager that puts a desktop
//! session on the first free VT would otherwise take it. Before vtium
//! disables a getty or moves the template link, it writes a record of
//! what was there under `/var/lib/vtium/console/`, and `disable` puts back
//! what the records say. It passes by every VT another program holds: a
//! VT that a unit enabled at boot, such as the display manager's, claims,
//! judged from what is on the machine (`held`), and a VT a graphical
//! session, a greeter or another console is on now, or that is the
//! controlling terminal of a program running now (`process`). `fallback N`, which
//! `vtium-getty@.service` runs when vtium on VT N fails, starts the VT's
//! getty only when that start stops no other program (`fallback`).
//!
//! The subcommand starts and stops nothing, since either ends every
//! session on the VT, and masks nothing, since a mask is the
//! administrator's. It refuses a VT that kmscon's console holds.
//!
//! Everything it touches is reached through [`Host`]: a root directory,
//! `/` on the running system and a scratch directory in the tests, and a
//! [`Systemd`], which runs `systemctl` and `loginctl` on the running system
//! and is a stand-in in the tests. It reads systemd's links itself, and changes them
//! through `systemctl enable`, `disable`, `add-wants` and `daemon-reload`,
//! but for the links no systemctl verb makes or removes as they were: the
//! template link, a VT's `autovt@ttyN.service` link that `disable` leaves,
//! and a getty's want restored to the unit file it linked to.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

mod advice;
mod fallback;
mod graphical;
mod held;
mod process;
mod state;
mod systemd;
mod text;
mod verbs;

pub use state::{Enabled, enabled_vts, running_vts};
pub use systemd::{Host, Reply, RunningSystemd, Systemd};

use state::names;
use systemd::{Record, instance};
use verbs::{Run, Step, Stop};

/// The highest VT number the kernel has (`MAX_NR_CONSOLES`).
pub const LAST_VT: u16 = 63;

/// The VTs logind starts consoles on by itself: 1 to `NAutoVTs=`, whose
/// default is 6 (logind.conf(5)).
const LOGIND_VTS: u16 = 6;

/// The packaged vtium's unit in direct mode, `vtium@.service`, by its
/// template's prefix.
const VTIUM: &str = "vtium";
/// The packaged vtium's unit in seat mode, `vtium-seat@.service`.
const SEAT: &str = "vtium-seat";
/// The packaged vtium's units, one for each mode.
const VTIUMS: [&str; 2] = [VTIUM, SEAT];
/// The development install's unit (ADR-1500 D4).
const DEV: &str = "vtium-dev";
/// systemd's getty.
const GETTY: &str = "getty";
/// kmscon's console, the released `kmscon`'s unit `kmsconvt@.service`.
const KMSCON: &str = "kmsconvt";
/// The consoles `vtium console` knows by name, each with rules of its own.
const CONSOLES: [&str; 5] = [VTIUM, SEAT, DEV, GETTY, KMSCON];

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
/// The file logind keeps for seat0 while it runs and manages that seat, in
/// the directory sd-login's `sd_get_seats` lists.
const SEAT0: &str = "run/systemd/seats/seat0";
/// The PAM stack seat mode's unit opens vtium's own session through
/// (`PAMName=vtium`, ADR-1400 D3).
const SEAT_PAM: &str = "etc/pam.d/vtium";

/// How vtium runs on a VT (ADR-1400 D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// vtium opens the screen and the keyboards itself, through
    /// `vtium@.service`.
    Direct,
    /// vtium takes the seat from logind through a session of its own,
    /// through `vtium-seat@.service`.
    Seat,
}

impl Mode {
    /// The prefix of the mode's unit.
    fn prefix(self) -> &'static str {
        match self {
            Mode::Direct => VTIUM,
            Mode::Seat => SEAT,
        }
    }

    /// The flag that names the mode.
    pub fn flag(self) -> &'static str {
        match self {
            Mode::Direct => "--direct",
            Mode::Seat => "--seat",
        }
    }
}

/// The prefix of the other mode's unit.
fn other_vtium(prefix: &str) -> &'static str {
    if prefix == SEAT { VTIUM } else { SEAT }
}

/// Which VTs a verb acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    All,
    Vt(u16),
}

/// What the subcommand's words ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Enable(Which, Mode),
    /// `enable` with no mode: advice on the mode, for the VTs named or for
    /// every VT.
    Advise(Option<Which>),
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
            Verb::Enable(w, m) => format!("enable {} {}", m.flag(), which_word(w)),
            Verb::Advise(Some(w)) => format!("enable {}", which_word(w)),
            Verb::Advise(None) => "enable".to_owned(),
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
        "enable" => {
            let mut mode: Option<Mode> = None;
            let mut vts: Option<Which> = None;
            for word in rest {
                let flag = match *word {
                    "--direct" => Some(Mode::Direct),
                    "--seat" => Some(Mode::Seat),
                    _ => None,
                };
                if let Some(flag) = flag {
                    if mode.is_some_and(|m| m != flag) {
                        return Err(text::TWO_MODES.to_owned());
                    }
                    if mode.is_some() {
                        return Err(text::extra_word(word));
                    }
                    mode = Some(flag);
                } else if vts.is_some() {
                    return Err(text::extra_word(word));
                } else {
                    vts = Some(which(word)?);
                }
            }
            Ok(Parsed::Run(match mode {
                Some(mode) => match vts {
                    Some(vts) => Verb::Enable(vts, mode),
                    None => return Err(text::needs_which(&format!("enable {}", mode.flag()))),
                },
                None => Verb::Advise(vts),
            }))
        }
        "disable" => {
            let which = match rest {
                [] => return Err(text::needs_which(verb)),
                [word] if *word == "--direct" || *word == "--seat" => {
                    return Err(text::disable_no_mode(word));
                }
                [word] => which(word)?,
                [_, extra, ..] => return Err(text::extra_word(extra)),
            };
            Ok(Parsed::Run(Verb::Disable(which)))
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
                seat_build: cfg!(all(feature = "libseat", feature = "logind")),
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
