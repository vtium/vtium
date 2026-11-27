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
//! The wire format is `vtium-config`'s serve mode's (`serve/wire.rs`), whose
//! constants this module repeats.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
#[cfg(test)]
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vtium_term::ClusterWidth;

use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Script, Variant};
use vtium_platform::vt::font::ConsoleFont;

use crate::deferred::{Shared, Shown, Text};

mod answers;
mod cells;
mod launcher;
mod process;
mod sandbox;
mod search;
mod wire;

pub(crate) use answers::escaped;
pub use cells::Answer;
use cells::{Ask, Bits, CellState, Fill};
pub use launcher::{Command, Launcher, PROGRAM, font_dirs, nobody, open_program, program};
use process::Running;
#[cfg(feature = "split")]
pub use process::{Launched, Root};
use sandbox::failure;
pub use sandbox::{sandbox_args, sandbox_on};
use search::{Search, Suspect, bit, source_lines};
use wire::{ASK, COVERAGE, FILL, Kind, OWN_CELL, SOURCES, STOP, cell_answer};
pub(crate) use wire::{Event, frame};
pub use wire::{FritText, NoCell, OwnCell};

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
    /// vtium's root process, which starts and stops each of this helper's
    /// processes under the privilege split; `None` when this process
    /// starts them itself.
    #[cfg(feature = "split")]
    root: Option<Rc<dyn Root>>,
    /// How many of the helper's frames [`Helper::poll`] has taken, for a
    /// test that moves its clock only once the helper's frames are in.
    #[cfg(test)]
    taken: u64,
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
            #[cfg(feature = "split")]
            root: None,
            #[cfg(test)]
            taken: 0,
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

    /// Makes `root` start and stop each of the helper's processes, in
    /// place of this process. Under the privilege split the font helper
    /// runs as another account than the drawing process, so only vtium's
    /// root process can start it or signal it.
    ///
    /// References:
    /// - ADR-1400 D4, "What the broker does": the broker starts and stops
    ///   the helpers and passes each helper's socket.
    #[cfg(feature = "split")]
    #[must_use]
    pub fn through_root(mut self, root: Rc<dyn Root>) -> Helper {
        self.root = Some(root);
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
