//! The glyph helper's client (ADR-1301 D2): the console starts
//! `vtium-config serve` on its first need for a frit's glyphs, and takes
//! each answer as it arrives without ever waiting on it.
//!
//! It asks under the operator's fill model (2026-10-28). An ask names the
//! characters and the clusters a cell shows that the store does not hold,
//! has not been told the frit lacks, and has not already asked for; the
//! helper answers it ahead of anything else, each cluster shaped and drawn
//! whole (ADR-1200 D21). The console's record of each cell keys a glyph as
//! the store does (ADR-1200 D20), a cluster by its whole text, in each
//! style and script. An ask names one style, regular, bold, italic or
//! bold italic, and one script, normal, superscript or subscript, and the
//! console sends an ask for each style and script the screen shows that
//! needs one (ADR-1200 D21, ADR-1202 D11); the helper draws it from the
//! frit's face for that style or synthesises it, and makes a superscript
//! or subscript glyph small from the source that draws the normal glyph.
//! A fill is regular and normal, and fills a cell's coverage while nothing
//! is asked: the screen's characters, then printable Latin, punctuation,
//! arrows, box drawing and geometric shapes, then the rest, a batch at a
//! time, a waiting ask answered before the next batch. The fill stops at
//! the store's limit ([`crate::deferred`]), so speculative glyphs never
//! churn the store; only an ask past the limit evicts, compacting the
//! store by the overrun, the glyphs least recently shown leaving, and an
//! evicted glyph is asked for again when the screen shows it. Once the
//! helper has answered a cell's first asks, the console asks once for bold
//! printable ASCII there while the store is under its limit, because the
//! fill brings no bold glyph ([`BOLD_EARLY`]).
//!
//! The helper runs with an empty environment in `/`, in its own process
//! group, holding only its pipes, as `nobody` when the console is root
//! (ADR-1400 D2's last account before login), and sandboxes itself. The
//! console reads that drop back from the helper's `/proc/<pid>/status`
//! before it sends a request, and refuses a helper that holds another
//! uid or gid or any supplementary group (`Launcher::dropped`). A
//! thread writes the requests and another reads the answers, so neither
//! a slow nor a hung helper stalls the loop; the reader wakes the loop
//! after each answer. A third reaps the helper as it exits, ending first
//! each process it started in its process group, so none holds its
//! output past it. Each frame is
//! checked before it is trusted: its length against [`FRAME_BYTES_MAX`]
//! before anything is allocated, a set through vtium-font's reader, which
//! bounds every field, every strike at a cell asked for and in the style
//! and the script asked for, regular and normal for a fill, an ask's glyphs and clusters among those it asked for and not yet
//! sent, and a fill's among those the cell does not hold, with no cluster. The helper says when it takes
//! each stop, and a cell's fill frames before that belong to the fill
//! stopped and are dropped, so a cell filled again at once takes only its
//! new fill.
//!
//! A warning the frit's text gave as the helper read it, such as a key that
//! does nothing, is logged once a boot, at most [`WARNINGS_MAX`] of them,
//! and past that bound one line says that no more are logged.
//!
//! Before its first answer, the helper reports which parts of its sandbox
//! are in force, and why each other part is not. The console logs one
//! line once a boot when a part is missing, naming each missing part and
//! its reason, and whether the fallback filter stands in for the landlock
//! domain (`sandbox_notice`). The setting `helper-sandbox=off`, from
//! vtium.conf or vtium's command line, reaches each helper as `--sandbox
//! off` ([`sandbox_args`]).
//!
//! References:
//! - ADR-1200 D15: the sandbox report, its reasons and the setting.
//!
//! A helper that answers what is not a frame, what was not asked, or a
//! failure is given up for the boot, logged once, and the built-in font
//! goes on drawing what it has not sent, its clusters unshaped, a glyph a
//! codepoint (ADR-1301 D1). A helper that crashes, reaches its CPU
//! limit, gives no answer for [`ANSWER_MS`], leaves an ask unanswered for
//! [`REQUEST_MS`], or exits with work outstanding fails one source of the
//! font set and nothing else. vtium finds that source by starting the
//! helper again with the font set's sources added one at a time, through
//! the helper's leave-out request, and taking no answer from a helper that
//! reads only some of them. It then leaves the source out for the rest of the boot, logs
//! once which source and why, and starts the helper again without it, its
//! asks and fills sent again, so the other sources go on drawing, with
//! shaping. The whole font set falls back to the built-in font when every
//! source has failed, when the helper fails with no source to read, when
//! it cannot start, or when it fails again after its [`RESTARTS_MAX`]
//! chances a boot, which the searches and the plain restarts share
//! (`Helper::restart`). A helper that exits 0 with nothing outstanding
//! left at its idle interval, and the next need starts another.
//!
//! A frit that states no cell takes the cell its first source gives
//! (the operator, 2026-10-28; the maintainer, 2026-11-01), which the
//! console asks the helper for at the height it draws at
//! ([`Helper::own_cell`]): a bitmap source's own cell, whatever the
//! height, or an outline source's cell at that height. The console waits
//! for the answer at most [`OWN_CELL_MS`] before the first frame and at a
//! zoom of an outline frit. For a frit whose first source is a bitmap font
//! with strikes, the asks of one zoom step, or of one text area request,
//! share one such wait. Nothing else in the console waits on the helper.
//! An answer that
//! comes after the wait is kept, with the height its query asked at, and
//! [`Helper::late_cells`] hands it on (ADR-1200 D17, the maintainer,
//! 2026-11-03).
//!
//! The wire format is `vtium-config`'s serve mode's (`serve.rs`), whose
//! constants this module repeats.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as Process, ExitStatus, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use vtium_term::ClusterWidth;

use rustix::fs::{FileType, Mode, OFlags};
use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, kill_process_group, pidfd_open,
    pidfd_send_signal, waitid,
};
use vtium_font::set::{Set, SetFace};
use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Doubled, Script, Variant};
use vtium_platform::vt::font::ConsoleFont;

use vtium_settings::batch::{Batch, Entry};

use crate::config::Config;
use crate::config::keys::{FONT_HELPER, HELPER_SANDBOX};
use crate::deferred::{Shared, Shown, Text};
use crate::pointer_file::{FRAME_NO_POINTER, FRAME_POINTER};

/// How long the helper may take for each answer while work is
/// outstanding: a CJK frit's first batch on a Cortex-A53 takes about a
/// second.
pub const ANSWER_MS: u64 = 10_000;
/// How long one ask may take whole, however often frames come.
pub const REQUEST_MS: u64 = 60_000;
/// How many chances a boot gives the helper after it fails. Each search
/// for the source that made the helper fail uses one when it begins,
/// whatever its end, however many times it starts the helper. So does
/// each restart that looks for no source: a restart of a helper with no
/// source, such as the pointer file's, and a restart after the helper
/// crashed, hung or reached its CPU limit with no work outstanding. A
/// failure past the last chance gives the font set up, and the built-in
/// font draws. The helper started after vtium leaves a source out uses
/// no chance.
///
/// References:
/// - ADR-1200 D15, "Restarted after a failure", and its note of
///   2026-11-21: each start re-randomizes the helper's address layout, so
///   an unbounded count would let an exploit retry until its guess lands,
///   and three chances a boot bound the searches and the plain restarts
///   together.
/// - ADR-1200 D14: a crash, a fault or a timeout fails one source and
///   nothing else.
pub const RESTARTS_MAX: u32 = 3;
/// The most bytes one frame's payload holds, the helper's
/// `FRAME_BYTES_MAX`.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// The most bytes of a failure's reason kept.
pub const REASON_BYTES_MAX: u32 = 4096;
/// The most codepoints one request names, the helper's `FIRST_MAX`, and the
/// most clusters.
pub const FIRST_MAX: usize = 32_000;
/// The most clusters a cell's record of those the frit lacks holds: past it
/// the record is emptied, and a cluster shown again is asked for again.
// A program can write clusters without end, each different; a screen shows at most FIRST_MAX, and 4,096 is a screen's worth of distinct ones on minspec's 80x25.
pub const CLUSTERS_LACKED_MAX: usize = 4096;
/// The most clusters a cell has asked for and not been answered: past it a
/// cluster shown draws unshaped, and is asked for on a later change.
pub const CLUSTERS_WAITING_MAX: usize = CLUSTERS_LACKED_MAX;
/// The most asks the helper has not answered whole: past it nothing more
/// is asked, and what the screen shows is asked for on a later change, so
/// a program that writes faster than the helper answers grows no queue.
// One ask a cell a turn, a few cells at most; 64 is many turns of a busy screen behind.
pub const ASKS_MAX: usize = 64;
/// The characters the console asks for in bold at a cell once the helper
/// has answered every ask sent at that cell: printable ASCII, U+0021 to
/// U+007E. A program's interface shows its bold text in these characters
/// most often, and the background fill draws regular glyphs alone, so
/// without this ask each bold character would wait for the screen to show
/// it.
pub const BOLD_EARLY: (char, char) = ('!', '~');
/// The most warnings about a frit logged a boot: those its text gives,
/// each source the helper refuses, and each font it needs that is missing.
/// A helper that sends more is not given up. The console logs once that it
/// logs no more of them, and the rest are not logged.
// A frit gives a warning a line at most, and a frit worth drawing has few lines that do nothing and few fonts this machine lacks; the bound keeps a helper that sends warnings without end out of the log, and `vtium check-config` and `vtium config fonts` name every missing font whatever the bound.
pub const WARNINGS_MAX: usize = 16;
/// How long the console waits before its first frame for a frit's own
/// cell.
// The helper starts and reads a PSF well inside it on minspec, and a start that waits longer stalls visibly (the operator, 2026-10-28).
pub const OWN_CELL_MS: u64 = 100;

thread_local! {
    /// The wait a [`CellWait`] sets for the cell queries asked on this
    /// thread, in place of the console's.
    static CELL_WAIT: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// While it lives, each cell query asked on the thread that made it waits
/// up to its bound for the answer, in place of the wait the console asks
/// for ([`OWN_CELL_MS`]). A test whose helper answers in time holds one, so
/// that the test judges the answer and not how fast the host starts and
/// runs the helper; the bound only makes a helper that never answers fail
/// the test instead of hanging it. The console never makes one. A test of
/// the wait itself, or of an answer that comes after it, holds none.
#[must_use = "the wait goes back to the console's when the guard drops"]
pub struct CellWait(Option<Duration>);

impl CellWait {
    /// Waits up to `bound` for each cell answer on this thread until the
    /// guard drops.
    pub fn new(bound: Duration) -> CellWait {
        CellWait(CELL_WAIT.with(|w| w.replace(Some(bound))))
    }
}

impl Drop for CellWait {
    fn drop(&mut self) {
        CELL_WAIT.with(|w| w.set(self.0));
    }
}

const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQA";
/// Why the console gives the helper up when the two read different layouts
/// of their format: a helper of an older build refuses this console's
/// request as "not a serve request", and an answer of an older layout does
/// not read. The console and the font helper are installed together, so
/// reinstalling vtium mends it.
const OTHER_BUILD: &str = "the console and the font helper come from different builds of vtium, so they cannot read each other's messages; reinstall vtium so that both come from one build";

/// The reason the helper failed, as the console logs it: a helper of an
/// older build's refusal of this console's request says what to do. A
/// helper that does not know the active fonts' request is of an older
/// build too, since the console sends only kinds its own build knows.
fn failure(why: &str) -> String {
    match why.trim() {
        "not a serve request" | "a malformed request: an unknown request kind" => {
            OTHER_BUILD.to_owned()
        }
        why => format!("the helper failed: {why}"),
    }
}
/// Why the helper's seccomp filters are not in force, as the byte of its
/// sandbox report at [`SANDBOX_SECCOMP_SHIFT`] numbers it: the setting is
/// off, vtium has no filter for the machine's architecture, or the kernel
/// offers no seccomp filters. `None` for any other number, 0 included,
/// which names no reason vtium knows.
fn seccomp_reason(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "the setting helper-sandbox=off turns them off",
        2 => "vtium has no seccomp filter for this machine's architecture",
        3 => "this kernel is new enough to offer seccomp but was built without it",
        4 => "this kernel was built with seccomp but without seccomp filters",
        5 => "this kernel is older than Linux 3.8, too old for vtium's seccomp filters",
        _ => return None,
    })
}

/// Why the helper's landlock domain is not in force, as the byte of its
/// sandbox report at [`SANDBOX_DOMAIN_SHIFT`] numbers it: the setting is
/// off, or the kernel is too old for landlock, was built without it, or
/// did not turn it on at boot. `None` for any other number, 0 included.
fn domain_reason(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "the setting helper-sandbox=off turns it off",
        2 => "this kernel is older than Linux 5.13, which added landlock",
        3 => "this kernel was built without landlock",
        4 => {
            "this kernel did not turn landlock on at boot, as when the boot's lsm= list leaves it out"
        }
        _ => return None,
    })
}

/// The end of a sentence that says a part of the helper's sandbox is
/// missing: the reason `why`, or, for a number vtium does not know, that
/// number, so the line never names a reason the helper did not give.
fn because(why: Option<&str>, code: u32) -> String {
    match why {
        Some(why) => format!(", because {why}."),
        None => format!(
            ". The font helper's report gives the reason as number {code}, which vtium does not know."
        ),
    }
}

/// The line the console logs when the helper's report of its sandbox,
/// `report`, lacks a part: each part not in force and the reason the
/// helper gives, and whether the fallback filter stands in for the
/// landlock domain. `None` when every part is in force.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": the line
///   and the reasons it tells apart.
fn sandbox_notice(report: u32) -> Option<String> {
    let parts = report & SANDBOX_PARTS;
    let seccomp = report >> SANDBOX_SECCOMP_SHIFT & 0xff;
    let domain = report >> SANDBOX_DOMAIN_SHIFT & 0xff;
    if parts == 0 && seccomp == 1 && domain == 1 {
        return Some(
            "The font helper runs without its seccomp filters and its landlock domain, because the setting helper-sandbox=off turns them off. A font that took over the font helper could open network connections and change any file that the font helper's account may change.".to_owned(),
        );
    }
    let domain = (parts & SANDBOX_DOMAIN == 0).then(|| {
        let stands_in = if parts & SANDBOX_FALLBACK != 0 {
            "A seccomp filter refuses in its place the file, signal and tracing calls that the domain refuses."
        } else {
            "No filter takes the domain's place, so a font that took over the font helper could change any file that the font helper's account may change."
        };
        format!(
            "The font helper runs without its landlock domain{} {stands_in}",
            because(domain_reason(domain), domain)
        )
    });
    let sockets = (parts & SANDBOX_SOCKETS == 0).then(|| {
        format!(
            "The font helper runs without its seccomp filters{}",
            because(seccomp_reason(seccomp), seccomp)
        )
    });
    let lines: Vec<String> = domain.into_iter().chain(sockets).collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// Whether this boot has logged a font helper's sandbox line: one line a
/// boot, whichever helper reports first, for every font set and user.
static SANDBOX_LOGGED: AtomicBool = AtomicBool::new(false);

/// Logs `line` unless `boot` says a helper's sandbox line was logged
/// already, and marks it logged; whether it logged.
fn log_sandbox_once(boot: &AtomicBool, line: &str) -> bool {
    let first = !boot.swap(true, Ordering::Relaxed);
    if first {
        log::warn!("{line}");
    }
    first
}

/// Whether the helpers lock themselves in with their seccomp filters and
/// landlock domains: `config`'s `helper-sandbox`, on unless it is `off`.
///
/// References:
/// - ADR-1200 D15, "A setting turns the sandbox off": the key, on by
///   default, which only root's vtium.conf and vtium's command line set.
pub fn sandbox_on(config: &Config) -> bool {
    config.text(&HELPER_SANDBOX) != Some("off")
}

/// The options that tell the font helper `helper-sandbox=off`, `--sandbox
/// off`; none when the sandbox is on, since a helper started without the
/// option sandboxes itself in full.
pub fn sandbox_args(config: &Config) -> Vec<OsString> {
    if sandbox_on(config) {
        Vec::new()
    } else {
        vec!["--sandbox".into(), "off".into()]
    }
}

/// An ask. A console built with right-to-left text asks with the kind that
/// also takes the part of a cluster the font draws only without some of
/// its marks (ADR-1200 D22 step 3), which its lookup lays the missing marks
/// over; the helper sends a console of the minspec build none.
#[cfg(not(feature = "bidi"))]
const ASK: u32 = 1;
#[cfg(feature = "bidi")]
const ASK: u32 = 6;
const FILL: u32 = 2;
const STOP: u32 = 3;
const OWN_CELL: u32 = 4;
const SOURCES: u32 = 5;
const COVERAGE: u32 = 8;
/// The active fonts, which the console sends first to each helper it
/// starts ([`Launcher::with_active`]).
const ACTIVE: u32 = 10;
/// The sources the helper leaves out, a bit for each `source=` line.
const LEAVE_OUT: u32 = 12;
/// The most bytes the paths of the active fonts hold together, the
/// helper's bound: the batch file's own.
const ACTIVE_BYTES_MAX: usize = vtium_settings::batch::BATCH_MAX;
/// The longest path the active fonts name, the helper's bound.
const ACTIVE_PATH_MAX: usize = 4096;
const FRAME_GLYPHS: u32 = 1;
const FRAME_DONE: u32 = 2;
const FRAME_FAILED: u32 = 3;
const FRAME_FILL: u32 = 4;
const FRAME_FILLED: u32 = 5;
const FRAME_OWN_CELL: u32 = 6;
const FRAME_NO_OWN_CELL: u32 = 7;
const FRAME_STOPPED: u32 = 8;
const FRAME_WARNING: u32 = 9;
const FRAME_OUTLINE_CELL: u32 = 10;
const FRAME_SOURCES: u32 = 11;
const FRAME_DRAWS_NOTHING: u32 = 12;
const FRAME_SANDBOX: u32 = 14;
const FRAME_COVERAGE: u32 = 15;
/// The bits of a [`FRAME_SANDBOX`]: the helper's landlock domain, its
/// fallback filter and its socket filter are in force.
const SANDBOX_DOMAIN: u32 = 1;
const SANDBOX_FALLBACK: u32 = 2;
const SANDBOX_SOCKETS: u32 = 4;
const SANDBOX_PARTS: u32 = 0xff;
/// Where a [`FRAME_SANDBOX`] carries why the seccomp filters are not in
/// force ([`seccomp_reason`]) and why the landlock domain is not
/// ([`domain_reason`]), a byte each.
const SANDBOX_SECCOMP_SHIFT: u32 = 8;
const SANDBOX_DOMAIN_SHIFT: u32 = 16;

/// `SIGXCPU`, which an exhausted `RLIMIT_CPU` sends.
const SIGXCPU: i32 = 24;

/// The helper's program vtium is built with, `font-helper`'s default, set
/// at build time with `VTIUM_FONT_HELPER`: the key table's.
pub const PROGRAM: &str = vtium_settings::keys::HELPER_PROGRAM;

/// The account the helper runs as when the console is root: `nobody`,
/// its uid and gid read from `passwd`.
pub fn nobody(passwd: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some("nobody")).then_some(())?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        (uid != 0 && gid != 0).then_some((uid, gid))
    })
}

/// How to start the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The uid and gid to run as; `None` keeps the console's.
    pub account: Option<(u32, u32)>,
}

impl Command {
    /// `program serve`, as `nobody` when the console is root; an error as
    /// root with no `nobody` in `/etc/passwd`, since the helper never runs
    /// as root.
    pub fn system(program: &Path) -> Result<Command, String> {
        let account = if rustix::process::geteuid().is_root() {
            let passwd = std::fs::read_to_string("/etc/passwd").map_err(|e| {
                format!("cannot read /etc/passwd to find the font helper's account: {e}")
            })?;
            Some(
                nobody(&passwd)
                    .ok_or("/etc/passwd has no user nobody to run the font helper as")?,
            )
        } else {
            None
        };
        Ok(Command {
            program: program.to_owned(),
            args: vec!["serve".into()],
            account,
        })
    }
}

/// The helper's program, opened once, and the command each start runs it
/// with. Every helper starts from the opened file, so a package upgrade
/// that renames a new program into place reaches a console only when the
/// console starts again, and an old console never talks to a new helper.
#[derive(Clone, Debug)]
pub struct Launcher {
    command: Command,
    program: Arc<OwnedFd>,
    /// The active fonts' request, which each helper is sent first; `None`
    /// sends none, for a test's stand-in helper.
    active: Option<Arc<[u8]>>,
}

/// The request that names the active fonts to the helper: `owner`, whose
/// files pass the trust rule beside root's, and each path `batch` lists,
/// in the helper's layout (vtium-config's `serve.rs`). The helper refuses
/// the whole request over one path that is not absolute UTF-8, holds a NUL
/// byte or is too long, and then exits, so such a path is left out here
/// and the other active fonts still reach the helper. The paths stop
/// before they would pass the helper's bound, which a batch file within
/// its own bound never reaches.
fn active_request(owner: u32, batch: &Batch) -> Vec<u8> {
    let mut paths = Vec::new();
    let mut total = 0usize;
    for Entry::Path(path) in batch.entries() {
        let Some(text) = path
            .to_str()
            .filter(|t| t.starts_with('/') && !t.contains('\0') && t.len() <= ACTIVE_PATH_MAX)
        else {
            continue;
        };
        total = total.saturating_add(text.len());
        if total > ACTIVE_BYTES_MAX {
            break;
        }
        paths.push(text);
    }
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, ACTIVE);
    put32(&mut out, owner);
    put32(&mut out, len32(paths.len()));
    for path in paths {
        put32(&mut out, len32(path.len()));
        out.extend_from_slice(path.as_bytes());
    }
    out
}

impl Launcher {
    /// Opens `command`'s program ([`open_program`]), for the account vtium
    /// runs as.
    pub fn open(command: Command) -> Result<Launcher, String> {
        let me = rustix::process::geteuid().as_raw();
        let program = open_program(&command.program, me)?;
        Ok(Launcher {
            command,
            program: Arc::new(program),
            active: None,
        })
    }

    /// The helper `config` names, `font-helper`, opened, with the font
    /// folders `font-paths` lists in place of the build's, and told the
    /// active fonts: the batch of the console's gate
    /// ([`crate::font::gate`]), under the account vtium runs as.
    ///
    /// References:
    /// - ADR-1200 D25: `font-paths` names the font folders.
    /// - ADR-1205 D4: the helper draws only from the active fonts.
    pub fn configured(config: &Config) -> Result<Launcher, String> {
        let mut command = Command::system(&program(config))?;
        command.args.extend(font_dirs(config));
        command.args.extend(sandbox_args(config));
        let me = rustix::process::geteuid().as_raw();
        Launcher::open(command).map(|l| l.with_active(me, &crate::font::gate().batch))
    }

    /// This launcher, telling each helper it starts that the fonts `batch`
    /// lists are the active ones, and that `owner`'s files pass the trust
    /// rule beside root's. `owner` is the account vtium runs as: root, or
    /// the account of a test or a run by hand, which the helper runs as
    /// too. The helper draws from no other font file.
    ///
    /// References:
    /// - ADR-1205 D4: vtium draws only from the active resources, and an unprivileged vtium accepts its own account's files.
    pub fn with_active(mut self, owner: u32, batch: &Batch) -> Launcher {
        self.active = Some(active_request(owner, batch).into());
        self
    }

    /// The request naming the active fonts that each helper this launcher
    /// starts is sent first; `None` when it sends none.
    pub fn active(&self) -> Option<&[u8]> {
        self.active.as_deref()
    }

    /// This launcher, starting each helper with the console's `locale`
    /// (`--locale`), so that the helper picks the built-in font the console
    /// draws, which in a zh, ja or ko locale is Unifont at every height.
    ///
    /// References:
    /// - ADR-1200 D7: the built-in font for a cell height and locale.
    /// - ADR-1204 D5: the built-in font draws the text of a set whose text
    ///   font is missing.
    pub fn with_locale(mut self, locale: &str) -> Launcher {
        self.command
            .args
            .extend([OsString::from("--locale"), OsString::from(locale)]);
        self
    }

    /// The arguments each helper starts with, for a test.
    #[cfg(test)]
    pub(crate) fn args(&self) -> &[OsString] {
        &self.command.args
    }

    /// The process that starts a helper: the opened program, through
    /// `/proc/self/fd`, which the kernel lets a process read as any
    /// account, with the program's path as its name.
    fn process(&self) -> Process {
        let c = &self.command;
        let mut process = Process::new(format!("/proc/self/fd/{}", self.program.as_raw_fd()));
        process
            .arg0(&c.program)
            .args(&c.args)
            .env_clear()
            .current_dir("/")
            .process_group(0);
        if let Some((uid, gid)) = c.account {
            // std drops the supplementary groups before setuid when root
            process.uid(uid).gid(gid);
        }
        process
    }

    /// Checks that the helper started as `pid` holds the account the
    /// console dropped it to, read back from its `/proc/<pid>/status` once
    /// it runs. std drops the supplementary
    /// groups before it sets the uid, but goes on when the kernel refuses
    /// it that, so a console without the capability to change its groups
    /// would start a helper holding root's groups. A helper whose four
    /// uids are not the account's uid, whose four gids are not its gid, or
    /// that holds any supplementary group is refused. A console that keeps
    /// its own account checks nothing, since the helper holds what the
    /// console holds.
    fn dropped(&self, pid: u32) -> Result<(), String> {
        let Some(account) = self.command.account else {
            return Ok(());
        };
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).map_err(|e| {
            format!("vtium cannot read the font helper's ids from /proc/{pid}/status: {e}")
        })?;
        holds_account(&status, account)
    }

    /// Why a start failed with `e`. vtium starts the helper from the file it
    /// opened, through `/proc/self/fd`, so the message says `/proc` must be
    /// mounted: with `/proc` unmounted the start fails there, not at the
    /// program's path.
    fn failed(&self, e: &std::io::Error) -> String {
        format!(
            "vtium cannot start the font helper {}: {e}. vtium starts it through /proc/self/fd, so /proc must be mounted.",
            self.command.program.display(),
        )
    }
}

/// The ids a `/proc/<pid>/status` line names after its `key`, such as
/// `Uid:`'s real, effective, saved and filesystem uids, or `Groups:`'s
/// supplementary groups; `None` when the line is missing or a field is not
/// a number.
fn status_ids(status: &str, key: &str) -> Option<Vec<u32>> {
    let line = status.lines().find_map(|l| l.strip_prefix(key))?;
    line.split_whitespace().map(|id| id.parse().ok()).collect()
}

/// Whether `status`, a helper's `/proc/<pid>/status`, shows it holding
/// `(uid, gid)` alone: each of its four uids `uid`, each of its four gids
/// `gid`, and no supplementary group. Says why not.
fn holds_account(status: &str, (uid, gid): (u32, u32)) -> Result<(), String> {
    let ids = |key: &str| {
        status_ids(status, key).ok_or_else(|| {
            format!("the helper's ids do not read: its status has no {key} line vtium reads")
        })
    };
    let (uids, gids, groups) = (ids("Uid:")?, ids("Gid:")?, ids("Groups:")?);
    if uids.len() != 4 || uids.iter().any(|&u| u != uid) {
        return Err(format!(
            "the helper runs as uids {uids:?}, not as {uid} alone, the account vtium started it as"
        ));
    }
    if gids.len() != 4 || gids.iter().any(|&g| g != gid) {
        return Err(format!(
            "the helper runs with gids {gids:?}, not with {gid} alone, the account vtium started it as"
        ));
    }
    if !groups.is_empty() {
        return Err(format!(
            "the helper holds the supplementary groups {groups:?}, which vtium could not drop as it started it as uid {uid}"
        ));
    }
    Ok(())
}

/// The options that hand the helper the folders `font-paths` lists, a
/// `--font-dir` before each, in their order; none when it is unset, so
/// the helper searches the build's list (ADR-1200 D25).
pub fn font_dirs(config: &Config) -> Vec<OsString> {
    config
        .text(&crate::config::keys::FONT_PATHS)
        .map(|paths| {
            paths
                .split(':')
                .flat_map(|dir| [OsString::from("--font-dir"), OsString::from(dir)])
                .collect()
        })
        .unwrap_or_default()
}

/// The helper's program `config` names, `font-helper`.
pub fn program(config: &Config) -> PathBuf {
    PathBuf::from(config.text(&FONT_HELPER).unwrap_or(PROGRAM))
}

/// Opens the helper's program at `path` to start it from, close-on-exec
/// so no child keeps it and above stdio so no child's stdio replaces it,
/// and checks the opened file, so the file checked is the file that runs. It refuses a path that is not absolute, a file
/// that is not a regular file, and a file an account other than root or
/// `me`, the account vtium runs as, could change: one another account
/// owns, or that its group or others may write; and a setuid or setgid
/// file, which would run with its owner's or group's rights.
pub fn open_program(path: &Path, me: u32) -> Result<OwnedFd, String> {
    let name = path.display();
    if !path.is_absolute() {
        return Err(format!(
            "vtium does not start the font helper {name}: the path is not absolute"
        ));
    }
    let opened = rustix::fs::open(path, OFlags::PATH | OFlags::CLOEXEC, Mode::empty())
        .map_err(|e| format!("cannot open the font helper {name}: {e}"))?;
    // A descriptor from 0 to 2 would be replaced by a child's stdio before
    // its exec. Rust's runtime opens /dev/null on each of them that vtium
    // starts with closed, so this keeps that from mattering to the helper
    // if it ever stops.
    let file = rustix::io::fcntl_dupfd_cloexec(&opened, 3)
        .map_err(|e| format!("cannot prepare the font helper {name}: {e}"))?;
    drop(opened);
    let stat = rustix::fs::fstat(&file).map_err(|e| {
        format!("cannot read the owner and permissions of the font helper {name}: {e}")
    })?;
    match refusal(
        FileType::from_raw_mode(stat.st_mode),
        stat.st_uid,
        stat.st_mode,
        me,
    ) {
        None => Ok(file),
        Some(why) => Err(format!(
            "vtium does not start the font helper {name}: it {why}"
        )),
    }
}

/// Why vtium refuses a helper program of `kind`, owned by `owner`, with
/// `mode`, when it runs as `me`; `None` when it does not.
fn refusal(kind: FileType, owner: u32, mode: u32, me: u32) -> Option<String> {
    if kind != FileType::RegularFile {
        return Some("is not a regular file".to_owned());
    }
    if mode & 0o6000 != 0 {
        // vtium drops to nobody before the exec, and the helper sets
        // no_new_privs only after it, so a setuid root helper runs as root
        return Some(format!("is setuid or setgid (mode {:o})", mode & 0o7777));
    }
    if owner != 0 && owner != me {
        return Some(format!(
            "belongs to uid {owner}, which is neither root nor vtium's own account"
        ));
    }
    if mode & 0o022 != 0 {
        return Some(format!(
            "can be changed by its group or by other users (mode {:o})",
            mode & 0o7777
        ));
    }
    None
}

/// What glyphs a request is about besides their cell: their variant, a
/// style and a script, and their double size, `None` for the normal size.
type Kind = (Variant, Option<Doubled>);

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A frit as the console hands it to the helper: its name and its text,
/// read from its file or made on the fly (ADR-1200 D9), so the helper
/// needs no file for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FritText {
    pub name: String,
    pub text: String,
}

impl FritText {
    /// The frit at `path` in the frits' directory `frits`, named for its
    /// file less `.vtfrit`, read once the ownership check `trust` makes
    /// passes ([`crate::font::frit_text`]), with the layers of the frits'
    /// directory in place ([`crate::font::with_layers`]). Each sentence
    /// about a `layer=` line that adds nothing is logged once a boot.
    pub fn read(
        path: &Path,
        frits: &Path,
        trust: &vtium_settings::Trust,
    ) -> Result<FritText, String> {
        let (frit, notes) = FritText::read_noting(path, frits, trust)?;
        for note in &notes {
            crate::font::log_layer_note(note);
        }
        Ok(frit)
    }

    /// The frit at `path`, as [`FritText::read`] reads it, and the
    /// sentences about its `layer=` lines that add nothing, which the
    /// caller reports.
    pub fn read_noting(
        path: &Path,
        frits: &Path,
        trust: &vtium_settings::Trust,
    ) -> Result<(FritText, Vec<String>), String> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n))
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("there is no font set named {}", path.display()))?;
        let text = crate::font::frit_text(path, trust)
            .map_err(|why| crate::font::refusal(name, path, &why))?;
        let (text, notes) = crate::font::with_layers(name, frits, &text, trust);
        Ok((
            FritText {
                name: name.to_owned(),
                text,
            },
            notes,
        ))
    }
}

/// One request's bytes: `kind` at `cell`, whose line spacing adds `band`
/// (ADR-1302 D13), in `variant`, a style and a script, at the double size
/// `doubled` or the normal size, for `frit`, naming
/// `codepoints` and
/// `clusters` (each cut at [`FIRST_MAX`]) and `held`, with the kernel's
/// font. The display's capabilities are reserved, and sent empty.
fn encode(
    frit: &FritText,
    console: Option<&ConsoleFont>,
    (kind, cell, band, (variant, doubled), rule): (u32, Cell, Band, Kind, ClusterWidth),
    (codepoints, clusters): (&[u32], &[Text]),
    held: &[(u32, u32)],
) -> Vec<u8> {
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, kind);
    for field in [&frit.name, &frit.text] {
        put32(&mut out, len32(field.len()));
        out.extend_from_slice(field.as_bytes());
    }
    put32(&mut out, cell.width);
    put32(&mut out, cell.height);
    put32(&mut out, band.above);
    put32(&mut out, band.below);
    let style = variant.style;
    put32(
        &mut out,
        u32::from(style.bold) | u32::from(style.italic) << 1,
    );
    put32(
        &mut out,
        match variant.script {
            Script::Baseline => 0,
            Script::Super => 1,
            Script::Sub => 2,
        },
    );
    put32(
        &mut out,
        match rule {
            ClusterWidth::Double => 0,
            ClusterWidth::Single => 1,
        },
    );
    put32(
        &mut out,
        match doubled {
            None => 0,
            Some(Doubled::Width) => 1,
            Some(Doubled::Height) => 2,
        },
    );
    put32(&mut out, 0);
    let codepoints = codepoints.get(..FIRST_MAX).unwrap_or(codepoints);
    put32(&mut out, len32(codepoints.len()));
    for &c in codepoints {
        put32(&mut out, c);
    }
    let clusters = clusters.get(..FIRST_MAX).unwrap_or(clusters);
    put32(&mut out, len32(clusters.len()));
    for cluster in clusters {
        put32(&mut out, len32(cluster.len()));
        for &c in cluster {
            put32(&mut out, u32::from(c));
        }
    }
    put32(&mut out, len32(held.len()));
    for &(a, b) in held {
        put32(&mut out, a);
        put32(&mut out, b);
    }
    match console {
        None => put32(&mut out, 0),
        Some(f) => {
            put32(&mut out, 1);
            for v in [f.width, f.height, f.charcount, len32(f.data.len())] {
                put32(&mut out, v);
            }
            out.extend_from_slice(&f.data);
            put32(&mut out, len32(f.unimap.len()));
            for p in &f.unimap {
                out.extend_from_slice(&p.unicode.to_le_bytes());
                out.extend_from_slice(&p.fontpos.to_le_bytes());
            }
        }
    }
    out
}

/// The cell a frit's first source gives, a cell query's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnCell {
    /// A bitmap source's own cell, which the console takes at the largest
    /// whole multiple not over the height it draws at.
    Bitmap(Cell),
    /// An outline source's cell at the height asked, taken as it is.
    Outline(Cell),
}

/// Why a cell query has no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoCell {
    pub why: String,
    /// No answer came in time, as opposed to an answer that says there is
    /// none, which the frit's text decides and asking again cannot change.
    pub late: bool,
    /// The helper answered that the frit's fonts give no cell at the
    /// height asked, as below the shortest strike of a bitmap font with
    /// strikes. False when the helper could not answer: it failed, ended,
    /// is not running, sent a malformed answer, or answered too late.
    pub from_frit: bool,
}

impl NoCell {
    /// The helper could not answer, for `why`.
    fn refused(why: String) -> NoCell {
        NoCell {
            why,
            late: false,
            from_frit: false,
        }
    }

    /// The helper answered that there is no cell, for `why`.
    fn answered(why: String) -> NoCell {
        NoCell {
            why,
            late: false,
            from_frit: true,
        }
    }
}

impl std::fmt::Display for NoCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

/// What the frame `answer` says to a cell query asked at `height`: the
/// cell, or the helper's refusal. The error is why the frame is malformed,
/// an outline cell of another height. `answer` is a cell frame's event.
fn cell_answer(answer: Event, height: u32) -> Result<Result<OwnCell, NoCell>, String> {
    match answer {
        Event::OwnCell(cell) => Ok(Ok(OwnCell::Bitmap(cell))),
        Event::OutlineCell(cell) if cell.height == height => Ok(Ok(OwnCell::Outline(cell))),
        Event::OutlineCell(cell) => Err(format!(
            "an outline cell {} pixels high, where {height} was asked",
            cell.height
        )),
        Event::NoOwnCell(why) => Ok(Err(NoCell::answered(why.trim().to_owned()))),
        _ => Err("a frame that answers no cell query".to_owned()),
    }
}

/// What the reader thread saw.
#[derive(Debug)]
pub(crate) enum Event {
    /// A set, an ask's or a fill's, the codepoints it maps as `(first,
    /// count)` runs, and the bytes its frame took.
    Glyphs {
        fill: bool,
        /// The rows line spacing adds to the set's cell (ADR-1302 D13).
        band: Band,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    },
    /// The oldest ask is answered.
    Done,
    /// A cell's fill is done, at its band.
    Filled(SpacedCell),
    /// A stop at a cell and band is taken: the frames of the fill it ended
    /// are all sent.
    Stopped(SpacedCell),
    /// A bitmap frit's own cell, a cell query's answer.
    OwnCell(Cell),
    /// An outline frit's cell at the height asked, a cell query's answer.
    OutlineCell(Cell),
    /// The frit has no own cell: why.
    NoOwnCell(String),
    /// A warning the frit's text gave as the helper read it.
    Warning(String),
    /// The parts of the helper's sandbox in force, as [`FRAME_SANDBOX`]'s
    /// bits.
    Sandbox(u32),
    /// A coverage query's answer: the letters asked that the frit covers,
    /// and an installed font that covers the most of the rest, empty when
    /// none does.
    Coverage(Vec<u32>, String),
    /// A sources query's answer: one line for each source the helper left
    /// out and for each font that differs from what its source needs.
    Sources(String),
    /// The frit draws nothing, a coverage or sources query's answer: why.
    DrawsNothing(String),
    /// A frame that answers a pointer ask: its kind, a picture or a shape
    /// drawn nothing for, and its payload, which
    /// [`crate::pointer_file::Reply`] checks.
    Pointer(u32, Vec<u8>),
    Failed(String),
    /// Bytes that are not a frame, or a set its reader refused.
    Malformed(String),
    /// The helper closed its end and was reaped: how it exited.
    Closed(Option<ExitStatus>),
}

/// Reads one frame's head: its kind and length, or `None` at the end of
/// the stream. An end inside the head is the end of the stream too: the
/// helper has closed its output, as when the kernel ended it while it
/// wrote, and how it exited says why.
fn head(r: &mut impl Read) -> Result<Option<(u32, u32)>, String> {
    let mut b = [0u8; 8];
    let mut got = 0;
    while got < b.len() {
        let rest = b.get_mut(got..).unwrap_or_default();
        match r.read(rest) {
            Ok(0) => return Ok(None),
            Ok(n) => got = got.saturating_add(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("cannot read the helper's answer: {e}")),
        }
    }
    let [k0, k1, k2, k3, n0, n1, n2, n3] = b;
    Ok(Some((
        u32::from_le_bytes([k0, k1, k2, k3]),
        u32::from_le_bytes([n0, n1, n2, n3]),
    )))
}

/// The codepoints a set maps, as `(first, count)` runs, read from its
/// `CMAP` section as vtium-font's format lays it out: every offset and
/// count is checked against the bytes, so a malformed set is an error,
/// and the set's reader checks the rest.
fn mapped_runs(bytes: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let bad = || "glyphs whose map vtium cannot read".to_owned();
    let field = |at: usize, n: usize| bytes.get(at..at.checked_add(n)?);
    let u32_at = |at: usize| {
        field(at, 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let u64_at = |at: usize| {
        field(at, 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
    };
    let usize_of = |v: u64| usize::try_from(v).ok();
    let sections = u32_at(12).ok_or_else(bad)?;
    let (mut off, mut len) = (None, None);
    for i in 0..sections {
        let entry = usize_of(u64::from(i))
            .and_then(|i| i.checked_mul(20))
            .and_then(|e| e.checked_add(16))
            .ok_or_else(bad)?;
        if field(entry, 4).ok_or_else(bad)? == b"CMAP" {
            off = entry.checked_add(4).and_then(u64_at).and_then(usize_of);
            len = entry.checked_add(12).and_then(u64_at).and_then(usize_of);
            break;
        }
    }
    let (off, len) = off.zip(len).ok_or_else(bad)?;
    let cmap = bytes
        .get(off..off.checked_add(len).ok_or_else(bad)?)
        .ok_or_else(bad)?;
    let at32 = |at: usize| {
        cmap.get(at..at.checked_add(4)?)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let strikes = at32(0).ok_or_else(bad)?;
    let record = usize_of(u64::from(at32(4).ok_or_else(bad)?)).ok_or_else(bad)?;
    if record < 16 {
        return Err(bad());
    }
    let mut at = 8usize;
    let mut runs = Vec::new();
    for _ in 0..strikes {
        let count = at32(at).ok_or_else(bad)?;
        at = at.checked_add(4).ok_or_else(bad)?;
        for _ in 0..count {
            let first = at32(at).ok_or_else(bad)?;
            let n = at.checked_add(4).and_then(at32).ok_or_else(bad)?;
            runs.push((first, n));
            at = at.checked_add(record).ok_or_else(bad)?;
        }
    }
    Ok(runs)
}

/// Reads one frame into an event, or `None` at the end of the stream: its
/// length checked against its kind's bound before its payload is
/// allocated. A stream that ends inside a frame has ended, and the
/// helper's exit status, which follows, says why.
pub(crate) fn frame(r: &mut impl Read) -> Option<Event> {
    let (kind, len) = match head(r) {
        Ok(Some(h)) => h,
        Ok(None) => return None,
        Err(why) => return Some(Event::Malformed(why)),
    };
    let max = match kind {
        FRAME_GLYPHS | FRAME_FILL => FRAME_BYTES_MAX,
        FRAME_DONE => 0,
        FRAME_FILLED | FRAME_STOPPED => 16,
        FRAME_OWN_CELL | FRAME_OUTLINE_CELL => 8,
        FRAME_SANDBOX => 4,
        FRAME_COVERAGE | FRAME_SOURCES => FRAME_BYTES_MAX,
        FRAME_POINTER => crate::pointer_file::PICTURE_BYTES_MAX,
        FRAME_NO_POINTER => REASON_BYTES_MAX.saturating_add(8),
        FRAME_FAILED | FRAME_NO_OWN_CELL | FRAME_WARNING | FRAME_DRAWS_NOTHING => REASON_BYTES_MAX,
        other => return Some(Event::Malformed(format!("a frame of unknown kind {other}"))),
    };
    if len > max {
        return Some(Event::Malformed(format!(
            "a frame of {len} bytes, past its kind's {max}"
        )));
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    match r.read_exact(&mut payload) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
        Err(e) => {
            return Some(Event::Malformed(format!(
                "cannot read the helper's answer: {e}"
            )));
        }
    }
    let word = |b: &[u8], i: usize| -> Option<u32> {
        let at = i.checked_mul(4)?;
        Some(u32::from_le_bytes(
            b.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    Some(match kind {
        FRAME_GLYPHS | FRAME_FILL => {
            // the rows line spacing adds to the set's cell, then the set
            let (Some(above), Some(below)) = (word(&payload, 0), word(&payload, 1)) else {
                return Some(Event::Malformed("glyphs without their band".to_owned()));
            };
            let band = Band { above, below };
            let payload = payload.split_off(8);
            // the store counts the set's bytes, without the band
            let len = u64::try_from(payload.len()).unwrap_or(u64::MAX);
            match mapped_runs(&payload) {
                Err(why) => Event::Malformed(why),
                Ok(runs) => match Set::from_bytes(payload) {
                    Ok(set) => Event::Glyphs {
                        fill: kind == FRAME_FILL,
                        band,
                        set: Arc::new(set),
                        runs,
                        bytes: len,
                    },
                    Err(e) => Event::Malformed(format!("glyphs vtium cannot read: {e}")),
                },
            }
        }
        FRAME_DONE => Event::Done,
        FRAME_FILLED | FRAME_STOPPED => match (0..4).map(|i| word(&payload, i)).collect() {
            Some::<Vec<u32>>(w) if w.len() == 4 && payload.len() == 16 => {
                let at = SpacedCell {
                    cell: Cell {
                        width: w.first().copied().unwrap_or(0),
                        height: w.get(1).copied().unwrap_or(0),
                    },
                    band: Band {
                        above: w.get(2).copied().unwrap_or(0),
                        below: w.get(3).copied().unwrap_or(0),
                    },
                };
                if kind == FRAME_FILLED {
                    Event::Filled(at)
                } else {
                    Event::Stopped(at)
                }
            }
            // a fill's end of the layout before the band
            _ if payload.len() == 8 => Event::Malformed(OTHER_BUILD.to_owned()),
            _ => Event::Malformed("a fill's end without its cell".to_owned()),
        },
        FRAME_OWN_CELL | FRAME_OUTLINE_CELL => match *payload.as_slice() {
            [w0, w1, w2, w3, h0, h1, h2, h3] => {
                let cell = Cell {
                    width: u32::from_le_bytes([w0, w1, w2, w3]),
                    height: u32::from_le_bytes([h0, h1, h2, h3]),
                };
                let side = 1..=vtium_font::CELL_SIDE_MAX;
                if !(side.contains(&cell.width) && side.contains(&cell.height)) {
                    Event::Malformed("an own cell past CELL_SIDE_MAX".to_owned())
                } else if kind == FRAME_OWN_CELL {
                    Event::OwnCell(cell)
                } else {
                    Event::OutlineCell(cell)
                }
            }
            _ => Event::Malformed("an own cell without its size".to_owned()),
        },
        FRAME_NO_OWN_CELL => Event::NoOwnCell(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_WARNING => Event::Warning(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_SANDBOX => match word(&payload, 0) {
            Some(parts) => Event::Sandbox(parts),
            _ => Event::Malformed("a sandbox report without its parts".to_owned()),
        },
        FRAME_COVERAGE => coverage_answer(&payload),
        FRAME_SOURCES => Event::Sources(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_DRAWS_NOTHING => Event::DrawsNothing(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_POINTER | FRAME_NO_POINTER => Event::Pointer(kind, payload),
        _ => Event::Failed(String::from_utf8_lossy(&payload).into_owned()),
    })
}

/// A coverage query's answer, [`FRAME_COVERAGE`]'s payload: the count and
/// each codepoint the frit covers, each a scalar value, then a family name
/// in UTF-8.
fn coverage_answer(payload: &[u8]) -> Event {
    let word = |i: usize| -> Option<u32> {
        let at = i.checked_mul(4)?;
        Some(u32::from_le_bytes(
            payload.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };
    let Some(n) = word(0).and_then(|n| usize::try_from(n).ok()) else {
        return Event::Malformed("a coverage answer without its count".to_owned());
    };
    let covered: Option<Vec<u32>> = (1..=n)
        .map(|i| word(i).filter(|&c| char::from_u32(c).is_some()))
        .collect();
    let rest = n
        .checked_add(1)
        .and_then(|k| k.checked_mul(4))
        .and_then(|at| payload.get(at..));
    match (covered, rest.map(std::str::from_utf8)) {
        (Some(covered), Some(Ok(font))) => Event::Coverage(covered, font.to_owned()),
        _ => Event::Malformed("a coverage answer cut short or not UTF-8".to_owned()),
    }
}

/// An answer the console takes: glyphs now in the store at `cell`, drawn
/// through `face`, for `chars`, sorted: each codepoint it brought with
/// those it decomposes to, and each character of each cluster, so the
/// cells showing one, or a cluster that composes to one, redraw.
#[derive(Debug)]
pub struct Answer {
    pub at: SpacedCell,
    pub face: SetFace,
    pub chars: Vec<char>,
}

/// The helper's process: its pidfd, which ends it, the writer's queue and
/// the reader's events. The reaper thread owns the child.
struct Running {
    pidfd: Option<OwnedFd>,
    requests: mpsc::Sender<Vec<u8>>,
    events: mpsc::Receiver<Event>,
}

/// One bit for each Unicode codepoint, [`BITS_BYTES`] once a bit is set
/// and nothing before, so a record that never holds a codepoint costs
/// nothing.
struct Bits(Vec<u64>);

/// The bytes a [`Bits`] takes once a bit is set: 136 KiB.
const BITS_BYTES: usize = 0x11_0000 / 8;

impl Bits {
    fn new() -> Bits {
        Bits(Vec::new())
    }

    fn has(&self, c: u32) -> bool {
        self.0
            .get((c / 64) as usize)
            .is_some_and(|w| w & (1 << (c % 64)) != 0)
    }

    fn put(&mut self, c: u32, on: bool) {
        if on && self.0.is_empty() {
            self.0 = vec![0; BITS_BYTES / 8];
        }
        if let Some(w) = self.0.get_mut((c / 64) as usize) {
            if on {
                *w |= 1 << (c % 64);
            } else {
                *w &= !(1 << (c % 64));
            }
        }
    }

    /// The codepoints set, as ranges.
    fn ranges(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for (i, &w) in self.0.iter().enumerate() {
            if w == 0 {
                continue;
            }
            let base = u32::try_from(i).unwrap_or(0).saturating_mul(64);
            for b in (0..64).filter(|b| w & (1 << b) != 0) {
                let c = base.saturating_add(b);
                match out.last_mut() {
                    Some((_, last)) if last.checked_add(1) == Some(c) => *last = c,
                    _ => out.push((c, c)),
                }
            }
        }
        out
    }
}

#[cfg(feature = "bidi")]
/// Whether `part` is a part of the cluster `key` the helper shapes when a
/// mark of `key` has no glyph: `key` with one or more of its marks or
/// default ignorables left out, everything else kept in order (ADR-1200
/// D22 step 3).
fn part_of(part: &[char], key: &[char]) -> bool {
    if part.len() >= key.len() || part.is_empty() {
        return false;
    }
    let mut rest = part.iter().peekable();
    for &c in key {
        if rest.peek() == Some(&&c) {
            rest.next();
        } else if !crate::deferred::droppable(c) {
            return false;
        }
    }
    rest.next().is_none()
}

/// Each codepoint of `runs`, `None` past `u32`.
fn codepoints(runs: &[(u32, u32)]) -> impl Iterator<Item = Option<u32>> + '_ {
    runs.iter()
        .flat_map(|&(first, n)| (0..n).map(move |i| first.checked_add(i)))
}

/// Where a cell's fill stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fill {
    /// Never started.
    None,
    Active,
    /// Ended at the store's limit or by a display gone.
    Stopped,
    Done,
}

/// What the console knows of one cell's clusters, as of its codepoints.
#[derive(Default)]
struct Clusters {
    held: BTreeSet<Text>,
    /// At most [`CLUSTERS_LACKED_MAX`].
    absent: BTreeSet<Text>,
    pending: BTreeSet<Text>,
}

/// What the console knows of one cell in one variant, a style and a
/// script: the codepoints and clusters the store holds, those the frit
/// lacks, those asked for and not yet answered, and, for the regular
/// normal variant, its fill.
/// Its four codepoint records take [`BITS_BYTES`] each once one is set,
/// and a variant other than regular normal never fills, so its `brought`
/// stays empty: at most 544 KiB for the regular normal record and 408 KiB
/// for each other variant's.
struct CellState {
    /// The cell and the rows line spacing adds to it (ADR-1302 D13): one
    /// cell size can carry two bands on two heads, each with glyphs of
    /// its own.
    at: SpacedCell,
    variant: Variant,
    /// The double size its glyphs are drawn at, on a line of double size,
    /// or `None` for the normal size. The glyphs of each size are asked
    /// for and held apart.
    doubled: Option<Doubled>,
    held: Bits,
    absent: Bits,
    pending: Bits,
    clusters: Clusters,
    fill: Fill,
    /// The codepoints the fill asked for last has brought, held or not.
    brought: Bits,
    /// The fill's first codepoints, sent again with a restart.
    first: Vec<u32>,
    /// When the fill was asked for, and when its first glyphs came.
    started: Option<Instant>,
    first_glyphs: Option<Instant>,
    /// The console has decided its one early ask for bold printable ASCII
    /// at the cell ([`Helper::ask_bold_early`]): it sent the ask, or the
    /// store was at its limit. Kept in the regular normal record alone.
    bold_early: bool,
    /// The early bold ask fell due while the last start of a search ran,
    /// and waits until the search ends ([`Helper::ask_bold_waiting`]).
    bold_waits: bool,
}

impl CellState {
    /// The store holds `text`'s glyph at the cell.
    fn hold(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), true),
            _ => {
                self.clusters.held.insert(text);
            }
        }
    }

    /// The store no longer holds `text`'s glyph at the cell.
    fn forget(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), false),
            _ => {
                self.clusters.held.remove(&text);
            }
        }
    }

    fn new(at: SpacedCell, (variant, doubled): Kind) -> CellState {
        CellState {
            at,
            variant,
            doubled,
            held: Bits::new(),
            absent: Bits::new(),
            pending: Bits::new(),
            clusters: Clusters::default(),
            fill: Fill::None,
            brought: Bits::new(),
            first: Vec::new(),
            started: None,
            first_glyphs: None,
            bold_early: false,
            bold_waits: false,
        }
    }
}

/// An ask the helper has not answered whole: its cell and variant, the
/// codepoints and clusters it has yet to send, and when it was sent.
struct Ask {
    at: SpacedCell,
    variant: Variant,
    /// The double size the ask's glyphs are drawn at, or `None`.
    doubled: Option<Doubled>,
    /// The cells the terminal gives a cluster with a spacing mark or a
    /// virama conjunct, which the helper places the clusters in (ADR-1006
    /// D8).
    cluster_width: ClusterWidth,
    want: BTreeSet<u32>,
    clusters: BTreeSet<Text>,
    sent: Instant,
}

/// The search for the source that made the helper fail
/// ([`Helper::restart`]).
struct Search {
    /// How the helper failed, for the line that says no source fails on
    /// its own.
    why: String,
    /// How many of the sources still in use the helper of the search
    /// reads: the first `reads` of them, in the font set's order.
    reads: usize,
    /// The work that helper was sent and has not ended, by kind, so that
    /// an end counts only against the kind it ends.
    open: Open,
    /// When that helper was sent its work, from which its asks may take
    /// [`REQUEST_MS`].
    sent: Instant,
}

impl Search {
    /// When the asks the helper of the search was sent must have ended,
    /// `request` after they were sent, while one is open.
    fn asks_end(&self, request: Duration) -> Option<Instant> {
        (self.open.asks > 0).then(|| after(self.sent, request))
    }
}

/// The work a helper was sent at its start and has not ended: its asks,
/// the cells of its fills, and its cell queries.
#[derive(Default)]
struct Open {
    asks: usize,
    fills: Vec<SpacedCell>,
    cells: usize,
}

impl Open {
    fn is_empty(&self) -> bool {
        self.asks == 0 && self.fills.is_empty() && self.cells == 0
    }

    /// An ask ended, done or failed.
    fn ask(&mut self) {
        self.asks = self.asks.saturating_sub(1);
    }

    /// The fill at `at` ended, filled or stopped.
    fn fill(&mut self, at: SpacedCell) {
        if let Some(i) = self.fills.iter().position(|&c| c == at) {
            self.fills.remove(i);
        }
    }

    /// A cell query was answered.
    fn cell(&mut self) {
        self.cells = self.cells.saturating_sub(1);
    }
}

/// The last start of a search, which reads every source still in use, as
/// the helper that failed did. vtium keeps it as the running helper and
/// takes its answers. While the work it was sent at its start is open, a
/// failure blames the source the search added last; once that work has
/// ended, no source fails on its own. vtium sends it new work as it comes,
/// which the search never tried, so a failure after that work blames no
/// source and begins a new search.
struct Suspect {
    /// The place in the frit of the source the search added last.
    source: usize,
    /// How the helper failed, for the line that says no source fails on
    /// its own.
    why: String,
    open: Open,
    /// vtium sent the helper a request after its start.
    later: bool,
}

/// The words that count chance `n` of the [`RESTARTS_MAX`] a boot gives the
/// helper, for the line that says vtium starts it again or looks for the
/// font that makes it fail.
fn chance(n: u32) -> String {
    format!("chance {n} of the {RESTARTS_MAX} that vtium gives the font helper each boot")
}

/// The reason vtium gives a font set up when the helper fails after its
/// last chance: what happened to the helper, `why`, such as "was ended by
/// signal 11", and that vtium has used every chance a boot gives it.
/// Every caller passes `why` without a subject, so the reason reads the
/// same whichever path gives the font set up.
fn no_chance_left(why: &str) -> String {
    format!(
        "the font helper {why} after vtium had used all {RESTARTS_MAX} chances that it gives the font helper each boot"
    )
}

/// The bit of the source at `i` in a leave-out request.
fn bit(i: usize) -> u32 {
    u32::try_from(i)
        .ok()
        .and_then(|i| 1u32.checked_shl(i))
        .unwrap_or(0)
}

/// A leave-out request, which names the sources the helper leaves out by
/// the bits of `mask`.
fn leave_out_request(mask: u32) -> Vec<u8> {
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, LEAVE_OUT);
    put32(&mut out, mask);
    out
}

/// The spelling of each `source=` line of the frit's text `text`, in
/// order, at most [`vtium_font::set::layer::SET_SOURCES_MAX`]: the sources
/// the helper reads, numbered as its frit reader numbers them. A line's
/// text after `#` is a comment, and its key and value are trimmed, as the
/// reader takes them. The console parses no font, and finds only the
/// lines.
fn source_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .filter_map(|line| {
            let body = line.split('#').next().unwrap_or_default().trim();
            let (key, value) = body.split_once('=')?;
            (key.trim() == "source").then(|| value.trim().to_owned())
        })
        .take(vtium_font::set::layer::SET_SOURCES_MAX)
        .collect()
}

/// What a frit's helpers have used up this boot: the chances, and the
/// sources left out because they made a helper fail. The console keeps one
/// record for each frit, by the frit's name, outside the frit's helper, so
/// the record outlives the helper. A frit that no head draws any more is
/// dropped with its helper, and a program can bring it back at once with
/// two OSC 50 switches. The helper started then takes the same record, so
/// it has no fresh chances, and it leaves out the same fonts. Each left-out
/// source is kept by its spelling on its `source=` line, so the record
/// still names the right fonts when the frit's file is read again with
/// other lines.
///
/// References:
/// - ADR-1200 D15, "Restarted after a failure", and its note of
///   2026-11-21: three chances a boot, and a source left out stays left
///   out for the rest of the boot.
#[derive(Debug, Default)]
pub struct Failures {
    /// How many of the [`RESTARTS_MAX`] chances this boot has used: each
    /// search begun, and each restart that looked for no source.
    restarts: u32,
    /// The spelling of each source that made the helper fail.
    left_out: BTreeSet<String>,
}

/// A frit's [`Failures`], shared by the console's record of the frit and
/// the frit's helper while it lives.
pub type FailureRecord = Rc<RefCell<Failures>>;

/// The helper as the console sees it.
pub struct Helper {
    launcher: Launcher,
    /// Wakes the loop: called by the reader thread after each frame.
    wake: Arc<dyn Fn() + Send + Sync>,
    frit: FritText,
    console: Option<ConsoleFont>,
    store: Shared,
    limit: u64,
    running: Option<Running>,
    cells: Vec<CellState>,
    /// The cell and band of each stop sent that the helper has not yet
    /// taken: a cell's fill frames are dropped while it has one, as they
    /// belong to the fill stopped, however soon the cell is filled again.
    stopped: Vec<SpacedCell>,
    asks: VecDeque<Ask>,
    /// The height of each cell query left unanswered past its wait, the
    /// oldest first. The helper answers queries in the order they are
    /// sent, so a cell answer that comes while one is listed answers the
    /// oldest.
    late: VecDeque<u32>,
    /// The late answers taken and not yet handed on by
    /// [`Helper::late_cells`], each with the height its query asked at.
    kept: Vec<(u32, Result<OwnCell, NoCell>)>,
    /// What the helper sent while the console waited for its own cell,
    /// for the next [`Helper::poll`].
    early: VecDeque<Event>,
    /// When the next answer is due, while work is outstanding.
    due: Option<Instant>,
    /// The chances used and the sources left out this boot, which every
    /// helper of the frit started from then on shares.
    failures: FailureRecord,
    /// The spelling of each `source=` line of the frit's text, in order:
    /// the sources the helper reads, numbered as it numbers them.
    sources: Vec<String>,
    /// The search for the source that made the helper fail, while it
    /// runs.
    search: Option<Search>,
    /// The heights of the cell queries made during a search, which the
    /// helper started after it is asked at its start.
    deferred: Vec<u32>,
    /// The last start of a search while it has work it was sent and has
    /// not ended: a failure of the helper then blames the source it names,
    /// unless vtium has sent the helper new work since its start.
    suspect: Option<Suspect>,
    /// A source was left out since the console last asked
    /// ([`Helper::take_blamed`]).
    blamed: bool,
    /// The store reached its limit: logged once.
    full: bool,
    /// The frit's warnings logged this boot, at most [`WARNINGS_MAX`].
    warned: BTreeSet<String>,
    /// Whether the frit's warnings are logged: true but for a helper that
    /// `vtium check-config` starts ([`Helper::quiet`]).
    logs_warnings: bool,
    /// Whether a warning past [`WARNINGS_MAX`] came, and the console logged
    /// that it logs no more of them.
    warnings_dropped: bool,
    /// The line logged this boot about a sandbox the helper runs without,
    /// once one has been ([`sandbox_notice`]).
    sandbox_noted: Option<String>,
    failed: Option<String>,
    /// The pointer ask the helper has not answered whole, its frames so
    /// far, and when it was sent.
    pointer: Option<(crate::pointer_file::Reply, Instant)>,
    /// The answer to the last pointer ask, or why the helper gave none,
    /// until [`Helper::pointer_answer`] takes it.
    pointer_done: Option<Result<crate::pointer_file::Answer, String>>,
    /// Whether this helper serves pointer asks alone, so its failures
    /// reach the pointer file's log line and not the font's.
    pointers_only: bool,
    answer: Duration,
    request: Duration,
    /// When vtium started, for the line each fill logs.
    epoch: Instant,
    /// Whether the console makes its early ask for bold printable ASCII
    /// at each cell ([`Helper::ask_bold_early`]). It does, but for the
    /// stand-ins of the tests whose answers are written out in full.
    asks_bold_early: bool,
}

impl Helper {
    /// The helper `launcher` starts for `frit`, handing it the kernel's
    /// `console` font, whose glyphs go into `store` up to `limit` bytes;
    /// its reader calls `wake` after each answer, in a vtium started at
    /// `epoch`.
    pub fn new(
        launcher: Launcher,
        wake: Arc<dyn Fn() + Send + Sync>,
        epoch: Instant,
        frit: FritText,
        console: Option<ConsoleFont>,
        store: Shared,
        limit: u64,
    ) -> Helper {
        let sources = source_lines(&frit.text);
        Helper {
            launcher,
            wake,
            frit,
            console,
            store,
            limit,
            epoch,
            running: None,
            cells: Vec::new(),
            stopped: Vec::new(),
            asks: VecDeque::new(),
            late: VecDeque::new(),
            kept: Vec::new(),
            early: VecDeque::new(),
            due: None,
            failures: FailureRecord::default(),
            sources,
            search: None,
            deferred: Vec::new(),
            suspect: None,
            blamed: false,
            full: false,
            warned: BTreeSet::new(),
            logs_warnings: true,
            warnings_dropped: false,
            sandbox_noted: None,
            failed: None,
            pointer: None,
            pointer_done: None,
            pointers_only: false,
            answer: Duration::from_millis(ANSWER_MS),
            request: Duration::from_millis(REQUEST_MS),
            asks_bold_early: true,
        }
    }

    /// Makes the helper count its chances in `record` and leave out the
    /// sources `record` names, the record of what the frit's earlier
    /// helpers used up this boot. The console calls this as it starts a
    /// frit's helper, before the helper's first start.
    #[must_use]
    pub fn remembering(mut self, record: FailureRecord) -> Helper {
        self.failures = record;
        self
    }

    /// How many of the [`RESTARTS_MAX`] chances this boot has used, for a
    /// test.
    #[cfg(test)]
    fn restarts(&self) -> u32 {
        self.failures.borrow().restarts
    }

    /// Uses one of the [`RESTARTS_MAX`] chances, and returns how many are
    /// used now, or `None` when none was left.
    fn use_chance(&mut self) -> Option<u32> {
        let mut failures = self.failures.borrow_mut();
        if failures.restarts >= RESTARTS_MAX {
            return None;
        }
        failures.restarts = failures.restarts.saturating_add(1);
        Some(failures.restarts)
    }

    /// The sources that made a helper fail, a bit for each by its place
    /// in the frit, which every helper started from then on leaves out.
    fn left_out(&self) -> u32 {
        let failures = self.failures.borrow();
        self.sources
            .iter()
            .enumerate()
            .filter(|(_, source)| failures.left_out.contains(source.as_str()))
            .fold(0, |mask, (i, _)| mask | bit(i))
    }

    /// Tells the helper the active fonts of `launcher`, the console's
    /// launcher once the batch changed at a new login: the request goes to
    /// the running helper at once, and to each helper started after this
    /// one ends, so every frit the helper reads from now on draws from the
    /// new batch.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: an activation takes effect at the next login, with no restart.
    pub fn activate(&mut self, launcher: &Launcher) {
        self.launcher.active.clone_from(&launcher.active);
        if let (Some(running), Some(active)) = (&self.running, &self.launcher.active) {
            let _ = running.requests.send(active.to_vec());
        }
    }

    /// Starts the frit's helper afresh under the active fonts of
    /// `launcher`, the console's launcher once a new login found the batch
    /// changed. vtium ends the running helper and forgets every cell, every
    /// waiting ask and every stored glyph, so the next ask starts a helper
    /// that reads the new active fonts before any ask. An answer to an ask
    /// sent before this call comes from a font the old batch allowed, and
    /// the console never takes it, because the ended helper's output is
    /// closed with it. A search for a source that made the helper fail
    /// ends with it, and a source already left out stays left out. The
    /// cell queries still waiting for an answer, those that waited for the
    /// search among them, are asked of the next helper as it starts, and
    /// their answers come late ([`Helper::late_cells`]), so the console
    /// learns each cell it asked for under the new active fonts.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: an activation or a deactivation takes effect at the next login, with no restart.
    pub fn begin_again(&mut self, launcher: &Launcher) {
        self.launcher.active.clone_from(&launcher.active);
        self.deferred.extend(self.late.drain(..));
        self.stop();
        self.asks.clear();
        self.early.clear();
        self.cells.clear();
        self.due = None;
        self.search = None;
        self.suspect = None;
        crate::deferred::lock(&self.store).retain(&[]);
    }

    /// The helper `launcher` starts for pointer asks alone, which a
    /// pointer file's loader sends one at a time
    /// ([`crate::pointer_file::Loader`]); its reader calls `wake` after
    /// each answer. It starts, restarts and gives up as a frit's helper
    /// does.
    pub fn for_pointers(
        launcher: Launcher,
        wake: Arc<dyn Fn() + Send + Sync>,
        epoch: Instant,
    ) -> Helper {
        let frit = FritText {
            name: String::new(),
            text: String::new(),
        };
        let mut helper = Helper::new(launcher, wake, epoch, frit, None, Shared::default(), 0);
        helper.pointers_only = true;
        helper
    }

    /// Sends the pointer ask `ask`, starting the helper first when it is
    /// not running. A helper given up answers at once with why. One ask is
    /// answered before the next is sent.
    pub fn pointer(&mut self, ask: &crate::pointer_file::Ask, now: Instant) {
        if let Some(why) = &self.failed {
            self.pointer_done = Some(Err(why.clone()));
            return;
        }
        self.pointer = Some((crate::pointer_file::Reply::new(ask), now));
        self.submit(ask.encode(), now);
    }

    /// The answer to the last pointer ask, once the helper ended it, or
    /// why there is none: the helper failed, ended or gave no answer in
    /// time.
    pub fn pointer_answer(&mut self) -> Option<Result<crate::pointer_file::Answer, String>> {
        self.pointer_done.take()
    }

    /// The pointer ask under way gets no answer, because of `why`.
    fn lose_pointer(&mut self, why: &str) {
        if self.pointer.take().is_some() {
            self.pointer_done = Some(Err(why.to_owned()));
        }
    }

    /// Why the helper was given up, once it has been.
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// When [`Helper::poll`] must next run: the next answer due, or the
    /// oldest ask's end, whichever comes first.
    pub fn deadline(&self) -> Option<Instant> {
        let ask = match &self.search {
            Some(search) => search.asks_end(self.request),
            None => self.oldest_sent().map(|sent| after(sent, self.request)),
        };
        match (self.due, ask) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// When the oldest ask, or the pointer ask, that the helper has not
    /// answered whole was sent.
    fn oldest_sent(&self) -> Option<Instant> {
        self.asks
            .front()
            .map(|a| a.sent)
            .or(self.pointer.as_ref().map(|(_, sent)| *sent))
    }

    /// Whether the helper owes an answer: an ask, a pointer ask or a fill
    /// is outstanding, or a cell query whose answer comes late. While it
    /// owes one, a helper that sends nothing for [`ANSWER_MS`] is ended
    /// as a hung helper. So a console that holds a program's replies until
    /// a late cell comes never holds them for good: the helper started
    /// again answers, or vtium gives the helper up.
    fn owes(&self) -> bool {
        self.busy() || !self.late.is_empty()
    }

    /// Whether an ask, a pointer ask or a fill is outstanding.
    pub fn busy(&self) -> bool {
        !self.asks.is_empty()
            || self.pointer.is_some()
            || self.cells.iter().any(|c| c.fill == Fill::Active)
    }

    /// What the console knows of `at` in `variant`, made on first use.
    fn state(&mut self, at: SpacedCell, variant: Variant) -> Option<&mut CellState> {
        self.state_of(at, (variant, None))
    }

    /// [`Helper::state`] for glyphs of `kind`, at a double size or not.
    fn state_of(&mut self, at: SpacedCell, kind: Kind) -> Option<&mut CellState> {
        if !self
            .cells
            .iter()
            .any(|c| (c.at, (c.variant, c.doubled)) == (at, kind))
        {
            self.cells.push(CellState::new(at, kind));
        }
        self.known_of(at, kind)
    }

    /// What the console knows of `at` in `variant`, if it knows anything.
    fn known(&mut self, at: SpacedCell, variant: Variant) -> Option<&mut CellState> {
        self.known_of(at, (variant, None))
    }

    /// [`Helper::known`] for glyphs of `kind`, at a double size or not.
    fn known_of(&mut self, at: SpacedCell, kind: Kind) -> Option<&mut CellState> {
        self.cells
            .iter_mut()
            .find(|c| (c.at, (c.variant, c.doubled)) == (at, kind))
    }

    /// Fills `cell`'s coverage of regular glyphs, `first` first, while
    /// nothing is asked; bold and italic come by ask alone. Line spacing
    /// adds `band` to the cell (ADR-1302 D13).
    pub fn fill(&mut self, cell: Cell, band: Band, first: &[char], now: Instant) {
        if self.failed.is_some() {
            return;
        }
        let at = SpacedCell { cell, band };
        let Some(state) = self.state(at, Variant::default()) else {
            return;
        };
        state.fill = Fill::Active;
        state.brought = Bits::new();
        state.first = first.iter().map(|&c| u32::from(c)).collect();
        state.started = Some(now);
        state.first_glyphs = None;
        let (codepoints, held) = (state.first.clone(), state.held.ranges());
        self.send(
            (FILL, at, (Variant::default(), None), ClusterWidth::Double),
            (&codepoints, &[]),
            &held,
            now,
        );
    }

    /// Asks for each character and each cluster `shown` at `cell`, in its
    /// variant, that the store does not hold in that variant, the frit is not
    /// known to lack and no ask has named: at most [`FIRST_MAX`] of each, at most
    /// [`CLUSTERS_WAITING_MAX`] clusters waiting at the cell, and nothing
    /// while [`ASKS_MAX`] asks wait; what is left is asked for on a later
    /// change. A cluster longer than [`vtium_font::CLUSTER_SCALARS_MAX`]
    /// is never asked for. A cluster not asked for draws unshaped. Line
    /// spacing adds `band` to the cell (ADR-1302 D13).
    pub fn ask(&mut self, cell: Cell, band: Band, shown: &Shown, now: Instant) {
        if self.failed.is_some() || self.asks.len() >= ASKS_MAX {
            return;
        }
        let (variant, doubled) = (shown.variant, shown.doubled);
        // the helper draws no superscript or subscript at a double size,
        // and none past the largest cell; the renderer doubles the pixels
        // of those glyphs at the normal size
        if doubled.is_some_and(|d| d.cell(cell).is_none() || variant.script != Script::Baseline) {
            return;
        }
        let at = SpacedCell { cell, band };
        let Some(state) = self.state_of(at, (variant, doubled)) else {
            return;
        };
        let mut want = BTreeSet::new();
        for c in shown.chars.iter().map(|&c| u32::from(c)) {
            if want.len() >= FIRST_MAX {
                break;
            }
            if !state.held.has(c) && !state.absent.has(c) && !state.pending.has(c) {
                state.pending.put(c, true);
                want.insert(c);
            }
        }
        let longest = usize::try_from(vtium_font::CLUSTER_SCALARS_MAX).unwrap_or(usize::MAX);
        let known = &mut state.clusters;
        let room = CLUSTERS_WAITING_MAX
            .saturating_sub(known.pending.len())
            .min(FIRST_MAX);
        let mut clusters = BTreeSet::new();
        for t in shown
            .clusters
            .iter()
            .filter(|t| (2..=longest).contains(&t.len()))
        {
            if clusters.len() >= room {
                break;
            }
            if !known.held.contains(t) && !known.absent.contains(t) && !known.pending.contains(t) {
                known.pending.insert(t.clone());
                clusters.insert(t.clone());
            }
        }
        if want.is_empty() && clusters.is_empty() {
            return;
        }
        let codepoints: Vec<u32> = want.iter().copied().collect();
        let texts: Vec<Text> = clusters.iter().cloned().collect();
        self.asks.push_back(Ask {
            at,
            variant,
            doubled,
            cluster_width: shown.cluster_width,
            want,
            clusters,
            sent: now,
        });
        self.send(
            (ASK, at, (variant, doubled), shown.cluster_width),
            (&codepoints, &texts),
            &[],
            now,
        );
    }

    /// The cell the frit's first source gives at `height`, the height the
    /// console draws at, asked of the helper and waited for at most `wait`
    /// (the operator, 2026-10-28; the maintainer, 2026-11-01): a bitmap
    /// source's own cell, whatever `height` is, or an outline source's
    /// cell `height` pixels high. A warning the frit's text gives as the
    /// helper reads it is logged, and what else the helper sends meanwhile,
    /// such as a fill's glyphs, is kept for the next [`Helper::poll`]
    /// while the wait goes on, and the late answer to an earlier query is
    /// kept for [`Helper::late_cells`]. The error is why there is none:
    /// the helper's refusal, a helper that cannot start or ends first, no
    /// answer in time, whose late answer is kept when it comes, or an
    /// outline cell of another height, which is refused as malformed.
    /// `clock` tells the time as the wait goes on. A [`CellWait`] on this
    /// thread sets the wait in place of `wait`.
    pub fn own_cell(
        &mut self,
        height: u32,
        wait: Duration,
        clock: &dyn Fn() -> Instant,
    ) -> Result<OwnCell, NoCell> {
        let wait = CELL_WAIT.with(std::cell::Cell::get).unwrap_or(wait);
        if self.search.is_some() && self.failed.is_none() {
            // the helper started after the search answers the query, and
            // its answer comes late
            self.deferred.push(height);
            return Err(NoCell {
                why: "the font helper is starting again after a failure".to_owned(),
                late: true,
                from_frit: false,
            });
        }
        let now = clock();
        let query = Cell { width: 0, height };
        if self.failed.is_none() {
            self.send(
                (
                    OWN_CELL,
                    SpacedCell::plain(query),
                    (Variant::default(), None),
                    ClusterWidth::Double,
                ),
                (&[], &[]),
                &[],
                now,
            );
        }
        if let Some(why) = &self.failed {
            return Err(NoCell::refused(why.clone()));
        }
        let end = after(now, wait);
        loop {
            let left = end.saturating_duration_since(clock());
            let event = self
                .running
                .as_ref()
                .ok_or_else(|| NoCell::refused("the helper is not running".to_owned()))?
                .events
                .recv_timeout(left);
            return match event {
                Ok(answer @ (Event::OwnCell(_) | Event::OutlineCell(_) | Event::NoOwnCell(_))) => {
                    // The late answer to an earlier query, which is kept,
                    // or this query's.
                    let (asked, late) = match self.late.pop_front() {
                        Some(asked) => (asked, true),
                        None => (height, false),
                    };
                    let answered = cell_answer(answer, asked);
                    if answered.is_ok() {
                        if late && let Some(suspect) = &mut self.suspect {
                            suspect.open.cell();
                            // as poll does after each answer, so that a
                            // search this answer ends is not left waiting
                            // for an answer already taken
                            self.settle(clock());
                        }
                        // As poll does after each answer: the watchdog
                        // runs from this answer while the helper still
                        // owes work, and stops once it owes none. A late
                        // answer taken here no longer arms it, so a helper
                        // that answered every query is never ended as hung.
                        self.due = (self.owes() || self.suspect.is_some())
                            .then(|| after(clock(), self.answer));
                    }
                    match answered {
                        Ok(answer) if late => {
                            self.kept.push((asked, answer));
                            continue;
                        }
                        Ok(answer) => answer,
                        Err(why) => {
                            self.early.push_back(Event::Malformed(why.clone()));
                            Err(NoCell::refused(why))
                        }
                    }
                }
                Ok(Event::Warning(why)) => {
                    self.warn(why.trim());
                    continue;
                }
                Ok(Event::Sandbox(parts)) => {
                    self.sandboxed(parts);
                    continue;
                }
                Ok(
                    other @ (Event::Glyphs { .. }
                    | Event::Done
                    | Event::Filled(_)
                    | Event::Stopped(_)),
                ) => {
                    self.early.push_back(other);
                    continue;
                }
                // The helper ended while it answered, as when a font
                // crashed it: the next poll takes the end and starts the
                // helper again, which is asked again and answers late.
                Ok(closed @ Event::Closed(_)) => {
                    self.deferred.push(height);
                    self.early.push_back(closed);
                    Err(NoCell {
                        why: "the font helper ended, and vtium starts it again".to_owned(),
                        late: true,
                        from_frit: false,
                    })
                }
                Ok(other) => {
                    let why = match &other {
                        Event::Failed(why) => failure(why),
                        Event::Malformed(why) => format!("the helper's answer was refused: {why}"),
                        _ => "the helper sent what answers no cell query".to_owned(),
                    };
                    self.early.push_back(other);
                    Err(NoCell::refused(why))
                }
                Err(_) => {
                    self.late.push_back(height);
                    // The late answer is work the helper owes, so a helper
                    // that never gives it is ended as a hung helper is.
                    self.due.get_or_insert(after(clock(), self.answer));
                    Err(NoCell {
                        why: format!("the helper gave no answer in {} ms", wait.as_millis()),
                        late: true,
                        from_frit: false,
                    })
                }
            };
        }
    }

    /// Which of `letters` the frit covers, and the family of an installed
    /// font that covers the most of the rest, `None` when none does: the
    /// helper's answer to a coverage query (ADR-1401 D7). The console
    /// parses no font, so the helper reads the frit's sources and the
    /// installed fonts. The error is why there is no answer: the helper
    /// cannot start, refuses the frit, ends first, or gives no answer in
    /// `wait`. `clock` tells the time as the wait goes on. `vtium
    /// check-config` asks it, with no other request in flight.
    pub fn coverage(
        &mut self,
        letters: &[u32],
        wait: Duration,
        clock: &dyn Fn() -> Instant,
    ) -> Result<(Vec<u32>, Option<String>), String> {
        let now = clock();
        let query = Cell {
            width: 0,
            height: 0,
        };
        if self.search.is_some() {
            return Err("the font helper is starting again after a failure".to_owned());
        }
        if self.failed.is_none() {
            self.send(
                (
                    COVERAGE,
                    SpacedCell::plain(query),
                    (Variant::default(), None),
                    ClusterWidth::Double,
                ),
                (letters, &[]),
                &[],
                now,
            );
        }
        if let Some(why) = &self.failed {
            return Err(why.clone());
        }
        let end = after(now, wait);
        loop {
            let left = end.saturating_duration_since(clock());
            let event = self
                .running
                .as_ref()
                .ok_or_else(|| "the helper is not running".to_owned())?
                .events
                .recv_timeout(left);
            return match event {
                Ok(Event::Coverage(covered, font)) => {
                    Ok((covered, Some(font).filter(|f| !f.is_empty())))
                }
                Ok(Event::DrawsNothing(why)) => Err(why.trim().to_owned()),
                Ok(Event::Warning(why)) => {
                    self.warn(why.trim());
                    continue;
                }
                Ok(Event::Sandbox(parts)) => {
                    self.sandboxed(parts);
                    continue;
                }
                Ok(Event::Failed(why)) => Err(failure(&why)),
                Ok(Event::Malformed(why)) => Err(format!("the helper's answer was refused: {why}")),
                Ok(other) => {
                    self.early.push_back(other);
                    continue;
                }
                Err(_) => Err(format!(
                    "the helper gave no answer in {} ms",
                    wait.as_millis()
                )),
            };
        }
    }

    /// The helper's answer to a sources query on the frit: one line for
    /// each source it left out, `missing` or `refused`, and one for each
    /// font that differs from what its source needs, `lacks`, `older` or
    /// `sha256`, each with the source's line, its spelling and a value,
    /// tab-separated. The console parses no font, so the helper finds the
    /// fonts. The error is why there is no answer, as for
    /// [`Helper::coverage`]: the helper cannot start, draws nothing from
    /// the frit, ends first, or gives no answer in `wait`. `vtium
    /// check-config` asks it, with no other request in flight.
    ///
    /// References:
    /// - ADR-1204 D7: `vtium check-config` names each missing font.
    pub fn sources(
        &mut self,
        wait: Duration,
        clock: &dyn Fn() -> Instant,
    ) -> Result<String, String> {
        let now = clock();
        let query = Cell {
            width: 0,
            height: 0,
        };
        if self.search.is_some() {
            return Err("the font helper is starting again after a failure".to_owned());
        }
        if self.failed.is_none() {
            self.send(
                (
                    SOURCES,
                    SpacedCell::plain(query),
                    (Variant::default(), None),
                    ClusterWidth::Double,
                ),
                (&[], &[]),
                &[],
                now,
            );
        }
        if let Some(why) = &self.failed {
            return Err(why.clone());
        }
        let end = after(now, wait);
        loop {
            let left = end.saturating_duration_since(clock());
            let event = self
                .running
                .as_ref()
                .ok_or_else(|| "the helper is not running".to_owned())?
                .events
                .recv_timeout(left);
            return match event {
                Ok(Event::Sources(lines)) => Ok(lines),
                Ok(Event::DrawsNothing(why)) => Err(why.trim().to_owned()),
                Ok(Event::Warning(why)) => {
                    self.warn(why.trim());
                    continue;
                }
                Ok(Event::Sandbox(parts)) => {
                    self.sandboxed(parts);
                    continue;
                }
                Ok(Event::Failed(why)) => Err(failure(&why)),
                Ok(Event::Malformed(why)) => Err(format!("the helper's answer was refused: {why}")),
                Ok(other) => {
                    self.early.push_back(other);
                    continue;
                }
                Err(_) => Err(format!(
                    "the helper gave no answer in {} ms",
                    wait.as_millis()
                )),
            };
        }
    }

    /// The answers to cell queries that came after their wait, since the
    /// last call, each with the height its query asked at, in the order
    /// they came (ADR-1200 D17, the maintainer, 2026-11-03).
    pub fn late_cells(&mut self) -> Vec<(u32, Result<OwnCell, NoCell>)> {
        std::mem::take(&mut self.kept)
    }

    /// Whether vtium left a source of the frit out since the last call.
    /// The cell the console took from an earlier answer may be the cell of
    /// the source left out, so the console asks for each cell again
    /// ([`Helper::ask_cells_again`]).
    pub fn take_blamed(&mut self) -> bool {
        std::mem::take(&mut self.blamed)
    }

    /// Says a source was left out, as [`Helper::blame`] does, for a test of
    /// the console that has no failing helper to drive.
    #[cfg(test)]
    pub(crate) fn set_blamed(&mut self) {
        self.blamed = true;
    }

    /// Uses a chance and leaves out the source at `i`, as a search that
    /// finds the source that crashed the helper does, for a test of the
    /// console that has no failing helper to drive.
    #[cfg(test)]
    pub(crate) fn crashed_on(&mut self, i: usize, now: Instant) {
        assert!(self.use_chance().is_some(), "a chance is left");
        self.blame(i, "was ended by signal 11", now);
    }

    /// How many chances this boot has used, and the bits of the sources
    /// left out, for a test of the console.
    #[cfg(test)]
    pub(crate) fn used(&self) -> (u32, u32) {
        (self.restarts(), self.left_out())
    }

    /// The heights of the cell queries the running helper has been asked
    /// and has not answered, whose answers come late, for a test.
    #[cfg(test)]
    pub(crate) fn asked_late(&self) -> Vec<u32> {
        self.late.iter().copied().collect()
    }

    /// Answers the oldest cell query that is still waiting with `answer`,
    /// as [`Helper::poll`] keeps a late answer from the helper, for a test
    /// of the console whose stand-in helper never answers. Returns false,
    /// answering nothing, when no query waits, as the real helper's answer
    /// to a query nobody asked is refused.
    #[cfg(test)]
    pub(crate) fn answer_late(&mut self, answer: Result<OwnCell, NoCell>) -> bool {
        let Some(asked) = self.late.pop_front() else {
            return false;
        };
        self.kept.push((asked, answer));
        true
    }

    /// Gives the helper up for `why`, as a helper that fails past its last
    /// restart is given up, for a test of the console that has no failing
    /// helper to drive.
    #[cfg(test)]
    pub(crate) fn give_up(&mut self, why: &str) {
        self.fail(why);
    }

    /// Asks for the cell at each of `heights` again, without waiting: the
    /// answers come late ([`Helper::late_cells`]). A running helper is
    /// asked at once. During a search the queries wait for the helper
    /// started after it, and a helper given up is asked nothing.
    pub fn ask_cells_again(&mut self, heights: &[u32], now: Instant) {
        if self.failed.is_some() {
            return;
        }
        self.deferred.extend_from_slice(heights);
        if self.search.is_none() {
            self.send_deferred(now);
        }
    }

    /// Asks the helper, starting it first, the cell queries that waited
    /// for it, each answered late.
    fn send_deferred(&mut self, now: Instant) {
        for height in std::mem::take(&mut self.deferred) {
            let query = SpacedCell::plain(Cell { width: 0, height });
            let bytes = self.request(
                (
                    OWN_CELL,
                    query,
                    (Variant::default(), None),
                    ClusterWidth::Double,
                ),
                (&[], &[]),
                &[],
            );
            self.post(bytes, now);
            if self.running.is_some() {
                self.late.push_back(height);
                self.due.get_or_insert(after(now, self.answer));
            }
        }
    }

    /// Forgets every cell but `cells`, as no display draws at them: their
    /// glyphs of every style leave the store, and a running helper is told
    /// to stop each, whatever its fill.
    pub fn retain(&mut self, cells: &[SpacedCell], now: Instant) {
        let mut gone: Vec<SpacedCell> = self
            .cells
            .iter()
            .filter(|c| !cells.contains(&c.at))
            .map(|c| c.at)
            .collect();
        gone.sort_unstable_by_key(|a| (a.cell.height, a.cell.width, a.band));
        gone.dedup();
        if self.running.is_some() {
            for at in gone {
                self.stop_fill(at, now);
            }
        }
        self.cells.retain(|c| cells.contains(&c.at));
        crate::deferred::lock(&self.store).retain(cells);
    }

    /// Tells the helper to stop the fill at `at`, and drops its fill frames
    /// until the helper says it took the stop.
    fn stop_fill(&mut self, at: SpacedCell, now: Instant) {
        self.send(
            (STOP, at, (Variant::default(), None), ClusterWidth::Double),
            (&[], &[]),
            &[],
            now,
        );
        if self.running.is_some() && self.search.is_none() {
            self.stopped.push(at);
        }
    }

    /// Sends one request of `kind` at a cell and the rows line spacing
    /// adds to it, in `variant`, starting the helper first when it is not
    /// running.
    fn send(
        &mut self,
        what: (u32, SpacedCell, Kind, ClusterWidth),
        named: (&[u32], &[Text]),
        held: &[(u32, u32)],
        now: Instant,
    ) {
        let bytes = self.request(what, named, held);
        self.submit(bytes, now);
    }

    /// One request's bytes, of `kind` at a cell and the rows line spacing
    /// adds to it, in `variant`.
    fn request(
        &self,
        (kind, at, of, rule): (u32, SpacedCell, Kind, ClusterWidth),
        named: (&[u32], &[Text]),
        held: &[(u32, u32)],
    ) -> Vec<u8> {
        encode(
            &self.frit,
            self.console.as_ref(),
            (kind, at.cell, at.band, of, rule),
            named,
            held,
        )
    }

    /// Sends one request's `bytes` as [`Helper::post`] does. During a
    /// search it sends nothing: the ask or fill waits, and the helper
    /// started after the search is sent every waiting ask and fill.
    fn submit(&mut self, bytes: Vec<u8>, now: Instant) {
        if self.search.is_none() {
            self.post(bytes, now);
        }
    }

    /// Sends one request's `bytes`, starting the helper first when it is
    /// not running, and sets when its answer is due. A helper started
    /// outside a search is first asked the cell queries that waited for
    /// it. A helper given up takes nothing, so a helper that cannot start
    /// while vtium sends its waiting asks again is started once, not once
    /// for each ask.
    fn post(&mut self, bytes: Vec<u8>, now: Instant) {
        if self.failed.is_some() {
            return;
        }
        if self.running.is_none() {
            match self.start() {
                Ok(r) => self.running = Some(r),
                Err(why) => return self.fail(&why),
            }
            if self.search.is_none() {
                self.send_deferred(now);
            }
        }
        if let Some(r) = &self.running {
            // a send fails only once the writer is gone, which the reader
            // reports as the helper closing
            let _ = r.requests.send(bytes);
        }
        if let Some(suspect) = &mut self.suspect {
            suspect.later = true;
        }
        if self.owes() {
            self.due.get_or_insert(after(now, self.answer));
        }
    }

    fn start(&self) -> Result<Running, String> {
        // a descriptor the console holds, the VT's and the display's among
        // them, would otherwise reach the helper
        vtium_platform::cloexec_above_stdio().map_err(|e| {
            format!("vtium cannot keep its own descriptors from the font helper: {e}")
        })?;
        let mut child = self
            .launcher
            .process()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| self.launcher.failed(&e))?;
        // the child is not reaped before the check, so its pid is its own
        if let Err(why) = self.launcher.dropped(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(why);
        }
        // a pidfd names this child alone, even once the reaper has reaped it
        let pidfd = i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .and_then(|pid| pidfd_open(pid, PidfdFlags::empty()).ok());
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
        // the reaper starts first, so the child is reaped whatever fails
        // after
        let (exited, status) = mpsc::channel();
        let (give, given) = mpsc::channel::<Child>();
        if let Err(e) = thread("reaper", move || {
            if let Ok(child) = given.recv() {
                let _ = exited.send(reap(child));
            }
        }) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "vtium cannot start the font helper's reaper thread: {e}"
            ));
        }
        let _ = give.send(child);
        // a thread that cannot start ends the child, which its reaper reaps
        let end = |what: &str, e: std::io::Error| {
            if let Some(fd) = &pidfd {
                let _ = pidfd_send_signal(fd, Signal::KILL);
            }
            format!("vtium cannot start the font helper's {what} thread: {e}")
        };
        let (requests, to_write) = mpsc::channel::<Vec<u8>>();
        // the active fonts go first, before any request reads a frit, and
        // then the sources the frit leaves out
        if let Some(active) = &self.launcher.active {
            let _ = requests.send(active.to_vec());
        }
        let leave = self.leave_out();
        if leave != 0 {
            let _ = requests.send(leave_out_request(leave));
        }
        if let Some(mut stdin) = stdin {
            thread("writer", move || {
                for bytes in to_write {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| end("writer", e))?;
        }
        let (tx, events) = mpsc::channel();
        let wake = Arc::clone(&self.wake);
        thread("reader", move || {
            if let Some(mut stdout) = stdout {
                while let Some(e) = frame(&mut stdout) {
                    let malformed = matches!(e, Event::Malformed(_));
                    if tx.send(e).is_err() {
                        break;
                    }
                    wake();
                    if malformed {
                        break;
                    }
                }
            }
            // its end is told once it has exited too, so a helper that
            // closes its output a moment before it exits is waited for,
            // never ended
            let status = status.recv().ok().flatten();
            let _ = tx.send(Event::Closed(status));
            wake();
        })
        .map_err(|e| end("reader", e))?;
        Ok(Running {
            pidfd,
            requests,
            events,
        })
    }

    /// Gives the helper up for the boot: ends it, drops what was asked
    /// and logs `why` once.
    fn fail(&mut self, why: &str) {
        self.stop();
        self.lose_pointer(why);
        self.search = None;
        self.suspect = None;
        self.deferred.clear();
        self.asks.clear();
        for c in &mut self.cells {
            if c.fill == Fill::Active {
                c.fill = Fill::Stopped;
            }
        }
        self.due = None;
        if self.failed.is_none() {
            if !self.pointers_only {
                log::warn!(
                    "the font helper could not draw the font ({why}), so vtium uses its built-in font, one character at a time with no combined characters"
                );
            }
            self.failed = Some(why.to_owned());
        }
    }

    /// Ends the helper's process, if it runs; its reaper ends what it
    /// started and reaps it, and nothing it sent is taken.
    fn stop(&mut self) {
        self.stopped.clear();
        self.late.clear();
        if let Some(fd) = self.running.take().and_then(|r| r.pidfd) {
            let _ = pidfd_send_signal(&fd, Signal::KILL);
        }
    }

    /// Ends the helper after `why`, a crash, a hang, the CPU limit or an
    /// exit with work outstanding, and finds the source of the font set
    /// that caused it. vtium cannot tell from one failure which source the
    /// helper was reading, since an ask, a fill or a cell query can draw
    /// from every source. So it starts the helper again with none of the
    /// sources still in use, then with the first of them, then the first
    /// two, and so on, and sends each start every waiting ask, fill and
    /// cell query. The sources are added in the font set's order, the
    /// order in which the helper looks for a character, so each start
    /// draws each character from the source the font set draws it from,
    /// or from none. The first start that fails names the source it added
    /// last. vtium leaves that source out for the rest of the boot, and the
    /// helper started after the search draws from the others. A start that
    /// fails with no source shows that the helper itself fails, and vtium
    /// gives it up. The last start reads every source still in use, so
    /// vtium keeps it as the running helper and takes its answers; once it
    /// has ended the work it was sent, no source fails on its own. When it
    /// fails after vtium sent it new work, the search never tried that
    /// work, so the failure begins a new search.
    ///
    /// Each search uses one of the [`RESTARTS_MAX`] chances a boot gives
    /// the helper when it begins, whatever its end, and so does each
    /// restart that names no source: a helper whose frit has no source,
    /// such as the pointer file's, and an exit with no work outstanding.
    /// A failure past the last chance gives the font set up. The helper
    /// started after a blame uses no chance.
    ///
    /// References:
    /// - ADR-1200 D14: a crash, a fault or a timeout fails one source and
    ///   nothing else.
    /// - ADR-1200 D15, "Restarted after a failure": three chances a boot,
    ///   shared by the searches and the plain restarts.
    fn restart(&mut self, why: &str, now: Instant) {
        // the cell queries the ended helper did not answer are asked again
        self.deferred.extend(self.late.drain(..));
        self.stop();
        self.lose_pointer(&format!("the font helper {why}"));
        if self.sources.is_empty() {
            return self.start_again(why, now);
        }
        // a last start that was sent work after its start may have failed
        // on that work, which says nothing about the source the search
        // added last, so the failure begins a new search
        if let Some(suspect) = self.suspect.take()
            && !suspect.later
        {
            return self.blame(suspect.source, why, now);
        }
        let Some(search) = self.search.take() else {
            let Some(used) = self.use_chance() else {
                return self.fail(&no_chance_left(why));
            };
            log::warn!(
                "the font helper {why}; vtium starts it again with the fonts of the font set {} added one at a time, to find the font that makes it fail ({})",
                escaped(&self.frit.name),
                chance(used)
            );
            self.search = Some(Search {
                why: why.to_owned(),
                reads: 0,
                open: Open::default(),
                sent: now,
            });
            return self.try_sources(now);
        };
        let Some(i) = search
            .reads
            .checked_sub(1)
            .and_then(|last| self.in_use().get(last).copied())
        else {
            return self.fail(&format!("it {why} before it read any font"));
        };
        self.blame(i, why, now);
    }

    /// Leaves out the source at `i` for the rest of the boot, since the
    /// helper `why` when vtium added it, and starts the helper again
    /// without it, its work sent again. A font set left with no source
    /// falls back to the built-in font.
    fn blame(&mut self, i: usize, why: &str, now: Instant) {
        if let Some(source) = self.sources.get(i) {
            self.failures.borrow_mut().left_out.insert(source.clone());
        }
        self.blamed = true;
        log::warn!(
            "the font set {} leaves out source={} until vtium starts again, because the font helper {why} when vtium added that font; the font set's other fonts go on drawing",
            escaped(&self.frit.name),
            escaped(self.sources.get(i).map_or("", String::as_str))
        );
        if self.in_use().is_empty() {
            return self.fail("it failed on every font of the font set");
        }
        self.resend(now, false);
        self.ask_bold_waiting(now);
    }

    /// Starts the helper again after `why`, a failure of a helper whose
    /// frit has no source, its asks sent again, or gives it up past
    /// [`RESTARTS_MAX`].
    fn start_again(&mut self, why: &str, now: Instant) {
        let Some(used) = self.use_chance() else {
            return self.fail(&no_chance_left(why));
        };
        log::warn!(
            "the font helper {why}; vtium starts it again ({})",
            chance(used)
        );
        self.resend(now, false);
    }

    /// Starts the next helper of the search, which reads as many of the
    /// sources still in use as the search says, and sends it every waiting
    /// ask, fill and cell query. The last start reads all of them and is
    /// kept as the running helper ([`Suspect`]).
    fn try_sources(&mut self, now: Instant) {
        self.stop();
        // what the ended helper sent is none of this helper's
        self.early.clear();
        let in_use = self.in_use();
        let Some(search) = &self.search else {
            return;
        };
        if search.reads >= in_use.len() {
            let suspect = Suspect {
                source: in_use.last().copied().unwrap_or(0),
                why: search.why.clone(),
                open: Open::default(),
                later: false,
            };
            self.search = None;
            let open = self.resend(now, false);
            if self.failed.is_none() {
                self.due = (!open.is_empty()).then(|| after(now, self.answer));
                self.suspect = Some(Suspect { open, ..suspect });
                self.settle(now);
            }
            return;
        }
        let open = self.resend(now, true);
        let empty = open.is_empty();
        // a start sent only cell queries is still due to answer them
        self.due = (!empty).then(|| after(now, self.answer));
        if let Some(search) = &mut self.search {
            search.open = open;
            search.sent = now;
        }
        if empty {
            self.tried(now);
        }
    }

    /// The helper of the search ended the work it was sent: the next one
    /// reads one more source.
    fn tried(&mut self, now: Instant) {
        if let Some(search) = &mut self.search {
            search.reads = search.reads.saturating_add(1);
            self.try_sources(now);
        }
    }

    /// Ends the search once the last start has ended the work it was
    /// sent: no source fails on its own, and vtium goes on with that
    /// helper, with every source still in use. The early bold asks that
    /// waited for the search go out then.
    fn settle(&mut self, now: Instant) {
        if self.suspect.as_ref().is_some_and(|s| s.open.is_empty())
            && let Some(suspect) = self.suspect.take()
        {
            log::warn!(
                "the font helper {}, and no font of the font set {} makes it fail again, so vtium keeps every font and goes on with the font helper it started last",
                suspect.why,
                escaped(&self.frit.name)
            );
            self.ask_bold_waiting(now);
        }
    }

    /// Sends the helper, starting it first, each waiting ask, each fill
    /// under way and each cell query that waited for a helper, and returns
    /// that work. A helper of the search (`search`) is sent copies of the
    /// cell queries, whose answers vtium does not take. Otherwise the
    /// queries are asked for good, their answers to come late
    /// ([`Helper::late_cells`]), and each fill starts afresh.
    fn resend(&mut self, now: Instant, search: bool) -> Open {
        self.due = None;
        let mut open = Open::default();
        let waiting = self.deferred.len();
        // each waiting ask's cell, variant, cluster width and what it named
        type Resend = (SpacedCell, Kind, ClusterWidth, Vec<u32>, Vec<Text>);
        let asks: Vec<Resend> = self
            .asks
            .iter_mut()
            .map(|a| {
                a.sent = now;
                let want = a.want.iter().copied().collect();
                let clusters = a.clusters.iter().cloned().collect();
                (
                    a.at,
                    (a.variant, a.doubled),
                    a.cluster_width,
                    want,
                    clusters,
                )
            })
            .collect();
        for (at, of, rule, want, clusters) in asks {
            let bytes = self.request((ASK, at, of, rule), (&want, &clusters), &[]);
            self.post(bytes, now);
            open.asks = open.asks.saturating_add(1);
        }
        let fills: Vec<SpacedCell> = self
            .cells
            .iter()
            .filter(|c| c.fill == Fill::Active)
            .map(|c| c.at)
            .collect();
        for at in fills {
            let Some(state) = self.known(at, Variant::default()) else {
                continue;
            };
            if !search {
                state.brought = Bits::new();
            }
            let (first, held) = (state.first.clone(), state.held.ranges());
            let bytes = self.request(
                (FILL, at, (Variant::default(), None), ClusterWidth::Double),
                (&first, &[]),
                &held,
            );
            self.post(bytes, now);
            open.fills.push(at);
        }
        if search {
            for height in self.deferred.clone() {
                let query = SpacedCell::plain(Cell { width: 0, height });
                let bytes = self.request(
                    (
                        OWN_CELL,
                        query,
                        (Variant::default(), None),
                        ClusterWidth::Double,
                    ),
                    (&[], &[]),
                    &[],
                );
                self.post(bytes, now);
                open.cells = open.cells.saturating_add(1);
            }
        } else {
            // a helper started above was asked them as it started
            self.send_deferred(now);
            open.cells = waiting;
        }
        if self.failed.is_some() {
            return Open::default();
        }
        open
    }

    /// The sources not left out, by their place in the frit.
    fn in_use(&self) -> Vec<usize> {
        let left_out = self.left_out();
        (0..self.sources.len())
            .filter(|&i| left_out & bit(i) == 0)
            .collect()
    }

    /// The sources the next helper leaves out: each that made a helper
    /// fail, and during a search each in use past those the search reads.
    fn leave_out(&self) -> u32 {
        let past = self.search.as_ref().map_or(usize::MAX, |s| s.reads);
        self.in_use()
            .into_iter()
            .skip(past)
            .fold(self.left_out(), |mask, i| mask | bit(i))
    }

    /// Takes `event` from a helper of the search, and returns whether
    /// [`Helper::poll`] stops taking events. vtium takes none of the
    /// answers: a helper that reads only some of the sources draws a
    /// character the font set draws from a later source from none, or from
    /// another one, as an emoji drawn in color by a later source is drawn
    /// as text by an earlier one. Only the end of each ask, fill and cell
    /// query counts, against the work of its kind the helper was sent, and
    /// only such an end or glyphs for open work of their kind move the
    /// next answer's due time. A failed frame ends an ask while one is
    /// open, else a fill, else a cell query, since the helper fails each
    /// kind of request with the same frame. A malformed frame fails the
    /// start, as a crash does.
    fn search_event(&mut self, event: Event, now: Instant) -> bool {
        let Some(open) = self.search.as_mut().map(|s| &mut s.open) else {
            return false;
        };
        let progress = match event {
            Event::Done if open.asks > 0 => {
                open.ask();
                true
            }
            Event::Failed(_) if open.asks > 0 => {
                open.ask();
                true
            }
            Event::Failed(_) if !open.fills.is_empty() => {
                open.fills.remove(0);
                true
            }
            Event::Failed(_) if open.cells > 0 => {
                open.cell();
                true
            }
            Event::Filled(at) | Event::Stopped(at) if open.fills.contains(&at) => {
                open.fill(at);
                true
            }
            Event::OwnCell(_) | Event::OutlineCell(_) | Event::NoOwnCell(_) if open.cells > 0 => {
                open.cell();
                true
            }
            Event::Glyphs { fill: false, .. } => open.asks > 0,
            Event::Glyphs { fill: true, .. } => !open.fills.is_empty(),
            Event::Warning(why) => {
                self.warn(why.trim());
                return false;
            }
            Event::Sandbox(parts) => {
                self.sandboxed(parts);
                return false;
            }
            Event::Malformed(why) => {
                self.restart(&format!("sent an answer vtium refused ({why})"), now);
                return true;
            }
            Event::Closed(status) => {
                self.closed(status, now);
                return true;
            }
            _ => false,
        };
        if !progress {
            return false;
        }
        self.due = Some(after(now, self.answer));
        if self.search.as_ref().is_some_and(|s| s.open.is_empty()) {
            self.tried(now);
        }
        false
    }

    /// Takes what the helper has sent since the last call; ends and starts
    /// again a helper whose answer or ask is past due.
    pub fn poll(&mut self, now: Instant) -> Vec<Answer> {
        let mut out = Vec::new();
        while self.failed.is_none() {
            let early = self.early.pop_front();
            let Some(event) =
                early.or_else(|| self.running.as_ref().and_then(|r| r.events.try_recv().ok()))
            else {
                break;
            };
            if self.search.is_some() {
                if self.search_event(event, now) {
                    break;
                }
                continue;
            }
            let result = match event {
                Event::Glyphs {
                    fill,
                    band,
                    set,
                    runs,
                    bytes,
                } => self
                    .glyphs((fill, band), &set, runs, bytes, now)
                    .map(|a| out.extend(a)),
                Event::Done if self.pointer.is_some() => {
                    self.pointer_done = self.pointer.take().map(|(reply, _)| Ok(reply.finish()));
                    Ok(())
                }
                Event::Done => {
                    if let Some(suspect) = &mut self.suspect {
                        suspect.open.ask();
                    }
                    self.done(now)
                }
                Event::Filled(at) => {
                    if let Some(suspect) = &mut self.suspect {
                        suspect.open.fill(at);
                    }
                    self.filled(at, now)
                }
                Event::Stopped(at) => {
                    if let Some(suspect) = &mut self.suspect {
                        suspect.open.fill(at);
                    }
                    self.stop_taken(at)
                }
                // a cell query's answer after the console stopped waiting,
                // kept (ADR-1200 D17)
                answer @ (Event::OwnCell(_) | Event::OutlineCell(_) | Event::NoOwnCell(_)) => {
                    match self.late.pop_front() {
                        Some(asked) => {
                            if let Some(suspect) = &mut self.suspect {
                                suspect.open.cell();
                            }
                            cell_answer(answer, asked).map(|a| self.kept.push((asked, a)))
                        }
                        None => Err("the helper answered a cell query not made".to_owned()),
                    }
                }
                // a warning answers nothing, so the answer is still due
                Event::Warning(why) => {
                    self.warn(why.trim());
                    continue;
                }
                // nor does the sandbox's report
                Event::Sandbox(parts) => {
                    self.sandboxed(parts);
                    continue;
                }
                Event::Coverage(..) | Event::Sources(_) | Event::DrawsNothing(_) => {
                    Err("the helper answered a coverage or sources query not made".to_owned())
                }
                Event::Pointer(kind, payload) => match &mut self.pointer {
                    Some((reply, _)) => reply.frame(kind, &payload),
                    None => Err("the helper answered a pointer ask not made".to_owned()),
                },
                Event::Failed(why) => Err(failure(&why)),
                // the last start of a search fails as its other starts do,
                // and the source it added is left out
                Event::Malformed(why) if self.suspect.is_some() => {
                    self.restart(&format!("sent an answer vtium refused ({why})"), now);
                    break;
                }
                Event::Malformed(why) => Err(format!("the helper's answer was refused: {why}")),
                Event::Closed(status) => {
                    self.closed(status, now);
                    break;
                }
            };
            if let Err(why) = result {
                self.fail(&why);
                break;
            }
            self.settle(now);
            self.due = (self.owes() || self.suspect.is_some()).then(|| after(now, self.answer));
        }
        if self.failed.is_none() {
            let asks_end = match &self.search {
                Some(search) => search.asks_end(self.request),
                None => self.oldest_sent().map(|sent| after(sent, self.request)),
            };
            if asks_end.is_some_and(|end| end <= now) {
                self.restart(
                    &format!("left an ask unanswered for {} s", self.request.as_secs()),
                    now,
                );
            } else if self.due.is_some_and(|d| d <= now) {
                self.restart(
                    &format!("gave no answer for {} s", self.answer.as_secs()),
                    now,
                );
            }
        }
        out
    }

    /// Takes a frame of glyphs, or refuses it: an ask's must be at the
    /// oldest ask's cell, every strike in its style and its script, and
    /// among the
    /// codepoints and clusters it has yet to send; a fill's at a cell
    /// filling, every strike regular and normal, among those the fill has not
    /// brought, and with no cluster. The store takes the frame without the
    /// glyphs it holds, as a request may send again a glyph an earlier one
    /// brought (ADR-1200 D18), and a frame with nothing new adds nothing.
    /// An ask's past the store's limit compacts the store by the overrun,
    /// the glyphs least recently shown leaving
    /// ([`crate::deferred::Generated::make_room`]), and a frame that then
    /// carries a glyph that left adds it again; a fill's past the limit is
    /// dropped and the fill stopped.
    fn glyphs(
        &mut self,
        (fill, band): (bool, Band),
        set: &Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
        now: Instant,
    ) -> Result<Option<Answer>, String> {
        let not_asked = || "the helper answered what was not asked".to_owned();
        let cells = set.cells();
        let &[drawn] = cells.as_slice() else {
            return Err(not_asked());
        };
        // a fill's glyphs are regular and normal, an ask's in the style
        // and the script it names, and at the double size it names, whose
        // set is at the cell that size gives the ask's cell
        let (variant, doubled) = if fill {
            (Variant::default(), None)
        } else {
            let ask = self.asks.front().ok_or_else(not_asked)?;
            (ask.variant, ask.doubled)
        };
        let at = match doubled {
            None => SpacedCell { cell: drawn, band },
            Some(d) => {
                let ask = self.asks.front().ok_or_else(not_asked)?;
                if d.cell(ask.at.cell) != Some(drawn) || ask.at.band != band {
                    return Err(not_asked());
                }
                ask.at
            }
        };
        if !set.strikes().all(|s| s.style == variant.style) {
            return Err("the helper answered in a style that was not asked for".to_owned());
        }
        if !set.strikes().all(|s| s.script == variant.script) {
            return Err(
                "the helper answered in a superscript or subscript that was not asked for"
                    .to_owned(),
            );
        }
        let clusters: Vec<Text> = set.clusters().map(|(_, _, key)| key).collect();
        let store = Arc::clone(&self.store);
        let without_held = || {
            let store = crate::deferred::lock(&store);
            match doubled {
                None => store.fresh((at, variant), Arc::clone(set), &runs, bytes),
                Some(d) => store.fresh_doubled((at, variant, d), Arc::clone(set), &runs, bytes),
            }
            .ok_or_else(not_asked)
        };
        let fresh = if fill {
            if !clusters.is_empty() {
                return Err("the helper's fill carries a cluster".to_owned());
            }
            if self.stopped.contains(&at) {
                return Ok(None);
            }
            let state = self.known(at, variant).ok_or_else(not_asked)?;
            match state.fill {
                Fill::Active => {}
                Fill::Stopped | Fill::Done => return Ok(None),
                Fill::None => return Err(not_asked()),
            }
            for c in codepoints(&runs) {
                let Some(c) = c.filter(|&c| !state.brought.has(c)) else {
                    return Err("the helper's fill repeats a glyph".to_owned());
                };
                state.brought.put(c, true);
            }
            let fresh = without_held()?;
            if fresh.texts().next().is_none() {
                return Ok(None);
            }
            if crate::deferred::lock(&self.store)
                .bytes()
                .saturating_add(fresh.bytes())
                > self.limit
            {
                self.store_full(at, now);
                return Ok(None);
            }
            fresh
        } else {
            let ask = self.asks.front_mut().ok_or_else(not_asked)?;
            if ask.at != at {
                return Err(not_asked());
            }
            for c in codepoints(&runs) {
                if !c.is_some_and(|c| ask.want.remove(&c)) {
                    return Err("the helper answered a glyph the ask did not name".to_owned());
                }
            }
            // in a build with right-to-left text, a cluster the ask named or
            // a part of one, which the helper sends when a mark of the
            // cluster has no glyph (ADR-1200 D22 step 3) and which leaves the
            // cluster asked for
            #[cfg(feature = "bidi")]
            {
                let named = |t: &Text| {
                    ask.clusters.contains(t) || ask.clusters.iter().any(|k| part_of(t, k))
                };
                if !clusters.iter().all(named) {
                    return Err("the helper answered a cluster the ask did not name".to_owned());
                }
                for t in &clusters {
                    ask.clusters.remove(t);
                }
            }
            #[cfg(not(feature = "bidi"))]
            for t in &clusters {
                if !ask.clusters.remove(t) {
                    return Err("the helper answered a cluster the ask did not name".to_owned());
                }
            }
            let Some(state) = self.known_of(at, (variant, doubled)) else {
                // no display draws at the cell any more
                return Ok(None);
            };
            // answered, held or not; what the store cannot take is asked
            // for again when the screen shows it
            for c in codepoints(&runs).flatten() {
                state.pending.put(c, false);
            }
            for t in &clusters {
                state.clusters.pending.remove(t);
            }
            let mut fresh = without_held()?;
            loop {
                if fresh.bytes() > self.limit {
                    return Ok(None);
                }
                let held = crate::deferred::lock(&self.store).bytes();
                let overrun = held
                    .saturating_add(fresh.bytes())
                    .saturating_sub(self.limit);
                if overrun == 0 {
                    break fresh;
                }
                let left = crate::deferred::lock(&self.store).make_room(overrun);
                for (held, gone) in left {
                    if let Some(state) = self.known_of(held.at, (held.variant, held.doubled)) {
                        for t in gone {
                            state.forget(t);
                        }
                    }
                }
                if crate::deferred::lock(&self.store).bytes() == held {
                    // nothing left to free
                    return Ok(None);
                }
                // a glyph the frame carries may have left the store, and
                // the frame then adds it again
                fresh = without_held()?;
            }
        };
        let added: Vec<Text> = fresh.texts().cloned().collect();
        let Some(face) = crate::deferred::lock(&self.store).add(fresh) else {
            return Ok(None);
        };
        // a cluster that decomposes from an arrived codepoint may draw it
        // now (D22 step 2), so the cells showing its base redraw too
        let mut chars: Vec<char> = added
            .iter()
            .flatten()
            .flat_map(|&c| crate::deferred::decomposition(c))
            .collect();
        // a presentation form draws the letters it presents where their
        // joined form is missing (ADR-1004 D6), so their cells redraw too
        #[cfg(feature = "bidi")]
        chars.extend(
            added
                .iter()
                .flatten()
                .flat_map(|&c| crate::deferred::presented(c)),
        );
        chars.sort_unstable();
        chars.dedup();
        if let Some(state) = self.state_of(at, (variant, doubled)) {
            for t in added {
                state.hold(t);
            }
            if fill {
                state.first_glyphs.get_or_insert(now);
            }
        }
        Ok(Some(Answer { at, face, chars }))
    }

    /// Makes this helper log none of the frit's warnings. `vtium
    /// check-config` starts its helpers so: it names each font a set lacks
    /// in its own words, from the sources query's answer, and the lines the
    /// console logs would only repeat that on standard error.
    ///
    /// References:
    /// - ADR-1204 D7: check-config names each missing font.
    pub fn quiet(&mut self) {
        self.logs_warnings = false;
    }

    /// Logs the frit's warning `why`, once a boot and at most
    /// [`WARNINGS_MAX`] of them: a helper started again reads the frit
    /// again and sends its warnings again. Past the bound, it logs once
    /// that it logs no more of them, so a missing font past the bound is
    /// never dropped in silence. Its control characters are escaped
    /// ([`escaped`]).
    fn warn(&mut self, why: &str) {
        if !self.logs_warnings {
            return;
        }
        let why = escaped(why);
        if self.warned.contains(&why) {
            return;
        }
        if self.warned.len() < WARNINGS_MAX {
            log::warn!("the frit {why}");
            self.warned.insert(why);
        } else if !self.warnings_dropped {
            self.warnings_dropped = true;
            log::warn!(
                "the font helper sent more than {WARNINGS_MAX} warnings about the font set {}, so vtium logs no more of them this boot; run vtium config fonts to see each font the set lacks",
                escaped(&self.frit.name)
            );
        }
    }

    /// Logs the helper's report of its sandbox, `report`, when a part is
    /// missing: once for this helper, which reports again each time it
    /// starts, and once a boot for every helper ([`SANDBOX_LOGGED`]).
    fn sandboxed(&mut self, report: u32) {
        if self.sandbox_noted.is_some() {
            return;
        }
        if let Some(line) = sandbox_notice(report) {
            log_sandbox_once(&SANDBOX_LOGGED, &line);
            self.sandbox_noted = Some(line);
        }
    }

    /// The store is full while `cell` fills: the fill stops there, logged
    /// once a boot.
    fn store_full(&mut self, at: SpacedCell, now: Instant) {
        if let Some(state) = self.state(at, Variant::default()) {
            state.fill = Fill::Stopped;
        }
        self.stop_fill(at, now);
        if !self.full {
            self.full = true;
            log::info!(
                "the glyph store is full at {} KiB: the font's other glyphs are generated as the screen shows them",
                self.limit / 1024
            );
        }
    }

    /// The oldest ask is answered whole: what it did not send, the frit
    /// lacks or cannot shape. When it was the last ask waiting at its cell,
    /// at the normal size, the console makes its early bold ask there.
    fn done(&mut self, now: Instant) -> Result<(), String> {
        let ask = self
            .asks
            .pop_front()
            .ok_or_else(|| "the helper answered an ask not made".to_owned())?;
        if ask.doubled.is_none() && !self.asks.iter().any(|a| a.at == ask.at) {
            self.ask_bold_early(ask.at, now);
        }
        if let Some(state) = self.known_of(ask.at, (ask.variant, ask.doubled)) {
            for &c in &ask.want {
                state.pending.put(c, false);
                state.absent.put(c, true);
            }
            let known = &mut state.clusters;
            for t in ask.clusters {
                known.pending.remove(&t);
                if known.absent.len() >= CLUSTERS_LACKED_MAX {
                    known.absent.clear();
                }
                known.absent.insert(t);
            }
        }
        Ok(())
    }

    /// Asks once at `at` for bold printable ASCII ([`BOLD_EARLY`]), once
    /// the helper has answered every ask sent at the cell. The screen's
    /// first asks then reach the helper first, and the console's first
    /// frame waits for nothing more. The background fill draws regular
    /// glyphs alone, so this ask is what brings a program's bold text
    /// before the program shows it. The console decides once for each
    /// cell it knows of, and decides again only for a cell dropped and
    /// made again. It sends the ask only while the store is under its
    /// limit, so glyphs the screen does not show are never asked for into
    /// a full store. Once sent, the ask's glyphs count in eviction as any
    /// glyphs do: a glyph evicted before the screen shows it comes back
    /// with the screen's own ask.
    ///
    /// While the last start of a search runs, the ask waits until the
    /// search ends. A request sent to that start after it began is work
    /// the search never tried, so a crash on its fill would then begin a
    /// new search and cost another chance, where it should leave out the
    /// source the search added last at once. When [`ASKS_MAX`] asks wait,
    /// the console has not decided, so the cell's next answer tries again.
    ///
    /// References:
    /// - ADR-1200 D15, "Restarted after a failure": the chances a boot
    ///   gives the helper, which a search uses.
    /// - ADR-1200 D18, note of 2026-11-25: the one ask for glyphs the
    ///   screen does not show yet, and when the console sends it.
    /// - ADR-1200 D21, note of 2026-11-25: this ask is not the fill.
    fn ask_bold_early(&mut self, at: SpacedCell, now: Instant) {
        if !self.asks_bold_early || !self.cells.iter().any(|c| c.at == at) {
            return;
        }
        let searching = self.suspect.is_some();
        let full = self.asks.len() >= ASKS_MAX;
        let under = crate::deferred::lock(&self.store).bytes() < self.limit;
        let Some(state) = self.state(at, Variant::default()) else {
            return;
        };
        if state.bold_early {
            return;
        }
        state.bold_waits = searching;
        if searching || full {
            return;
        }
        state.bold_early = true;
        if !under {
            return;
        }
        let bold = Shown {
            variant: vtium_font::Style {
                bold: true,
                italic: false,
            }
            .into(),
            doubled: None,
            cluster_width: ClusterWidth::default(),
            chars: (BOLD_EARLY.0..=BOLD_EARLY.1).collect(),
            clusters: Vec::new(),
        };
        self.ask(at.cell, at.band, &bold, now);
    }

    /// Sends the early bold asks that waited while the last start of a
    /// search ran, once the search has ended. A cell where an ask still
    /// waits makes its bold ask when the helper answers that ask.
    fn ask_bold_waiting(&mut self, now: Instant) {
        if self.search.is_some() || self.suspect.is_some() {
            return;
        }
        let waiting: Vec<SpacedCell> = self
            .cells
            .iter_mut()
            .filter_map(|c| std::mem::take(&mut c.bold_waits).then_some(c.at))
            .collect();
        for at in waiting {
            if !self.asks.iter().any(|a| a.at == at) {
                self.ask_bold_early(at, now);
            }
        }
    }

    /// The helper took the oldest stop at `cell`: the frames after it are
    /// the next fill's. One with no stop sent was not asked.
    fn stop_taken(&mut self, at: SpacedCell) -> Result<(), String> {
        let i = self
            .stopped
            .iter()
            .position(|&c| c == at)
            .ok_or_else(|| "the helper took a stop not sent".to_owned())?;
        self.stopped.remove(i);
        Ok(())
    }

    /// A cell's fill is done: logged with its timings, or refused when the
    /// cell holds no glyph. The end of a fill stopped is dropped.
    fn filled(&mut self, at: SpacedCell, now: Instant) -> Result<(), String> {
        if self.stopped.contains(&at) {
            return Ok(());
        }
        let cell = at.cell;
        let epoch = self.epoch;
        let state = self
            .known(at, Variant::default())
            .ok_or_else(|| "the helper ended a fill not asked for".to_owned())?;
        if state.fill != Fill::Active {
            return Ok(());
        }
        if state.held.ranges().is_empty() {
            return Err(format!(
                "the helper answered no glyph at {}x{}",
                cell.width, cell.height
            ));
        }
        state.fill = Fill::Done;
        let ms = |d: Duration| d.as_millis();
        let started = state.started.unwrap_or(now);
        let since = |t: Instant| ms(t.saturating_duration_since(started));
        log::info!(
            "the font's glyphs at {}x{} are generated: asked {} ms after vtium started, the first in {} ms, the last in {} ms",
            cell.width,
            cell.height,
            ms(started.saturating_duration_since(epoch)),
            state.first_glyphs.map_or(0, since),
            since(now)
        );
        Ok(())
    }

    /// The helper closed its end and was reaped. With nothing outstanding,
    /// an exit 0 is its idle exit and the next need starts another; any
    /// other end, or an exit with work outstanding, counts as a start
    /// again, made now when work is outstanding. A cell query the helper
    /// has not answered is work outstanding, and it is asked again of the
    /// next helper.
    fn closed(&mut self, status: Option<ExitStatus>, now: Instant) {
        self.running = None;
        self.stopped.clear();
        self.deferred.extend(self.late.drain(..));
        let clean = status.is_some_and(|s| s.success());
        let why = match status.and_then(|s| s.signal()) {
            Some(SIGXCPU | 9) => "reached its CPU limit".to_owned(),
            Some(sig) => format!("was ended by signal {sig}"),
            None => match status {
                Some(s) => format!("exited ({s})"),
                None => "exited".to_owned(),
            },
        };
        // a helper of the search that ends before it answered fails, and
        // so does the last start before it ended the work it was sent
        if self.busy()
            || !self.deferred.is_empty()
            || self.search.is_some()
            || self.suspect.is_some()
        {
            return self.restart(&why, now);
        }
        self.due = None;
        if !clean {
            let Some(used) = self.use_chance() else {
                return self.fail(&no_chance_left(&why));
            };
            log::warn!(
                "the font helper {why}; vtium restarts it when it next needs it ({})",
                chance(used)
            );
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `text` from the helper with its control characters escaped, so that the
/// helper's text cannot steer what shows the log.
pub(crate) fn escaped(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escaped = c.is_control().then(|| c.escape_default());
            escaped
                .into_iter()
                .flatten()
                .chain((!c.is_control()).then_some(c))
        })
        .collect()
}

/// Starts one of a helper's threads, `name` its role.
fn thread(name: &str, f: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name(format!("vtium-helper-{name}"))
        .spawn(f)
        .map(drop)
}

/// Waits for the helper `child` to exit, ends each process it started in
/// its process group, and reaps it. It returns how the helper exited. Until it is reaped its
/// pid, and with it the group's id, names no other process, so the
/// group's end reaches only what the helper started.
fn reap(mut child: Child) -> Option<ExitStatus> {
    if let Some(pid) = i32::try_from(child.id()).ok().and_then(Pid::from_raw) {
        loop {
            match waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
            ) {
                Err(rustix::io::Errno::INTR) => {}
                Ok(_) => {
                    let _ = kill_process_group(pid, Signal::KILL);
                    break;
                }
                Err(_) => break,
            }
        }
    }
    child.wait().ok()
}

fn after(t: Instant, d: Duration) -> Instant {
    t.checked_add(d).unwrap_or(t)
}

/// The shell the tests start in place of the font helper: a copy of
/// `/bin/sh` that the test's own account owns, beside the test binary.
/// vtium starts a helper only when root or its own account owns the
/// program, and `/bin/sh` belongs to root only where the tests see root's
/// files as root's. In a user namespace that does not map root, such as
/// the one `make arm-test` runs the tests in (tools/arm/README.md), every
/// file of root's belongs to the overflow uid, 65534. Each test process
/// makes its copy once and renames it into place, so a test process that
/// runs at the same time never starts a copy that is still being written.
/// `cp` writes the copy (`vtium_testscript::executable_copy`), so this
/// process never holds the copy open for writing. Had it done so, a child
/// that another test thread forked during the copy would keep the copy
/// busy until that child ran its own program, and the kernel would refuse
/// to start the shell with ETXTBSY.
#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
pub(crate) fn stand_in_shell() -> &'static Path {
    static SHELL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    SHELL.get_or_init(|| {
        let exe = std::env::current_exe().expect("the test binary");
        let dir = exe.parent().expect("the test binary's directory");
        let shell = dir.join("vtium-stand-in-sh");
        let part = dir.join(format!("vtium-stand-in-sh.{}", std::process::id()));
        vtium_testscript::executable_copy(Path::new("/bin/sh"), &part);
        std::fs::rename(&part, &shell).expect("the copy in place");
        shell
    })
}

/// `script` for the stand-in shell, with traps first that end the shell
/// without a core dump when SIGSEGV or SIGXCPU reaches it. The tests end
/// the stand-in with those signals on purpose, and the kernel would write a
/// core dump of the shell for each one. Where `core_pattern` pipes to
/// systemd-coredump, the kernel ignores the core limit, so each dump lands
/// in the journal and the coredump store of the machine that runs the
/// tests. A process can turn its own dumps off with
/// `prctl(PR_SET_DUMPABLE, 0)`, but each exec turns them on again, and the
/// shell cannot make that call. So each trap runs this test binary in the
/// shell's place, its [`tests::the_stand_in_dies_without_a_core_dump`]
/// alone, which turns its dumps off and then sends itself the signal. The
/// process that vtium started still ends by the signal the script sent.
#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
pub(crate) fn undumpable(script: &str) -> String {
    let exe = std::env::current_exe().expect("the test binary");
    let exe = exe.to_str().expect("the test binary's path in UTF-8");
    assert!(
        !exe.contains(['\'', '"', '$', '`', '\\']),
        "the test binary's path needs no quoting: {exe}"
    );
    let trap = |signal: &str| {
        format!(
            "trap '{DIE_BY}={signal} exec \"{exe}\" --exact helper::tests::the_stand_in_dies_without_a_core_dump -q >/dev/null 2>&1' {signal}; "
        )
    };
    format!("{}{}{script}", trap("SEGV"), trap("XCPU"))
}

/// The environment variable that names the signal by which
/// [`tests::the_stand_in_dies_without_a_core_dump`] ends its process.
#[cfg(test)]
const DIE_BY: &str = "VTIUM_TEST_DIE_BY";

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests;
