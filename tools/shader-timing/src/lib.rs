//! Times spatial shaders off-screen on a render node, so that the cap on a
//! chain's reads and the frame-rate floor can be set from real numbers.
//!
//! The harness draws each shader over a terminal-like frame at a display's
//! size, on a GL context with no surface, and reports each frame's time on
//! the GPU. README.md says how to run it and what each column means.
//!
//! References:
//! - ADR-1304 D18, "measuring first": what the harness records for each
//!   shader, and the list of shaders it times.
//! - ADR-1304 D16: the floor a look's cost is judged against, and how a
//!   frame is timed.

pub mod device;
pub mod source;
pub mod timing;

use source::{Mode, Shader};
use timing::{Bench, Pass, Sample, Surface};

/// One thing to time: a shader alone, or two chained.
#[derive(Clone, Debug)]
pub enum Plan {
    Single(Shader),
    /// The second filter's reads run the first in the same pass.
    OnePass(Shader, Shader),
    /// The first filter draws into a texture, which the second reads.
    TwoPasses(Shader, Shader),
}

impl Plan {
    pub fn label(&self) -> String {
        match self {
            Plan::Single(s) => s.name.to_owned(),
            Plan::OnePass(a, b) => format!("{} then {}, one pass", a.name, b.name),
            Plan::TwoPasses(a, b) => format!("{} then {}, two passes", a.name, b.name),
        }
    }

    /// The texels each pixel reads, as vtium's checker would count them: a
    /// smooth read counts as 4, and a chain in one pass multiplies its
    /// filters' reads, the smooth reads of the second included.
    pub fn texels(&self, mode: Mode) -> u32 {
        let t = mode.texels();
        match self {
            Plan::Single(s) => s.reads * t,
            Plan::OnePass(a, b) => a.reads * t * b.reads * t,
            Plan::TwoPasses(a, b) => (a.reads + b.reads) * t,
        }
    }

    pub fn work(&self) -> String {
        match self {
            Plan::Single(s) => s.work.clone(),
            Plan::OnePass(a, b) | Plan::TwoPasses(a, b) => format!("{}; then {}", a.work, b.work),
        }
    }
}

/// The plans ADR-1304 D18 lists, from cheap to expensive, ending with crt
/// then bloom in one pass and in two.
pub fn plans(shaders: &[Shader]) -> Vec<Plan> {
    let mut out: Vec<Plan> = shaders.iter().cloned().map(Plan::Single).collect();
    let find = |name: &str| shaders.iter().find(|s| s.name == name).cloned();
    if let (Some(crt), Some(bloom)) = (find("crt"), find("bloom")) {
        out.push(Plan::OnePass(crt.clone(), bloom.clone()));
        out.push(Plan::TwoPasses(crt, bloom));
    }
    out
}

/// One timed plan at one size and in one read mode.
#[derive(Clone, Debug)]
pub struct Row {
    pub plan: String,
    pub mode: Mode,
    pub width: u32,
    pub height: u32,
    pub texels: u32,
    pub frames: usize,
    /// The GPU clock's median and largest frame, in milliseconds.
    pub gpu: Option<(f64, f64)>,
    /// The CPU clock's median and largest frame, `glFinish` on both
    /// sides, in milliseconds.
    pub wall: (f64, f64),
    pub refresh: f32,
}

impl Row {
    /// The median frame's time the rate is judged by: the CPU's clock
    /// with `glFinish` on both sides. It is right on every driver the
    /// harness has run on. llvmpipe offers timer queries, but they report a
    /// fraction of a heavy frame: 8.7 ms for a fireworks-rockets frame that
    /// takes 57.5 ms. On the two GPUs measured, the CPU's clock reads at
    /// most 0.25 ms more than the GPU's for a frame of one pass.
    pub fn frame_ms(&self) -> f64 {
        self.wall.0
    }

    /// Frames a second, one second over the median frame's time.
    pub fn fps(&self) -> f64 {
        1000.0 / self.frame_ms()
    }

    /// Whether the rate meets a floor of the display's refresh.
    pub fn meets(&self) -> bool {
        self.fps() >= f64::from(self.refresh)
    }

    pub const HEADER: &'static str = "plan\tmode\tsize\ttexels\tframes\tgpu_median_ms\tgpu_max_ms\twall_median_ms\twall_max_ms\tfps\tfloor\tmeets";

    pub fn tsv(&self) -> String {
        let (gm, gx) = self.gpu.map_or(("-".to_owned(), "-".to_owned()), |(m, x)| {
            (format!("{m:.3}"), format!("{x:.3}"))
        });
        format!(
            "{}\t{}\t{}x{}\t{}\t{}\t{gm}\t{gx}\t{:.3}\t{:.3}\t{:.0}\t{}\t{}",
            self.plan,
            self.mode.name(),
            self.width,
            self.height,
            self.texels,
            self.frames,
            self.wall.0,
            self.wall.1,
            self.fps(),
            self.refresh,
            if self.meets() { "yes" } else { "no" }
        )
    }
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

/// The median and largest of a run's frames, in milliseconds, on each
/// clock.
pub fn summarize(samples: &[Sample]) -> (Option<(f64, f64)>, (f64, f64)) {
    let mut wall: Vec<u64> = samples.iter().map(|s| s.wall_ns).collect();
    let mut gpu: Vec<u64> = samples.iter().filter_map(|s| s.gpu_ns).collect();
    let (wm, wx) = timing::median_max(&mut wall).unwrap_or((0, 0));
    // a GPU clock that missed more than half the frames judges nothing
    let gpu = (gpu.len() * 2 > samples.len())
        .then(|| timing::median_max(&mut gpu))
        .flatten()
        .map(|(m, x)| (ms(m), ms(x)));
    (gpu, (ms(wm), ms(wx)))
}

/// Builds, draws and times one plan.
pub fn time_plan(
    bench: &Bench,
    plan: &Plan,
    mode: Mode,
    warmup: u32,
    frames: u32,
    refresh: f32,
) -> Result<Row, String> {
    let linear = mode.linear_sampler();
    let samples = match plan {
        Plan::Single(s) => {
            let p = bench.program(&source::single(s, mode)?)?;
            let pass = Pass {
                program: &p,
                input: Surface::Frame,
                output: Surface::Frame,
                linear,
            };
            let r = bench.run(&[pass], warmup, frames, refresh);
            bench.delete(p);
            r?
        }
        Plan::OnePass(a, b) => {
            let p = bench.program(&source::chain(a, b, mode)?)?;
            let pass = Pass {
                program: &p,
                input: Surface::Frame,
                output: Surface::Frame,
                linear,
            };
            let r = bench.run(&[pass], warmup, frames, refresh);
            bench.delete(p);
            r?
        }
        Plan::TwoPasses(a, b) => {
            let pa = bench.program(&source::single(a, mode)?)?;
            let pb = match bench.program(&source::single(b, mode)?) {
                Ok(p) => p,
                Err(e) => {
                    bench.delete(pa);
                    return Err(e);
                }
            };
            let passes = [
                Pass {
                    program: &pa,
                    input: Surface::Frame,
                    output: Surface::Between,
                    linear,
                },
                Pass {
                    program: &pb,
                    input: Surface::Between,
                    output: Surface::Frame,
                    linear,
                },
            ];
            let r = bench.run(&passes, warmup, frames, refresh);
            bench.delete(pa);
            bench.delete(pb);
            r?
        }
    };
    let (gpu, wall) = summarize(&samples);
    Ok(Row {
        plan: plan.label(),
        mode,
        width: bench.width,
        height: bench.height,
        texels: plan.texels(mode),
        frames: samples.len(),
        gpu,
        wall,
        refresh,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plans_follow_the_record_and_end_with_the_chain() {
        let shaders = source::catalog().unwrap();
        let plans = plans(&shaders);
        let labels: Vec<String> = plans.iter().map(Plan::label).collect();
        assert_eq!(labels[0], "negative");
        assert_eq!(labels[labels.len() - 2], "crt then bloom, one pass");
        assert_eq!(labels[labels.len() - 1], "crt then bloom, two passes");
        // crt reads 8 and bloom 25, so one pass of both reads 200
        assert_eq!(plans[plans.len() - 2].texels(Mode::Plain), 200);
        assert_eq!(plans[plans.len() - 2].texels(Mode::Smooth), 3200);
        assert_eq!(plans[plans.len() - 1].texels(Mode::Plain), 33);
    }

    #[test]
    fn a_gpu_clock_that_misses_most_frames_is_dropped() {
        let s = |gpu: Option<u64>| Sample {
            gpu_ns: gpu,
            wall_ns: 2_000_000,
        };
        let (gpu, wall) = summarize(&[s(Some(1_000_000)), s(None), s(None)]);
        assert_eq!(gpu, None);
        assert_eq!(wall, (2.0, 2.0));
        let (gpu, _) = summarize(&[s(Some(1_000_000)), s(Some(3_000_000)), s(None)]);
        assert_eq!(gpu, Some((2.0, 3.0)));
    }
}
