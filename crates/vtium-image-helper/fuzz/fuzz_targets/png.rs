//! Any input, as a PNG file the helper decodes, alone and inside zlib, as
//! kitty's `f=100` and `f=100,o=z` and iTerm2's pictures reach it: no
//! panic, and a picture is exactly its size's premultiplied pixels, within
//! the bounds its header was checked against.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image_helper::sandbox::Locked;
use vtium_image_helper::decode::{self, Bounds};
use vtium_image_helper::inflate::{Goal, inflate_bounded};

const BOUNDS: Bounds = Bounds {
    largest: (512, 512),
    max_bytes: 1 << 20,
};

fn check(picture: Result<decode::Picture, vtium_image_helper::job::Failure>) {
    if let Ok(p) = picture {
        assert!(p.width <= BOUNDS.largest.0 && p.height <= BOUNDS.largest.1);
        assert_eq!(p.rgba.len() as u64, u64::from(p.width) * u64::from(p.height) * 4);
        for px in p.rgba.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3], "premultiplied");
        }
    }
}

fuzz_target!(|data: &[u8]| {
    check(decode::png(data, BOUNDS, &Locked::unsandboxed()));
    check(decode::any(data, BOUNDS, &Locked::unsandboxed()));
    let goal = Goal::Budget {
        min: 0,
        max: BOUNDS.max_bytes,
    };
    if let Ok(inflated) = inflate_bounded(data, goal, &Locked::unsandboxed()) {
        check(decode::png(&inflated.bytes, BOUNDS, &Locked::unsandboxed()));
    }
});
