//! The render threads and the frames: each head's painter, the frames the
//! loop takes from each display, the GPU path and the fallback to the CPU,
//! and the render timer's log lines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use vtium_platform::display::{DisplayId, Dpms};
use vtium_render::{DrawState, Orientation};

use crate::config::keys;
use crate::head::Head;
use crate::osd::{self, Osd};
use crate::painter::{Note, Painter};
use crate::term::{Prepared, ScrollDraw, View};
use crate::video::Video;

use super::{App, BRIEF_NEVER, after, scale};

/// How a head draws on its displays, as [`App::layout_of`] records it for
/// [`App::layout_moved`] to compare with after a layout: the head's grid,
/// and the character cell and the orientation of each display.
pub(super) struct Layout {
    grid: (usize, usize),
    displays: Vec<((PathBuf, DisplayId), vtium_font::Cell, Orientation)>,
}

impl App {
    /// How head `h` draws on each of its displays now.
    pub(super) fn layout_of(&self, h: usize) -> Layout {
        let Some(head) = self.heads.get(h) else {
            return Layout {
                grid: (0, 0),
                displays: Vec::new(),
            };
        };
        Layout {
            grid: head.grid,
            displays: head
                .views
                .iter()
                .map(|(key, view)| (key.clone(), view.cell(), view.orientation))
                .collect(),
        }
    }

    /// Logs the display line again for each display in `before` that head
    /// `h` now draws differently: on another grid, in another character
    /// cell, turned another way, or with other pixels left over. A new mode
    /// can keep the grid and the cell and still change the pixels left
    /// over, as 1366×768 and 1360×768 both hold 170×48 cells of 8×16. A
    /// display that left the head logs nothing, and one that joined it logs
    /// its own line. So after a turn, a new mode, a display that comes or
    /// goes, or a change of font or size, the log says how each display
    /// draws now. While the head's
    /// render thread has not yet said which path draws it, vtium logs
    /// nothing here, since every display line of the head follows once the
    /// thread has said so.
    ///
    /// References:
    /// - ADR-1303 D10: the display line names the path that draws the display.
    /// - ADR-1310 D12, "vtium logs it": the line on the pixels left over, again at a mode change.
    pub(super) fn layout_moved(&self, h: usize, before: &Layout) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let path = match &head.painter {
            Some(p) if !p.known() => return,
            Some(p) => p.path(),
            None => crate::painter::Path::default(),
        };
        let regrid = head.grid != before.grid;
        for (key, cell, orientation) in &before.displays {
            let Some(view) = head.views.get(key) else {
                continue;
            };
            let fit = view.fit_changed();
            if (regrid || fit.is_some() || view.cell() != *cell || view.orientation != *orientation)
                && let Some(video) = self.videos.get(&key.0)
            {
                display_line_fit(&video.name(key.1), video.backend(), path, view, fit);
            }
        }
    }

    /// What `render-timer` asks for: whether the console writes each
    /// display's figures to the log, and whether it draws them on screen.
    /// The figures describe the state of the system, which `osd = silent`
    /// hides from whoever is at the screen, so under `silent` they go to
    /// the log alone, or nowhere under `render-timer = osd`.
    ///
    /// References:
    /// - ADR-1401 D5, "The key `osd` and the render timer".
    pub(super) fn render_timer(&self) -> (bool, bool) {
        let (log, screen) = match self.config.text(&keys::RENDER_TIMER) {
            Some("log") => (true, false),
            Some("osd") => (false, true),
            Some("both") => (true, true),
            _ => (false, false),
        };
        (log, screen && !self.osd.silent())
    }

    /// Whether the console measures each display's frames, for the log,
    /// the screen or both.
    pub(super) fn measure(&self) -> bool {
        let (log, osd) = self.render_timer();
        log || osd
    }

    pub(super) fn draw(&mut self, now: Instant) {
        self.osd(now);
        self.notice_cycle(now);
        for h in 0..self.heads.len() {
            self.painter(h);
        }
        let App {
            heads,
            videos,
            clock,
            ..
        } = self;
        for head in heads.iter_mut() {
            draw_head(head, videos, &**clock, now);
        }
    }

    /// Each head's on-screen display for this turn's frames (ADR-1401 D5):
    /// under `osd = full` it shows after a change to the shown console or
    /// the number of consoles once the head's sessions have started, which
    /// `minimal`'s dot does not draw. It shows for `osd-timeout` from the
    /// first frame that draws it, or always under `never`.
    fn osd(&mut self, now: Instant) {
        let Osd { mode, timeout } = self.osd;
        let draws = self.osd.draws_status();
        let changes = mode == osd::Mode::Full && self.osd.shows_on_change();
        // the notice screen is no head, and shows no status
        for (h, head) in self
            .heads
            .iter_mut()
            .enumerate()
            .filter(|(_, h)| !h.notice_only)
        {
            // A head opens its other consoles as it starts, which is no
            // change to show.
            let seen = head
                .started()
                .then(|| (head.console.id, head.console_count()));
            let was = std::mem::replace(&mut head.osd_seen, seen);
            if changes && was.is_some_and(|was| Some(was) != seen) {
                head.show_osd();
            }
            let shown = draws && (timeout.is_none() || head.osd_shown);
            if let Some(time) = timeout.filter(|_| shown && head.osd_due.is_none())
                && !head.views.is_empty()
            {
                head.osd_due = Some(after(now, time));
            }
            let active = self.focus == Some(h);
            let mark = (shown && mode == osd::Mode::Minimal).then_some(if active {
                vtium_render::Mark::Focus
            } else {
                vtium_render::Mark::NoFocus
            });
            let full = (shown && mode == osd::Mode::Full).then(|| {
                let at = head.shown_console().saturating_add(1);
                crate::notice::osd_line(active, head.name(), at, head.console_count())
            });
            // The brief line answers one press at a cap: for `osd-timeout`,
            // or 3 s under `never`, from its first frame, in place of the
            // full line. [`Head::show_brief`] sets none under `off` or
            // `silent`.
            head.arm_brief(after(now, timeout.unwrap_or(BRIEF_NEVER)));
            let status = head.brief().map(str::to_owned).or(full);
            if head.mark != mark || head.status != status {
                head.mark = mark;
                head.status = status;
                head.draw_now = true;
            }
        }
    }

    /// Lets go of every head's render thread at once, as at a head file
    /// reload and at vtium's exit: heads whose GPU driver hangs hold the loop
    /// [`crate::painter::LET_GO_MS`] in all (ADR-1303 D9).
    pub(super) fn let_go_of_painters(&mut self) {
        let clock = Rc::clone(&self.clock);
        crate::painter::let_go(
            self.heads.iter_mut().filter_map(|h| h.painter.take()),
            &*clock,
        );
    }

    /// Starts head `h`'s render thread at its first frame (ADR-1401 D2),
    /// with a wake-up into the loop for each frame it composes. One that
    /// cannot start is logged once, and the loop composes the head's
    /// frames.
    pub(super) fn painter(&mut self, h: usize) {
        let Some(head) = self.heads.get(h) else {
            return;
        };
        if head.painter.is_some() || head.painter_refused || head.views.is_empty() {
            return;
        }
        let gpu = self.gpu_source(h).unwrap_or_else(|why| {
            log::warn!("{why}");
            None
        });
        let Some(head) = self.heads.get(h) else {
            return;
        };
        let started = vtium_platform::calloop::ping::make_ping()
            .map_err(|e| format!("cannot make its wake-up: {e}"))
            .and_then(|(ping, source)| {
                self.handle
                    .insert_source(source, |(), (), _: &mut App| {})
                    .map_err(|e| format!("cannot watch its wake-up: {e}"))?;
                Painter::start(head.name(), gpu, move || ping.ping()).map_err(|e| e.to_string())
            });
        let Some(head) = self.heads.get_mut(h) else {
            return;
        };
        match started {
            Ok(painter) => head.painter = Some(painter),
            Err(why) => {
                log::warn!(
                    "head {}: the render thread cannot start ({why}): the loop draws its frames",
                    head.name()
                );
                head.painter_refused = true;
                // The head draws on the CPU path, which a vtlook that
                // requires the GPU path does not accept.
                #[cfg(feature = "filters")]
                self.give_colors_again(h);
            }
        }
    }

    /// Where head `h`'s render thread makes its GL context: nowhere unless
    /// `hwaccel` is on in a build with `gpu`; a test's GPU where one is
    /// set; else the render node of the card that drives the head's first
    /// display. A card with no render node, such as a framebuffer, leaves
    /// the head on the CPU path, and the error is the line that says so,
    /// for the caller that starts the render thread to log once. The
    /// function itself logs nothing, so a check that only asks which path
    /// the head draws on can call it as often as it needs.
    ///
    /// References:
    /// - ADR-1303 D9: `hwaccel` and the `gpu` feature decide whether a head draws on the GPU path.
    pub(super) fn gpu_source(&self, h: usize) -> Result<Option<crate::painter::GpuSource>, String> {
        if !self.config.switch(&keys::HWACCEL) || !cfg!(feature = "gpu") {
            return Ok(None);
        }
        if let Some(source) = &self.gpu_source {
            return Ok(Some(source.clone()));
        }
        let Some(head) = self.heads.get(h) else {
            return Ok(None);
        };
        let Some((node, _)) = head.views.keys().next() else {
            return Ok(None);
        };
        #[cfg(feature = "gpu")]
        return match vtium_platform::gpu::render_node(node) {
            Ok(render) => Ok(Some(crate::painter::GpuSource::RenderNode(render))),
            Err(e) => Err(format!(
                "head {}: {} has no render node ({e}): it draws on the CPU path",
                head.name(),
                node.display()
            )),
        };
        #[cfg(not(feature = "gpu"))]
        {
            let _ = node;
            Ok(None)
        }
    }

    /// Takes the frames each head's render thread composed: each display
    /// gets its drawing back and, while the VT is ours and awake, shows
    /// its frame.
    pub(super) fn frames(&mut self, now: Instant) {
        let App {
            heads,
            videos,
            clock,
            awake,
            ..
        } = self;
        let mut moved = Vec::new();
        for (h, head) in heads.iter_mut().enumerate() {
            let Some(painter) = &mut head.painter else {
                continue;
            };
            let path = |p: &Painter| (p.known(), p.path());
            let before = path(painter);
            let frames = painter.take();
            let notes = painter.take_notes();
            // the render thread told the loop which path draws the head
            if painter.take_known() {
                display_lines(head, videos);
            }
            gpu_notes(head, videos, notes);
            for mut composed in frames {
                if let Some(why) = composed.left_gpu.take() {
                    left_gpu(head, videos, &why);
                }
                if let Some(why) = composed.refused_gpu.take() {
                    refused_gpu_display(head, videos, &composed.key, &why);
                }
                if composed.back_on_gpu {
                    back_on_gpu(head, videos);
                }
                let (Some(view), Some(video)) = (
                    head.views.get_mut(&composed.key),
                    videos.get_mut(&composed.key.0),
                ) else {
                    continue;
                };
                if let Some(why) = composed.failed_gpu.take() {
                    failed_gpu_frame(view, video.as_ref(), composed.key.1, &why);
                }
                let shown = view.present(
                    composed,
                    &mut head.console.term,
                    video.as_mut(),
                    *awake,
                    &**clock,
                );
                if shown.queued {
                    head.figures.frame(shown.compose, now);
                }
            }
            if head.painter.as_ref().map(path) != Some(before) {
                moved.push(h);
            }
        }
        // A head that moved between the GPU path and the CPU path, or
        // whose render thread first said which path draws it, has its
        // displays checked against the `gpu` requirement of its vtlook.
        #[cfg(feature = "filters")]
        for h in moved {
            self.give_colors_again(h);
        }
        #[cfg(not(feature = "filters"))]
        let _ = moved;
    }
}

/// Draws each of `head`'s displays that something asks to: its render
/// thread composes the frame, or the loop where the thread could not
/// start.
fn draw_head(
    head: &mut Head,
    videos: &mut BTreeMap<PathBuf, Box<dyn Video>>,
    clock: &dyn Fn() -> Instant,
    now: Instant,
) {
    let state = head.draw_state();
    // each display draws its own render timer's line in the box
    let overlays: Vec<crate::term::Overlay> = head
        .views
        .values()
        .map(|v| head.overlay(v.timer_line.as_deref()))
        .collect();
    // The notice screen's notice stays for as long as it is lit (D7).
    if head.has_notices()
        && head.notice_due.is_none()
        && !head.views.is_empty()
        && !head.notice_only
    {
        head.notice_due = Some(after(now, crate::head::NOTICE_SHOWN));
    }
    head.arm_kernel(now);
    let frame_end = std::mem::take(&mut head.console.term.present);
    let now_all = std::mem::take(&mut head.draw_now);
    let term = &mut head.console.term;
    let holding = term.sync.holding(&term.terminal);
    let base = head.cell;
    let mut snapshot = None;
    // A head that holds a GPU path draws every scroll again, into buffers
    // the screen's size, even while a display holds it on the CPU path for
    // now: the GPU path then sees each frame, and takes the head back once
    // it can set the display up. Until its render thread says which path
    // draws it, each view keeps the way it has.
    let scroll = match &head.painter {
        Some(p) if !p.known() => None,
        Some(p) if p.may_hold_gpu() => Some(ScrollDraw::Redraw),
        _ => Some(head.scroll_draw),
    };
    for ((key, view), overlay) in head.views.iter_mut().zip(&overlays) {
        let Some(video) = videos.get_mut(&key.0) else {
            continue;
        };
        let draws = view.draws(term, video.as_ref(), now_all, frame_end, holding, now);
        // A frame the display kept from a busy swap goes out once its flip
        // ends, unless this pass composes a newer frame, which is drawn
        // over it and carries it.
        if !draws || (holding && !view.lost) {
            let sent = view.send_kept(term, video.as_mut());
            if sent.queued {
                head.figures.frame(sent.compose, now);
            }
        }
        if !draws {
            continue;
        }
        if holding && !view.lost {
            if !view.owed
                && let Some(t) = &mut view.timer
            {
                t.held();
            }
            view.owed = true;
            continue;
        }
        // The pointer is kept in the base cell's pixels; a display
        // drawing larger cells draws it where its own cells put it.
        let c = view.cell();
        let state = DrawState {
            pointer: state
                .pointer
                .map(|(x, y)| (scale(x, c.width, base.0), scale(y, c.height, base.1))),
            ..state
        };
        let hook = head.draw_hook.as_ref();
        view.overlay.clone_from(overlay);
        if let Some(way) = scroll {
            view.scroll = way;
        }
        let Prepared::Job(job) = view.prepare(
            key,
            term,
            &mut snapshot,
            video.as_mut(),
            &state,
            hook,
            clock,
        ) else {
            continue;
        };
        match &mut head.painter {
            Some(painter) => {
                if let Some(job) = painter.send(job) {
                    view.refused(*job, video.as_mut());
                }
            }
            None => {
                let composed = job.compose(None);
                let shown = view.present(composed, term, video.as_mut(), true, clock);
                if shown.queued {
                    head.figures.frame(shown.compose, now);
                }
            }
        }
    }
    // The output after a synchronized update's end goes on once every
    // display that can draw has taken the finished frame. A display that
    // waits for its flip takes the frame at the flip's end. A display that
    // cannot take a frame by itself, such as one on a card handed to
    // another program, owes none.
    for (key, view) in &mut head.views {
        view.settle(videos.get(&key.0).map(|v| &**v));
    }
    term.drawn(head.views.values());
}

/// Head `head`'s GPU path ended during the run, for `why`: it draws on the
/// CPU path until vtium restarts, which the log says once, with each of its
/// displays' lines again naming the path (ADR-1303 D9, D10).
fn left_gpu(head: &mut Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>, why: &str) {
    log::warn!(
        "head {}: the GPU path failed ({why}): it draws on the CPU path until vtium restarts",
        head.name()
    );
    let Some(painter) = &mut head.painter else {
        return;
    };
    painter.left_gpu();
    display_lines(head, videos);
}

/// The GPU path failed a frame of `view`, the display `id` of `video`,
/// for `why`, and the CPU path drew it instead (ADR-1303 D9). The log says
/// so for the display's first such frame, naming its connector, and is
/// silent for the rest.
fn failed_gpu_frame(view: &mut View, video: &dyn Video, id: DisplayId, why: &str) {
    if view.gpu_failure_logged {
        return;
    }
    view.gpu_failure_logged = true;
    log::warn!(
        "display {}: a frame failed on the GPU path ({why}): the CPU path draws it, and later failures of this display's frames are not logged",
        video.name(id)
    );
}

/// Logs the GPU path's `notes` about the displays of `head`, each under
/// its display's connector, so that on a head of several displays the log
/// says which display a note is about.
fn gpu_notes(head: &Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>, notes: Vec<Note>) {
    for note in notes {
        let (node, id) = &note.display;
        let name = videos.get(node).map(|v| v.name(*id)).unwrap_or_default();
        log::warn!("head {}, display {name}: {}", head.name(), note.text);
    }
}

/// The GPU path could not set up the display `key` of `head` at its size,
/// for `why`: the whole head draws on the CPU path until that display's
/// size changes or it leaves the head (ADR-1303 D9). The log says so once,
/// naming the display's connector, and gives each of the head's display
/// lines again, naming `cpu`.
fn refused_gpu_display(
    head: &mut Head,
    videos: &BTreeMap<PathBuf, Box<dyn Video>>,
    key: &(PathBuf, DisplayId),
    why: &str,
) {
    let name = videos
        .get(&key.0)
        .map(|v| v.name(key.1))
        .unwrap_or_default();
    let (w, h) = head.views.get(key).map(|v| v.size).unwrap_or_default();
    log::warn!(
        "display {name}: the GPU path cannot set it up at {w}x{h} ({why}): head {} draws on the CPU path until the display's size changes or it leaves the head",
        head.name()
    );
    if let Some(painter) = &mut head.painter {
        painter.hold_on_cpu(true);
    }
    display_lines(head, videos);
}

/// No display of `head` is at a size the GPU path could not set up any
/// more: the head draws on the GPU path again (ADR-1303 D9). The log says
/// so and gives each of the head's display lines again, naming the path
/// that draws it.
fn back_on_gpu(head: &mut Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>) {
    log::info!(
        "head {}: the GPU path can set up each of its displays again: it draws on the GPU path",
        head.name()
    );
    if let Some(painter) = &mut head.painter {
        painter.hold_on_cpu(false);
    }
    display_lines(head, videos);
}

/// Each of `head`'s display lines, naming the path that draws each one
/// now (ADR-1303 D10).
fn display_lines(head: &Head, videos: &BTreeMap<PathBuf, Box<dyn Video>>) {
    let path = head.painter.as_ref().map(Painter::path).unwrap_or_default();
    for ((node, id), view) in &head.views {
        if let Some(video) = videos.get(node) {
            display_line(&video.name(*id), video.backend(), path, view);
        }
    }
}

/// The line vtium logs for `view`, a display it draws on a head on
/// `path`, naming the path that draws the display. When the display's
/// layout changed since the log last said, and it leaves pixels over on
/// either axis, an informational line follows that names the columns and
/// rows that fit and the pixels left over.
///
/// References:
/// - ADR-1303 D10: the display line names the path that draws the display.
/// - ADR-1310 D12, "vtium logs it": the line on the pixels left over, never a warning.
pub(super) fn display_line(name: &str, backend: &str, path: crate::painter::Path, view: &View) {
    display_line_fit(name, backend, path, view, view.fit_changed());
}

/// [`display_line`], with the columns and rows that fit given as `fit`
/// when they changed since the log last said, or `None` when they did not.
fn display_line_fit(
    name: &str,
    backend: &str,
    path: crate::painter::Path,
    view: &View,
    fit: Option<vtium_settings::fit::Fit>,
) {
    let cell = view.cell();
    log::info!(
        "Display {name} with backend [{backend}] text renderer [{}], cell size {}x{}",
        path.for_depth(view.depth()).name(),
        cell.width,
        cell.height
    );
    if let Some(line) = fit.and_then(|f| f.line(name)) {
        log::info!("{line}");
    }
}

/// Powers every display of `video` down.
pub(super) fn power_down(video: &mut dyn Video) {
    for id in video.displays() {
        if let Err(e) = video.set_dpms(id, Dpms::Off) {
            log::warn!("cannot blank a display: {e}");
        }
    }
}

/// The render timer's log text for a display, after the `terminal:`
/// target: the figures, then `display=NODE[ID]` (tier 3's `t3_bench.py`
/// reads each `key=value`).
pub fn render_timer_line(report: &vtium_render::Report, node: &Path, id: DisplayId) -> String {
    format!("{report} display={}[{}]", node.display(), id.0)
}

#[cfg(test)]
mod tests {
    use vtium_font::{Font, Unifont};
    use vtium_platform::display::DisplayId;
    use vtium_render::{Config as RenderConfig, Orientation};

    use super::display_line;
    use crate::app::App;
    use crate::font::DisplayFace;
    use crate::getty::tests::logged;
    use crate::pointer_file::Store as PointerFile;
    use crate::term::View;

    /// A view of unifont's 8×16 cell on a display of `size`, turned as
    /// `orientation` says.
    fn view(size: (u32, u32), orientation: Orientation) -> View {
        let face: DisplayFace = Box::new(Unifont::new(16));
        let config = RenderConfig {
            orientation,
            ..RenderConfig::default()
        };
        View::new(
            DisplayId(1),
            Font::new(face),
            size,
            None,
            config,
            false,
            PointerFile::default(),
        )
    }

    /// The lines on the pixels left over that the log wrote since the last
    /// call.
    fn fit_lines() -> Vec<String> {
        logged()
            .into_iter()
            .filter(|l| l.contains("left over"))
            .collect()
    }

    fn unifont() -> Font<DisplayFace> {
        let face: DisplayFace = Box::new(Unifont::new(16));
        Font::new(face)
    }

    // The display line is followed by one informational line whenever the
    // display's layout leaves pixels over: it names the axes that do not
    // divide evenly, with the cells that fit and the pixels left over. The
    // same layout logs nothing more, a display that divides evenly logs
    // nothing, and a change of size to one that leaves pixels over logs
    // again. A display turned on its side counts its rows along its width.
    // Every build logs the line, and so does a console with smooth
    // scrolling off.
    //
    // References:
    // - ADR-1310 D12, "vtium logs it": the line, once for each layout, on both axes.
    #[test]
    fn a_display_that_leaves_pixels_over_logs_them_once_for_each_layout() {
        let path = crate::painter::Path::default();
        let _ = logged();
        let mut v = view((1366, 768), Orientation::Normal);
        display_line("HDMI-A-1", "drm", path, &v);
        assert_eq!(
            fit_lines(),
            [
                "Display HDMI-A-1 is 1366×768 and holds 170 columns of 8 pixels, so 6 pixel columns are left over."
            ]
        );
        display_line("HDMI-A-1", "drm", path, &v);
        assert_eq!(
            fit_lines(),
            Vec::<String>::new(),
            "the same layout logs nothing more"
        );
        v.resize(unifont(), (1024, 768), RenderConfig::default());
        display_line("HDMI-A-1", "drm", path, &v);
        assert_eq!(fit_lines(), Vec::<String>::new(), "1024×768 divides evenly");
        v.resize(unifont(), (1366, 770), RenderConfig::default());
        display_line("HDMI-A-1", "drm", path, &v);
        assert_eq!(
            fit_lines(),
            [
                "Display HDMI-A-1 is 1366×770 and holds 170 columns of 8 pixels and 48 rows of 16 pixels, so 6 pixel columns and 2 pixel lines are left over."
            ]
        );
        let turned = view((770, 1366), Orientation::Right);
        display_line("DSI-1", "drm", path, &turned);
        assert_eq!(
            fit_lines(),
            [
                "Display DSI-1 is 770×1366 and holds 170 columns of 8 pixels and 48 rows of 16 pixels, so 6 pixel columns and 2 pixel lines are left over."
            ]
        );
    }

    // A new mode that keeps the head's grid and the display's cell still
    // logs the display line again when it changes the pixels left over.
    // With an 8×16 cell, 316×160 and 312×160 both hold 39 columns and 10
    // rows. At 312×160 the line says the display draws with nothing left
    // over, and back at 316×160 the line on the 4 pixel columns follows.
    //
    // References:
    // - ADR-1310 D12, "vtium logs it": the line again at a mode change.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        clippy::unwrap_used,
        reason = "a test's clock is the host's, and a test fails by panicking"
    )]
    fn a_new_mode_with_the_same_grid_logs_the_pixels_left_over_again() {
        use std::path::{Path, PathBuf};
        use std::time::Instant;

        use vtium_platform::display::Event;

        use crate::app::tests::{Still, no_fonts, seat, shown};

        // installs the log's capture before anything logs
        let _ = logged();
        let (_lp, mut app) = seat(None, no_fonts());
        let mut s = Still::new(&[(1, "A")]);
        s.size = (316, 160);
        app.add_video(PathBuf::from("/dev/mem0"), Box::new(s));
        shown(&mut app);
        // The display's first lines follow once the head's render thread
        // has told the loop which path draws it. The bound on the wait only
        // keeps the test from hanging when the thread never tells.
        let end = Instant::now() + std::time::Duration::from_secs(10);
        let mut first = Vec::new();
        while !first
            .iter()
            .any(|l: &String| l.starts_with("Display A is 316×160"))
            && Instant::now() < end
        {
            std::thread::yield_now();
            app.turn(Instant::now());
            first.extend(logged());
        }
        assert!(
            first.iter().any(|l| l.starts_with("Display A is 316×160")),
            "the display's first lines: {first:?}"
        );
        let grid = app.heads.first().map(|h| h.grid);
        assert_eq!(grid, Some((39, 10)), "316×160 in 8×16 cells");
        let moved = |app: &mut App, size| {
            let _ = logged();
            let video = app.videos.get_mut(Path::new("/dev/mem0")).unwrap();
            let s = video.as_any().downcast_mut::<Still>().unwrap();
            s.size = size;
            s.events.push(Event::Redraw(DisplayId(1)));
            app.turn(Instant::now());
            assert_eq!(app.heads.first().map(|h| h.grid), grid, "at {size:?}");
            logged()
                .into_iter()
                .filter(|l| l.contains(" with backend ") || l.contains("left over"))
                .collect::<Vec<_>>()
        };
        let line = "Display A with backend [still] text renderer [cpu], cell size 8x16";
        assert_eq!(
            moved(&mut app, (312, 160)),
            [line],
            "at 312×160 nothing is left over"
        );
        assert_eq!(
            moved(&mut app, (316, 160)),
            [
                line,
                "Display A is 316×160 and holds 39 columns of 8 pixels, so 4 pixel columns are left over."
            ]
        );
    }
}
