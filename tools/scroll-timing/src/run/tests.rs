use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;

/// A display that counts flips: each flip lands one refresh period
/// after it is asked for, when the run waits for it, so its clock
/// reads the flips landed so far, in refresh periods. `flipping` is
/// shared with the test, which reads it as each frame is drawn.
struct Counted {
    flipping: Rc<Cell<bool>>,
    landed: u32,
}

impl Flipper for Counted {
    fn wait_for_flip(&mut self) -> io::Result<bool> {
        let was = self.flipping.replace(false);
        self.landed += u32::from(was);
        Ok(was)
    }

    fn request_flip(&mut self, _: u32, _: u32) -> io::Result<()> {
        assert!(
            !self.flipping.get(),
            "a flip asked for while the one before is in flight"
        );
        self.flipping.set(true);
        Ok(())
    }

    fn seconds(&self) -> f64 {
        f64::from(self.landed)
    }
}

/// Runs `warmup` and `frames` frames on a counted display with
/// `buffers` framebuffers, and returns what the run timed and whether
/// a flip was in flight as each frame was drawn.
fn counted(buffers: u32, warmup: usize, frames: usize) -> (Timed, Vec<bool>) {
    let flipping = Rc::new(Cell::new(false));
    let mut display = Counted {
        flipping: flipping.clone(),
        landed: 0,
    };
    let drawn_while_flipping = RefCell::new(Vec::new());
    let fbs: Vec<u32> = (1..=buffers).collect();
    let t = pan_frames(&mut display, &fbs, warmup, frames, |_| {
        drawn_while_flipping.borrow_mut().push(flipping.get());
        (Counts::default(), 0)
    })
    .unwrap_or_else(|e| panic!("the counted display failed: {e}"));
    (t, drawn_while_flipping.into_inner())
}

/// F timed frames span F refresh periods, for every buffer count, so a
/// pan whose every frame fits a period reaches the display's refresh
/// rate.
#[test]
fn the_pans_timed_frames_span_one_refresh_period_each() {
    for buffers in 1..=3 {
        for (warmup, frames) in [(3, 10), (20, 120), (1, 1)] {
            let (t, _) = counted(buffers, warmup, frames);
            assert_eq!(
                t.elapsed, frames as f64,
                "{buffers} buffers, {warmup} warm-up frames: refresh periods for {frames} frames"
            );
            assert_eq!(t.cpu.len(), frames, "{buffers} buffers: frames timed");
            assert_eq!(
                t.commits as usize,
                warmup + frames,
                "{buffers} buffers: commits"
            );
        }
    }
}

/// With one and two buffers, no frame is drawn before the flip before
/// it lands; with three, every frame after the first is drawn while
/// that flip is in flight.
#[test]
fn one_and_two_buffers_wait_for_the_flip_before_they_draw() {
    for buffers in 1..=2 {
        let (_, flipping) = counted(buffers, 3, 10);
        assert!(
            flipping.iter().all(|&f| !f),
            "{buffers} buffers: drawn while a flip was in flight at frames {:?}",
            (0..flipping.len())
                .filter(|&f| flipping[f])
                .collect::<Vec<_>>()
        );
    }
    let (_, flipping) = counted(3, 3, 10);
    assert!(
        !flipping[0],
        "3 buffers: the first frame has no flip before it"
    );
    assert!(
        flipping[1..].iter().all(|&f| f),
        "3 buffers: a frame waited for the flip before it"
    );
}
