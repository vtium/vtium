//! The search for the font source that makes the font helper fail.

use std::time::{Duration, Instant};

use vtium_font::spacing::SpacedCell;

use super::answers::escaped;
use super::wire::{Event, LEAVE_OUT, REQUEST_MAGIC, put32};
use super::{Helper, RESTARTS_MAX, after};

/// The search for the source that made the helper fail
/// ([`Helper::restart`]).
pub(super) struct Search {
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
    pub(super) fn asks_end(&self, request: Duration) -> Option<Instant> {
        (self.open.asks > 0).then(|| after(self.sent, request))
    }
}

/// The work a helper was sent at its start and has not ended: its asks,
/// the cells of its fills, and its cell queries.
#[derive(Default)]
pub(super) struct Open {
    pub(super) asks: usize,
    pub(super) fills: Vec<SpacedCell>,
    pub(super) cells: usize,
}

impl Open {
    fn is_empty(&self) -> bool {
        self.asks == 0 && self.fills.is_empty() && self.cells == 0
    }

    /// An ask ended, done or failed.
    pub(super) fn ask(&mut self) {
        self.asks = self.asks.saturating_sub(1);
    }

    /// The fill at `at` ended, filled or stopped.
    pub(super) fn fill(&mut self, at: SpacedCell) {
        if let Some(i) = self.fills.iter().position(|&c| c == at) {
            self.fills.remove(i);
        }
    }

    /// A cell query was answered.
    pub(super) fn cell(&mut self) {
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
pub(super) struct Suspect {
    /// The place in the frit of the source the search added last.
    source: usize,
    /// How the helper failed, for the line that says no source fails on
    /// its own.
    why: String,
    pub(super) open: Open,
    /// vtium sent the helper a request after its start.
    pub(super) later: bool,
}

/// The words that count chance `n` of the [`RESTARTS_MAX`] a boot gives the
/// helper, for the line that says vtium starts it again or looks for the
/// font that makes it fail.
pub(super) fn chance(n: u32) -> String {
    format!("chance {n} of the {RESTARTS_MAX} that vtium gives the font helper each boot")
}

/// The reason vtium gives a font set up when the helper fails after its
/// last chance: what happened to the helper, `why`, such as "was ended by
/// signal 11", and that vtium has used every chance a boot gives it.
/// Every caller passes `why` without a subject, so the reason reads the
/// same whichever path gives the font set up.
pub(super) fn no_chance_left(why: &str) -> String {
    format!(
        "the font helper {why} after vtium had used all {RESTARTS_MAX} chances that it gives the font helper each boot"
    )
}

/// The bit of the source at `i` in a leave-out request.
pub(super) fn bit(i: usize) -> u32 {
    u32::try_from(i)
        .ok()
        .and_then(|i| 1u32.checked_shl(i))
        .unwrap_or(0)
}

/// A leave-out request, which names the sources the helper leaves out by
/// the bits of `mask`.
pub(super) fn leave_out_request(mask: u32) -> Vec<u8> {
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
pub(super) fn source_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .filter_map(|line| {
            let body = line.split('#').next().unwrap_or_default().trim();
            let (key, value) = body.split_once('=')?;
            (key.trim() == "source").then(|| value.trim().to_owned())
        })
        .take(vtium_font::set::layer::SET_SOURCES_MAX)
        .collect()
}

impl Helper {
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
    pub(super) fn restart(&mut self, why: &str, now: Instant) {
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
    pub(super) fn blame(&mut self, i: usize, why: &str, now: Instant) {
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
    pub(super) fn settle(&mut self, now: Instant) {
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

    /// The sources not left out, by their place in the frit.
    fn in_use(&self) -> Vec<usize> {
        let left_out = self.left_out();
        (0..self.sources.len())
            .filter(|&i| left_out & bit(i) == 0)
            .collect()
    }

    /// The sources the next helper leaves out: each that made a helper
    /// fail, and during a search each in use past those the search reads.
    pub(super) fn leave_out(&self) -> u32 {
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
    pub(super) fn search_event(&mut self, event: Event, now: Instant) -> bool {
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
}
