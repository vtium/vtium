//! What the console asks the font helper: the glyphs the screens show, the
//! character cells a head waits for, and what vtium does with an answer
//! that comes late or a refusal.

use std::rc::Rc;
use std::time::Instant;

use vtium_font::spacing::SpacedCell;

use crate::font::Chosen;
use crate::head::Head;
use crate::helper::{self, Answer, Helper};
use crate::term::View;

use super::fonts::draw_height;
use super::{App, physical_memory};

impl App {
    /// While head `h`'s font is a frit, fills each cell a display of a
    /// head drawing that frit draws at and that was not filled yet, the
    /// screen's characters first, and asks for the screen's characters
    /// there; drops the glyphs of cells no display draws at any more. At a
    /// cell drawn while the head waits for the frit's own cell, the asks go
    /// and the fill waits until the wait ends ([`App::start_held_fills`]),
    /// since the frit may give no glyph at that cell.
    ///
    /// References:
    /// - ADR-1301 D2, the operator's fill model: the helper draws the
    ///   characters the screen shows first, then the rest of the frit's
    ///   coverage, for each cell the displays draw at.
    pub(super) fn ask_glyphs(&mut self, h: usize) {
        let Some(name) = self.heads.get(h).and_then(Head::frit).map(str::to_owned) else {
            return;
        };
        let drawing: Vec<&Head> = self
            .heads
            .iter()
            .filter(|head| head.frit() == Some(name.as_str()))
            .collect();
        // each cell with the rows line spacing adds to it (ADR-1302 D13):
        // two heads can draw one cell size with different bands, and each
        // band has glyphs of its own
        let mut cells: Vec<SpacedCell> = drawing
            .iter()
            .flat_map(|head| head.views.values().map(View::spaced))
            .collect();
        cells.sort_unstable_by_key(|a| (a.cell.height, a.cell.width, a.band));
        cells.dedup();
        let now = (self.clock)();
        let Some(frit) = self.frits.get_mut(&name) else {
            return;
        };
        if let Some(helper) = &mut frit.helper {
            helper.retain(&cells, now);
        }
        frit.asked.retain(|c| cells.contains(c));
        let new: Vec<(SpacedCell, Vec<crate::deferred::Shown>)> = cells
            .into_iter()
            .filter(|c| !frit.asked.contains(c))
            .map(|at| {
                let showing = drawing
                    .iter()
                    .filter(|head| head.views.values().any(|v| v.spaced() == at));
                (at, shown(showing))
            })
            .collect();
        if new.is_empty() {
            return;
        }
        let waiting = self.cells_waiting(&name);
        let Some(helper) = self.helper(&name) else {
            return;
        };
        for (at, visible) in &new {
            for shown in visible {
                helper.ask(at.cell, at.band, shown, now);
            }
            // the fill is regular, and draws the regular glyphs shown first
            let first = visible
                .first()
                .map(|s| s.chars.as_slice())
                .unwrap_or_default();
            if waiting.contains(at) {
                helper.hold_fill(*at, first);
            } else {
                helper.fill(at.cell, at.band, first, now);
            }
        }
        if let Some(frit) = self.frits.get_mut(&name) {
            frit.asked.extend(new.into_iter().map(|(at, _)| at));
        }
    }

    /// While head `h`'s font is a frit and its screen changed, asks for
    /// each character and cluster it shows that the store does not hold,
    /// at each cell its displays draw at, and marks the glyphs of the
    /// others as used.
    fn ask_visible(&mut self, h: usize, now: Instant) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let Some(frit) = head.frit().and_then(|name| self.frits.get_mut(name)) else {
            return;
        };
        let Some(helper) = &mut frit.helper else {
            return;
        };
        let visible = head.visible();
        for &at in &frit.asked {
            if head.views.values().any(|v| v.spaced() == at) {
                for shown in &visible {
                    crate::deferred::lock(&frit.generated).touch(at, shown);
                    helper.ask(at.cell, at.band, shown, now);
                }
            }
        }
    }

    /// The helper of the frit `name`, started on the first need with a
    /// wake-up into the loop for its answers and the store its glyphs go
    /// into; `None`, logged once, when it cannot be.
    pub(super) fn helper(&mut self, name: &str) -> Option<&mut Helper> {
        let frit = self.frits.get(name)?;
        if frit.helper.is_none() && !frit.refused {
            let record = Rc::clone(self.failures.entry(name.to_owned()).or_default());
            let launcher = self.launcher.clone();
            let started = match launcher.and_then(|l| self.helper_wake().map(|w| (l, w))) {
                Ok((launcher, wake)) => Ok(Helper::new(
                    launcher,
                    wake,
                    self.epoch,
                    frit.frit.clone(),
                    self.console_font.clone(),
                    std::sync::Arc::clone(&frit.generated),
                    crate::head::frit_store(
                        crate::deferred::store_limit(physical_memory()),
                        self.heads.len(),
                    ),
                )
                .remembering(record))
                .map(|helper| match &self.root_helpers {
                    Some(root) => {
                        helper.through_root(Rc::clone(root) as Rc<dyn crate::helper::Root>)
                    }
                    None => helper,
                }),
                Err(why) => {
                    log::warn!(
                        "the font helper could not draw the font ({why}), so vtium uses its built-in font, one character at a time with no combined characters"
                    );
                    Err(())
                }
            };
            let frit = self.frits.get_mut(name)?;
            match started {
                Ok(helper) => frit.helper = Some(helper),
                Err(()) => frit.refused = true,
            }
        }
        self.frits.get_mut(name)?.helper.as_mut()
    }

    /// A wake-up into the loop for a helper's answers, which the helper's
    /// reader calls after each frame.
    pub(super) fn helper_wake(&self) -> Result<std::sync::Arc<dyn Fn() + Send + Sync>, String> {
        let (ping, source) = vtium_platform::calloop::ping::make_ping().map_err(|e| {
            format!("vtium cannot make a wake-up for the font helper's answers: {e}")
        })?;
        self.handle
            .insert_source(source, |(), (), _: &mut App| {})
            .map_err(|e| format!("vtium cannot watch for the font helper's answers: {e}"))?;
        let ping = std::sync::Mutex::new(ping);
        Ok(std::sync::Arc::new(move || {
            if let Ok(p) = ping.lock() {
                p.ping();
            }
        }))
    }

    /// Takes each helper's answers: each display at a set's cell, of every
    /// head drawing the frit, forgets the glyphs it cached for the set's
    /// characters, so none draws the built-in font once the frit sent it
    /// (ADR-1200 D7), and redraws the cells that show one. An answer to a
    /// cell query that came after its wait goes to [`App::late_cell`].
    pub(super) fn helper_turn(&mut self, now: Instant) {
        for h in 0..self.heads.len() {
            if self
                .heads
                .get(h)
                .is_some_and(|head| head.console.term.changed)
            {
                self.ask_visible(h, now);
            }
        }
        let mut late = Vec::new();
        let mut blamed = Vec::new();
        for (name, frit) in &mut self.frits {
            let Some(helper) = &mut frit.helper else {
                continue;
            };
            let answers = helper.poll(now);
            if helper.take_blamed() {
                blamed.push(name.clone());
            }
            late.extend(
                helper
                    .late_cells()
                    .into_iter()
                    .map(|(height, answer)| (name.clone(), height, answer)),
            );
            if answers.is_empty() {
                continue;
            }
            // Arrival is damage, as program output is: the answers since
            // the last turn merge into one set per cell, which the next
            // frame draws once, and only the cells showing their
            // characters draw.
            let mut arrived: Vec<(SpacedCell, Vec<char>)> = Vec::new();
            for Answer { at, chars, .. } in answers {
                match arrived.iter_mut().find(|(of, _)| *of == at) {
                    Some((_, all)) => all.extend(chars),
                    None => arrived.push((at, chars)),
                }
            }
            for head in &mut self.heads {
                if head.frit() != Some(name.as_str()) {
                    continue;
                }
                // every character shown, in any style
                let mut visible: Vec<char> =
                    head.visible().into_iter().flat_map(|s| s.chars).collect();
                visible.sort_unstable();
                visible.dedup();
                for (at, chars) in &arrived {
                    let (shown, hidden): (Vec<char>, Vec<char>) =
                        chars.iter().partition(|c| visible.binary_search(c).is_ok());
                    let mut missing = false;
                    for v in head.views.values_mut() {
                        if v.spaced() == *at {
                            v.forget_glyphs(&hidden);
                            if !shown.is_empty() {
                                v.glyphs_changed(&shown);
                            }
                            missing |= v.redraws_missing();
                        }
                    }
                    // a cell that drew the font's missing glyph draws it
                    // again from the set that arrived, though no cell shows
                    // a character of the set
                    head.draw_now |= !shown.is_empty() || missing;
                }
            }
        }
        for name in blamed {
            self.ask_cells_again(&name, now);
        }
        for (name, height, answer) in late {
            self.late_cell(&name, height, answer);
        }
        self.end_cell_waits();
        self.start_held_fills(now);
    }

    /// The cells the displays of each head drawing frit `name` draw at
    /// while the head waits for the frit's own cell ([`App::waits_for_cell`]).
    fn cells_waiting(&self, name: &str) -> Vec<SpacedCell> {
        let mut cells = Vec::new();
        for (h, head) in self.heads.iter().enumerate() {
            if head.frit() == Some(name) && self.waits_for_cell(h) {
                cells.extend(head.views.values().map(View::spaced));
            }
        }
        cells
    }

    /// Starts each fill that a frit's helper holds at a cell where no head
    /// waits for the frit's own cell any more. The wait ends with the
    /// helper's late answer: a cell that lays the head out in the frit's
    /// own cell drops the cell drawn while it waited, and its fill with it,
    /// and a refusal leaves the head in the cell it draws in, whose fill
    /// then starts. A failed helper ends the wait too, and fills nothing.
    /// The state alone ends the hold, never a timer.
    ///
    /// References:
    /// - ADR-1200 D17: a refusal leaves the display's cell in use, and an
    ///   answer that comes after the wait is used from the next frame.
    fn start_held_fills(&mut self, now: Instant) {
        let holding: Vec<String> = self
            .frits
            .iter()
            .filter(|(_, f)| f.helper.as_ref().is_some_and(Helper::holds_fills))
            .map(|(name, _)| name.clone())
            .collect();
        for name in holding {
            let waiting = self.cells_waiting(&name);
            if let Some(helper) = self.frits.get_mut(&name).and_then(|f| f.helper.as_mut()) {
                helper.start_held_fills(&waiting, now);
            }
        }
    }

    /// Whether head `h` draws a font set whose cell at the head's height
    /// the font helper has yet to give: the helper gave no answer in time,
    /// it still runs or starts again, and no later answer has come. The
    /// head draws in the cell it kept until the answer comes. A cell that
    /// vtium forgot and asked for again, after it left out a font that
    /// made the helper fail ([`App::ask_cells_again`]), is waited for in
    /// the same way until its answer comes.
    pub(super) fn waits_for_cell(&self, h: usize) -> bool {
        let Some(head) = self.heads.get(h) else {
            return false;
        };
        let at = draw_height(head, self.dpi(h), &self.locale);
        let wants = head
            .font
            .as_ref()
            .is_some_and(|f| f.wants_own_cell(at, head.sized()));
        let Some(frit) = head.frit().and_then(|name| self.frits.get(name)) else {
            return false;
        };
        let Some(helper) = frit.helper.as_ref().filter(|h| h.failed().is_none()) else {
            return false;
        };
        if !wants {
            return false;
        }
        let (struck, outline) = (head.chosen().struck(), head.chosen().outline());
        let known = if struck {
            frit.own.get(&at).cloned()
        } else {
            frit.answer_at(at, outline)
        };
        match known {
            Some(known) => known.is_err_and(|no| no.late),
            // A bitmap font set's cell holds at every height, so a query at
            // any height may bring it.
            None => helper.asks_cell((struck || outline).then_some(at)),
        }
    }

    /// Sends the shown console of each head the replies it held since its
    /// font switch, once the head waits no more for the new font set's
    /// cell: the helper's late answer laid the head out in the new cell,
    /// or the helper failed and the head keeps its cell. A zoom or a new
    /// font that leaves nothing to wait for ends the wait too. The terminal
    /// answers the held size reports from the cell and the grid the head
    /// has now, so once the program has the new font set's name, every
    /// size it is told is the new cell's. The state alone ends the wait,
    /// never a timer.
    ///
    /// A hidden console keeps its own grid and cell until it is shown, so
    /// the late cell does not lay it out. Its hold therefore goes on until
    /// the head shows it. Every switch of the console shown lays the head
    /// out again before this turn's end, so the console sends its replies
    /// from the new cell.
    ///
    /// References:
    /// - ADR-1401 D1: a hidden console keeps its own grid and cell.
    fn end_cell_waits(&mut self) {
        for h in 0..self.heads.len() {
            let held = self
                .heads
                .get(h)
                .is_some_and(|head| head.console.term.terminal.replies_held());
            if !held || self.waits_for_cell(h) {
                continue;
            }
            if let Some(head) = self.heads.get_mut(h) {
                head.console.send_held_replies();
            }
        }
    }

    /// Frit `name` left out a source that made its helper fail. That source
    /// may be the one that gave the frit's cell, so the console forgets
    /// every cell the helper answered and asks for each again without
    /// waiting. The late answers decide the cell ([`App::late_cell`]).
    ///
    /// References:
    /// - ADR-1200 D14: a font that makes the helper fail is left out, and
    ///   the font set's other fonts go on drawing.
    /// - ADR-1200 D17: an answer that comes after the wait takes the place
    ///   of the missing one.
    pub(super) fn ask_cells_again(&mut self, name: &str, now: Instant) {
        let Some(frit) = self.frits.get_mut(name) else {
            return;
        };
        let heights: Vec<u32> = frit.own.keys().copied().collect();
        frit.own.clear();
        if let Some(helper) = &mut frit.helper {
            helper.ask_cells_again(&heights, now);
        }
    }

    /// The helper's answer to frit `name`'s cell query at `height`, which
    /// came after the console stopped waiting for it (ADR-1200 D17, the
    /// maintainer, 2026-11-03). It takes the place of a missing answer
    /// alone, a timeout or no answer at all, and is then logged in one
    /// line, so a refusal holds as one in time does. A cell is missing
    /// where [`Frit::answer_at`] finds no answer in time for it: for a
    /// bitmap first source's cell, at any height, and otherwise at
    /// `height`. An answer the frit already holds, as when an OSC 50 switch
    /// back asked again and its answer came in time behind this one, stays,
    /// and the late one is dropped. A cell taken lays out again each head
    /// drawing the frit that takes it: for a bitmap first source's cell
    /// every such head, whatever its height, and for an outline source's
    /// those drawing at `height` alone, so the next frame draws in it.
    pub(super) fn late_cell(
        &mut self,
        name: &str,
        height: u32,
        answer: Result<helper::OwnCell, helper::NoCell>,
    ) {
        // A frit with strikes holds each answer at its own height, as an
        // outline frit does.
        let struck = self
            .heads
            .iter()
            .any(|head| head.frit() == Some(name) && head.chosen().struck());
        let Some(frit) = self.frits.get_mut(name) else {
            return;
        };
        let outline = match &answer {
            Ok(helper::OwnCell::Bitmap(_)) => Some(struck),
            Ok(helper::OwnCell::Outline(_)) => Some(true),
            Err(_) => None,
        };
        let held = match outline {
            Some(outline) if !struck => frit.answer_at(height, outline),
            _ => frit.own.get(&height).cloned(),
        };
        if held.is_some_and(|known| !known.is_err_and(|no| no.late)) {
            return;
        }
        match &answer {
            Ok(helper::OwnCell::Bitmap(cell)) => log::info!(
                "the frit {name}'s own {}x{} cell came after the wait: it draws in it from the next frame",
                cell.width,
                cell.height
            ),
            Ok(helper::OwnCell::Outline(cell)) => log::info!(
                "the frit {name}'s {}x{} cell at {height} pixels came after the wait: it draws in it from the next frame",
                cell.width,
                cell.height
            ),
            Err(_) => {}
        }
        let refused = answer.as_ref().err().map(ToString::to_string);
        frit.own.insert(height, answer);
        if let Some(why) = refused {
            return self.late_refusal(name, height, &why);
        }
        let Some(outline) = outline else {
            return;
        };
        for h in 0..self.heads.len() {
            let dpi = self.dpi(h);
            let takes = self.heads.get(h).is_some_and(|head| {
                let at = draw_height(head, dpi, &self.locale);
                head.frit() == Some(name)
                    && (!outline || at == height)
                    && head
                        .font
                        .as_ref()
                        .is_some_and(|f| f.wants_own_cell(at, head.sized()))
            });
            // The cell's width can change at the height a view already
            // draws at, so each view is rebuilt with the new face.
            if takes {
                if let Some(head) = self.heads.get_mut(h) {
                    head.refont = true;
                }
                self.relayout(h);
            }
        }
    }

    /// The helper refused frit `name`'s cell at `height` after the wait,
    /// for `why`. A head that draws in a cell the frit gave it earlier at
    /// that height stops drawing in it: the frit is chosen afresh for the
    /// head and laid out again, so the head draws in the display's cell, or
    /// as wide as the built-in font, as after a refusal in time. This
    /// happens when vtium left out the font that gave the cell, after the
    /// font made the font helper fail, and asked for the cell again. A
    /// head that drew in no such cell keeps the cell it draws in.
    ///
    /// References:
    /// - ADR-1200 D14: a font that makes the helper fail is left out, and
    ///   the font set's other fonts go on drawing.
    /// - ADR-1204 D5: a missing text font leaves the cell to the
    ///   configuration.
    fn late_refusal(&mut self, name: &str, height: u32, why: &str) {
        let mut drawing = Vec::new();
        for h in 0..self.heads.len() {
            let dpi = self.dpi(h);
            let Some(head) = self.heads.get(h) else {
                continue;
            };
            let at = draw_height(head, dpi, &self.locale);
            let took = match &head.font {
                Some(Chosen::Frit {
                    name: drawn,
                    own,
                    heights,
                    ..
                }) if drawn == name => own.is_some() || (at == height && heights.contains_key(&at)),
                _ => false,
            };
            if took {
                drawing.push(h);
            }
        }
        if drawing.is_empty() {
            log::info!(
                "the helper refused the frit {name}'s own cell after the wait ({why}): it keeps the cell it draws in"
            );
            return;
        }
        log::info!(
            "the helper refused the frit {name}'s own cell after the wait ({why}): it no longer draws in the cell the frit gave earlier, from the next frame"
        );
        for h in drawing {
            let Some(head) = self.heads.get_mut(h) else {
                continue;
            };
            let fresh = match &head.font {
                Some(Chosen::Frit { frit, .. }) => {
                    Chosen::frit(frit.clone(), head.sized(), head.look_cell()).ok()
                }
                _ => None,
            };
            let Some(fresh) = fresh else {
                continue;
            };
            head.font = Some(fresh);
            head.refont = true;
            self.share(h);
            self.relayout(h);
        }
    }

    /// What the console knows of frit `name`'s cell: the helper's answer to
    /// each cell query, or why there was none, by the height asked at
    /// (ADR-1200 D17).
    pub fn frit_cells(&self, name: &str) -> Vec<(u32, Result<helper::OwnCell, helper::NoCell>)> {
        self.frits.get(name).map_or_else(Vec::new, |frit| {
            frit.own.iter().map(|(h, a)| (*h, a.clone())).collect()
        })
    }
}

/// What the screens of `heads` show, merged variant by variant: each
/// character and cluster any of them shows in that variant, sorted and
/// once, the regular normal variant first.
fn shown<'a>(heads: impl Iterator<Item = &'a &'a Head>) -> Vec<crate::deferred::Shown> {
    let mut all = vec![crate::deferred::Shown::default()];
    for one in heads.flat_map(|head| head.visible()) {
        match all.iter_mut().find(|s| s.variant == one.variant) {
            Some(s) => {
                s.chars.extend(one.chars);
                s.clusters.extend(one.clusters);
            }
            None => all.push(one),
        }
    }
    for s in &mut all {
        s.chars.sort_unstable();
        s.chars.dedup();
        s.clusters.sort_unstable();
        s.clusters.dedup();
    }
    all
}
