//! Clusters through the shaper and the rasteriser (ADR-1200 D14, ADR-1301
//! D1): no panic, whatever the cluster or the font. Its first byte picks
//! what the input is. Even: the rest is up to 16 clusters for the test
//! font of `tests/common/ttf.rs`, each a length byte and that many scalar
//! values of three bytes, folded into Unicode. Odd: the rest is the font,
//! asked for a family, a flag, a keycap and a letter with its marks.
#![no_main]

#[path = "../../tests/common/ttf.rs"]
#[allow(dead_code)]
mod ttf;

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_vitrify::outline::{OutlineRequest, rasterise};
use vtium_vitrify::shaping::ask;

/// Up to 16 clusters of `bytes`, each 1 to 16 scalar values.
fn clusters(mut bytes: &[u8]) -> Vec<Vec<u32>> {
    let mut out = Vec::new();
    while let [n, rest @ ..] = bytes {
        let n = usize::from(n % 16) + 1;
        let (key, more) = rest.split_at((n * 3).min(rest.len()));
        let key: Vec<u32> = key
            .chunks(3)
            .map(|c| c.iter().fold(0u32, |v, &b| v << 8 | u32::from(b)) % 0x11_0000)
            .filter(|&c| char::from_u32(c).is_some())
            .collect();
        if !key.is_empty() {
            out.push(key);
        }
        bytes = more;
        if out.len() == 16 {
            break;
        }
    }
    out
}

fuzz_target!(|data: &[u8]| {
    let [pick, rest @ ..] = data else {
        return;
    };
    let (font, keys): (Arc<[u8]>, Vec<Vec<u32>>) = if pick % 2 == 0 {
        (ttf::test_shaping().build().into(), clusters(rest))
    } else {
        let keys = [
            &ttf::FAMILY[..],
            &ttf::FLAG[..],
            &['1', '\u{fe0f}', '\u{20e3}'][..],
            &['e', '\u{302}', '\u{301}'][..],
        ]
        .iter()
        .map(|k| k.iter().map(|&c| u32::from(c)).collect())
        .collect();
        (rest.into(), keys)
    };
    let cover: Vec<(u32, u32)> = keys.iter().flatten().map(|&c| (c, c)).collect();
    let request = OutlineRequest {
        font,
        family: String::new(),
        cells: vec![(0, 16), (8, 16), (12, 24)],
        cover,
        clusters: keys.iter().map(|k| ask(k)).collect(),
    };
    let _ = rasterise(&request);
});
