//! One run of `vtium console`: the steps of each verb, `enable`,
//! `disable` and `status`, and what each step prints.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use crate::ordered;

use super::held::Holder;
use super::state::{State, names, names_vtium, running, systemctl_quiet};
use super::systemd::{Link, Record, UnitFile, drop_in_names, instance, template_prefix};
use super::{
    BOOTED, CONSOLES, DEV, DISPLAY_MANAGER, ETC, GETTY, Host, KMSCON, LOGIND_VTS, Mode, RECORDS,
    RESERVED_VT, Reply, SEAT, SEAT_PAM, SEAT0, TEMPLATE, UNIT_DIRS, VTIUM, VTIUMS, Verb, WANTS,
    Which, advice, graphical, held, other_vtium, process, text, vt_digits, which_word,
};

/// Who holds which VT, a VT once for each holder.
type OnVts = Vec<(u16, Holder)>;

/// What logind says of the sessions on the VTs.
#[derive(Default)]
struct Sessions {
    /// The graphical sessions and greeters.
    graphics: OnVts,
    /// The other sessions, such as a user's login at a getty.
    logins: OnVts,
    /// The VT of each session, by its scope's name.
    scopes: BTreeMap<String, u16>,
}

/// A step that could not be taken: what to print, on stderr.
pub(super) struct Stop(pub(super) String);

pub(super) type Step<T = ()> = Result<T, Stop>;

/// One run of the subcommand.
pub(super) struct Run<'h, 'a, 'o> {
    pub(super) host: &'h mut Host<'a>,
    pub(super) out: &'o mut dyn Write,
}

impl Run<'_, '_, '_> {
    fn path(&self, rel: &str) -> PathBuf {
        self.host.root.join(rel)
    }

    pub(super) fn say(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
    }

    pub(super) fn verb(&mut self, verb: Verb) -> Step<i32> {
        if !self.path(BOOTED).is_dir() {
            return Err(Stop(text::NO_SYSTEMD.to_owned()));
        }
        if let Verb::Advise(which) = verb {
            return self.advise(which);
        }
        // Seat mode's refusals come before the root check, so a user is
        // never told to use sudo for a command that cannot work.
        if let Verb::Enable(which, Mode::Seat) = verb
            && let Some(block) = advice::seat_block(&self.facts())
        {
            return Err(Stop(text::seat_cannot_run(block, &which_word(which))));
        }
        if verb != Verb::Status && !self.host.root_user {
            return Err(Stop(text::needs_root(&verb.words())));
        }
        if let Verb::Fallback(n) = verb {
            return self.fallback(n);
        }
        let prefix = match verb {
            Verb::Enable(_, mode) => mode.prefix(),
            _ => VTIUM,
        };
        let unit = self.unit_file(prefix);
        let mut state = self.read(prefix)?;
        let problem = match unit {
            UnitFile::At(_) => None,
            UnitFile::Masked => Some(text::template_masked(prefix)),
            UnitFile::Missing => Some(text::not_installed(prefix)),
        };
        // enable needs its mode's unit. disable and status work on whichever
        // mode each VT has, so either unit will do for them.
        let usable = match verb {
            Verb::Enable(..) => problem.is_none(),
            _ => VTIUMS
                .iter()
                .any(|p| matches!(self.unit_file(p), UnitFile::At(_))),
        };
        if verb == Verb::Status {
            // A line on a unit that is masked or missing, when vtium has
            // neither unit, or when a link still names it: a line on a mode
            // nothing uses means nothing to the person who reads it.
            for p in VTIUMS {
                let problem = match self.unit_file(p) {
                    UnitFile::At(_) => continue,
                    UnitFile::Masked => text::template_masked(p),
                    UnitFile::Missing => text::not_installed(p),
                };
                let named = state.template().names(p)
                    || state
                        .vts()
                        .into_iter()
                        .any(|n| state.wanted(n, p) || state.own(n).names(p));
                if named || (!usable && p == VTIUM) {
                    self.say(&problem);
                }
            }
            self.status(&state);
            return Ok(0);
        }
        if !usable {
            return Err(Stop(problem.unwrap_or_default()));
        }
        let unit = match unit {
            UnitFile::At(unit) => unit,
            UnitFile::Masked | UnitFile::Missing => String::new(),
        };
        match verb {
            Verb::Status | Verb::Advise(_) => Ok(0),
            Verb::Enable(Which::Vt(n), mode) => {
                if self.refuse_one(&state, n, mode)? {
                    self.say(&text::already_given(n, mode));
                    return Ok(0);
                }
                let moved = state.other_mode(n);
                if self.enable_vt(&mut state, n, &unit)? {
                    self.reload()?;
                }
                if moved {
                    self.say(&text::moved(n, mode));
                }
                self.say(&text::given(n, mode, state.runs(n, prefix)));
                Ok(0)
            }
            Verb::Enable(Which::All, mode) => self.enable_all(&mut state, &unit, mode),
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
                self.say(&text::taken_back(n, &state.vtium_running(n)));
                // `disable N` has undone everything `enable N` did, so it
                // succeeds, and it says what the template link still does
                // on the VT, which only `disable --all` changes.
                if names_vtium(state.template()) && !state.wanted(n, GETTY) {
                    self.say(&text::template_still_vtium(
                        n,
                        state.template().unit().unwrap_or(VTIUM),
                    ));
                }
                Ok(0)
            }
            Verb::Disable(Which::All) => self.disable_all(&mut state),
            Verb::Fallback(_) => Ok(0),
        }
    }

    /// What the advice and seat mode's refusals judge, read from the host.
    fn facts(&self) -> advice::Facts {
        advice::Facts {
            seat0: self.path(SEAT0).exists(),
            seat_build: self.host.seat_build,
            seat_unit: match self.unit_file(SEAT) {
                UnitFile::At(_) => advice::SeatUnit::Present,
                UnitFile::Masked => advice::SeatUnit::Masked,
                UnitFile::Missing => advice::SeatUnit::Missing,
            },
            pam: self.path(SEAT_PAM).is_file(),
            display_manager: self.display_manager(),
        }
    }

    /// `enable` with no mode: says what vtium finds on this machine, which
    /// command it recommends, and why `enable` would refuse the VTs named
    /// in a mode, and changes nothing. It offers no command that `enable`
    /// would refuse.
    fn advise(&mut self, which: Option<Which>) -> Step<i32> {
        let facts = self.facts();
        let which = which.unwrap_or(Which::All);
        let refusals = advice::Refusals {
            direct: self.refusal(which, Mode::Direct)?,
            seat: match advice::seat_block(&facts) {
                None => self.refusal(which, Mode::Seat)?,
                Some(_) => None,
            },
        };
        let sudo = !self.host.root_user;
        for line in text::advice(&facts, which, &refusals, sudo) {
            self.say(&line);
        }
        Ok(0)
    }

    /// Why `enable` in `mode` would refuse `which` now, in its own words,
    /// or None when it would go ahead. It reads what `enable` reads, makes
    /// the checks `enable` makes before it changes anything, and changes
    /// nothing.
    fn refusal(&mut self, which: Which, mode: Mode) -> Step<Option<String>> {
        let state = self.read(mode.prefix())?;
        let refused = match self.unit_file(mode.prefix()) {
            UnitFile::At(_) => match which {
                Which::Vt(n) => self.refuse_one(&state, n, mode).map(drop),
                Which::All => self.refuse_all(&state, mode),
            },
            UnitFile::Masked => Err(Stop(text::template_masked(mode.prefix()))),
            UnitFile::Missing => Err(Stop(text::not_installed(mode.prefix()))),
        };
        Ok(refused.err().map(|Stop(why)| why))
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

    fn read(&mut self, prefix: &'static str) -> Step<State> {
        let mut state = State::links(&self.host.root, prefix);
        state.display_manager = self.display_manager();
        let mut scopes = BTreeMap::new();
        match self.sessions() {
            Ok(sessions) => {
                for (n, holder) in sessions.graphics {
                    state.held.entry(n).or_default().push(holder);
                }
                for (n, holder) in sessions.logins {
                    state.logins.entry(n).or_default().push(holder);
                }
                scopes = sessions.scopes;
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
        self.judge_processes(&mut state, &scopes);
        self.judge_boot_units(&mut state);
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
                    let fits = template_prefix(path).is_some_and(|p| !VTIUMS.contains(&p))
                        && !path.contains('\n');
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
        let mut units = ordered::set(display_manager.map(str::to_owned));
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
    /// greeters, the others, and the VT of each session's scope
    /// ([`graphical::scopes`]).
    fn sessions(&mut self) -> Step<Sessions> {
        let list = self.loginctl(&["list-sessions", "--no-legend"])?;
        let ids = graphical::session_ids(&list.stdout);
        if ids.is_empty() {
            return Ok(Sessions::default());
        }
        let mut args = vec!["show-session"];
        args.extend(ids.iter().map(String::as_str));
        args.extend([
            "-p", "Id", "-p", "Name", "-p", "VTNr", "-p", "Service", "-p", "Type", "-p", "Class",
        ]);
        let show = self.loginctl(&args)?;
        Ok(Sessions {
            graphics: graphical::graphical(&show.stdout),
            logins: graphical::logins(&show.stdout),
            scopes: graphical::scopes(&show.stdout),
        })
    }

    /// Adds to `state` each program whose controlling terminal is a VT now
    /// ([`process`]) and that nothing else on that VT accounts for: a
    /// process in a unit that is an instance on the same VT, such as
    /// `getty@ttyN.service`, which the rules for consoles judge, or in the
    /// scope of a logind session on the same VT, which the sessions judge,
    /// holds nothing more. Any other such program holds its VT, whether or
    /// not it has a session: a display manager's X server, a debug shell,
    /// or an X server started from a text login on another VT.
    fn judge_processes(&self, state: &mut State, scopes: &BTreeMap<String, u16>) {
        let processes = process::on_vts(&self.host.root);
        for p in process::one_each(processes) {
            let accounted = p.unit.as_deref().is_some_and(|unit| {
                instance(unit).is_some_and(|(_, n)| n == p.vt) || scopes.get(unit) == Some(&p.vt)
            });
            if !accounted {
                state.held.entry(p.vt).or_default().push(Holder::Process {
                    pid: p.pid,
                    program: p.program,
                    unit: p.unit,
                });
            }
        }
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

    /// The consoles running on each VT ([`running`]).
    fn running(&mut self) -> Step<BTreeMap<u16, BTreeSet<String>>> {
        running(self.host.systemd)
    }

    /// Runs systemctl and fails the step when it fails, printing nothing.
    pub(super) fn systemctl_quiet(&mut self, args: &[&str]) -> Step<Reply> {
        systemctl_quiet(self.host.systemd, args)
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

    /// Refuses VT `n` while kmscon's console holds it.
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

    /// Refuses VT `n` for `enable N` in `mode` before it changes anything,
    /// or says whether vtium is enabled there in `mode` already.
    fn refuse_one(&self, state: &State, n: u16, mode: Mode) -> Step<bool> {
        self.refuse_kmscon(state, n)?;
        self.refuse_held(state, n)?;
        self.refuse_vt(state, n)?;
        if state.given(n) {
            return Ok(true);
        }
        // logind starts autovt@tty6 on every switch to its reserved VT, so
        // vtium enabled there would end a login there at the next switch,
        // unless vtium runs there already and the login is inside it.
        if n == RESERVED_VT
            && !state.vtium_runs(n)
            && let Some(logins) = state.logins.get(&n)
        {
            return Err(Stop(text::login_on_reserved_vt(n, logins, false)));
        }
        // vtium in the other mode there would be stopped at the next
        // switch. Seat mode's logins carry no VT number, so the logins on
        // the VT cannot show what that would end.
        if n == RESERVED_VT && state.runs(n, other_vtium(state.prefix)) {
            return Err(Stop(text::other_mode_on_reserved_vt(n, mode)));
        }
        Ok(false)
    }

    /// Refuses VT `n` for `enable N` when the development install holds
    /// it, vtium's unit is masked there, or its own link names another
    /// unit.
    fn refuse_vt(&self, state: &State, n: u16) -> Step {
        if state.holds(n, DEV) {
            return Err(Stop(text::dev_holds(n)));
        }
        if state.masked.contains(&n) {
            return Err(Stop(text::masked(n, state.prefix)));
        }
        match state.own(n) {
            Link::Absent => Ok(()),
            link if names_vtium(link) => Ok(()),
            _ if state.stranded_getty(n) => Ok(()),
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

    /// `enable N`'s changes on VT `n`, once its refusals have passed, for
    /// the mode whose unit file is at `unit`: the getty's record; the VT's
    /// own link moved from the other mode's unit to `unit`, where it names
    /// the other mode; the mode's enable; the other mode's disable; then
    /// the getty's disable, each skipped when done. So a run cut short
    /// always leaves a console enabled on VT `n`. Returns whether a link
    /// was changed by hand, which the manager has yet to read.
    fn enable_vt(&mut self, state: &mut State, n: u16, unit: &str) -> Step<bool> {
        let (prefix, other) = (state.prefix, other_vtium(state.prefix));
        let getty = state.wanted(n, GETTY);
        self.record_vt(state, n)?;
        let mut by_hand = false;
        // systemctl enable refuses an alias that links elsewhere, so the
        // VT's own link moves to the mode's unit first, from the other
        // mode's unit or from a getty an earlier claim left it naming.
        if state.own(n).names(other) || state.stranded_getty(n) {
            self.link(ETC, &format!("autovt@tty{n}.service"), Some(unit))?;
            state.own.insert(n, Link::To(unit.to_owned()));
            by_hand = true;
        }
        if !(state.wanted(n, prefix) && state.own(n).names(prefix)) {
            self.systemctl(&["enable", &format!("{prefix}@tty{n}.service")])?;
            state
                .wants
                .entry(n)
                .or_default()
                .insert(prefix.to_owned(), Link::To(unit.to_owned()));
            // The enable links the VT's own link too, from the unit's
            // Alias=autovt@.service.
            state.own.insert(n, Link::To(unit.to_owned()));
        }
        if state.wanted(n, other) {
            self.systemctl(&["disable", &format!("{other}@tty{n}.service")])?;
            if let Some(w) = state.wants.get_mut(&n) {
                w.remove(other);
            }
        }
        if getty {
            self.disable_getty(state, n)?;
        }
        Ok(by_hand)
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
        for prefix in VTIUMS {
            if state.wanted(n, prefix) {
                self.systemctl(&["disable", &format!("{prefix}@tty{n}.service")])?;
            }
        }
        if names_vtium(state.own(n)) {
            self.link(ETC, &format!("autovt@tty{n}.service"), None)?;
            by_hand = true;
        }
        if record.is_some() {
            self.forget(&format!("tty{n}"))?;
        }
        Ok(by_hand)
    }

    /// Refuses `enable --all` in `mode` before it changes anything: while
    /// kmscon holds the template link or someone wrote it as a file, while
    /// vtium cannot tell which sessions run, while a program on logind's
    /// reserved VT would meet vtium there at the next switch, while a mask
    /// does not keep vtium from a VT the template link reaches, and where
    /// `enable 1` would refuse VT 1.
    fn refuse_all(&self, state: &State, mode: Mode) -> Step {
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
        // vtium in the other mode on VT 6 would be stopped at the next
        // switch, when logind starts the asked mode's unit there through
        // the template link or through VT 6's own link once it moves.
        if state.runs(RESERVED_VT, other_vtium(state.prefix))
            && (state.templated(RESERVED_VT) || state.other_mode(RESERVED_VT))
        {
            return Err(Stop(text::other_mode_on_reserved_vt(RESERVED_VT, mode)));
        }
        if state.templated(RESERVED_VT) {
            // A login inside a vtium that runs on VT 6 already is safe: the
            // switch's start of vtium@tty6 finds it active.
            if let Some(logins) = state
                .logins
                .get(&RESERVED_VT)
                .filter(|_| !state.vtium_runs(RESERVED_VT))
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
        // A mask keeps vtium from a VT only where logind does not load
        // vtium through the template link: on VT 1, where logind starts no
        // console, above its VTs, and where an enabled getty holds the VT
        // from boot. On VT 6, logind's reserved VT, it loads vtium at every
        // switch. A VT that another program, kmscon or the development
        // install holds keeps it whatever its mask.
        let mut vts = state.vts();
        vts.extend(2..=LOGIND_VTS);
        for n in vts {
            let kept = state.held.contains_key(&n) || state.holds(n, KMSCON) || state.holds(n, DEV);
            let through_template = state.templated(n)
                && (n == RESERVED_VT || ((2..=LOGIND_VTS).contains(&n) && !state.wanted(n, GETTY)));
            if !kept && state.masked.contains(&n) && through_template {
                return Err(Stop(text::masked_under_template(n, n == RESERVED_VT)));
            }
        }
        // VT 1 gets `enable N` unless the development install, kmscon or
        // graphics hold it or vtium is masked there, so it is refused as
        // `enable N` would be.
        if !vt1_kept(state) {
            self.refuse_vt(state, 1)?;
        }
        Ok(())
    }

    fn enable_all(&mut self, state: &mut State, unit: &str, mode: Mode) -> Step<i32> {
        self.refuse_all(state, mode)?;
        // What each VT keeps, the VTs from 2 to 6 that vtium starts on at
        // boot, and the VTs above 6 that move from the other mode.
        let mut kept: Vec<String> = Vec::new();
        let mut boot: Vec<u16> = Vec::new();
        let mut moves: Vec<u16> = Vec::new();
        for (n, holders) in &state.held {
            kept.push(text::kept_graphics(*n, holders));
        }
        let mut vts = state.vts();
        vts.extend(2..=LOGIND_VTS);
        for n in vts {
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
            if state.masked.contains(&n) {
                // refuse_all refused a mask the template link reaches.
                // The mask does not reach autovt@ttyN.service, which the
                // VT's own link still loads as vtium (measured).
                kept.push(if names_vtium(state.own(n)) {
                    text::kept_masked_own_link(n)
                } else {
                    text::kept_masked(n, state.wanted(n, GETTY))
                });
                continue;
            }
            if n == 1 {
                continue;
            }
            // A VT whose own link names another unit keeps that unit, its
            // getty included: `enable N` refuses it. A getty's link that an
            // earlier claim left behind is claimed like the rest.
            match state.own(n) {
                link @ Link::To(_) if !names_vtium(link) && !state.stranded_getty(n) => {
                    kept.push(text::kept_own_link(n, link.unit().unwrap_or("?")));
                    continue;
                }
                Link::Other => {
                    kept.push(text::kept_own_link(n, "autovt"));
                    continue;
                }
                _ => {}
            }
            // vtium starts at boot on each VT it claims from 2 to 6, so it
            // holds each one open from boot and a display manager that
            // picks the first free VT passes it by.
            if n <= LOGIND_VTS {
                if !state.given(n) {
                    boot.push(n);
                }
            } else if state.other_mode(n) {
                moves.push(n);
            } else if state.wanted(n, GETTY) {
                kept.push(text::kept_high_getty(n));
            }
        }
        let vt1_kept = vt1_kept(state);
        let template_done = state.template_etc.names(state.prefix);
        let vt1_done = vt1_kept || state.given(1);
        if template_done && vt1_done && boot.is_empty() && moves.is_empty() {
            // A run whose last step, the reload, failed finds its links
            // done, so a run that changes nothing reloads.
            self.reload()?;
            self.say(&text::already_all(mode));
            for line in &kept {
                self.say(line);
            }
            return Ok(0);
        }
        // The VTs from 1 to 6 that move from the other mode, named once
        // their move is done.
        let mut moved: Vec<u16> = Vec::new();
        if !vt1_kept && state.other_mode(1) {
            moved.push(1);
        }
        moved.extend(boot.iter().copied().filter(|&n| state.other_mode(n)));
        if !template_done {
            // The record keeps what the template named before vtium took
            // it, never one of vtium's own units: a move between modes
            // keeps the record the first enable wrote.
            if state.template_record.is_none() && !names_vtium(&state.template_etc) {
                let body = match &state.template_etc {
                    Link::To(to) => format!("{to}\n"),
                    _ => String::new(),
                };
                self.record(TEMPLATE, &body, text::TEMPLATE_WHAT)?;
            }
            self.link(ETC, TEMPLATE, Some(unit))?;
        }
        if !vt1_done {
            self.enable_vt(state, 1, unit)?;
        }
        for &n in boot.iter().chain(&moves) {
            self.enable_vt(state, n, unit)?;
        }
        self.reload()?;
        moved.extend(&moves);
        for &n in &moved {
            self.say(&text::moved(n, mode));
        }
        let at_boot: Vec<u16> = (1..=LOGIND_VTS).filter(|&n| state.given(n)).collect();
        // The VTs from 2 to 5 where logind starts vtium through the
        // template link: no link of their own, nothing at boot, and no
        // other program on them.
        let switched: Vec<u16> = (2..RESERVED_VT)
            .filter(|&n| {
                state.templated(n)
                    && state.wants.get(&n).is_none_or(BTreeMap::is_empty)
                    && !state.held.contains_key(&n)
            })
            .collect();
        let vt6 = boot.contains(&RESERVED_VT) || state.templated(RESERVED_VT);
        self.say(&text::given_all(mode, &at_boot, &switched, vt6));
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
        let template = names_vtium(&state.template_etc);
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
            if state.vtium_runs(n) {
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
        let mut still = ordered::set(running);
        still.extend(state.vts().into_iter().filter(|&n| state.vtium_runs(n)));
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
        let mut vts = ordered::set(1..=LOGIND_VTS);
        vts.extend(state.vts());
        vts.extend(state.held.keys());
        let rows: Vec<[String; 4]> = vts
            .into_iter()
            .map(|n| {
                let mut running_graphics = String::new();
                for holder in state.held.get(&n).into_iter().flatten() {
                    // A unit that holds the VT shows at boot, and a console
                    // running there in the running column already.
                    if let Holder::Session { .. } | Holder::Process { .. } = holder {
                        running_graphics = join(&running_graphics, &text::holder_cell(holder));
                    }
                }
                let running = join(
                    &state
                        .running
                        .get(&n)
                        .map(|r| units(r.iter(), n))
                        .unwrap_or_default(),
                    &running_graphics,
                );
                [
                    n.to_string(),
                    state.boot_cell(n),
                    state.switched_cell(n),
                    running,
                ]
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
        // The VTs from 2 to 5 that nothing holds open at boot, whatever
        // their own links name, as on a machine claimed before vtium
        // started at boot on each claimed VT: a display manager that picks
        // the first free VT can take them. logind holds VT 6 open itself,
        // so no display manager takes it.
        let mode = match state.template().unit() {
            Some(VTIUM) => Some(Mode::Direct),
            Some(SEAT) => Some(Mode::Seat),
            _ => None,
        };
        if let Some(mode) = mode {
            let free: Vec<u16> = (2..RESERVED_VT.min(LOGIND_VTS + 1))
                .filter(|&n| {
                    state.wants.get(&n).is_none_or(BTreeMap::is_empty)
                        && !state.held.contains_key(&n)
                })
                .collect();
            if !free.is_empty() {
                self.say(&text::not_held_at_boot(&free, mode));
            }
        }
        // Why each VT a unit that starts at boot, or a program with no
        // session, holds is held.
        for (n, holders) in &state.held {
            let units: Vec<Holder> = holders
                .iter()
                .filter(|h| matches!(h, Holder::Unit { .. } | Holder::Process { .. }))
                .cloned()
                .collect();
            if !units.is_empty() {
                self.say(&text::status_held(*n, &units));
            }
        }
    }
}

/// Whether VT 1 keeps what it has at `enable --all`: the development
/// install, kmscon or graphics hold it, or vtium is masked there.
fn vt1_kept(state: &State) -> bool {
    state.holds(1, DEV)
        || state.holds(1, KMSCON)
        || state.held.contains_key(&1)
        || state.masked.contains(&1)
}

pub(super) fn units<'p>(prefixes: impl Iterator<Item = &'p String>, n: u16) -> String {
    prefixes
        .map(|p| format!("{p}@tty{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a}, {b}"),
    }
}
