//! How the console takes each answer the font helper sends.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vtium_term::ClusterWidth;

use vtium_font::set::Set;
use vtium_font::spacing::{Band, SpacedCell};
use vtium_font::{Cell, Variant};

use crate::deferred::{Shown, Text};

#[cfg(feature = "bidi")]
use super::cells::part_of;
use super::cells::{Answer, Bits, Fill, codepoints};
use super::sandbox::{SANDBOX_LOGGED, failure, log_sandbox_once, sandbox_notice};
use super::search::{Open, chance, no_chance_left};
use super::wire::{ASK, Event, FILL, Kind, OWN_CELL, cell_answer};
use super::{ASKS_MAX, BOLD_EARLY, CLUSTERS_LACKED_MAX, Helper, WARNINGS_MAX, after};

/// `SIGXCPU`, which an exhausted `RLIMIT_CPU` sends.
const SIGXCPU: i32 = 24;

impl Helper {
    /// Asks the helper, starting it first, the cell queries that waited
    /// for it, each answered late.
    pub(super) fn send_deferred(&mut self, now: Instant) {
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

    /// Sends the helper, starting it first, each waiting ask, each fill
    /// under way and each cell query that waited for a helper, and returns
    /// that work. A helper of the search (`search`) is sent copies of the
    /// cell queries, whose answers vtium does not take. Otherwise the
    /// queries are asked for good, their answers to come late
    /// ([`Helper::late_cells`]), and each fill starts afresh.
    pub(super) fn resend(&mut self, now: Instant, search: bool) -> Open {
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
            #[cfg(test)]
            {
                self.taken = self.taken.saturating_add(1);
            }
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
                Fill::None | Fill::Held => return Err(not_asked()),
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
    pub(super) fn warn(&mut self, why: &str) {
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
    pub(super) fn sandboxed(&mut self, report: u32) {
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
    pub(super) fn done(&mut self, now: Instant) -> Result<(), String> {
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
    pub(super) fn ask_bold_early(&mut self, at: SpacedCell, now: Instant) {
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
    pub(super) fn ask_bold_waiting(&mut self, now: Instant) {
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
    pub(super) fn closed(&mut self, status: Option<ExitStatus>, now: Instant) {
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
