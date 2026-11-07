//! The renderer on the GPU path (ADR-1303): the same front end as the CPU
//! path decides what a frame draws, in the display's pixels, and
//! [`GpuBack`] maps it into the upright frame's clears and quads, which the
//! pipeline composes and presents turned. The renderer's own record stays
//! in the display's pixels on both paths, so a frame of either path follows
//! a frame of the other.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vtium_font::Face;
use vtium_term::Frame;

use super::{DrawState, Drawn, Renderer, Stats, sorted};
use crate::blend::pixel;
use crate::cache::Budgets;
use crate::deco::Sprites;
use crate::gpu::compose::{GpuBack, receiving};
use crate::gpu::glyphs::Spec;
#[cfg(feature = "images")]
use crate::gpu::pictures::Shown;
use crate::gpu::turn::Turn;
use crate::gpu::{ArrowAt, DrawTarget, Error, ErrorKind, Gl, Pipeline, Size, Work};
use crate::mono::{self, Depth, Fallback};
use crate::notice::Placed;
use crate::reach::Reach;
use crate::renderer::Back;

/// The next renderer's id: it tells the GPU path's glyphs whose font drew
/// them and which history they follow, and never reaches a pixel.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// What the GPU path keeps of a renderer's.
#[derive(Debug)]
pub(super) struct State {
    /// The glyph cache's budgets, which the GPU path's pools share.
    budgets: Budgets,
    /// The decoration sprites upright, which the GPU path's cells read.
    sprites: Sprites,
    /// Whose glyphs a pipeline holds. A new id makes the pipeline replace
    /// them and compose its next frame whole.
    id: u64,
    /// Characters the GPU path's glyphs forget on its next frame.
    forget: Vec<char>,
}

impl State {
    pub(super) fn new(budgets: Budgets, sprites: Sprites) -> State {
        State {
            budgets,
            sprites,
            id: next_id(),
            forget: Vec::new(),
        }
    }

    /// The font now draws each of `chars` differently.
    pub(super) fn forget(&mut self, chars: &[char]) {
        self.forget.extend_from_slice(chars);
    }

    /// This state, a turned renderer's, with `old`'s identity and the
    /// characters it had still to forget, so a pipeline that holds `old`'s
    /// glyphs keeps them. Its budgets and sprites are this state's own.
    pub(super) fn keeping_identity(self, old: State) -> State {
        State {
            id: old.id,
            forget: old.forget,
            ..self
        }
    }

    /// The font's lines changed: the upright `sprites` are made from the
    /// new ones, and a fresh id makes the next GPU frame replace the
    /// pipeline's glyphs and sprites and compose whole.
    pub(super) fn lines_changed(&mut self, sprites: Sprites) {
        self.sprites = sprites;
        self.forget.clear();
        self.id = next_id();
    }

    /// The CPU path drew a frame: the renderer's history moved on while
    /// the pipeline's upright frame did not. A fresh id makes the next GPU
    /// frame replace the glyphs and compose whole, so nothing is left to
    /// forget.
    pub(super) fn drew_on_cpu(&mut self) {
        self.forget.clear();
        self.id = next_id();
    }
}

impl<F: Face> Renderer<F> {
    /// Draws `frame` on the GPU path (ADR-1303): composes what changed into
    /// `pipeline`'s upright frame through the same front end as
    /// [`Renderer::draw`], with `overlays`, the console's notices and focus
    /// mark, over it, and presents into `target`, the display's buffer `age`
    /// frames old, what that buffer lacks. Gives the rectangles of the
    /// buffer it drew and the frame's figures, as [`Renderer::draw`] does.
    ///
    /// A notice the last frame drew and this one does not, or draws
    /// changed, has the cells beneath it drawn again. The renderer draws
    /// the whole picture when the pipeline composes its upright frame
    /// whole: when it is new, after a failed frame, for a new border color,
    /// after a turn, or after another renderer's frame, or a frame of the
    /// CPU path's, went between. The pipeline takes this renderer's
    /// orientation ([`Pipeline::turn`]). A 1-bit display, which needs
    /// present's depth reduction, is refused until it is built.
    ///
    /// A frame that fails leaves the pipeline to compose its next frame
    /// whole, and the error says what failed, for the head to sort
    /// (ADR-1303 D9). A frame that fails after the renderer planned it also
    /// makes the renderer forget its history, since that history already
    /// records the failed frame's changes as drawn: the next frame draws
    /// every cell and the border, and so does the first later frame into
    /// each other buffer.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "the CPU path's draw and the GPU's context, pipeline and notices"
    )]
    pub fn draw_gpu<G: Gl>(
        &mut self,
        gl: &mut G,
        pipeline: &mut Pipeline,
        frame: &Frame<'_>,
        target: &DrawTarget,
        age: u32,
        state: &DrawState,
        overlays: &[Placed],
    ) -> Result<Drawn<'_>, Error> {
        let start = Instant::now();
        if self.config.depth == Depth::One {
            return Err(Error::new(
                ErrorKind::Misuse,
                "the GPU path draws no 1-bit display until present reduces the depth",
            ));
        }
        let display = Size::new(self.layout.width, self.layout.height);
        if pipeline.size() != display || target.size() != display {
            return Err(Error::new(
                ErrorKind::Misuse,
                "the pipeline or the buffer is not the renderer's display",
            ));
        }
        let turn = Turn::new(self.config.orientation, display);
        pipeline.turn(gl, turn.orientation)?;
        let border = pixel(mono::border(self.config.depth, state.background));
        let begun = pipeline.begin(gl, &self.gpu_spec(), border, overlays)?;
        for r in begun.gone {
            self.redraw_area(turn.down(r));
        }
        // the upright frame holds the last frame composed, unless the
        // pipeline composes it whole
        let plan = self.plan(frame, if begun.whole { 0 } else { 1 }, state);
        let glyphs = match pipeline.glyphs() {
            Ok(glyphs) => glyphs,
            Err(e) => return Err(self.failed_after_plan(e)),
        };
        glyphs.forget(&sorted(&core::mem::take(&mut self.gpu.forget)));
        // the reaches are read as the CPU path reads them (Renderer::draw)
        let reaching = glyphs.reaching();
        if reaching && !self.reaches_kept {
            self.relearn_reaches();
        }
        glyphs.begin_ink(plan.grid.cols, plan.grid.rows);
        let mut back = GpuBack::new(gl, &mut *glyphs, turn);
        let mut counts = self.paint(&plan, frame, state, &mut back);
        if !reaching && back.reaching(&self.compose) {
            // the first glyph whose ink passes its cells: the frame is
            // drawn again, whole, with every reach read
            self.relearn_reaches();
            back.restart();
            counts = self.paint(&plan, frame, state, &mut back);
        }
        #[cfg(feature = "images")]
        let (text, tints, marks) = (back.text, back.tints, back.marks);
        #[cfg(feature = "images")]
        let shown = self.shown(turn, back.picture_rects);
        let (clear, cells, mut error) = (back.clear, back.cells, back.error);
        // the neighbours named as sources of ink take the reach each cell
        // has once the whole frame is scanned, and the jobs that receive
        // their ink the display lines it reaches
        let reach = |k: usize, r: usize| {
            self.cells
                .get(k, r)
                .map_or((Reach::NONE, 0), |c| (c.reach(), c.span()))
        };
        #[cfg(not(feature = "images"))]
        let lists = [&cells];
        #[cfg(feature = "images")]
        let lists = [&cells, &text];
        let cell = glyphs.cell();
        let jobs = lists
            .into_iter()
            .flatten()
            .flat_map(|c| &c.quads)
            .filter_map(|q| receiving(q, cell));
        glyphs.settle_ink(reach, jobs);
        if error.is_none()
            && let Err(e) = glyphs.ink_upload(gl)
        {
            error = Some(e);
        }
        glyphs.end_frame();
        let uploads = glyphs.take_uploads();
        let gpu_counts = glyphs.take_counts();
        let missing = glyphs.take_missing();
        let drawn = self.finish(&plan, counts, missing, start);
        let changed: Vec<_> = drawn.damage.iter().map(|&r| turn.up(r)).collect();
        let stats = drawn.stats;
        if let Some(e) = error {
            pipeline.redraw_whole();
            return Err(self.failed_after_plan(e));
        }
        let work = Work {
            border,
            clear: &clear,
            changed: &changed,
            arrow: plan.pointer.map(|p| {
                let at = turn.up_area(p.at);
                ArrowAt {
                    image: &self.upright_arrow,
                    x: at.x,
                    y: at.y,
                }
            }),
            overlays,
            cells: &cells,
            uploads,
            #[cfg(feature = "images")]
            pictures: &shown,
            #[cfg(feature = "images")]
            text: &text,
            #[cfg(feature = "images")]
            tints: &tints,
            #[cfg(feature = "images")]
            marks: &marks,
        };
        let damage = match pipeline.frame(gl, &work, target, age) {
            Ok(damage) => damage,
            Err(e) => return Err(self.failed_after_plan(e)),
        };
        Ok(Drawn {
            damage: self.cells.set_damage(damage),
            stats: Stats {
                misses: gpu_counts.misses,
                uncached: gpu_counts.uncached,
                ..stats
            },
        })
    }

    /// A frame failed after its plan stamped its changes into the
    /// renderer's history, though no buffer shows them: the renderer
    /// forgets its history ([`Renderer::forget_history`]). Gives `e` back.
    fn failed_after_plan(&mut self, e: Error) -> Error {
        self.forget_history();
        e
    }

    /// The frame's pictures for slot 1, in draw order, in the upright
    /// frame, each with `rects`, where the frame draws it. Every picture the
    /// frame shows is here, so slot 1 keeps its textures from eviction.
    #[cfg(feature = "images")]
    fn shown(&self, turn: Turn, mut rects: Vec<Vec<crate::target::Rect>>) -> Vec<Shown> {
        rects.resize_with(self.compose.pictures.pics().len(), Vec::new);
        self.compose
            .pictures
            .pics()
            .iter()
            .zip(rects)
            .map(|(p, rects)| {
                let at = turn.up_area(p.rect());
                Shown {
                    key: p.key(),
                    pixels: p.pixels().clone(),
                    x: at.x,
                    y: at.y,
                    opaque: p.opaque(),
                    above: p.layer() == vtium_image::Layer::AboveText,
                    rects,
                }
            })
            .collect()
    }

    /// What the GPU path's glyphs need of this renderer.
    fn gpu_spec(&self) -> Spec<'_> {
        Spec {
            owner: self.gpu.id,
            budgets: self.gpu.budgets,
            storage: self.config.storage,
            fallback: Fallback::new(self.config.depth, self.font.cell()),
            cell: Size::new(
                u32::try_from(self.layout.cw).unwrap_or(1),
                u32::try_from(self.layout.ch).unwrap_or(1),
            ),
            sprites: &self.gpu.sprites,
            bound: self.bound,
        }
    }
}
