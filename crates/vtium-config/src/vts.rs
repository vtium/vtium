//! What an install does after it writes `/etc/vtium/vtium.conf` (ADR-1201
//! D5). vtium reads vtium.conf only when it starts on a VT, and a head
//! whose `vt` names VT N appears only on the vtium of VT N (ADR-1401 D2),
//! which runs only where vtium is enabled (ADR-1500 D7). So after an
//! install vtium-config reads `vtium console status`, runs `vtium console
//! enable` on each VT a head names where vtium is not enabled, and asks
//! whether to start vtium on those VTs and restart it where it runs, so
//! that the new file applies now.
//!
//! `vtium console` decides whether vtium can have a VT, with its own
//! checks of what holds the VT; vtium-config checks nothing itself and
//! shows what `vtium console` prints, refusals included. Each command that
//! changes the system runs through `via`, the route that ran `apply DIR`:
//! sudo for a user, nothing for root. `systemctl` starts and restarts
//! vtium only after the person's yes.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;

use crate::apply::{Ask, Runner};

/// How vtium runs on a VT (ADR-1400 D3), as `vtium console` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// vtium opens the screen and the keyboards itself, through
    /// `vtium@.service`.
    Direct,
    /// vtium takes the seat from logind, through `vtium-seat@.service`.
    Seat,
}

impl Mode {
    /// The option of `vtium console enable` that names the mode.
    pub fn flag(self) -> &'static str {
        match self {
            Mode::Direct => "--direct",
            Mode::Seat => "--seat",
        }
    }

    /// vtium's unit in this mode on VT `vt`, as systemd names it.
    pub fn unit(self, vt: u16) -> String {
        match self {
            Mode::Direct => format!("vtium@tty{vt}"),
            Mode::Seat => format!("vtium-seat@tty{vt}"),
        }
    }

    fn words(self) -> &'static str {
        match self {
            Mode::Direct => "direct mode",
            Mode::Seat => "seat mode",
        }
    }
}

/// Where vtium is, as `vtium console status` says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Each VT where a unit of vtium's starts at boot or when the VT is
    /// switched to, with that unit's mode.
    pub enabled: BTreeMap<u16, Mode>,
    /// Each VT where vtium runs now, with its mode.
    pub running: BTreeMap<u16, Mode>,
    /// Each VT where something other than vtium runs now, as the status
    /// names it, such as `getty@tty12`.
    pub others: BTreeMap<u16, String>,
}

/// The titles of the columns of `vtium console status`'s table.
const AT_BOOT: &str = "at boot";
const SWITCHED: &str = "when switched to";
const RUNNING: &str = "running";

/// Reads `vtium console status`'s table: its title row, which starts with
/// `VT` and names the three columns, and below it a row for each VT, which
/// starts with the VT's number. Each column starts where its title starts.
/// `None` when the text holds no such table.
pub fn read_status(text: &str) -> Option<Status> {
    let mut lines = text.lines();
    let title = lines.by_ref().find(|l| {
        l.starts_with("VT ") && [AT_BOOT, SWITCHED, RUNNING].iter().all(|c| l.contains(c))
    })?;
    let boot = title.find(AT_BOOT)?;
    let switched = title.find(SWITCHED)?;
    let running = title.find(RUNNING)?;
    let mut status = Status::default();
    for line in lines {
        let Some(vt) = line
            .split_whitespace()
            .next()
            .and_then(|w| w.parse::<u16>().ok())
        else {
            break;
        };
        let cell = |from: usize, to: Option<usize>| -> &str {
            let to = to.unwrap_or(line.len()).min(line.len());
            line.get(from..to).unwrap_or("").trim()
        };
        let enabled =
            mode_in(cell(boot, Some(switched)), vt).or(mode_in(cell(switched, Some(running)), vt));
        if let Some(mode) = enabled {
            status.enabled.insert(vt, mode);
        }
        let now = cell(running, None);
        if let Some(mode) = mode_in(now, vt) {
            status.running.insert(vt, mode);
        }
        let others: Vec<&str> = parts(now)
            .filter(|p| *p != "-" && *p != Mode::Direct.unit(vt) && *p != Mode::Seat.unit(vt))
            .collect();
        if !others.is_empty() {
            status.others.insert(vt, others.join(", "));
        }
    }
    Some(status)
}

/// The parts of a cell of the table, which separates them with commas.
fn parts(cell: &str) -> impl Iterator<Item = &str> {
    cell.split(", ").map(str::trim).filter(|p| !p.is_empty())
}

/// The mode of the unit of vtium's that the cell names for VT `vt`, as a
/// part of its own: `vtium@tty2 masked` is not one.
fn mode_in(cell: &str, vt: u16) -> Option<Mode> {
    parts(cell).find_map(|part| {
        [Mode::Direct, Mode::Seat]
            .into_iter()
            .find(|m| part == m.unit(vt))
    })
}

/// The VTs the heads of vtium.conf's text `conf` name in their `vt`, each
/// with the heads that name it, as vtium reads the sections.
pub fn named(conf: &str) -> BTreeMap<u16, Vec<String>> {
    let mut out: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return out;
    };
    for head in &reading.parsed.config.heads {
        for vt in &head.vts {
            out.entry(*vt).or_default().push(head.name.clone());
        }
    }
    out
}

/// Why vtium-config enables vtium in the mode it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// Every VT vtium is enabled on uses this mode.
    Shared,
    /// vtium is enabled on no VT, so direct mode.
    NoVt,
    /// vtium uses both modes, so direct mode.
    Mixed,
}

/// The mode vtium-config enables vtium in: the one the VTs vtium is
/// enabled on use, else direct mode.
pub fn mode(status: &Status) -> (Mode, Chosen) {
    let mut modes = status.enabled.values();
    let Some(first) = modes.next() else {
        return (Mode::Direct, Chosen::NoVt);
    };
    if modes.all(|m| m == first) {
        (*first, Chosen::Shared)
    } else {
        (Mode::Direct, Chosen::Mixed)
    }
}

/// What the step after an install did, for the TUI's message line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The VTs `vtium console` would not enable vtium on, each with the
    /// heads that name it.
    pub refused: Vec<(u16, Vec<String>)>,
    pub applied: Applied,
}

/// What became of the start and restart that apply the new file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Applied {
    /// There was none to offer, or nobody to ask.
    #[default]
    Nothing,
    /// The person answered no.
    Declined,
    /// vtium started or restarted on each of these VTs.
    Done(Vec<u16>),
    /// `systemctl` failed.
    Failed,
}

/// The start and restart that apply a new vtium.conf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    /// The route each command runs through: `sudo`, or nothing for root.
    pub via: Vec<OsString>,
    /// The VTs vtium was just enabled on, where it does not run yet.
    pub start: Vec<(u16, Mode)>,
    /// The VTs vtium runs on now.
    pub restart: Vec<(u16, Mode)>,
}

impl Offer {
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.restart.is_empty()
    }

    /// The commands that apply the file: `systemctl start` on the VTs to
    /// start and `systemctl restart` on those to restart.
    pub fn commands(&self) -> Vec<Vec<OsString>> {
        let mut out = Vec::new();
        for (verb, vts) in [("start", &self.start), ("restart", &self.restart)] {
            if vts.is_empty() {
                continue;
            }
            let mut argv = self.via.clone();
            argv.extend(["systemctl".into(), verb.into()]);
            argv.extend(vts.iter().map(|(vt, mode)| mode.unit(*vt).into()));
            out.push(argv);
        }
        out
    }

    /// Each VT the commands start vtium on or restart it on.
    fn vts(&self) -> Vec<u16> {
        let mut vts: Vec<u16> = self
            .start
            .iter()
            .chain(&self.restart)
            .map(|(vt, _)| *vt)
            .collect();
        vts.sort_unstable();
        vts
    }
}

/// A command as the person would type it.
pub fn shown(argv: &[OsString]) -> String {
    argv.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Asks `question` on standard input, the terminal: writes it to `out`
/// and takes `y` or `yes` as yes, and anything else as no.
pub fn ask_line(out: &mut dyn Write, question: &str) -> bool {
    let _ = write!(out, "{question} [y/N] ");
    let _ = out.flush();
    let mut answer = String::new();
    let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut answer);
    matches!(answer.trim(), "y" | "Y" | "yes")
}

/// Writes `text` to `out` as a line.
fn say(out: &mut dyn Write, text: &str) {
    let _ = writeln!(out, "{text}");
}

/// Writes what a command printed to `out`, when it printed anything.
fn said(out: &mut dyn Write, printed: &str) {
    let printed = printed.trim_end();
    if !printed.is_empty() {
        say(out, printed);
    }
}

/// After an install wrote `conf` as the system's vtium.conf: reads `vtium
/// console status` with the console `vtium`, enables vtium on each VT a
/// head names where it is not enabled, and offers to start vtium on those
/// VTs and restart it where it runs, asking through `ask`; with no `ask`,
/// it prints the commands that would. Everything it does and everything
/// the commands print goes to `out`.
pub fn reach(
    conf: &str,
    vtium: &OsString,
    via: &[OsString],
    run: Runner<'_>,
    ask: Option<Ask<'_>>,
    out: &mut dyn Write,
) -> Outcome {
    let mut outcome = Outcome::default();
    let argv: Vec<OsString> = vec![vtium.clone(), "console".into(), "status".into()];
    let ran = run(&argv);
    let status = match &ran {
        Ok((true, printed)) => read_status(printed),
        _ => None,
    };
    let Some(status) = status else {
        say(out, STATUS_UNREADABLE);
        say(out, &shown(&argv));
        match ran {
            Ok((_, printed)) => said(out, &printed),
            Err(e) => say(out, &cannot_run(&argv, &e)),
        }
        return outcome;
    };
    let missing: Vec<(u16, Vec<String>)> = named(conf)
        .into_iter()
        .filter(|(vt, _)| !status.enabled.contains_key(vt))
        .collect();
    let mut offer = Offer {
        via: via.to_vec(),
        start: Vec::new(),
        restart: status.running.iter().map(|(vt, m)| (*vt, *m)).collect(),
    };
    if !missing.is_empty() {
        let (mode, chosen) = mode(&status);
        say(out, HEAD);
        for (vt, heads) in &missing {
            say(out, &not_enabled(*vt, heads));
        }
        say(out, &mode_chosen(mode, chosen));
        for (vt, heads) in missing {
            let mut argv = via.to_vec();
            argv.extend([
                vtium.clone(),
                "console".into(),
                "enable".into(),
                mode.flag().into(),
                vt.to_string().into(),
            ]);
            say(out, &shown(&argv));
            let enabled = match run(&argv) {
                Ok((ok, printed)) => {
                    said(out, &printed);
                    ok
                }
                Err(e) => {
                    say(out, &cannot_run(&argv, &e));
                    false
                }
            };
            if !enabled {
                say(out, &refused(vt, &heads, &shown(&argv)));
                outcome.refused.push((vt, heads));
            } else if !status.running.contains_key(&vt) {
                offer.start.push((vt, mode));
            }
        }
    }
    if offer.is_empty() {
        return outcome;
    }
    say(out, READ_AT_START);
    outcome.applied = match ask {
        None => {
            say(out, &later(&offer));
            Applied::Nothing
        }
        Some(ask) => {
            if ask(out, &question(&offer, &status)) {
                apply(&offer, run, out)
            } else {
                say(out, NOT_RESTARTED);
                say(out, &later(&offer));
                Applied::Declined
            }
        }
    };
    outcome
}

/// Runs `offer`'s commands through `run`, showing each and what it
/// prints.
pub fn apply(offer: &Offer, run: Runner<'_>, out: &mut dyn Write) -> Applied {
    for argv in offer.commands() {
        say(out, &shown(&argv));
        match run(&argv) {
            Ok((true, printed)) => said(out, &printed),
            Ok((false, printed)) => {
                said(out, &printed);
                say(out, SYSTEMCTL_FAILED);
                return Applied::Failed;
            }
            Err(e) => {
                say(out, &cannot_run(&argv, &e));
                say(out, SYSTEMCTL_FAILED);
                return Applied::Failed;
            }
        }
    }
    let vts = offer.vts();
    say(out, &applied(&vts));
    Applied::Done(vts)
}

/// VT numbers as a person reads them: "VT 4", "VTs 4 and 5", or "VTs 2,
/// 3, and 5".
pub fn vt_list(vts: &[u16]) -> String {
    match vts {
        [] => String::new(),
        [n] => format!("VT {n}"),
        [a, b] => format!("VTs {a} and {b}"),
        [first @ .., last] => {
            let first: Vec<String> = first.iter().map(u16::to_string).collect();
            format!("VTs {}, and {last}", first.join(", "))
        }
    }
}

/// Heads' names as a person reads them: "head a", "heads a and b".
fn head_list(heads: &[String]) -> String {
    match heads {
        [] => String::new(),
        [one] => format!("head {one}"),
        [a, b] => format!("heads {a} and {b}"),
        [first @ .., last] => format!("heads {}, and {last}", first.join(", ")),
    }
}

/// Capitalizes the first letter of `text`.
fn first_up(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    })
}

// The words this step shows (AGENTS.md, "Strings on screen").

/// What `vtium console status` failing means.
pub const STATUS_UNREADABLE: &str = "vtium-config cannot tell where vtium runs, because vtium console status failed. So vtium-config enables vtium on no VT, and starts or restarts nothing. The new vtium.conf applies on each VT when vtium next starts there. This is the command and what it printed:";

/// What a head is, said before the VTs a head names where vtium is not
/// enabled.
pub const HEAD: &str = "A head is a terminal for one or more displays. A head with a vt line appears only on the VTs that line names, and only where vtium is enabled.";

/// A VT the heads `heads` name, where vtium is not enabled.
pub fn not_enabled(vt: u16, heads: &[String]) -> String {
    let verb = if heads.len() == 1 { "runs" } else { "run" };
    format!(
        "{} {verb} on VT {vt}, but vtium is not enabled there.",
        first_up(&head_list(heads))
    )
}

/// The mode vtium-config enables vtium in, and why.
pub fn mode_chosen(mode: Mode, chosen: Chosen) -> String {
    let why = match chosen {
        Chosen::Shared => "the mode vtium uses on the VTs it is enabled on now".to_owned(),
        Chosen::NoVt => "because vtium is not enabled on any VT yet".to_owned(),
        Chosen::Mixed => {
            "because vtium uses direct mode on some VTs and seat mode on others".to_owned()
        }
    };
    format!(
        "vtium-config enables vtium there in {}, {why}:",
        mode.words()
    )
}

/// What vtium-config says when `vtium console` does not enable vtium on
/// VT `vt`; `command` is the command that tries again.
pub fn refused(vt: u16, heads: &[String], command: &str) -> String {
    format!(
        "vtium console did not enable vtium on VT {vt}, and the lines above say why. {} does not appear until VT {vt} is free and vtium is enabled there. The new vtium.conf is installed all the same. Once VT {vt} is free, run: {command}",
        first_up(&head_list(heads))
    )
}

/// A command that could not start.
pub fn cannot_run(argv: &[OsString], e: &std::io::Error) -> String {
    let program = argv
        .first()
        .map(|a| a.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("vtium-config cannot run {program}: {e}.")
}

/// Why the new file needs vtium to start again.
pub const READ_AT_START: &str = "vtium reads vtium.conf only when it starts on a VT. The new file applies on each VT when vtium starts there again.";

/// The question that offers the start and restart; `status` says what
/// else runs on the VTs to start.
pub fn question(offer: &Offer, status: &Status) -> String {
    let start: Vec<u16> = offer.start.iter().map(|(vt, _)| *vt).collect();
    let restart: Vec<u16> = offer.restart.iter().map(|(vt, _)| *vt).collect();
    let mut text = String::new();
    if !restart.is_empty() {
        text.push_str("A restart keeps the sessions on a VT running, because systemd holds their terminals open while vtium starts again. Each screen is cleared and drawn again, and its scrollback is lost. ");
    }
    for (vt, _) in &offer.start {
        if let Some(other) = status.others.get(vt) {
            text.push_str(&format!(
                "Starting vtium on VT {vt} stops what runs there now, {other}, and ends its sessions. "
            ));
        }
    }
    let ask = match (start.is_empty(), restart.is_empty()) {
        (false, false) => format!(
            "Start vtium on {}, and restart it on {}?",
            vt_list(&start),
            vt_list(&restart)
        ),
        (false, true) => format!("Start vtium on {}?", vt_list(&start)),
        _ => format!("Restart vtium on {}?", vt_list(&restart)),
    };
    text.push_str(&ask);
    text
}

/// What the TUI says before it takes its screen back, when the step wrote
/// something after its last question.
pub const PRESS_ENTER: &str = "Press Enter to go back to vtium-config.";

/// What vtium-config says when the person answers no.
pub const NOT_RESTARTED: &str = "Nothing was started or restarted.";

/// The commands that apply the new file later, one a line.
pub fn later(offer: &Offer) -> String {
    let mut text = "To apply the new vtium.conf now, run:".to_owned();
    for argv in offer.commands() {
        text.push_str("\n  ");
        text.push_str(&shown(&argv));
    }
    text
}

/// What vtium-config says when `systemctl` fails.
pub const SYSTEMCTL_FAILED: &str =
    "systemctl failed, and the lines above say why. The new vtium.conf is installed all the same.";

/// What vtium-config says once vtium started or restarted on `vts`.
pub fn applied(vts: &[u16]) -> String {
    format!(
        "vtium now runs with the new vtium.conf on {}.",
        vt_list(vts)
    )
}

/// The TUI's message after an install of the draft `name` that ran this
/// step.
pub fn message(name: &str, outcome: &Outcome) -> String {
    let mut text = match &outcome.applied {
        Applied::Done(vts) => format!(
            "Installed draft {name}. vtium now runs with it on {}.",
            vt_list(vts)
        ),
        Applied::Failed => format!(
            "Installed draft {name}, but systemctl could not start or restart vtium on every VT. Run vtium console status to see where vtium runs."
        ),
        Applied::Nothing | Applied::Declined => crate::tui::words::installed(name),
    };
    for (vt, heads) in &outcome.refused {
        text.push_str(&format!(
            " vtium is not enabled on VT {vt}, so {} does not appear there.",
            head_list(heads)
        ));
    }
    text
}
