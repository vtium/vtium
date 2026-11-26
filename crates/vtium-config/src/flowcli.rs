//! The flow's commands (ADR-1403 D8): `flow`, `diff`, `install`,
//! `set`, `get`, `revert`, `new`, `rule`, `head`, `look`, `draft`,
//! `history`, `restore`, `library` and `edit`. Each that changes the
//! configuration reads the draft and the devices afresh, makes its change
//! through [`crate::ops`], writes the draft whatever the check says, and
//! prints what `vtium check-config` finds in it; `--dry-run` prints the
//! change as a difference and the check's verdict and writes nothing, and
//! `--install` then installs the draft. Every command asks nothing but
//! sudo's password, and that only when it installs, apart from `install`
//! at a terminal, which asks before it installs a draft with no complete
//! route and before it starts and restarts vtium after the install
//! ([`crate::vts`]). The exit codes are [`Fail`]'s.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use vtium_platform::listing::Roots;
use vtium_settings::Of;

use crate::apply::Ask;
use crate::drafts::{Checked, From, Session, State, Unrouted, Verdict, diff, run_child};
use crate::flowmodel::{self, Present};
use crate::ops::{self, RuleChange, To};
use crate::outcome::Fail;
use crate::store::{Store, valid_name};
use crate::tui::Paths;

/// Where the devices present are read: sysfs and udev's database, and
/// udev's control socket.
#[derive(Clone, Debug)]
pub struct Devices {
    pub roots: Roots,
    pub control: PathBuf,
}

/// A command's words split into its arguments and its options, each
/// option of `valued` taking the word after it.
struct Args {
    words: Vec<String>,
    options: BTreeMap<String, Option<String>>,
}

impl Args {
    fn parse(words: &[String], switches: &[&str], valued: &[&str]) -> Result<Args, Fail> {
        let mut out = Args {
            words: Vec::new(),
            options: BTreeMap::new(),
        };
        let mut it = words.iter();
        while let Some(w) = it.next() {
            if valued.contains(&w.as_str()) {
                let v = it
                    .next()
                    .ok_or_else(|| Fail::Usage(format!("{w} needs a value")))?;
                out.options.insert(w.clone(), Some(v.clone()));
            } else if switches.contains(&w.as_str()) {
                out.options.insert(w.clone(), None);
            } else if w.starts_with("--") {
                return Err(Fail::Usage(format!("unknown option {w}")));
            } else {
                out.words.push(w.clone());
            }
        }
        Ok(out)
    }

    fn has(&self, name: &str) -> bool {
        self.options.contains_key(name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.options.get(name).and_then(|v| v.as_deref())
    }
}

/// A rule's number as a command takes it.
fn number(word: &str) -> Result<usize, Fail> {
    word.parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            Fail::Usage(format!(
                "{word:?} is not a rule number; rules are numbered from 1"
            ))
        })
}

/// The options every command that changes the configuration takes.
const CHANGE: [&str; 2] = ["--dry-run", "--install"];

/// Runs the flow command `command` with its words, the devices read from
/// `devices`; returns the exit status.
pub fn run(
    command: &str,
    words: &[String],
    paths: &Paths,
    devices: &Devices,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let store = match Store::system() {
        Ok(store) => store,
        Err(fail) => return report(&fail, err),
    };
    let session = Session { store, paths };
    let cmd = Command {
        session: &session,
        devices,
        out,
    };
    match cmd.dispatch(command, words) {
        Ok(()) => 0,
        Err(fail) => report(&fail, err),
    }
}

/// A change to one font set that `new-frit` and `remove-frit` keep in the
/// current draft's font set folder ([`font_set`]).
pub enum FontSetChange {
    /// The draft adds or replaces the frit, whose whole text this is.
    Keep(String),
    /// The draft removes the installed frit, or no longer adds the frit.
    Remove,
}

/// Runs `new-frit` or `remove-frit` once the command has checked its
/// words: keeps `change` to the frit `frit` in the current draft's font
/// set folder; returns the exit status.
pub fn font_set(
    frit: &str,
    change: FontSetChange,
    paths: &Paths,
    devices: &Devices,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let store = match Store::system() {
        Ok(store) => store,
        Err(fail) => return report(&fail, err),
    };
    let session = Session { store, paths };
    let mut cmd = Command {
        session: &session,
        devices,
        out,
    };
    match cmd.font_set(frit, change) {
        Ok(()) => 0,
        Err(fail) => report(&fail, err),
    }
}

/// Prints `fail` and gives its exit code.
fn report(fail: &Fail, err: &mut dyn Write) -> u8 {
    let _ = writeln!(err, "vtium-config: {}", fail.text());
    if fail.code() == 2 {
        let _ = writeln!(err, "Try vtium-config --help.");
    }
    fail.code()
}

struct Command<'a, 'b> {
    session: &'a Session<'a>,
    devices: &'a Devices,
    out: &'b mut dyn Write,
}

impl Command<'_, '_> {
    fn say(&mut self, line: &str) {
        let _ = writeln!(self.out, "{line}");
    }

    fn dispatch(mut self, command: &str, words: &[String]) -> Result<(), Fail> {
        match command {
            "flow" => self.flow(words),
            "diff" => self.diff(words),
            "install" => self.install(words),
            "get" => self.get(words),
            "set" => self.set(words),
            "revert" | "new" => self.reset(command, words),
            "rule" => self.rule(words),
            "head" => self.head(words),
            "look" => self.look(words),
            "draft" => self.draft(words),
            "history" => self.history(words),
            "restore" => self.restore(words),
            "library" => self.library(words),
            "edit" => self.edit(words),
            other => Err(Fail::Usage(format!("unknown command {other}"))),
        }
    }

    fn present(&self) -> Present {
        flowmodel::present(&self.devices.roots, &self.devices.control)
    }

    /// The draft's flow and its check: the flow, the check, and what a
    /// complete route still lacks.
    fn judged(
        &self,
        name: &str,
        text: &str,
    ) -> Result<(crate::tui::flow::Flow, State, Checked), Fail> {
        let state = self.session.state(name, text)?;
        let checked = self
            .session
            .check(name, text, &self.session.store.draft_path(name)?)?;
        let (added, removed) = state.counts();
        let draft = crate::tui::flow::Draft {
            name: name.to_owned(),
            changes: added.max(removed),
            problems: checked.count(),
        };
        Ok((
            flowmodel::build(text, &self.present(), Some(draft)),
            state,
            checked,
        ))
    }

    /// `flow [--json]`.
    fn flow(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--json"], &[])?;
        if !args.words.is_empty() {
            return Err(Fail::Usage("flow takes only --json".to_owned()));
        }
        let (name, text) = self.session.current()?;
        let (flow, state, checked) = self.judged(&name, &text)?;
        let missing = missing(&checked);
        let printed = if args.has("--json") {
            let (frits, _) = self.session.frit_changes(&name)?;
            flowmodel::json(&flow, &name, &state, &checked, &missing, &frits)
        } else {
            flowmodel::text(
                &flow,
                &flowmodel::state_line(&name, &state, self.beside(&name)?, checked.count()),
                &missing,
            )
        };
        let _ = write!(self.out, "{printed}");
        Ok(())
    }

    /// `diff`: the installed file to the draft, then the installed batch
    /// file to the one the draft's install writes, when the draft changes
    /// the active resources.
    ///
    /// References:
    /// - ADR-1403 D8: `diff` shows the changes that wait beside the draft too.
    fn diff(&mut self, words: &[String]) -> Result<(), Fail> {
        if !words.is_empty() {
            return Err(Fail::Usage("diff takes no argument".to_owned()));
        }
        let (name, text) = self.session.current()?;
        let installed = self.session.installed()?.unwrap_or_default();
        let shown = diff(
            &self.session.installed_path().to_string_lossy(),
            &installed,
            &self.session.store.draft_path(&name)?.to_string_lossy(),
            &text,
        );
        let _ = write!(self.out, "{shown}");
        if let Some(edit) = self.session.batch_edit(&name)? {
            if let crate::batch::Installed::Refused(why) = self.session.installed_batch() {
                let _ = writeln!(self.out, "{why}");
            }
            let path = edit.path.to_string_lossy();
            let shown = diff(
                &path,
                edit.old.as_deref().unwrap_or_default(),
                &format!("{path} (after install)"),
                &edit.new,
            );
            let _ = write!(self.out, "{shown}");
        }
        // each font set change, under the frit's path in the frits'
        // directory: a new frit as added lines, a replaced one as its
        // difference from the installed frit, a removed one as removed
        // lines
        let (frits, notes) = self.session.frit_changes(&name)?;
        for change in frits {
            let path = change.path.to_string_lossy();
            let old = change
                .old
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let shown = diff(
                &path,
                &old,
                &format!("{path} (after install)"),
                change.new.as_deref().unwrap_or_default(),
            );
            let _ = write!(self.out, "{shown}");
        }
        for note in notes {
            let _ = writeln!(self.out, "{note}");
        }
        Ok(())
    }

    /// What waits beside the draft `name`, as the state line counts it:
    /// its changes to the active resources and its font set changes.
    fn beside(&self, name: &str) -> Result<(usize, usize), Fail> {
        Ok((
            self.session.store.batch_changes(name)?.len(),
            self.session.frit_changes(name)?.0.len(),
        ))
    }

    /// `install [--force]`: installs the draft. A draft whose only problem
    /// is that it wires no complete route installs with `--force`, or
    /// after a yes at a terminal; with neither it is refused, exit 1
    /// (ADR-1403 D8).
    fn install(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--force"], &[])?;
        if !args.words.is_empty() {
            return Err(Fail::Usage("install takes only --force".to_owned()));
        }
        let (name, text) = self.session.current()?;
        let terminal = std::io::IsTerminal::is_terminal(&std::io::stdin());
        let mut no_route = crate::vts::ask_line;
        let mut restart = crate::vts::ask_line;
        let unrouted = if args.has("--force") {
            Unrouted::Install
        } else if terminal {
            Unrouted::Ask(&mut no_route)
        } else {
            Unrouted::Refuse
        };
        let ask: Option<Ask<'_>> = if terminal { Some(&mut restart) } else { None };
        self.unused_looks(&text);
        self.session
            .install(&name, &text, unrouted, &mut run_child, ask, self.out)
    }

    /// Prints a note for each look in the draft `text` that no head uses.
    /// Such a look changes nothing on screen, which a person who just made
    /// it or set it would not otherwise learn; the note gives the command
    /// that gives it to the first head.
    fn unused_looks(&mut self, text: &str) {
        let flow = flowmodel::build(text, &flowmodel::Present::default(), None);
        let head = flow.heads.first().map(|h| h.name.clone());
        for (i, look) in flow.looks.iter().enumerate() {
            if !flow.used(i) {
                self.say(&crate::tui::words::no_head_uses(
                    &look.name,
                    head.as_deref(),
                ));
            }
        }
    }

    /// `get KEY`: the key, its value in the draft's top level and where it
    /// comes from, the draft or the default.
    fn get(&mut self, words: &[String]) -> Result<(), Fail> {
        let [key] = words else {
            return Err(Fail::Usage("get takes one key".to_owned()));
        };
        let key = crate::commands::conf_key(key).map_err(Fail::Usage)?;
        let (_, text) = self.session.current()?;
        let (value, set) = crate::commands::value_text(key, Some(&text));
        let from = if set { "draft" } else { "default" };
        self.say(&format!("{}\t{value}\t{from}", key.name));
        Ok(())
    }

    /// `set KEY VALUE` and `set --unset KEY`.
    fn set(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &[&CHANGE[..], &["--unset"]].concat(), &[])?;
        match (args.has("--unset"), args.words.as_slice()) {
            (true, [key]) => self.change(&args, |t| ops::unset_top(t, key)),
            (false, [key, value]) => self.change(&args, |t| ops::set_top(t, key, value)),
            _ => Err(Fail::Usage(
                "set takes KEY VALUE, or --unset KEY".to_owned(),
            )),
        }
    }

    /// Makes `op`'s change to the current draft, as every changing command
    /// does: with `--dry-run` it prints the difference and the check's
    /// verdict and writes nothing; otherwise it writes the draft whatever
    /// the check finds, prints what the check finds, and with `--install`
    /// installs it.
    fn change(
        &mut self,
        args: &Args,
        op: impl FnOnce(&str) -> Result<String, Fail>,
    ) -> Result<(), Fail> {
        let (name, text) = self.session.current()?;
        let new = op(&text)?;
        self.write(
            &name,
            &text,
            &new,
            args.has("--dry-run"),
            args.has("--install"),
        )
    }

    /// Writes `new` over the draft `name`, which was `old`.
    fn write(
        &mut self,
        name: &str,
        old: &str,
        new: &str,
        dry_run: bool,
        install: bool,
    ) -> Result<(), Fail> {
        let path = self.session.store.draft_path(name)?;
        if dry_run {
            let shown = path.to_string_lossy();
            let _ = write!(
                self.out,
                "{}",
                diff(&shown, old, &format!("{shown} (after)"), new)
            );
            let checked = self.session.check(name, new, &path)?;
            self.print_check(&checked);
            self.say(if checked.verdict == Verdict::Passed {
                "the check passes; nothing was saved (--dry-run)"
            } else {
                "the check finds problems; nothing was saved (--dry-run)"
            });
            return Ok(());
        }
        if new == old {
            self.say(&format!("draft {name} unchanged"));
        } else {
            self.session.store.write_draft(name, new)?;
            self.say(&format!("wrote {}", path.display()));
        }
        self.after_write(name, new, install)
    }

    /// After a change to the draft `name`, whose text is now `new`, was
    /// written: prints what the check finds, the state line and a note for
    /// each look that no head uses, and with `install` installs the draft. A check that cannot run is printed
    /// and fails nothing, since the change is written whatever the check
    /// says, unless the draft is to be installed.
    fn after_write(&mut self, name: &str, new: &str, install: bool) -> Result<(), Fail> {
        let path = self.session.store.draft_path(name)?;
        let state = self.session.state(name, new)?;
        let checked = match self.session.check(name, new, &path) {
            Ok(checked) => checked,
            // the draft is written whatever the check says, even when it
            // cannot run
            Err(fail) if !install => {
                self.say(fail.text());
                return Ok(());
            }
            Err(fail) => return Err(fail),
        };
        self.print_check(&checked);
        let beside = self.beside(name)?;
        self.say(&flowmodel::state_line(
            name,
            &state,
            beside,
            checked.count(),
        ));
        self.unused_looks(new);
        if install {
            self.session
                .install(name, new, Unrouted::Refuse, &mut run_child, None, self.out)?;
        }
        Ok(())
    }

    /// `new-frit` and `remove-frit`: keeps `change` to the frit `frit` in
    /// the current draft's font set folder, then prints what the check of
    /// the draft finds and the state line. A removal of an installed frit
    /// writes `FRIT.removed`, and a removal of a frit that only the draft
    /// adds takes the addition back.
    /// A removal of a frit that is neither installed nor in the folder is
    /// refused, exit 1.
    ///
    /// References:
    /// - ADR-1403 D8, "Every font set change is kept at once": `new-frit` and `remove-frit` write the current draft's folder and install nothing.
    fn font_set(&mut self, frit: &str, change: FontSetChange) -> Result<(), Fail> {
        let (name, text) = self.session.current()?;
        let store = &self.session.store;
        let dir = store.frits_path(&name)?;
        let file = |ending: &str| dir.join(format!("{frit}{ending}")).display().to_string();
        match change {
            FontSetChange::Keep(frit_text) => {
                store.keep_frit(&name, frit, &frit_text)?;
                self.say(&format!("wrote {}", file(".vtfrit")));
            }
            FontSetChange::Remove => {
                let frits = self
                    .session
                    .paths
                    .frits
                    .as_ref()
                    .map_err(|e| Fail::File(e.clone()))?;
                if frits.join(format!("{frit}.vtfrit")).is_file() {
                    store.keep_removal(&name, frit)?;
                    self.say(&format!("wrote {}", file(".removed")));
                } else if store.frit_changes(&name)?.kept.contains_key(frit) {
                    store.forget_frit(&name, frit)?;
                    self.say(&format!("removed {}", file(".vtfrit")));
                } else {
                    return Err(Fail::Check(format!(
                        "There is no font set named {frit} to remove: no installed font set has that name, and draft {name} does not add one."
                    )));
                }
            }
        }
        self.after_write(&name, &text, false)
    }

    fn print_check(&mut self, checked: &Checked) {
        for line in checked.problems.iter().chain(&checked.notes) {
            self.say(line);
        }
    }

    /// `revert` and `new`: the draft back to the installed file, or emptied
    /// to the top level at its defaults and one empty look.
    fn reset(&mut self, command: &str, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--dry-run"], &[])?;
        if !args.words.is_empty() {
            return Err(Fail::Usage(format!("{command} takes only --dry-run")));
        }
        let (name, text) = self.session.current()?;
        let installed = self.session.installed()?;
        let new = match command {
            "revert" => installed
                .clone()
                .unwrap_or_else(|| crate::drafts::HEADER.to_owned()),
            _ => ops::new_text(crate::drafts::HEADER),
        };
        if args.has("--dry-run") {
            let (added, removed) = crate::drafts::counts(&new, &text);
            let (batch, frits) = self.beside(&name)?;
            let beside = [
                (batch > 0).then(|| crate::batch::resource_changes(batch)),
                (frits > 0).then(|| crate::drafts::frit_count(frits)),
            ];
            let batch: String = beside
                .into_iter()
                .flatten()
                .map(|w| format!(", and {w}"))
                .collect();
            self.say(&format!(
                "{command} would lose {} of draft {name}{batch}",
                crate::drafts::lines(added, removed)
            ));
            return Ok(());
        }
        if command == "revert" {
            self.session
                .store
                .set_base(&name, installed.as_deref().map(str::as_bytes))?;
        }
        // the draft starts over, so its changes to the active resources
        // and its font set changes go
        self.session.drop_batch(&name)?;
        self.session.store.drop_frits(&name)?;
        self.write(&name, &text, &new, false, false)
    }

    /// `rule add`, `rule set`, `rule move` and `rule rm`.
    fn rule(&mut self, words: &[String]) -> Result<(), Fail> {
        let (sub, rest) = words
            .split_first()
            .ok_or_else(|| Fail::Usage("rule takes add, set, move or rm".to_owned()))?;
        match sub.as_str() {
            "add" => {
                let args = Args::parse(rest, &CHANGE, &["--before"])?;
                let [rule] = args.words.as_slice() else {
                    return Err(Fail::Usage(
                        "rule add takes one rule, such as 'input class:keyboard -> roaming'"
                            .to_owned(),
                    ));
                };
                let before = args.value("--before").map(number).transpose()?;
                self.change(&args, |t| ops::rule_add(t, rule, before))
            }
            "set" => {
                let args = Args::parse(rest, &CHANGE, &["--selector", "--target"])?;
                let change = match args.words.as_slice() {
                    [_, rule] if !args.has("--selector") && !args.has("--target") => {
                        RuleChange::Whole(rule.clone())
                    }
                    [_] if args.has("--selector") || args.has("--target") => RuleChange::Parts {
                        selector: args.value("--selector").map(str::to_owned),
                        target: args.value("--target").map(str::to_owned),
                    },
                    _ => {
                        return Err(Fail::Usage(
                            "rule set takes N and a rule, or N with --selector or --target"
                                .to_owned(),
                        ));
                    }
                };
                let n = number(args.words.first().map_or("", String::as_str))?;
                self.change(&args, |t| ops::rule_set(t, n, &change))
            }
            "move" => {
                let args = Args::parse(rest, &[&CHANGE[..], &["--last"]].concat(), &["--before"])?;
                let [n] = args.words.as_slice() else {
                    return Err(Fail::Usage(
                        "rule move takes N with --before M or --last".to_owned(),
                    ));
                };
                let n = number(n)?;
                let to = match (args.value("--before"), args.has("--last")) {
                    (Some(m), false) => To::Before(number(m)?),
                    (None, true) => To::Last,
                    _ => {
                        return Err(Fail::Usage(
                            "rule move takes --before M or --last".to_owned(),
                        ));
                    }
                };
                self.change(&args, |t| ops::rule_move(t, n, to))
            }
            "rm" => {
                let args = Args::parse(rest, &CHANGE, &[])?;
                let [n] = args.words.as_slice() else {
                    return Err(Fail::Usage("rule rm takes N".to_owned()));
                };
                let n = number(n)?;
                self.change(&args, |t| ops::rule_rm(t, n))
            }
            other => Err(Fail::Usage(format!(
                "rule takes add, set, move or rm, not {other}"
            ))),
        }
    }

    /// `head add`, `head set`, `head rm` and `head active`.
    fn head(&mut self, words: &[String]) -> Result<(), Fail> {
        let (sub, rest) = words
            .split_first()
            .ok_or_else(|| Fail::Usage("head takes add, set, rm or active".to_owned()))?;
        match sub.as_str() {
            "add" => {
                let args = Args::parse(rest, &CHANGE, &["--look"])?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("head add takes NAME".to_owned()));
                };
                let look = args.value("--look").map(str::to_owned);
                self.change(&args, |t| ops::head_add(t, name, look.as_deref()))
            }
            "set" => {
                const KEYS: [(&str, &str); 6] = [
                    ("--look", "look"),
                    ("--consoles", "consoles"),
                    ("--login", "login"),
                    ("--when-display-lost", "when-display-lost"),
                    ("--isolated", "isolated"),
                    ("--switch-on-enter", "switch-on-enter"),
                ];
                let valued: Vec<&str> = KEYS.iter().map(|(o, _)| *o).chain(["--name"]).collect();
                let args = Args::parse(rest, &CHANGE, &valued)?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("head set takes NAME and the settings to change".to_owned()));
                };
                let mut keys: Vec<(&str, Option<&str>)> = KEYS
                    .iter()
                    .filter_map(|(o, k)| args.value(o).map(|v| (*k, Some(v))))
                    .collect();
                // `switch-on-enter` is `no` when unset, so no takes its
                // line out and the draft holds only what is set
                for (key, value) in &mut keys {
                    if *key == "switch-on-enter" {
                        *value = match *value {
                            Some("yes") => Some("yes"),
                            Some("no") => None,
                            other => {
                                return Err(Fail::Usage(format!(
                                    "--switch-on-enter takes yes or no, not {}.",
                                    other.unwrap_or_default()
                                )));
                            }
                        };
                    }
                }
                let rename = args.value("--name").map(str::to_owned);
                if keys.is_empty() && rename.is_none() {
                    return Err(Fail::Usage("head set needs at least one setting to change".to_owned()));
                }
                self.change(&args, |t| {
                    let set = ops::section_keys(t, Of::Head(name), &keys)?;
                    match &rename {
                        Some(new) => ops::rename(&set, Of::Head(name), new),
                        None => Ok(set),
                    }
                })
            }
            "rm" => {
                let args = Args::parse(rest, &CHANGE, &[])?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("head rm takes NAME".to_owned()));
                };
                let (_, text) = self.session.current()?;
                let broken = ops::rules_to(&text, name);
                if !broken.is_empty() && !args.has("--dry-run") {
                    let shown: Vec<String> = broken.iter().map(usize::to_string).collect();
                    self.say(&format!(
                        "rules {} still send to {name}; the check reports them until you change or remove each one",
                        shown.join(", ")
                    ));
                }
                self.change(&args, |t| ops::head_rm(t, name))
            }
            "active" => Err(Fail::Usage(
                "head active does not work yet: the running vtium has no way to receive the request".to_owned(),
            )),
            other => Err(Fail::Usage(format!("head takes add, set, rm or active, not {other}"))),
        }
    }

    /// `look add`, `look set` and `look rm`.
    fn look(&mut self, words: &[String]) -> Result<(), Fail> {
        let (sub, rest) = words
            .split_first()
            .ok_or_else(|| Fail::Usage("look takes add, set or rm".to_owned()))?;
        match sub.as_str() {
            "add" => {
                let valued: Vec<&str> = ops::LOOK_KEYS
                    .iter()
                    .map(|(_, o)| *o)
                    .chain([VTLOOK_OPTION])
                    .collect();
                let args = Args::parse(rest, &CHANGE, &valued)?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("look add takes NAME".to_owned()));
                };
                let keys = look_keys(&args);
                self.change(&args, |t| ops::look_add(t, name, &keys))
            }
            "set" => {
                let valued: Vec<&str> = ops::LOOK_KEYS
                    .iter()
                    .map(|(_, o)| *o)
                    .chain([VTLOOK_OPTION, "--name"])
                    .collect();
                let args = Args::parse(rest, &CHANGE, &valued)?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage(
                        "look set takes NAME and the settings to change".to_owned(),
                    ));
                };
                let keys = look_keys(&args);
                let rename = args.value("--name").map(str::to_owned);
                if keys.is_empty() && rename.is_none() {
                    return Err(Fail::Usage(
                        "look set needs at least one setting to change".to_owned(),
                    ));
                }
                self.change(&args, |t| {
                    let set = ops::section_keys(t, Of::Look(name), &keys)?;
                    match &rename {
                        Some(new) => ops::rename(&set, Of::Look(name), new),
                        None => Ok(set),
                    }
                })
            }
            "rm" => {
                let args = Args::parse(rest, &[&CHANGE[..], &["--force"]].concat(), &[])?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("look rm takes NAME".to_owned()));
                };
                let force = args.has("--force");
                self.change(&args, |t| ops::look_rm(t, name, force))
            }
            other => Err(Fail::Usage(format!(
                "look takes add, set or rm, not {other}"
            ))),
        }
    }

    /// `draft list`, `draft new`, `draft switch` and `draft rm`.
    fn draft(&mut self, words: &[String]) -> Result<(), Fail> {
        let store = &self.session.store;
        let (sub, rest) = words
            .split_first()
            .ok_or_else(|| Fail::Usage("draft takes list, new, switch or rm".to_owned()))?;
        match (sub.as_str(), rest) {
            ("list", []) => {
                let current = store.settings()?.draft;
                for name in store.drafts()? {
                    let text = store.read_draft(&name)?.unwrap_or_default();
                    let state = self.session.state(&name, &text)?;
                    let mark = if current.as_deref() == Some(name.as_str()) { '*' } else { ' ' };
                    self.say(&format!("{mark} {name}\t{}", state.words()));
                }
                Ok(())
            }
            ("new", _) => {
                let args = Args::parse(rest, &[], &["--from"])?;
                let name = match args.words.as_slice() {
                    [] => store.free_name(&crate::store::hostname())?,
                    [name] if !valid_name(name) => {
                        return Err(Fail::Usage(format!("{name:?} cannot be a draft's name: a name has 1 to 64 letters, digits, '.', '_', '+' or '-'")));
                    }
                    [name] if store.read_draft(name)?.is_some() => {
                        return Err(Fail::Usage(format!("a draft named {name} already exists, and vtium-config never overwrites a draft")));
                    }
                    [name] => name.clone(),
                    _ => return Err(Fail::Usage("draft new takes at most one name".to_owned())),
                };
                let from = match args.value("--from") {
                    None | Some("installed") => From::Installed,
                    Some("empty") => From::Empty,
                    Some(other) => From::Draft(other.to_owned()),
                };
                self.session.make(&name, from)?;
                self.say(&format!("wrote {}", store.draft_path(&name)?.display()));
                self.say(&format!("draft {name} is now current"));
                Ok(())
            }
            ("switch", [name]) => {
                if store.read_draft(name)?.is_none() {
                    return Err(Fail::Usage(format!("there is no draft named {name}")));
                }
                self.session.seen(name)?;
                store.set_current(Some(name))?;
                self.say(&format!("draft {name} is now current"));
                Ok(())
            }
            ("rm", [] | [_]) => {
                let current = store.settings()?.draft;
                let name = match rest.first().or(current.as_ref()) {
                    Some(name) => name.clone(),
                    None => return Err(Fail::Usage("there is no current draft to remove".to_owned())),
                };
                if store.read_draft(&name)?.is_none() {
                    return Err(Fail::Usage(format!("there is no draft named {name}")));
                }
                store.remove_draft(&name)?;
                if current.as_deref() == Some(name.as_str()) {
                    store.set_current(None)?;
                }
                self.say(&format!("removed draft {name}"));
                Ok(())
            }
            _ => Err(Fail::Usage("draft takes list, new [NAME] [--from installed|empty|DRAFT], switch NAME or rm [NAME]".to_owned())),
        }
    }

    /// `history`: the installed history, newest first, or the line that
    /// says no install has replaced a vtium.conf yet.
    fn history(&mut self, words: &[String]) -> Result<(), Fail> {
        if !words.is_empty() {
            return Err(Fail::Usage("history takes no argument".to_owned()));
        }
        let history = self.session.store.history()?;
        if history.is_empty() {
            self.say(crate::tui::words::NO_HISTORY);
        }
        for (i, h) in history.iter().enumerate() {
            let when = crate::report::utc_text(h.when);
            self.say(&format!(
                "{}\t{}\t{when}",
                i.saturating_add(1),
                h.file_name()
            ));
        }
        Ok(())
    }

    /// `restore [N]`: history file N, the newest when none is given, into
    /// the current draft.
    fn restore(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--dry-run"], &[])?;
        let n = match args.words.as_slice() {
            [] => 1,
            [n] => number(n)?,
            _ => return Err(Fail::Usage("restore takes at most one number".to_owned())),
        };
        let history = self.session.store.history()?;
        let file = n
            .checked_sub(1)
            .and_then(|i| history.get(i))
            .ok_or_else(|| {
                Fail::Usage(format!(
                    "there is no earlier file {n}; vtium config history lists them"
                ))
            })?;
        let restored = crate::tui::read_conf(&file.path)
            .map_err(Fail::File)?
            .unwrap_or_default();
        let (name, text) = self.session.current()?;
        self.write(&name, &text, &restored, args.has("--dry-run"), false)
    }

    /// `library list`, `library add`, `library take` and `library rm`.
    fn library(&mut self, words: &[String]) -> Result<(), Fail> {
        let path = self.session.store.library_path();
        let library = crate::tui::read_conf(&path)
            .map_err(Fail::File)?
            .unwrap_or_default();
        let (sub, rest) = words
            .split_first()
            .ok_or_else(|| Fail::Usage("library takes list, add, take or rm".to_owned()))?;
        match (sub.as_str(), rest.iter().map(String::as_str).collect::<Vec<_>>().as_slice()) {
            ("list", []) => {
                for line in crate::library::list(&library) {
                    self.say(&line);
                }
                Ok(())
            }
            ("add", [kind @ ("look" | "head"), name]) => {
                let (_, text) = self.session.current()?;
                let of = of(kind, name);
                let new = crate::library::copy(&text, &library, of)?;
                crate::store::write(&path, &new)?;
                self.say(&format!("wrote {}", path.display()));
                Ok(())
            }
            ("take", [kind @ ("look" | "head"), name]) => {
                let (draft, text) = self.session.current()?;
                let new = crate::library::copy(&library, &text, of(kind, name))?;
                self.write(&draft, &text, &new, false, false)
            }
            ("add", ["rule", n]) => {
                let (_, text) = self.session.current()?;
                let rule = ops::rule_text(&text, number(n)?)?;
                let new = vtium_settings::add_rule(&library, &rule.line(), None)
                    .map_err(|e| Fail::Usage(e.to_string()))?;
                crate::store::write(&path, &new)?;
                self.say(&format!("wrote {}", path.display()));
                Ok(())
            }
            ("take", ["rule", n]) => {
                let rule = ops::rule_text(&library, number(n)?)?;
                let (draft, text) = self.session.current()?;
                let new = ops::rule_add(&text, &rule.line(), None)?;
                self.write(&draft, &text, &new, false, false)
            }
            ("rm", [kind @ ("look" | "head" | "rule"), name]) => {
                let new = match *kind {
                    "rule" => vtium_settings::remove_rule(&library, number(name)?),
                    kind => vtium_settings::remove_section(&library, of(kind, name)),
                }
                .map_err(|e| Fail::Usage(e.to_string()))?;
                crate::store::write(&path, &new)?;
                self.say(&format!("wrote {}", path.display()));
                Ok(())
            }
            _ => Err(Fail::Usage("library takes list, add look|head NAME, add rule N, take look|head NAME, take rule N or rm look|head NAME or rm rule N".to_owned())),
        }
    }

    /// `edit [--install]`: the draft in the user's editor, as visudo edits
    /// sudoers.
    fn edit(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--install"], &[])?;
        if !args.words.is_empty() {
            return Err(Fail::Usage("edit takes only --install".to_owned()));
        }
        let (name, text) = self.session.current()?;
        let copy = self.session.store.edit_copy(&name)?;
        crate::store::write(&copy, &text)?;
        let result = self.edit_loop(&name, &text, &copy, args.has("--install"));
        let _ = std::fs::remove_file(&copy);
        result
    }

    fn edit_loop(
        &mut self,
        name: &str,
        text: &str,
        copy: &Path,
        install: bool,
    ) -> Result<(), Fail> {
        let editor = editor(|k| std::env::var(k).ok());
        let mut draft = text.to_owned();
        loop {
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("sh")
                .arg(copy)
                .status()
                .map_err(|e| Fail::File(format!("{editor}: {e}")))?;
            if !status.success() {
                return Err(Fail::File(format!(
                    "the editor {editor} failed, so the draft is unchanged"
                )));
            }
            let edited = crate::tui::read_conf(copy)
                .map_err(Fail::File)?
                .unwrap_or_default();
            if edited == draft {
                self.say(&format!("draft {name} unchanged"));
                return Ok(());
            }
            self.session.store.write_draft(name, &edited)?;
            draft = edited;
            let path = self.session.store.draft_path(name)?;
            self.say(&format!("wrote {}", path.display()));
            let checked = self.session.check(name, &draft, &path)?;
            self.print_check(&checked);
            // a missing font is shown, and never asks for another edit
            if checked.clears() {
                break;
            }
            let _ = write!(self.out, "The check found problems. Edit again? [y/N] ");
            let _ = self.out.flush();
            let mut answer = String::new();
            let _ = std::io::stdin().lock().read_line(&mut answer);
            if !matches!(answer.trim(), "y" | "Y" | "yes") {
                if install {
                    return Err(Fail::Check(format!(
                        "nothing installed: the check found problems in draft {name}. The draft keeps your changes."
                    )));
                }
                return Ok(());
            }
        }
        if install {
            self.session.install(
                name,
                &draft,
                Unrouted::Refuse,
                &mut run_child,
                None,
                self.out,
            )?;
        }
        Ok(())
    }
}

/// The section a library command names.
fn of<'a>(kind: &str, name: &'a str) -> Of<'a> {
    if kind == "look" {
        Of::Look(name)
    } else {
        Of::Head(name)
    }
}

/// What a complete route lacks, from the check's lines that say so.
fn missing(checked: &Checked) -> Vec<String> {
    checked
        .problems
        .iter()
        .filter(|l| {
            vtium_settings::Missing::DEVICE_GAPS
                .iter()
                .any(|words| l.contains(words))
        })
        .cloned()
        .collect()
}

/// The editor `edit` opens: `$VISUAL`, else `$EDITOR`, else `vi`.
pub fn editor(var: impl Fn(&str) -> Option<String>) -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .find_map(|k| var(k).filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "vi".to_owned())
}

/// The option of `look add` and `look set` that sets a look's `vtlook`,
/// which the look card also sets. `vtlook` is not in [`ops::LOOK_KEYS`],
/// because a look's section takes it beside the key table, as a head
/// takes `look`.
///
/// References:
/// - ADR-1403 D8: every change the view makes is also a command.
const VTLOOK_OPTION: &str = "--vtlook";

/// The look keys that `args` gives values to, in the order of
/// [`ops::LOOK_KEYS`], then `vtlook`.
fn look_keys(args: &Args) -> Vec<(&'static str, Option<&str>)> {
    ops::LOOK_KEYS
        .iter()
        .chain([&("vtlook", VTLOOK_OPTION)])
        .filter_map(|(k, o)| args.value(o).map(|v| (*k, Some(v))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_is_visual_then_editor_then_vi() {
        assert_eq!(
            editor(|k| (k == "EDITOR").then(|| "nano".to_owned())),
            "nano"
        );
        assert_eq!(editor(|k| Some(k.to_lowercase())), "visual");
        assert_eq!(editor(|_| None), "vi");
    }

    // what a complete route lacks is read from vtium's own messages for
    // the gaps a connected device leaves, and from no other route message
    #[test]
    fn a_route_s_gaps_are_read_from_vtium_s_own_messages() {
        use vtium_settings::{Missing, Quoted};
        let q = |s: &str| Quoted::path(std::path::Path::new(s));
        let gaps = [
            Missing::NoDisplay {
                display: q("DP-8"),
                rule: Some(3),
            },
            Missing::NoDisplay {
                display: q("DP-8"),
                rule: None,
            },
            Missing::NoKeyboard {
                head: q("desk"),
                display: q("DP-8"),
            },
            Missing::Isolated { head: q("desk") },
        ];
        let others = [
            Missing::NoDisplayRule,
            Missing::NoKeyboardRule {
                head: q("desk"),
                rule: 2,
            },
        ];
        let line = |m: &Missing| format!("vtium.conf line 4: {m}");
        let checked = Checked {
            verdict: Verdict::Failed,
            problems: gaps.iter().chain(&others).map(line).collect(),
            notes: Vec::new(),
        };
        assert_eq!(missing(&checked), gaps.iter().map(line).collect::<Vec<_>>());
    }
}
