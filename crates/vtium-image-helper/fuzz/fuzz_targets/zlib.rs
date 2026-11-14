//! Any input, as zlib data the helper inflates for raw pixels (`o=z`),
//! the first byte choosing the pixels' count and layout, kitty's palette
//! layouts among them: no panic, an inflate never past its bound, the
//! pixels it converts exactly the picture's, and palette indices in the
//! reply's index form.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_image_helper::sandbox::Locked;
use vtium_image::decode::convert;
use vtium_image::decode::{Alpha, Layout};
use vtium_image_helper::inflate::{Goal, inflate_bounded};

fuzz_target!(|data: &[u8]| {
    let Some((&shape, rest)) = data.split_first() else {
        return;
    };
    // 3 picks kitty's palette layouts, f=8 or f=16 by the next bit, which
    // the helper returns unconverted, four bytes a pixel
    let (layout, alpha) = match shape & 3 {
        0 => (Layout::Rgb, Alpha::Straight),
        1 => (Layout::Rgba, Alpha::Straight),
        2 => (Layout::Rgba, Alpha::Premultiplied),
        _ if shape & 4 == 0 => (Layout::Index, Alpha::Straight),
        _ => (Layout::IndexAlpha, Alpha::Straight),
    };
    let pixels = usize::from(shape >> 2) * 37 + 1;
    let need = (pixels * layout.bytes()) as u64;
    for goal in [
        Goal::Stop(need),
        Goal::Exact(need),
        Goal::Budget { min: need, max: need * 2 },
    ] {
        if let Ok(inflated) = inflate_bounded(rest, goal, &Locked::unsandboxed()) {
            let n = inflated.bytes.len() as u64;
            match goal {
                Goal::Stop(m) | Goal::Exact(m) => assert_eq!(n, m),
                Goal::Budget { min, max } => assert!(n >= min && n <= max),
            }
            let reply = convert::into_reply(layout, alpha, inflated.bytes, pixels);
            assert_eq!(reply.as_ref().map(Vec::len), Some(pixels * 4));
            if layout.indexed()
                && let Some(reply) = reply
            {
                // the index form: index, alpha, then two zero bytes
                assert!(reply.chunks_exact(4).all(|g| g[2] == 0 && g[3] == 0));
                if layout == Layout::Index {
                    assert!(reply.chunks_exact(4).all(|g| g[1] == 0xff));
                }
            }
        }
    }
});
