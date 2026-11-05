//! The flow's commands (ADR-1403 D8): `flow`, `route`, `diff`, `install`,
//! `set`, `get`, `revert`, `new`, `rule`, `head`, `look`, `draft`,
//! `history`, `restore`, `library` and `edit`. Each that changes the
//! configuration reads the draft and the devices afresh, makes its change
//! through [`crate::ops`], writes the draft whatever the check says, and
//! prints what `vtium check-config` finds in it; `--dry-run` prints the
//! change as a difference and the check's verdict and writes nothing, and
//! `--install` then installs the draft. Every command asks nothing but
//! sudo's password, and that only when it installs. The exit codes are
//! [`Fail`]'s.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use vtium_platform::listing::Roots;
use vtium_settings::Of;

use crate::drafts::{Checked, From, Session, State, diff};
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
                    .ok_or_else(|| Fail::Usage(format!("{w} takes a value")))?;
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
                "{word:?} is not a rule's number; rules are numbered from 1"
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
            "route" => self.route(words),
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
            .check(text, &self.session.store.draft_path(name))?;
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
            flowmodel::json(&flow, &name, &state, &checked.problems, &missing)
        } else {
            flowmodel::text(
                &flow,
                &flowmodel::state_line(&name, &state, checked.count()),
                &missing,
            )
        };
        let _ = write!(self.out, "{printed}");
        Ok(())
    }

    /// `route ITEM [--json]`.
    fn route(&mut self, words: &[String]) -> Result<(), Fail> {
        let args = Args::parse(words, &["--json"], &[])?;
        let (name, text) = self.session.current()?;
        let (flow, _, _) = self.judged(&name, &text)?;
        let item = flowmodel::item(&flow, &args.words).ok_or_else(|| {
            Fail::Usage(format!(
                "route takes display NAME, rule N, head NAME, look NAME or input NAME, and the draft has no {}",
                args.words.join(" ")
            ))
        })?;
        let title = args.words.join(" ");
        let routes: Vec<_> = flowmodel::routes(&flow)
            .into_iter()
            .filter(|r| r.has(&flow, item))
            .collect();
        if args.has("--json") {
            let lines = routes.iter().map(|r| {
                let parts: Vec<String> = flowmodel::route_lines(&flow, r, item)
                    .into_iter()
                    .map(|(section, words, selected)| {
                        format!(
                            "{{\"section\":{},\"item\":{},\"selected\":{selected}}}",
                            flowmodel::quoted(section),
                            flowmodel::quoted(&words)
                        )
                    })
                    .collect();
                format!("[{}]", parts.join(","))
            });
            let says = if routes.is_empty() {
                flowmodel::unrouted(&flow, item)
            } else {
                format!(
                    "on {} complete route{}",
                    routes.len(),
                    if routes.len() == 1 { "" } else { "s" }
                )
            };
            let _ = writeln!(
                self.out,
                "{{\"version\":1,\"item\":{},\"says\":{},\"routes\":[{}]}}",
                flowmodel::quoted(&title),
                flowmodel::quoted(&says),
                lines.collect::<Vec<_>>().join(",")
            );
            return Ok(());
        }
        self.say(&format!("route card: {title}"));
        if routes.is_empty() {
            self.say(&flowmodel::unrouted(&flow, item));
        }
        for (i, r) in routes.iter().enumerate() {
            self.say(&format!(
                "route {} of {}",
                i.saturating_add(1),
                routes.len()
            ));
            for (section, words, selected) in flowmodel::route_lines(&flow, r, item) {
                let mark = if selected { '>' } else { ' ' };
                self.say(&format!("{mark} {section}: {words}"));
            }
        }
        Ok(())
    }

    /// `diff`: the installed file to the draft.
    fn diff(&mut self, words: &[String]) -> Result<(), Fail> {
        if !words.is_empty() {
            return Err(Fail::Usage("diff takes no argument".to_owned()));
        }
        let (name, text) = self.session.current()?;
        let installed = self.session.installed()?.unwrap_or_default();
        let shown = diff(
            &self.session.installed_path().to_string_lossy(),
            &installed,
            &self.session.store.draft_path(&name).to_string_lossy(),
            &text,
        );
        let _ = write!(self.out, "{shown}");
        Ok(())
    }

    fn install(&mut self, words: &[String]) -> Result<(), Fail> {
        if !words.is_empty() {
            return Err(Fail::Usage("install takes no argument".to_owned()));
        }
        let (name, text) = self.session.current()?;
        self.session.install(&name, &text, self.out)
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
        let path = self.session.store.draft_path(name);
        if dry_run {
            let shown = path.to_string_lossy();
            let _ = write!(
                self.out,
                "{}",
                diff(&shown, old, &format!("{shown} (after)"), new)
            );
            let checked = self.session.check(new, &path)?;
            self.print_check(&checked);
            self.say(if checked.passed {
                "the check passes; nothing is written"
            } else {
                "the check finds problems; nothing is written"
            });
            return Ok(());
        }
        if new == old {
            self.say(&format!("draft {name} unchanged"));
        } else {
            self.session.store.write_draft(name, new)?;
            self.say(&format!("wrote {}", path.display()));
        }
        let state = self.session.state(name, new)?;
        let checked = match self.session.check(new, &path) {
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
        self.say(&flowmodel::state_line(name, &state, checked.count()));
        if install {
            self.session.install(name, new, self.out)?;
        }
        Ok(())
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
                .unwrap_or_else(|| crate::drafts::EXAMPLE.to_owned()),
            _ => ops::new_text(crate::drafts::EXAMPLE),
        };
        if args.has("--dry-run") {
            let (added, removed) = crate::drafts::counts(&new, &text);
            self.say(&format!(
                "{command} would lose {} of draft {name}",
                crate::drafts::lines(added, removed)
            ));
            return Ok(());
        }
        if command == "revert" {
            self.session
                .store
                .set_base(&name, installed.as_deref().map(str::as_bytes))?;
        }
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
                const KEYS: [(&str, &str); 5] = [
                    ("--look", "look"),
                    ("--consoles", "consoles"),
                    ("--login", "login"),
                    ("--when-display-lost", "when-display-lost"),
                    ("--isolated", "isolated"),
                ];
                let valued: Vec<&str> = KEYS.iter().map(|(o, _)| *o).chain(["--name"]).collect();
                let args = Args::parse(rest, &CHANGE, &valued)?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("head set takes NAME and the keys to change".to_owned()));
                };
                let keys: Vec<(&str, Option<&str>)> = KEYS
                    .iter()
                    .filter_map(|(o, k)| args.value(o).map(|v| (*k, Some(v))))
                    .collect();
                let rename = args.value("--name").map(str::to_owned);
                if keys.is_empty() && rename.is_none() {
                    return Err(Fail::Usage("head set takes at least one key to change".to_owned()));
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
                self.change(&args, |t| ops::head_rm(t, name))
            }
            "active" => Err(Fail::Usage(
                "head active needs the control socket under /run/vtium that ADR-1401 D12 plans, which this build does not have, so vtium-config cannot ask the running vtium to change its active head".to_owned(),
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
                let args = Args::parse(rest, &CHANGE, &["--frit", "--size"])?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage("look add takes NAME".to_owned()));
                };
                let (frit, size) = (
                    args.value("--frit").map(str::to_owned),
                    args.value("--size").map(str::to_owned),
                );
                self.change(&args, |t| {
                    ops::look_add(t, name, frit.as_deref(), size.as_deref())
                })
            }
            "set" => {
                const KEYS: [(&str, &str); 5] = [
                    ("--frit", "font-name"),
                    ("--size", "font-size"),
                    ("--engine", "font-engine"),
                    ("--hinting", "font-hinting"),
                    ("--line-spacing", "font-line-spacing"),
                ];
                let valued: Vec<&str> = KEYS.iter().map(|(o, _)| *o).chain(["--name"]).collect();
                let args = Args::parse(rest, &CHANGE, &valued)?;
                let [name] = args.words.as_slice() else {
                    return Err(Fail::Usage(
                        "look set takes NAME and the keys to change".to_owned(),
                    ));
                };
                let keys: Vec<(&str, Option<&str>)> = KEYS
                    .iter()
                    .filter_map(|(o, k)| args.value(o).map(|v| (*k, Some(v))))
                    .collect();
                let rename = args.value("--name").map(str::to_owned);
                if keys.is_empty() && rename.is_none() {
                    return Err(Fail::Usage(
                        "look set takes at least one key to change".to_owned(),
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
                        return Err(Fail::Usage(format!("{name:?} cannot name a draft: 1 to 64 letters, digits, '.', '_', '+' and '-'")));
                    }
                    [name] if store.read_draft(name)?.is_some() => {
                        return Err(Fail::Usage(format!("a draft named {name} exists already; vtium-config never overwrites a draft")));
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
                self.say(&format!("wrote {}", store.draft_path(&name).display()));
                self.say(&format!("draft {name} is current"));
                Ok(())
            }
            ("switch", [name]) => {
                if store.read_draft(name)?.is_none() {
                    return Err(Fail::Usage(format!("there is no draft named {name}")));
                }
                store.set_current(Some(name))?;
                self.say(&format!("draft {name} is current"));
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

    /// `history`: the installed history, newest first.
    fn history(&mut self, words: &[String]) -> Result<(), Fail> {
        if !words.is_empty() {
            return Err(Fail::Usage("history takes no argument".to_owned()));
        }
        for (i, h) in self.session.store.history()?.iter().enumerate() {
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
                    "the installed history has no file {n}; vtium config history lists them"
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
            ("add" | "take" | "rm", ["rule", _]) | ("rm", [_, _]) => Err(Fail::Usage(
                "the library's rules, and removing a node from it, wait on vtium-settings's section writer, which cannot yet keep a rule whose head is not in the same file or remove a section".to_owned(),
            )),
            _ => Err(Fail::Usage("library takes list, add look|head NAME, add rule N, take look|head NAME, take rule NAME or rm look|head|rule NAME".to_owned())),
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
        let copy = self
            .session
            .store
            .state
            .join(format!("edit-{name}-{}.conf", std::process::id()));
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
                    "the editor {editor} failed, and the draft is as it was"
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
            let path = self.session.store.draft_path(name);
            self.say(&format!("wrote {}", path.display()));
            let checked = self.session.check(&draft, &path)?;
            self.print_check(&checked);
            if checked.passed {
                break;
            }
            let _ = write!(self.out, "The check finds problems. Edit again? [y/N] ");
            let _ = self.out.flush();
            let mut answer = String::new();
            let _ = std::io::stdin().lock().read_line(&mut answer);
            if !matches!(answer.trim(), "y" | "Y" | "yes") {
                if install {
                    return Err(Fail::Check(format!(
                        "the check found problems in draft {name}, so nothing is installed; the draft keeps every change"
                    )));
                }
                return Ok(());
            }
        }
        if install {
            self.session.install(name, &draft, self.out)?;
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
            l.contains("No display present")
                || l.contains("no keyboard present")
                || l.contains("isolated, so roaming")
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
}
