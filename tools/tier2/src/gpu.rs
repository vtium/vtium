//! The GPU path's screens (ADR-1303 D1, "How the GPU path is judged"; D11
//! part 3).
//!
//! Each screen the GPU path draws is drawn through `Renderer::draw_gpu` on
//! a GL, presented into buffers the GL makes in memory and read back, and
//! judged against the CPU path's drawing of the same screen. The CPU path
//! is the standard, judged exactly against the screen's golden by the run;
//! the GPU path is judged within its own sidecar, `gpu/<screen>.tol` in
//! this tree, never the golden's, and no mask hides a GPU difference. The
//! sidecars are exact for now, the maintainer's decision of 2026-11-03,
//! since every screen measured no difference on llvmpipe.
//!
//! The GPU path draws an upright display in color today. A turned screen
//! waits for the turn, and a 1-bit screen for present's depth reduction,
//! later parts of slice 0; [`waits`] says which.
//!
//! The GL is any [`Gl`]: under Mesa's llvmpipe once vtium-platform's GL
//! lands, and the tests' stand-in, which draws nothing, in the self-tests.

use std::fs;
use std::path::Path;

use vtium_font::{Face, Font};
use vtium_render::gpu::{Config as GpuConfig, DrawTarget, Gl, Pipeline, Size};
use vtium_render::{Canvas, Config, Depth, DrawState, Orientation, Picture, Placed, Renderer};
use vtium_term::{Age, Terminal};

use crate::compare::{Comparison, Verdict, compare, diff_image};
use crate::draw::{self, Paint, Screen};
use crate::ppm::Image;
use crate::report::{CaseReport, Status};
use crate::tol::Tolerance;

/// Why the GPU path does not draw `screen` yet, or `None` when it does.
pub fn waits(screen: &Screen) -> Option<&'static str> {
    if screen.orientation != Orientation::Normal {
        return Some("a turned display waits for the turn, part 4 of ADR-1303's slice 0");
    }
    if screen.depth == Depth::One {
        return Some("a 1-bit display waits for present's depth reduction (ADR-1301)");
    }
    None
}

/// The GPU path on the GL `gl`.
pub struct Gpu<'g, G: Gl> {
    pub gl: &'g mut G,
}

/// A display of two buffers the GL made, each knowing the frame it holds,
/// as `draw`'s display does.
struct Buffers {
    targets: [DrawTarget; 2],
    shown: [Option<u64>; 2],
    back: usize,
    swaps: u64,
}

impl Buffers {
    fn age(&self) -> u32 {
        self.shown[self.back].map_or(0, |n| u32::try_from(self.swaps - n + 1).unwrap_or(u32::MAX))
    }

    fn swap(&mut self) {
        self.swaps += 1;
        self.shown[self.back] = Some(self.swaps);
        self.back ^= 1;
    }
}

/// One frame of the terminal's screen through the GPU path into the back
/// buffer, swapped when it drew anything, as a display swaps.
fn frame<G: Gl, F: Face>(
    gl: &mut G,
    pipeline: &mut Pipeline,
    r: &mut Renderer<F>,
    buffers: &mut Buffers,
    (term, since): (&mut Terminal, &mut Age),
    overlays: &[Placed],
) -> Result<(), String> {
    let state = DrawState {
        background: term.palette().bg(),
        ..DrawState::default()
    };
    let age = buffers.age();
    let drew = {
        let frame = term.screen().frame(term.palette(), *since);
        let target = &buffers.targets[buffers.back];
        !r.draw_gpu(gl, pipeline, &frame, target, age, &state, overlays)
            .map_err(|e| format!("the GPU path: {e}"))?
            .damage
            .is_empty()
    };
    *since = term.screen_mut().checkpoint();
    if drew {
        buffers.swap();
    }
    Ok(())
}

/// What the console draws over `screen`, as the GPU path takes it: each
/// area the overlay covers, read back from the CPU path's drawing of it
/// alone, notice first and mark after.
fn overlays<F: Face>(
    screen: &Screen,
    r: &Renderer<F>,
    (w, h): (u32, u32),
) -> Result<Vec<Placed>, String> {
    let Some(overlay) = draw::console_overlay(screen)? else {
        return Ok(Vec::new());
    };
    let mut canvas = Canvas::new(w, h, 0);
    let areas = overlay.draw(
        r,
        (screen.cols, screen.rows),
        screen.orientation,
        &mut canvas,
    );
    if areas.is_empty() {
        return Err("the overlay drew nothing".to_owned());
    }
    areas
        .iter()
        .map(|a| {
            let pixels = (a.y..a.y + a.height)
                .flat_map(|y| (a.x..a.x + a.width).map(move |x| (x, y)))
                .map(|(x, y)| canvas.pixel(x, y).unwrap_or(0))
                .collect();
            let picture = Picture::from_pixels(a.width, a.height, pixels)
                .ok_or("an overlay's area past the display")?;
            Ok(Placed {
                x: a.x,
                y: a.y,
                picture,
            })
        })
        .collect()
}

/// Reads `target` back as an image.
fn read<G: Gl>(gl: &mut G, target: &DrawTarget) -> Result<Image, String> {
    let size = target.size();
    let mut rgba = vec![0u8; size.width as usize * size.height as usize * 4];
    gl.read(target, size.rect(), &mut rgba)
        .map_err(|e| format!("reading the buffer back: {e}"))?;
    let rgb = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, _]| [r, g, b])
        .collect();
    Image::new(size.width, size.height, rgb).ok_or_else(|| "an empty buffer".to_owned())
}

impl<G: Gl> Gpu<'_, G> {
    fn draw<F: Face>(
        &mut self,
        screen: &Screen,
        make_font: &impl Fn() -> Font<F>,
        config: Config,
        pipeline: &mut Pipeline,
        buffers: &mut Buffers,
    ) -> Result<Image, String> {
        let gl = &mut *self.gl;
        let font = make_font();
        let (mut term, w, h) = draw::prepare(screen, &font)?;
        let mut r = Renderer::new(font, w, h, draw::screen_config(screen, config));
        let mut since = Age::NEVER;
        let mut step = |gl: &mut G, r: &mut Renderer<F>, term: &mut Terminal, o: &[Placed]| {
            frame(gl, pipeline, r, buffers, (term, &mut since), o)
        };
        step(gl, &mut r, &mut term, &[])?;
        if let Some(next) = screen.next {
            step(gl, &mut r, &mut term, &[])?;
            draw::feed_next(&mut term, next)?;
            step(gl, &mut r, &mut term, &[])?;
            step(gl, &mut r, &mut term, &[])?;
        }
        // the console's drawing over the last frame, in a frame of its own
        let over = overlays(screen, &r, (w, h))?;
        if !over.is_empty() {
            step(gl, &mut r, &mut term, &over)?;
        }
        if screen.next.is_some() && over.is_empty() {
            let back = read(gl, &buffers.targets[buffers.back])?;
            let front = read(gl, &buffers.targets[buffers.back ^ 1])?;
            if back != front {
                return Err("the two buffers differ after the last frame".to_owned());
            }
        }
        read(gl, &buffers.targets[buffers.back ^ 1])
    }
}

impl<G: Gl> Paint for Gpu<'_, G> {
    fn paint<F: Face, M: Fn() -> Font<F>>(
        &mut self,
        screen: &Screen,
        make_font: &M,
        config: Config,
    ) -> Result<Image, String> {
        let (w, h) = {
            let (_, w, h) = draw::prepare(screen, &make_font())?;
            (w, h)
        };
        let size = Size::new(w, h);
        let gl = &mut *self.gl;
        let mut pipeline = Pipeline::new(gl, size, GpuConfig::default())
            .map_err(|e| format!("the GPU path: {e}"))?;
        let made = (gl.create_frame(size), gl.create_frame(size));
        let (a, b) = match made {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                for t in [a, b].into_iter().flatten() {
                    let _ = gl.free_target(t);
                }
                let _ = pipeline.free(gl);
                return Err("the GPU path: no buffer".to_owned());
            }
        };
        let mut buffers = Buffers {
            targets: [a, b],
            shown: [None; 2],
            back: 0,
            swaps: 0,
        };
        let drawn = self.draw(screen, make_font, config, &mut pipeline, &mut buffers);
        let gl = &mut *self.gl;
        let [a, b] = buffers.targets;
        let freed = gl
            .free_target(a)
            .and(gl.free_target(b))
            .and(pipeline.free(gl));
        let img = drawn?;
        freed.map_err(|e| format!("the GPU path: {e}"))?;
        Ok(img)
    }
}

/// One screen's GPU case: the comparison of the GPU path's drawing with the
/// CPU path's, or why it was not judged.
#[derive(Debug)]
pub struct GpuCase {
    pub name: &'static str,
    pub judged: Result<Judged, String>,
}

/// A judged GPU case.
#[derive(Debug)]
pub struct Judged {
    pub comparison: Comparison,
    /// The sidecar that judged it.
    pub tolerance: Tolerance,
    /// The GPU path's drawing.
    pub image: Image,
    /// The screen's pixels.
    pub area: u64,
}

impl Judged {
    /// The share of the screen's pixels that differ, in per cent.
    pub fn share(&self) -> f64 {
        self.comparison.pixels as f64 * 100.0 / self.area as f64
    }
}

/// The GPU path's sidecar for `name` under `root`, `gpu/<name>.tol`.
pub fn sidecar(root: &Path, name: &str) -> Result<Tolerance, String> {
    let path = root.join("gpu").join(format!("{name}.tol"));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Tolerance::parse(&text, &path.display().to_string())
}

/// Draws `screen` on the GPU path on `gl` and judges it against the CPU
/// path's drawing, within its sidecar under `root`.
pub fn judge<G: Gl>(screen: &Screen, root: &Path, gl: &mut G) -> Result<Judged, String> {
    let tolerance = sidecar(root, screen.name)?;
    let cpu = draw::draw(screen)?;
    let image = draw::draw_on(screen, &mut Gpu { gl })?;
    let comparison = compare(&cpu, &image, &tolerance)?;
    Ok(Judged {
        comparison,
        tolerance,
        image,
        area: u64::from(cpu.width()) * u64::from(cpu.height()),
    })
}

/// Judges each of `screens` the GPU path draws, on `gl`.
pub fn cases<G: Gl>(screens: &[Screen], root: &Path, gl: &mut G) -> Vec<GpuCase> {
    screens
        .iter()
        .filter(|s| waits(s).is_none())
        .map(|s| GpuCase {
            name: s.name,
            judged: judge(s, root, gl),
        })
        .collect()
}

/// A GPU case's name in the report: its screen's, with `@gpu` after it.
pub fn case_name(screen: &str) -> String {
    format!("{screen}@gpu")
}

/// The report's case for each of `screens` on the GPU path, judged on
/// `gl`: pass, near or fail as the GPU path's sidecar says, an error when
/// the screen did not draw, and pending, with the reason, for a screen the
/// GPU path does not draw yet. Each drawing is written to `actual_dir`, and
/// each difference to `diff_dir`, under the case's name.
pub fn reports<G: Gl>(
    screens: &[Screen],
    root: &Path,
    gl: &mut G,
    (actual_dir, diff_dir): (&Path, &Path),
) -> Vec<CaseReport> {
    let mut out = Vec::new();
    for s in screens {
        let name = case_name(s.name);
        if let Some(why) = waits(s) {
            let mut r = CaseReport::new(&name, Status::Pending);
            r.note = Some(why.to_owned());
            out.push(r);
            continue;
        }
        let mut r = CaseReport::new(&name, Status::Error);
        r.description = Some(format!(
            "the GPU path's drawing of {}, judged against the CPU path's",
            s.name
        ));
        match judge(s, root, gl) {
            Err(e) => r.note = Some(format!("not judged: {e}")),
            Ok(j) => {
                let c = &j.comparison;
                r.status = match c.verdict {
                    Verdict::Pass => Status::Pass,
                    Verdict::Near => Status::Near,
                    Verdict::Fail => Status::Fail,
                };
                r.tolerance = Some(j.tolerance.clone());
                r.pixels = c.pixels;
                r.max_delta = c.max_delta;
                r.regions = c.regions.clone();
                r.note = Some(format!(
                    "{:.3}% of the screen's pixels differ, by at most {}",
                    j.share(),
                    c.max_delta
                ));
                let actual = actual_dir.join(format!("{name}.ppm"));
                if fs::write(&actual, j.image.to_ppm()).is_ok() {
                    r.actual = Some(actual.display().to_string());
                }
                if c.pixels > 0 && c.size_mismatch.is_none() {
                    let diff = diff_dir.join(format!("{name}.ppm"));
                    let img = diff_image(&j.image, &j.tolerance, &c.points);
                    if fs::write(&diff, img.to_ppm()).is_ok() {
                        r.diff = Some(diff.display().to_string());
                    }
                }
            }
        }
        out.push(r);
    }
    out
}

/// vtium's GL on Mesa's llvmpipe, which needs no device: the context
/// tier 2 draws the GPU path's screens on, through the binary's adapter.
pub fn llvmpipe() -> Result<vtium::gpu::PlatformGl, String> {
    let context = vtium_platform::gpu::Context::software()
        .map_err(|e| format!("no llvmpipe context: {e}"))?;
    Ok(vtium::gpu::PlatformGl::new(context))
}
