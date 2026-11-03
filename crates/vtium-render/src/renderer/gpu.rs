//! The renderer on the GPU path (ADR-1303): the same front end as the CPU
//! path decides what a frame draws, and [`GpuBack`] turns it into the
//! upright frame's clears and quads, which the pipeline composes and
//! presents.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vtium_font::Face;
use vtium_term::Frame;

use super::{DrawState, Drawn, Renderer, Stats, sorted};
use crate::blend::pixel;
use crate::cache::Budgets;
use crate::gpu::compose::GpuBack;
use crate::gpu::glyphs::Spec;
use crate::gpu::{ArrowAt, DrawTarget, Error, ErrorKind, Gl, Pipeline, Size, Work};
use crate::mono::{self, Depth, Fallback};
use crate::notice::Placed;
use crate::target::Orientation;

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
    /// Whose glyphs a pipeline holds. A new id makes the pipeline replace
    /// them and compose its next frame whole.
    id: u64,
    /// Characters the GPU path's glyphs forget on its next frame.
    forget: Vec<char>,
}

impl State {
    pub(super) fn new(budgets: Budgets) -> State {
        State {
            budgets,
            id: next_id(),
            forget: Vec::new(),
        }
    }

    /// The font now draws each of `chars` differently.
    pub(super) fn forget(&mut self, chars: &[char]) {
        self.forget.extend_from_slice(chars);
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
    /// or after another renderer's frame, or a frame of the CPU path's, went
    /// between. A turned display, and a 1-bit one, which needs present's
    /// depth reduction, are refused until they are built.
    ///
    /// A frame that fails leaves the pipeline to compose its next frame
    /// whole, and the error says what failed, for the head to sort
    /// (ADR-1303 D9).
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
        if self.config.orientation != Orientation::Normal {
            return Err(Error::new(
                ErrorKind::Misuse,
                "the GPU path draws an upright display only",
            ));
        }
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
        let border = pixel(mono::border(self.config.depth, state.background));
        let begun = pipeline.begin(gl, &self.gpu_spec(), border, overlays)?;
        for r in begun.gone {
            self.redraw_area(r);
        }
        // the upright frame holds the last frame composed, unless the
        // pipeline composes it whole
        let plan = self.plan(frame, if begun.whole { 0 } else { 1 }, state);
        let glyphs = pipeline.glyphs()?;
        glyphs.forget(&sorted(&core::mem::take(&mut self.gpu.forget)));
        let mut back = GpuBack::new(gl, &mut *glyphs);
        let counts = self.paint(&plan, frame, state, &mut back);
        let (clear, cells, error) = (back.clear, back.cells, back.error);
        glyphs.end_frame();
        let uploads = glyphs.take_uploads();
        let gpu_counts = glyphs.take_counts();
        let drawn = self.finish(&plan, counts, start);
        let (changed, stats) = (drawn.damage.to_vec(), drawn.stats);
        if let Some(e) = error {
            pipeline.redraw_whole();
            return Err(e);
        }
        let work = Work {
            border,
            clear: &clear,
            changed: &changed,
            arrow: plan.pointer.map(|p| ArrowAt {
                image: &self.upright_arrow,
                x: p.at.x,
                y: p.at.y,
            }),
            overlays,
            cells: &cells,
            uploads,
        };
        let damage = pipeline.frame(gl, &work, target, age)?;
        Ok(Drawn {
            damage: self.cells.set_damage(damage),
            stats: Stats {
                misses: gpu_counts.misses,
                uncached: gpu_counts.uncached,
                ..stats
            },
        })
    }

    /// What the GPU path's glyphs need of this renderer.
    fn gpu_spec(&self) -> Spec<'_> {
        Spec {
            owner: self.gpu.id,
            budgets: self.gpu.budgets,
            storage: self.config.storage,
            fallback: Fallback::new(
                self.config.depth,
                self.font.cell(),
                self.font.decorations().underline_thickness,
            ),
            cell: Size::new(
                u32::try_from(self.layout.cw).unwrap_or(1),
                u32::try_from(self.layout.ch).unwrap_or(1),
            ),
            sprites: &self.compose.sprites,
        }
    }
}
