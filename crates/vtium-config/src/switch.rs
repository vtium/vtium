//! `use-frit NAME` (ADR-1200 D5): switches the running console among the frits
//! and the built-in fonts through xterm's font operation, OSC 50, on the
//! controlling terminal.
//!
//! NAME is checked first, as the console checks it: a built-in font, or a
//! frit's name with `NAME.vtfrit` a regular file in the frits' directory;
//! the command fails naming that file when it is neither. It then asks the
//! terminal what it is, with XTVERSION and DA1 ([`is_vtium`]), and sends
//! nothing more to a terminal that does not name vtium, since another
//! terminal may act on OSC 50, as xterm looks for an X font of that name
//! (ADR-1201 D4). To vtium it writes
//! `OSC 50 ; NAME ST`, the query `OSC 50 ; ? ST` and DA1 to `/dev/tty`, and
//! reads in raw mode until DA1's reply, or for at most [`WAIT`] from a
//! terminal that does not answer DA1, putting the terminal's mode back
//! after. SIGTERM, SIGHUP, SIGINT and SIGQUIT are
//! held while the terminal is raw, so one that comes then acts once the
//! mode is back; input left unread, such as a reply that came late, is
//! dropped first, so it never reaches the shell's prompt. It prints
//! `set NAME` and exits 0 when the reply names NAME; otherwise it says
//! what the console kept, or that no reply came: font operations are off
//! (`allow-font-ops`), or the terminal is not vtium.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::termios::{
    OptionalActions, QueueSelector, SpecialCodeIndex, Termios, tcflush, tcgetattr, tcsetattr,
};
use vtium_font::set::builtin;
use vtium_platform::calloop::signals::{Signal, Signals};

use crate::frit::{RESERVED, valid_name};

/// How long the reply is waited for.
pub const WAIT: Duration = Duration::from_secs(1);

/// The most bytes read while the reply is looked for.
const READ_MAX: usize = 4096;

/// What the terminal answered the query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The font in use, as the reply names it.
    Named(String),
    /// No reply before DA1's, or none within the wait.
    Silent,
}

/// Asks the terminal behind `tty`, already raw, what it is, with XTVERSION
/// and DA1, and reads until DA1's reply or `deadline`: whether XTVERSION
/// named vtium. Every terminal of the VT100 line answers DA1, and replies
/// come in the order of the requests, so no reply comes after DA1's.
pub fn is_vtium(tty: &mut (impl Read + Write), deadline: Instant) -> io::Result<bool> {
    use crate::tui::input::Reply;
    let replies = ask(tty, b"\x1b[>q", deadline)?;
    Ok(replies
        .iter()
        .any(|r| matches!(r, Reply::Version(v) if v.starts_with("vtium("))))
}

/// Writes `requests` and DA1 to `tty`, already raw, and reads until DA1's
/// reply or `deadline`: the replies that came before DA1's. Every terminal
/// of the VT100 line answers DA1, and replies come in the order of the
/// requests, so a reply that has not come before DA1's will not come.
pub fn ask(
    tty: &mut (impl Read + Write),
    requests: &[u8],
    deadline: Instant,
) -> io::Result<Vec<crate::tui::input::Reply>> {
    use crate::tui::input::{Decoder, Input, Reply};
    tty.write_all(requests)?;
    tty.write_all(b"\x1b[c")?;
    tty.flush()?;
    let mut decoder = Decoder::default();
    let mut replies = Vec::new();
    let mut buf = [0u8; 256];
    let mut read = 0usize;
    while now() < deadline && read < READ_MAX {
        let n = match tty.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        read = read.saturating_add(n);
        for input in decoder.feed(buf.get(..n).unwrap_or_default()) {
            match input {
                Input::Reply(Reply::Attributes) => return Ok(replies),
                Input::Reply(r) => replies.push(r),
                Input::Key(_) => {}
            }
        }
    }
    Ok(replies)
}

/// Asks the terminal behind `tty`, already raw, for `name`, and reads the
/// reply to the query until DA1's reply or `deadline`.
pub fn switch(tty: &mut (impl Read + Write), name: &str, deadline: Instant) -> io::Result<Answer> {
    use crate::tui::input::Reply;
    let request = format!("\x1b]50;{name}\x1b\\\x1b]50;?\x1b\\");
    let replies = ask(tty, request.as_bytes(), deadline)?;
    Ok(replies
        .into_iter()
        .find_map(|r| match r {
            Reply::Font(name) => Some(Answer::Named(name)),
            _ => None,
        })
        .unwrap_or(Answer::Silent))
}

#[expect(
    clippy::disallowed_methods,
    reason = "the reply's wait is bounded in time (ADR-1200 D5)"
)]
fn now() -> Instant {
    Instant::now()
}

/// The end of a wait of [`WAIT`] from now.
pub fn deadline() -> Instant {
    now().checked_add(WAIT).unwrap_or_else(now)
}

/// A terminal in raw mode, each read waiting at most a tenth of a second,
/// until it drops: the input left unread is dropped and the mode before
/// put back. Meanwhile the signals that end the command are held, and act
/// once it has dropped.
pub struct Raw<'a> {
    tty: &'a File,
    saved: Termios,
    /// Released after `drop` puts the mode back, as a field.
    _held: Signals,
}

impl<'a> Raw<'a> {
    pub fn new(tty: &'a File) -> io::Result<Raw<'a>> {
        let held = Signals::new(&[
            Signal::SIGTERM,
            Signal::SIGHUP,
            Signal::SIGINT,
            Signal::SIGQUIT,
        ])
        .map_err(io::Error::other)?;
        let saved = tcgetattr(tty.as_fd())?;
        let mut raw = saved.clone();
        raw.make_raw();
        raw.special_codes[SpecialCodeIndex::VMIN] = 0;
        raw.special_codes[SpecialCodeIndex::VTIME] = 1;
        tcsetattr(tty.as_fd(), OptionalActions::Now, &raw)?;
        Ok(Raw {
            tty,
            saved,
            _held: held,
        })
    }
}

impl Drop for Raw<'_> {
    fn drop(&mut self) {
        let _ = tcflush(self.tty.as_fd(), QueueSelector::IFlush);
        let _ = tcsetattr(self.tty.as_fd(), OptionalActions::Now, &self.saved);
    }
}

/// The frits' directory, or why vtium-config knows of none: the commands
/// that list, write or remove frits fail with the reason, and the others
/// go on without it.
pub type Frits = Result<PathBuf, String>;

/// The frits' directory the console reads with the vtium.conf in
/// `configdir`, the one file it reads (ADR-1403 D2): the `font-frits` of
/// the last line of its top level that sets it, else the key's default,
/// the directory the build names.
///
/// vtium-config takes `font-frits` only as an absolute path, as the
/// console does. The console ignores a relative or empty one and keeps the
/// directory an earlier line names. vtium-config refuses it and names the
/// file and the key, so it never lists or writes frits in a directory the
/// user did not mean. A file that cannot be read is refused with its
/// reason, as the TUI's start refuses it.
pub fn frits_dir(configdir: &Path) -> Frits {
    let key = vtium_settings::keys::FONT_FRITS.name;
    let file = configdir.join("vtium.conf");
    let set = crate::tui::read_conf(&file)?.and_then(|text| vtium_settings::setting(&text, key));
    let Some(value) = set else {
        return Ok(PathBuf::from(vtium_settings::keys::FRITS));
    };
    if vtium_settings::keys::absolute(&value) {
        Ok(PathBuf::from(value))
    } else {
        Err(format!(
            "{}: {key}={value:?}: {}, so vtium-config cannot find the font sets. Set {key} to an absolute path, or name the directory with --frits.",
            file.display(),
            vtium_settings::keys::NOT_ABSOLUTE
        ))
    }
}

/// The category folders of the catalog of font sets, in the order
/// vtium-config searches them, as the console searches them.
///
/// References:
/// - ADR-1204 D3 and D9: the six categories and their folders.
pub const CATEGORIES: [&str; 6] = ["console", "classic", "code", "nerd", "scripts", "small"];

/// Whether `path` names a regular file.
fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// The set `name` names in `frits`: `NAME.vtfrit` in the frits' directory
/// itself, where a person's own sets live, else in the first category
/// folder that holds it. Only a frit's name is looked up, so no name
/// reaches outside the directory.
///
/// References:
/// - ADR-1204 D9: the directory itself first, then each category folder.
pub fn frit_file(frits: &Path, name: &str) -> Option<PathBuf> {
    if !valid_name(name) {
        return None;
    }
    let own = frits.join(format!("{name}.vtfrit"));
    if is_file(&own) {
        return Some(own);
    }
    category_file(frits, name)
}

/// The set `name` in the first category folder of `frits` that holds it,
/// whether or not the frits' directory itself holds one of that name.
/// vtium-config installs no set of a name that a category folder holds.
///
/// References:
/// - ADR-1204 D9: a name stays unique across every folder.
pub fn category_file(frits: &Path, name: &str) -> Option<PathBuf> {
    if !valid_name(name) {
        return None;
    }
    let file = format!("{name}.vtfrit");
    CATEGORIES
        .iter()
        .map(|c| frits.join(c).join(&file))
        .find(|path| is_file(path))
}

/// Ok when vtium-config may install a set named `name` in the frits'
/// directory `frits`; else why not: a category folder of the catalog holds
/// a set of that name already. vtium-config opens the directory and asks
/// [`unheld_at`]. A directory it cannot open holds nothing to compare.
///
/// References:
/// - ADR-1204 D9: vtium-config refuses a name that another folder holds.
pub fn unheld(frits: &Path, name: &str) -> Result<(), String> {
    use rustix::fs::{Mode, OFlags};
    match rustix::fs::open(
        frits,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(dir) => unheld_at(&dir, frits, name),
        Err(_) => Ok(()),
    }
}

/// Ok when vtium-config may install a set named `name` in the frits'
/// directory that `dir` holds open, which the messages name as `frits`;
/// else why not. Each category folder is opened relative to `dir`, and the
/// name is looked up in it without following a symbolic link, so the
/// answer never depends on what a link points to. Anything at
/// `CATEGORY/NAME.vtfrit` counts as a set of that name: a link or a folder
/// as much as a regular file. vtium-config does not look into a category
/// folder that is itself a link, and adds no font set while one is a link,
/// because the console follows the link and could find a set of the same
/// name behind it. A name a frit cannot have is never looked up.
///
/// References:
/// - ADR-1204 D9: vtium-config refuses a name that another folder holds.
pub fn unheld_at(dir: &impl AsFd, frits: &Path, name: &str) -> Result<(), String> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};
    use rustix::io::Errno;
    if !valid_name(name) {
        return Ok(());
    }
    let file = format!("{name}.vtfrit");
    let linked = |folder: &Path| {
        format!(
            "The catalog folder {} is a symbolic link. vtium-config does not follow the link, so it cannot tell whether vtium's catalog already holds a font set named {name}. Replace the link with the folder it points to.",
            folder.display()
        )
    };
    for category in CATEGORIES {
        let folder = frits.join(category);
        match rustix::fs::statat(dir, category, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(st) => match FileType::from_raw_mode(st.st_mode) {
                FileType::Directory => {}
                FileType::Symlink => return Err(linked(&folder)),
                // the console finds no set below a file that is not a folder
                _ => continue,
            },
            Err(Errno::NOENT) => continue,
            Err(e) => return Err(format!("{}: {e}", folder.display())),
        }
        let held = rustix::fs::openat(
            dir,
            category,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| match e {
            // the folder became a link or a file after the look above
            Errno::LOOP | Errno::NOTDIR => linked(&folder),
            e => format!("{}: {e}", folder.display()),
        })?;
        let path = folder.join(&file);
        match rustix::fs::statat(&held, file.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => {
                return Err(format!(
                    "The font set {name} is already installed as {}, a set of vtium's catalog. Give your font set another name.",
                    path.display()
                ));
            }
            Err(Errno::NOENT) => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(())
}

/// Every set in `frits` by its name, sorted by name, each with the file
/// that [`frit_file`] finds for it.
pub fn frit_files(frits: &Path) -> Vec<(String, PathBuf)> {
    let mut names = std::collections::BTreeSet::new();
    for dir in std::iter::once(frits.to_owned()).chain(CATEGORIES.iter().map(|c| frits.join(c))) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name();
            if let Some(name) = file.to_str().and_then(|n| n.strip_suffix(".vtfrit")) {
                names.insert(name.to_owned());
            }
        }
    }
    names
        .into_iter()
        .filter_map(|name| frit_file(frits, &name).map(|path| (name, path)))
        .collect()
}

/// Whether the console takes `name`: a built-in font, or a frit's name
/// whose file in `frits` is a regular file.
fn known(name: &str, frits: &Path) -> bool {
    (RESERVED.contains(&name) && builtin(name).is_some())
        || frit_file(frits, name).is_some_and(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

/// `set` with its words and the frits' directory, writing to `out` and
/// `err`; returns the exit status.
pub fn command(words: &[String], frits: &Path, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let [name] = words else {
        let _ = writeln!(
            err,
            "vtium-config: use-frit takes one font set's name\nTry vtium-config --help."
        );
        return 2;
    };
    let fail = |err: &mut dyn Write, why: &str| {
        let _ = writeln!(err, "vtium-config: {why}");
        1
    };
    if !known(name, frits) {
        return fail(
            err,
            &format!(
                "there is no font set named {name}: font sets are in {}, and the built-in fonts are {}",
                frits.display(),
                RESERVED.join(" and ")
            ),
        );
    }
    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(tty) => tty,
        Err(e) => {
            return fail(
                err,
                &format!("use-frit must run in a vtium console: /dev/tty: {e}"),
            );
        }
    };
    let answer = Raw::new(&tty).and_then(|_raw| {
        let deadline = now().checked_add(WAIT).unwrap_or_else(now);
        if !is_vtium(&mut &tty, deadline)? {
            return Ok(None);
        }
        let deadline = now().checked_add(WAIT).unwrap_or_else(now);
        switch(&mut &tty, name, deadline).map(Some)
    });
    match answer {
        Ok(None) => {
            let _ = writeln!(
                out,
                "failed\t{name}\tthis terminal is not a vtium console, so nothing was sent"
            );
            1
        }
        Ok(Some(Answer::Named(n))) if n == *name => {
            let _ = writeln!(out, "set\t{name}");
            0
        }
        Ok(Some(Answer::Named(n))) => {
            let _ = writeln!(
                out,
                "failed\t{name}\tthe console kept font {n}; its log says why"
            );
            1
        }
        Ok(Some(Answer::Silent)) => {
            let _ = writeln!(
                out,
                "failed\t{name}\tno reply: allow-font-ops is off in this console"
            );
            1
        }
        Err(e) => fail(err, &format!("/dev/tty: {e}")),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// A frits' directory with a set in it and the catalog's sets in their
    /// category folders, under the test's own scratch path.
    fn catalog(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vtium-config-switch-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, name) in [
            ("console", "terminus"),
            ("code", "both"),
            ("small", "both"),
            ("code", "mine"),
            ("", "mine"),
            ("elsewhere", "stray"),
            ("layers", "layered"),
        ] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            std::fs::write(
                dir.join(folder).join(format!("{name}.vtfrit")),
                "source=kbd:x\n",
            )
            .unwrap();
        }
        dir
    }

    // A set is found by its name in the frits' directory itself, then in
    // each category folder in the catalog's order. A folder that is not a
    // category, and the layers' folder, hold no set.
    //
    // References:
    // - ADR-1204 D9: the category folders and the order of the search.
    #[test]
    fn a_set_is_found_in_the_frits_directory_and_then_in_its_category_folder() {
        let dir = catalog("found");
        let found = |name| frit_file(&dir, name);
        assert_eq!(found("terminus"), Some(dir.join("console/terminus.vtfrit")));
        assert_eq!(found("mine"), Some(dir.join("mine.vtfrit")));
        assert_eq!(found("both"), Some(dir.join("code/both.vtfrit")));
        assert_eq!(found("stray"), None);
        assert_eq!(found("layered"), None);
        assert_eq!(found("absent"), None);
        assert_eq!(found("../console/terminus"), None);
        let listed: Vec<(String, PathBuf)> = frit_files(&dir);
        assert_eq!(
            listed,
            [
                ("both".to_owned(), dir.join("code/both.vtfrit")),
                ("mine".to_owned(), dir.join("mine.vtfrit")),
                ("terminus".to_owned(), dir.join("console/terminus.vtfrit")),
            ]
        );
        assert_eq!(
            category_file(&dir, "mine"),
            Some(dir.join("code/mine.vtfrit"))
        );
        assert_eq!(
            category_file(&dir, "terminus"),
            Some(dir.join("console/terminus.vtfrit"))
        );
        assert_eq!(category_file(&dir, "absent"), None);
    }

    /// A terminal that answers the query with `name`, and DA1, once it has
    /// read the request, the query and DA1.
    struct Answering {
        name: &'static [u8],
        written: Vec<u8>,
    }

    impl Write for Answering {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Answering {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let reply = [b"\x1b]50;", self.name, b"\x07\x1b[?62;4c"].concat();
            let n = reply.len().min(buf.len());
            buf.get_mut(..n)
                .unwrap()
                .copy_from_slice(reply.get(..n).unwrap());
            Ok(n)
        }
    }

    #[test]
    fn the_request_and_the_query_are_written_and_the_reply_read() {
        let mut t = Answering {
            name: b"tiny",
            written: Vec::new(),
        };
        let deadline = now() + WAIT;
        assert_eq!(
            switch(&mut t, "tiny", deadline).unwrap(),
            Answer::Named("tiny".into())
        );
        assert_eq!(t.written, b"\x1b]50;tiny\x1b\\\x1b]50;?\x1b\\\x1b[c");
    }

    #[test]
    fn with_no_reply_the_wait_ends() {
        let mut silent = io::Cursor::new(Vec::new());
        let deadline = now() + Duration::from_millis(50);
        assert_eq!(
            switch(&mut silent, "tiny", deadline).unwrap(),
            Answer::Silent
        );
    }
}
