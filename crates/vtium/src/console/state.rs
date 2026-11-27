//! What `vtium console` reads of the host before it acts: which VTs each
//! console is enabled and running on, the template link, the masks and
//! vtium's records.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::ordered;

use super::held::Holder;
use super::systemd::{Link, Record, instance};
use super::verbs::{Step, Stop, join, units};
use super::{
    BOOTED, ETC, GETTY, LOGIND_VTS, Mode, RESERVED_VT, Reply, SEAT, Systemd, TEMPLATE, UNIT_DIRS,
    VTIUM, VTIUMS, WANTS, other_vtium, text,
};

/// What vtium reads of systemd's state and its own records.
#[derive(Debug, Default)]
pub(super) struct State {
    /// The prefix of the unit of the mode a verb gives VTs to: `vtium`, or
    /// `vtium-seat` for `enable --seat`.
    pub(super) prefix: &'static str,
    /// The template link in force: `autovt@.service` in the first unit
    /// directory that has it.
    template: Link,
    /// The template link that vtium manages, in `/etc/systemd/system`.
    pub(super) template_etc: Link,
    /// Whether a unit directory after `/etc/systemd/system` has the
    /// template link, which is in force without the one in `/etc`.
    pub(super) template_below: bool,
    /// The units `getty.target` wants on each VT, by prefix, and what each
    /// want links to.
    pub(super) wants: BTreeMap<u16, BTreeMap<String, Link>>,
    /// The display manager's unit, such as `sddm.service`, when
    /// `display-manager.service` is linked or there and not masked.
    pub(super) display_manager: Option<String>,
    /// What other programs hold each VT: the units enabled at boot that
    /// claim it, and the graphical sessions, greeters and other consoles on
    /// it now.
    pub(super) held: BTreeMap<u16, Vec<Holder>>,
    /// The other sessions on each VT, such as a user's login at a getty.
    pub(super) logins: BTreeMap<u16, Vec<Holder>>,
    /// Why logind could not be asked which sessions run, when it could not.
    pub(super) sessions_unknown: Option<String>,
    /// Each VT's own `autovt@ttyN.service` link.
    pub(super) own: BTreeMap<u16, Link>,
    /// The VTs whose unit of the mode asked for, such as `vtium@ttyN`, is
    /// masked.
    pub(super) masked: BTreeSet<u16>,
    /// The units running on each VT, by prefix.
    pub(super) running: BTreeMap<u16, BTreeSet<String>>,
    /// What each VT had before `enable` took it.
    pub(super) records: BTreeMap<u16, Record>,
    /// The template record: `None` with none; `Some(None)` for a template
    /// that linked nowhere; `Some(Some(path))` for one that linked to path.
    pub(super) template_record: Option<Option<String>>,
}

/// Whether a link names one of vtium's two units.
pub(super) fn names_vtium(link: &Link) -> bool {
    VTIUMS.iter().any(|p| link.names(p))
}

/// The prefixes of vtium's two units that a cell of `status`'s table
/// names on VT `n`, `vtium@ttyN` or `vtium-seat@ttyN`, each as one of its
/// parts, which `, ` separates.
fn vtium_parts(cell: &str, n: u16) -> impl Iterator<Item = &'static str> + '_ {
    cell.split(", ").filter_map(move |part| {
        VTIUMS
            .iter()
            .copied()
            .find(|p| part == format!("{p}@tty{n}"))
    })
}

/// The VTs vtium is enabled on, and the mode `vtium console enable` gives
/// another VT to match them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enabled {
    pub vts: BTreeSet<u16>,
    /// Seat mode when every unit of vtium's that the VTs name is seat
    /// mode's, and direct mode when they mix or none is enabled, the
    /// reading vtium-config's install uses.
    pub mode: Mode,
}

/// The template link in each unit directory under `root` that has one,
/// in the search path's order.
fn templates(root: &Path) -> Vec<Link> {
    UNIT_DIRS
        .iter()
        .map(|dir| Link::read(&root.join(dir).join(TEMPLATE)))
        .filter(|link| *link != Link::Absent)
        .collect()
}

/// Whether logind starts a console by itself on VT `n` when the user
/// switches there: on its VTs and its reserved VT, and never on VT 1,
/// since to ask whether a VT is busy it opens /dev/tty1 itself, and that
/// open makes VT 1 busy.
fn logind_starts(n: u16) -> bool {
    n != 1 && (n <= LOGIND_VTS || n == RESERVED_VT)
}

/// The VTs vtium is enabled on under `root`, in either mode: where
/// systemd starts it at boot, or where logind starts it when the user
/// switches there, as the "at boot" and "when switched to" cells of
/// `vtium console status`'s table name them (`State::vtium_enabled`).
/// It reads systemd's links alone, and runs nothing, so the cells hold
/// every part that names a unit of vtium's. The mode is the one those
/// units share, else direct mode. `None` when `root` was not booted with
/// systemd, so no link says anything.
pub fn enabled_vts(root: &Path) -> Option<Enabled> {
    if !root.join(BOOTED).is_dir() {
        return None;
    }
    let state = State::links(root, VTIUM);
    let mut all = ordered::set(1..=LOGIND_VTS);
    all.insert(RESERVED_VT);
    all.extend(state.vts());
    let mut vts = BTreeSet::new();
    let mut prefixes = BTreeSet::new();
    for n in all {
        let named = state.vtium_enabled(n);
        if !named.is_empty() {
            vts.insert(n);
            prefixes.extend(named);
        }
    }
    let mode = if prefixes.iter().eq([SEAT].iter()) {
        Mode::Seat
    } else {
        Mode::Direct
    };
    Some(Enabled { vts, mode })
}

/// The VTs vtium runs on now, in either mode, from systemd's running
/// units as `vtium console status` reads them, or why systemctl could not
/// say.
pub fn running_vts(systemd: &mut dyn Systemd) -> Result<BTreeSet<u16>, String> {
    let running = running(systemd).map_err(|Stop(why)| why)?;
    Ok(ordered::set(
        running
            .into_iter()
            .filter(|(_, prefixes)| VTIUMS.iter().any(|p| prefixes.contains(*p)))
            .map(|(n, _)| n),
    ))
}

/// The consoles running on each VT, from the running manager: every
/// active unit named `PREFIX@ttyN.service`.
pub(super) fn running(systemd: &mut dyn Systemd) -> Step<BTreeMap<u16, BTreeSet<String>>> {
    let args = [
        "list-units",
        "--plain",
        "--no-legend",
        "--state=active",
        "*@tty*.service",
    ];
    let reply = systemctl_quiet(systemd, &args)?;
    let mut running: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
    for line in reply.stdout.lines() {
        if let Some((prefix, n)) = line.split_whitespace().next().and_then(instance) {
            running.entry(n).or_default().insert(prefix.to_owned());
        }
    }
    Ok(running)
}

/// Runs systemctl and fails the step when it fails, printing nothing.
pub(super) fn systemctl_quiet(systemd: &mut dyn Systemd, args: &[&str]) -> Step<Reply> {
    let line = args.join(" ");
    let reply = systemd
        .systemctl(args)
        .map_err(|e| Stop(text::cannot_run(&line, &e.to_string())))?;
    if reply.ok {
        Ok(reply)
    } else {
        Err(Stop(text::failed(&line, &reply.stderr)))
    }
}

impl State {
    /// What systemd's links under `root` say for the mode whose unit's
    /// prefix is `prefix`: the template link, the units `getty.target`
    /// wants on each VT, each VT's own link, and the VTs where the mode's
    /// unit is masked.
    pub(super) fn links(root: &Path, prefix: &'static str) -> State {
        let etc = root.join(ETC);
        let template_etc = Link::read(&etc.join(TEMPLATE));
        let templates = templates(root);
        let below = usize::from(template_etc != Link::Absent);
        let mut state = State {
            prefix,
            template: templates.first().cloned().unwrap_or_default(),
            template_below: templates.len() > below,
            template_etc,
            ..State::default()
        };
        let wants = root.join(WANTS);
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
        for name in names(&etc) {
            match instance(&name) {
                Some(("autovt", n)) => {
                    state.own.insert(n, Link::read(&etc.join(&name)));
                }
                Some((p, n)) if p == prefix && Link::read(&etc.join(&name)).masks() => {
                    state.masked.insert(n);
                }
                _ => {}
            }
        }
        state
    }

    /// The template prefix of the unit logind starts on VT `n` when the
    /// user switches there: the unit the VT's own link names, else the
    /// template link's. `None` where logind starts nothing by itself, and
    /// where the link it follows names no template.
    fn switched_unit(&self, n: u16) -> Option<&str> {
        if !logind_starts(n) {
            return None;
        }
        match self.own(n) {
            Link::Absent => self.template().unit(),
            link => link.unit(),
        }
    }

    /// The "at boot" cell of VT `n`'s row in `status`'s table: the units
    /// `getty.target` wants there, the units that start at boot and hold
    /// it, a mask of the mode's unit, and the VT's record, each a part of
    /// its own.
    pub(super) fn boot_cell(&self, n: u16) -> String {
        let mut cell = self
            .wants
            .get(&n)
            .map(|w| units(w.keys(), n))
            .unwrap_or_default();
        for holder in self.held.get(&n).into_iter().flatten() {
            let part = text::holder_cell(holder);
            // An instance getty.target wants shows already.
            if let Holder::Unit { .. } = holder
                && !cell.split(", ").any(|c| c == part)
            {
                cell = join(&cell, &part);
            }
        }
        if self.masked.contains(&n) {
            cell = join(&cell, &text::masked_cell(n));
        }
        if let Some(record) = self.records.get(&n) {
            cell = join(&cell, text::recorded_cell(record));
        }
        cell
    }

    /// The "when switched to" cell of VT `n`'s row in `status`'s table:
    /// the unit logind starts there at a switch. logind starts a console
    /// by itself only on its VTs, and never on VT 1 ([`logind_starts`]).
    pub(super) fn switched_cell(&self, n: u16) -> String {
        match self.own(n) {
            Link::Other if logind_starts(n) => format!("autovt@tty{n}"),
            _ => self
                .switched_unit(n)
                .map(|u| format!("{u}@tty{n}"))
                .unwrap_or_default(),
        }
    }

    /// Where vtium is enabled on VT `n`, in either mode, read from
    /// `status`'s table as vtium-config reads it: the row's "at boot" or
    /// "when switched to" cell names `vtium@ttyN` or `vtium-seat@ttyN` as a
    /// part of its own. A mask's part, `vtium@ttyN masked`, and the
    /// development install's unit name neither. Gives the prefix of each
    /// unit of vtium's they name, none where vtium is not enabled.
    fn vtium_enabled(&self, n: u16) -> BTreeSet<&'static str> {
        ordered::set(
            [self.boot_cell(n), self.switched_cell(n)]
                .iter()
                .flat_map(|cell| vtium_parts(cell, n).collect::<Vec<_>>()),
        )
    }

    pub(super) fn template(&self) -> &Link {
        &self.template
    }

    pub(super) fn wanted(&self, n: u16, prefix: &str) -> bool {
        self.wants.get(&n).is_some_and(|w| w.contains_key(prefix))
    }

    pub(super) fn own(&self, n: u16) -> &Link {
        self.own.get(&n).unwrap_or(&Link::Absent)
    }

    /// Whether logind follows the template link on VT `n`: whether the VT
    /// has no link of its own.
    pub(super) fn templated(&self, n: u16) -> bool {
        *self.own(n) == Link::Absent
    }

    pub(super) fn runs(&self, n: u16, prefix: &str) -> bool {
        self.running.get(&n).is_some_and(|r| r.contains(prefix))
    }

    /// Whether `prefix`'s console is enabled, linked or running on VT `n`.
    pub(super) fn holds(&self, n: u16, prefix: &str) -> bool {
        self.wanted(n, prefix) || self.own(n).names(prefix) || self.runs(n, prefix)
    }

    /// Whether `enable N` in the mode asked for is done on VT `n`: its unit
    /// enabled there, and neither the getty nor the other mode's unit.
    pub(super) fn given(&self, n: u16) -> bool {
        self.wanted(n, self.prefix)
            && self.own(n).names(self.prefix)
            && !self.wanted(n, GETTY)
            && !self.wanted(n, other_vtium(self.prefix))
    }

    /// Whether VT `n`'s own link is a getty's that an earlier claim left
    /// behind: `systemctl enable getty@ttyN.service` links
    /// `autovt@ttyN.service` to the getty, from the getty's
    /// `Alias=autovt@.service`, and a claim that disabled the getty's want
    /// kept its record and left that link. `enable` claims such a VT.
    pub(super) fn stranded_getty(&self, n: u16) -> bool {
        self.own(n).names(GETTY)
            && !self.wanted(n, GETTY)
            && matches!(self.records.get(&n), Some(Record::Getty(_)))
    }

    /// Whether VT `n` has the other mode: its unit enabled there, or the
    /// VT's own link naming it.
    pub(super) fn other_mode(&self, n: u16) -> bool {
        let other = other_vtium(self.prefix);
        self.wanted(n, other) || self.own(n).names(other)
    }

    /// Whether anything of `enable N`'s, in either mode, is on VT `n`.
    pub(super) fn vtium_on(&self, n: u16) -> bool {
        VTIUMS
            .iter()
            .any(|p| self.wanted(n, p) || self.own(n).names(p))
            || self.records.contains_key(&n)
    }

    /// Whether vtium runs on VT `n`, in either mode.
    pub(super) fn vtium_runs(&self, n: u16) -> bool {
        VTIUMS.iter().any(|p| self.runs(n, p))
    }

    /// The units of vtium that run on VT `n`, such as `vtium@tty3.service`.
    pub(super) fn vtium_running(&self, n: u16) -> Vec<String> {
        VTIUMS
            .iter()
            .filter(|p| self.runs(n, p))
            .map(|p| format!("{p}@tty{n}.service"))
            .collect()
    }

    /// Every VT anything is enabled, linked, running or recorded on.
    pub(super) fn vts(&self) -> BTreeSet<u16> {
        let mut vts = ordered::set(self.wants.keys().copied());
        vts.extend(self.own.keys());
        vts.extend(self.running.keys());
        vts.extend(self.records.keys());
        vts.extend(&self.masked);
        vts
    }
}

/// Lists a directory's entry names, none when it cannot be read.
pub(super) fn names(dir: &Path) -> Vec<String> {
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
