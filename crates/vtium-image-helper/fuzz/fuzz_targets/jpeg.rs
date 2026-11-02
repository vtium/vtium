//! Any input, as a JPEG file the helper decodes for iTerm2's pictures: no
//! panic, and a picture is exactly its size's opaque pixels.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image_helper::sandbox::Locked;
use vtium_image_helper::decode::{self, Bounds};

fuzz_target!(|data: &[u8]| {
    let bounds = Bounds {
        largest: (512, 512),
        max_bytes: 1 << 20,
    };
    if let Ok(p) = decode::jpeg(data, bounds, &Locked::unsandboxed()) {
        assert_eq!(p.rgba.len() as u64, u64::from(p.width) * u64::from(p.height) * 4);
        assert!(p.rgba.chunks_exact(4).all(|px| px[3] == 0xff));
    }
});
