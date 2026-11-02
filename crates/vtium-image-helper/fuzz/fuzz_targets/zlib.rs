//! Any input, as zlib data the helper inflates for raw pixels (`o=z`),
//! the first two bytes choosing the pixels' size and layout: no panic, an
//! inflate never past its bound, and the pixels it converts exactly the
//! picture's.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image::decode::convert;
use vtium_image::decode::{Alpha, Layout};
use vtium_image_helper::inflate::{Goal, inflate_bounded};

fuzz_target!(|data: &[u8]| {
    let Some((&shape, rest)) = data.split_first() else {
        return;
    };
    let (layout, alpha) = match shape & 3 {
        0 => (Layout::Rgb, Alpha::Straight),
        1 => (Layout::Rgba, Alpha::Straight),
        _ => (Layout::Rgba, Alpha::Premultiplied),
    };
    let pixels = usize::from(shape >> 2) * 37 + 1;
    let need = (pixels * layout.bytes()) as u64;
    for goal in [
        Goal::Stop(need),
        Goal::Exact(need),
        Goal::Budget { min: need, max: need * 2 },
    ] {
        if let Ok(inflated) = inflate_bounded(rest, goal) {
            let n = inflated.bytes.len() as u64;
            match goal {
                Goal::Stop(m) | Goal::Exact(m) => assert_eq!(n, m),
                Goal::Budget { min, max } => assert!(n >= min && n <= max),
            }
            let reply = convert::into_reply(layout, alpha, inflated.bytes, pixels);
            assert_eq!(reply.map(|r| r.len()), Some(pixels * 4));
        }
    }
});
