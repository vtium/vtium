//! Any input, as a GIF file the helper decodes for iTerm2's pictures,
//! every frame composed whole on a canvas of its own size: no panic, each
//! frame exactly its size's premultiplied pixels, frames handed over in
//! order, and one delay for each frame kept.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image_helper::decode::{self, Bounds, FrameBounds};
use vtium_image_helper::sandbox::Locked;

fuzz_target!(|data: &[u8]| {
    let bounds = Bounds {
        largest: (512, 512),
        max_bytes: 1 << 20,
    };
    let frames = FrameBounds {
        max_frames: 64,
        max_frame_bytes: 8 << 20,
    };
    let mut written = 0u32;
    let mut size = 0usize;
    let got = decode::gif_frames(data, bounds, Some(frames), &Locked::unsandboxed(), &mut |n, rgba| {
        assert_eq!(n, written, "frames in order");
        written += 1;
        size = rgba.len();
        for px in rgba.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3], "premultiplied");
        }
        Ok(())
    });
    if let Ok(f) = got {
        assert_eq!(size as u64, u64::from(f.width) * u64::from(f.height) * 4);
        assert!(written <= frames.max_frames);
        let kept = if f.still.is_some() { 1 } else { written as usize };
        assert_eq!(f.delays.len(), kept);
    }
});
