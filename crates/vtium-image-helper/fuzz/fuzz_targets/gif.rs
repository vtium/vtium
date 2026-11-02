//! Any input, as a GIF file the helper decodes for iTerm2's pictures, its
//! first frame on a canvas of its own size: no panic, and a picture is
//! exactly its size's premultiplied pixels.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image_helper::sandbox::Locked;
use vtium_image_helper::decode::{self, Bounds};

fuzz_target!(|data: &[u8]| {
    let bounds = Bounds {
        largest: (512, 512),
        max_bytes: 1 << 20,
    };
    if let Ok(p) = decode::gif(data, bounds, &Locked::unsandboxed()) {
        assert_eq!(p.rgba.len() as u64, u64::from(p.width) * u64::from(p.height) * 4);
        for px in p.rgba.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3], "premultiplied");
        }
    }
});
