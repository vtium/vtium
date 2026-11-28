//! The command line's forms of the document's versions: `vtium config
//! update`, which merges the installed file's change into the current
//! draft as Update does, `vtium config install --overwrite`, which
//! installs the draft over a file changed on disk as Overwrite does, and
//! `vtium config draft rename NAME NEW`, which Manage configurations
//! calls. Each asks nothing but sudo's password, and that only when it
//! installs, and exits with [`Fail`]'s codes.
//!
//! References:
//! - ADR-1403 D8, "When the installed file changed on disk": `update` changes nothing where both sides changed the same lines, lists them and exits 1, and `install --overwrite` keeps the replaced file in the installed history.
//! - ADR-1403 D8, "Commands for scripts" and "vtium-config's store: drafts and the library": the two commands and `draft rename`.

use std::io::Write;

use crate::apply::Ask;
use crate::document::{Base, Content, Desk};
use crate::drafts::{Over, Session, Unrouted, run_child};
use crate::help::Command;
use crate::outcome::Fail;
use crate::places::Paths;
use crate::store::{Store, valid_name};

use super::base::{Disk, disk, recover};
use super::words;

/// Whether this module runs `command` with its `words`: `update`,
/// `install` with `--overwrite`, and `draft rename`. The flow's commands
/// run the rest.
pub fn takes(command: Command, words: &[String]) -> bool {
    match command {
        Command::Update => true,
        Command::Install => words.iter().any(|w| w == "--overwrite"),
        Command::Draft => words.first().is_some_and(|w| w == "rename"),
        _ => false,
    }
}

/// Runs `command`, one that [`takes`] takes, with its `words`; returns the
/// exit status.
pub fn run(
    command: Command,
    words: &[String],
    paths: &Paths,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let ran = Store::system().and_then(|store| {
        let session = Session { store, paths };
        match command {
            Command::Update => update(&session, words, out),
            Command::Install => install(&session, words, out),
            _ => rename(&session.store, words.get(1..).unwrap_or_default(), out),
        }
    });
    match ran {
        Ok(()) => 0,
        Err(fail) => {
            let _ = writeln!(err, "vtium-config: {}", fail.text());
            if fail.code() == 2 {
                let _ = writeln!(err, "Try vtium-config --help.");
            }
            fail.code()
        }
    }
}

/// `update [--dry-run]`.
fn update(session: &Session<'_>, args: &[String], out: &mut dyn Write) -> Result<(), Fail> {
    let dry_run = match args {
        [] => false,
        [d] if d == "--dry-run" => true,
        _ => return Err(Fail::Usage("update takes only --dry-run".to_owned())),
    };
    let store = &session.store;
    let (name, text) = session.current()?;
    let installed = session.installed()?;
    let mut base = Base {
        hash: store.base(&name)?.flatten(),
        text: store.base_text(&name)?,
    };
    if base.text.is_none() && base.hash.is_some() {
        let versions = crate::document::OnDisk::new(store.clone()).versions();
        base = recover(base, &versions.map_err(Fail::File)?);
    }
    let content = Content {
        text: text.clone(),
        base,
        ..Content::default()
    };
    let path = session.installed_path();
    let shown = path.display();
    let merged = match (disk(&content, installed.as_deref()), installed.as_deref()) {
        (Disk::Missing, _) | (_, None) => {
            let _ = writeln!(out, "{}", words::nothing_missing(&shown));
            return Ok(());
        }
        (Disk::Same, _) => {
            let _ = writeln!(out, "{}", words::nothing_changed(&shown, &name));
            return Ok(());
        }
        (Disk::Taken, Some(installed)) => installed.to_owned(),
        (Disk::Changed(changed), Some(_)) => {
            let merge = changed
                .merge()
                .map_err(|why| Fail::Check(words::cannot_merge(why, &name)))?;
            let Some(merged) = merge.text() else {
                for both in merge.both() {
                    let _ = write!(out, "{}", words::both_lines(both, &name, &shown));
                }
                return Err(Fail::Check(words::both_changed(
                    merge.settled().0,
                    &name,
                    &shown,
                )));
            };
            merged
        }
    };
    if dry_run {
        let draft = store.draft_path(&name)?;
        let _ = write!(
            out,
            "{}",
            crate::drafts::diff(
                &draft.to_string_lossy(),
                &text,
                &format!("{} (after update)", draft.display()),
                &merged
            )
        );
        return Ok(());
    }
    store.write_draft(&name, &merged)?;
    store.set_base(&name, installed.as_deref().map(str::as_bytes))?;
    let (added, removed) = crate::drafts::counts(&text, &merged);
    let _ = writeln!(out, "{}", words::updated(&name, &shown, added, removed));
    let checked = session.check(&name, &merged, &store.draft_path(&name)?)?;
    for line in session.report(&name, &merged, &checked)? {
        let _ = writeln!(out, "{line}");
    }
    Ok(())
}

/// `install --overwrite [--force]`: installs the draft as `install` does,
/// over an installed file changed since the draft's base.
fn install(session: &Session<'_>, args: &[String], out: &mut dyn Write) -> Result<(), Fail> {
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--overwrite" => {}
            "--force" => force = true,
            _ => {
                return Err(Fail::Usage(
                    "install takes only --force and --overwrite".to_owned(),
                ));
            }
        }
    }
    let (name, text) = session.current()?;
    let terminal = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let mut no_route = crate::vts::ask_line;
    let mut restart = crate::vts::ask_line;
    let unrouted = if force {
        Unrouted::Install
    } else if terminal {
        Unrouted::Ask(&mut no_route)
    } else {
        Unrouted::Refuse
    };
    let ask: Option<Ask<'_>> = if terminal { Some(&mut restart) } else { None };
    session.install_over(
        &name,
        &text,
        Over::Overwrite,
        unrouted,
        &mut run_child,
        ask,
        out,
    )
}

/// `draft rename NAME NEW`.
fn rename(store: &Store, args: &[String], out: &mut dyn Write) -> Result<(), Fail> {
    let [name, new] = args else {
        return Err(Fail::Usage("draft rename takes NAME and NEW".to_owned()));
    };
    if !valid_name(new) {
        return Err(Fail::Usage(words::bad_name(new)));
    }
    store.rename_draft(name, new)?;
    let _ = writeln!(out, "{}", words::renamed(name, new));
    Ok(())
}
