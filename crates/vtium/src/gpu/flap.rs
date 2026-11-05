//! The two counts that keep a head from flapping between the GPU path and
//! the CPU path (ADR-1303 D9). Both count events, never time, and neither
//! holds any GL.
//!
//! [`Window`] holds one display's last [`WINDOW`] frame results on the GPU
//! path and says when they end the head's GPU path: a GL error in the last
//! two of them, or in [`FAILED_IN_WINDOW`] of them. A fault that fails one
//! frame and draws the next therefore ends the path, and a lone error ages
//! out of the window. [`TripsBack`] counts how many times a head went back
//! to the GPU path after a display held it on the CPU path, and says when
//! the next hold keeps the head on the CPU path until vtium restarts.

/// The failed frames within a display's last [`WINDOW`] frames that end
/// its head's GPU path (the maintainer, 2026-11-04).
pub const FAILED_IN_WINDOW: u32 = 3;

/// The frames of a display that [`Window`] holds (the maintainer,
/// 2026-11-04).
pub const WINDOW: u32 = 60;

/// The times a head goes back to the GPU path after a display held it on
/// the CPU path, in a run (the maintainer, 2026-11-04).
pub const TRIPS_BACK: u32 = 3;

// one bit a frame in a u64, and two in a row is a stricter rule than the
// window's only when the window asks for more than two
const _: () = assert!(WINDOW <= u64::BITS && FAILED_IN_WINDOW > 2);

/// Why a display's frames end its head's GPU path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// A GL error in the display's last two frames.
    InARow,
    /// A GL error in [`FAILED_IN_WINDOW`] of the display's last [`WINDOW`]
    /// frames.
    InWindow,
}

impl std::fmt::Display for End {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            End::InARow => write!(f, "a GL error in two of a display's frames in a row"),
            End::InWindow => write!(
                f,
                "a GL error in {FAILED_IN_WINDOW} of a display's last {WINDOW} frames"
            ),
        }
    }
}

/// A display's last [`WINDOW`] frames on the GPU path, each failed or
/// drawn. A frame whose set-up failed counts as a failed frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// One bit for each frame, the newest lowest: a set bit is a frame a
    /// GL error failed. Bits past the window are cleared as frames come.
    failed: u64,
}

impl Window {
    /// Records the display's next frame, which a GL error failed when
    /// `failed`, and says whether that ends the head's GPU path. A drawn
    /// frame never does.
    pub fn frame(&mut self, failed: bool) -> Option<End> {
        let kept = u64::MAX >> (u64::BITS - WINDOW);
        self.failed = (self.failed << 1 | u64::from(failed)) & kept;
        if !failed {
            None
        } else if self.failed & 0b11 == 0b11 {
            Some(End::InARow)
        } else if self.failed.count_ones() >= FAILED_IN_WINDOW {
            Some(End::InWindow)
        } else {
            None
        }
    }
}

/// How long a display holds its head on the CPU path, once its set-up
/// failed for a reason the same size meets again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// While the display keeps its size: the head goes back to the GPU
    /// path when the display takes another or leaves the head.
    WhileItsSizeLasts,
    /// Until vtium restarts: the head went back [`TRIPS_BACK`] times
    /// already in this run.
    UntilRestart,
}

/// How many times a head went back to the GPU path after a display held
/// it on the CPU path, in this run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TripsBack {
    taken: u32,
}

impl TripsBack {
    /// The head went back to the GPU path.
    pub fn back(&mut self) {
        self.taken = self.taken.saturating_add(1);
    }

    /// A display holds the head on the CPU path: for how long.
    pub fn held(&self) -> Hold {
        if self.taken < TRIPS_BACK {
            Hold::WhileItsSizeLasts
        } else {
            Hold::UntilRestart
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame, counted from 0, at which `frames` end the GPU path, and
    /// why: in `frames`, `x` is a failed frame and `.` a drawn one.
    fn ends_at(frames: &str) -> Option<(usize, End)> {
        let mut window = Window::default();
        frames
            .chars()
            .enumerate()
            .find_map(|(i, c)| window.frame(c == 'x').map(|end| (i, end)))
    }

    /// `n` drawn frames.
    fn good(n: usize) -> String {
        ".".repeat(n)
    }

    /// A case's name, its frames, and where and why they end the path.
    type Case = (&'static str, String, Option<(usize, End)>);

    #[test]
    fn a_window_ends_the_gpu_path_at_the_failure_that_breaks_a_rule() {
        let lone_every_100 = format!("x{0}x{0}x{0}x", good(99));
        let cases: [Case; 10] = [
            ("no failure", good(200), None),
            ("one failure", format!("x{}", good(200)), None),
            ("two in a row", "..xx".into(), Some((3, End::InARow))),
            ("alternating", "x.x.x".into(), Some((4, End::InWindow))),
            ("two apart", "x.x".into(), None),
            ("a lone error every 100 frames", lone_every_100, None),
            (
                "three within 60 frames, the first and last 59 apart",
                format!("x{}x{}x", good(29), good(28)),
                Some((59, End::InWindow)),
            ),
            (
                "three 60 apart: the first has aged out",
                format!("x{}x{}x", good(29), good(29)),
                None,
            ),
            (
                "two that aged out, then two in a row",
                format!("x.x{}xx", good(60)),
                Some((64, End::InARow)),
            ),
            (
                "two that aged out, then two apart",
                format!("x.x{}x.x", good(60)),
                None,
            ),
        ];
        for (name, frames, want) in cases {
            assert_eq!(ends_at(&frames), want, "{name}");
        }
    }

    #[test]
    fn a_head_goes_back_to_the_gpu_path_three_times_and_the_fourth_hold_lasts() {
        let mut trips = TripsBack::default();
        let mut holds = Vec::new();
        for _ in 0..4 {
            holds.push(trips.held());
            if trips.held() == Hold::WhileItsSizeLasts {
                trips.back();
            }
        }
        assert_eq!(
            holds,
            [
                Hold::WhileItsSizeLasts,
                Hold::WhileItsSizeLasts,
                Hold::WhileItsSizeLasts,
                Hold::UntilRestart,
            ]
        );
        trips.back();
        assert_eq!(trips.held(), Hold::UntilRestart, "the hold stays");
    }
}
