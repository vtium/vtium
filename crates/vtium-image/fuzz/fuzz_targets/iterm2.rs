//! Any input, as the text of OSC 1337 strings the iTerm2 handler takes: no
//! panic, and the strings cut where the input's first byte says give the
//! screen and its pictures whole.
//!
//! The input's second byte chooses the helper's answer to every request:
//! a still picture, an animated GIF's frames whose count, size and loop
//! count come from that byte, or a refusal, so the handler's storing and
//! placing paths all run. The rest of the input is split at each `|` into
//! the text of one OSC 1337 string each, so `File=`, `MultipartFile=`,
//! `FilePart=` and `FileEnd` arrive in the order they came; a part may
//! carry any byte, a control that cuts its string included.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vtium_term::images::decode::{FrameData, Payload, Reply};
use vtium_term::images::{Iterm2, Switches};
use vtium_term::{ImageString, Terminal};

/// The helper's answer that `choice` picks.
fn answer(choice: u8) -> Reply {
    let (w, h) = (u32::from(choice % 7) + 1, u32::from(choice / 7 % 5) + 1);
    let px = |n: u32| vec![0x40; (w * h * 4 * n) as usize];
    match choice % 4 {
        0 => Reply::Ok(Payload::Rgba {
            width: w,
            height: h,
            data: px(1),
        }),
        1 | 2 => {
            let count = u32::from(choice % 5) + 2;
            Reply::Ok(Payload::Frames {
                width: w,
                height: h,
                delays: (0..count).map(|d| d as u16).collect(),
                loops: (choice & 0x80 != 0).then_some(u16::from(choice)),
                data: FrameData::Memory(px(count)),
            })
        }
        _ => Reply::Corrupt,
    }
}

fn fed(pieces: &[&[u8]], choice: u8) -> Terminal {
    let mut t = Terminal::new(20, 6, 16);
    t.screen_mut().set_image_limit(1 << 20);
    t.set_image_switches(Switches {
        sixel: true,
        kitty: false,
        iterm2: true,
    });
    t.set_image_helper(true);
    t.set_image_handler(ImageString::Iterm2, Some(Box::new(Iterm2::default())));
    for piece in pieces {
        let mut rest = *piece;
        while !rest.is_empty() {
            let taken = t.input(rest);
            rest = rest.get(taken..).unwrap_or_default();
            if t.take_image_request().is_some() {
                t.image_reply(answer(choice));
            } else if taken == 0 {
                break;
            }
        }
    }
    t
}

fuzz_target!(|data: &[u8]| {
    let [cut, choice, payload @ ..] = data else {
        return;
    };
    let mut stream = Vec::new();
    for string in payload.split(|&b| b == b'|') {
        stream.extend_from_slice(b"\x1b]1337;");
        stream.extend_from_slice(string);
        stream.push(0x07);
    }
    let at = usize::from(*cut)
        .saturating_mul(stream.len())
        .checked_div(255)
        .unwrap_or(0)
        .min(stream.len());
    let whole = fed(&[&stream], *choice);
    let (a, b) = stream.split_at(at);
    let cut = fed(&[a, b], *choice);
    assert!(whole.screen() == cut.screen());
});
