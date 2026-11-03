//! Any input, as the data of APC `G` strings the kitty graphics handler
//! takes: no panic, and the strings cut where the input's first byte says
//! give the screen and the replies they give whole.
//!
//! The input's second byte chooses the host: with its low bit set the
//! terminal has an image helper, which answers every request at once, by
//! the request alone, as the replay interface's fixture host answers from
//! its files: the picture a raw format's description gives, a one-pixel
//! picture for PNG, and OK for a discard, every picture premultiplied as
//! the helper sends it. The rest of the input is split at each `|` into
//! the data of one string each, so chunked uploads, deletes and puts
//! reach the handler in the order they came.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::images::decode::{Payload, Reply, Request};
use vtium_term::images::{Kitty, Switches};
use vtium_term::{ImageString, Terminal};

/// The helper's answer to `request`, from the request alone.
fn answer(request: &Request) -> Reply {
    if request.discard {
        return Reply::Ok(Payload::Discarded);
    }
    if request.png {
        return Reply::Ok(Payload::Rgba {
            width: 1,
            height: 1,
            data: vec![0x10, 0x20, 0x30, 0x80],
        });
    }
    let Some(d) = request.pixels else {
        return Reply::Corrupt;
    };
    let n = u64::from(d.width).saturating_mul(u64::from(d.height));
    match usize::try_from(n.saturating_mul(4)) {
        Ok(len) if len <= 1 << 20 => Reply::Ok(Payload::Rgba {
            width: d.width,
            height: d.height,
            data: vec![0x7f; len],
        }),
        _ => Reply::TooLarge,
    }
}

fn fed(pieces: &[&[u8]], helper: bool) -> Terminal {
    let mut t = Terminal::new(20, 6, 16);
    t.screen_mut().set_image_limit(1 << 24);
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.set_image_helper(helper);
    t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    for piece in pieces {
        let mut rest = *piece;
        while !rest.is_empty() {
            let taken = t.input(rest);
            rest = rest.get(taken..).unwrap_or_default();
            if let Some(request) = t.take_image_request() {
                t.image_reply(answer(&request));
            } else if taken == 0 {
                break;
            }
        }
    }
    t
}

fuzz_target!(|data: &[u8]| {
    let [cut, host, payload @ ..] = data else {
        return;
    };
    let helper = host & 1 != 0;
    let mut stream = Vec::new();
    for string in payload.split(|&b| b == b'|') {
        stream.extend_from_slice(b"\x1b_G");
        stream.extend_from_slice(string);
        stream.extend_from_slice(b"\x1b\\");
    }
    let at = usize::from(*cut)
        .saturating_mul(stream.len())
        .checked_div(255)
        .unwrap_or(0)
        .min(stream.len());
    let whole = fed(&[&stream], helper);
    let (a, b) = stream.split_at(at);
    let cut = fed(&[a, b], helper);
    assert!(whole.screen() == cut.screen());
    assert_eq!(whole.replies(), cut.replies());
});
