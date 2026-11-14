//! Clusters through the shaper and the rasteriser (ADR-1200 D14, ADR-1301
//! D1): no panic, whatever the cluster or the font. Its first byte picks
//! what the input is. Even: the rest is up to 16 clusters for the test
//! font of `tools/testfont/src/ttf.rs`, each a length byte and that many
//! scalar values of three bytes, folded into Unicode. Odd: the rest is the
//! font, asked for a family, a flag, a keycap and a letter with its marks.
//! The first byte's next two bits pick the style synthesised over the
//! glyphs (ADR-1302 D4): bold emboldens each outline and italic slants it.
//! The bits above them pick the script (ADR-1202 D2, D5): normal,
//! superscript or subscript, whose color glyphs fit two thirds of their
//! cells against the cell's top or bottom. Each cluster is also asked for
//! as a piece of a syllable (ADR-1203 D3), its place among the syllable's
//! cells taken from its own scalar values, and the font of an odd input
//! is asked for a piece of Kannada ka, virama and ra too, so a syllable is
//! placed across its cells and cut into its pieces.
#![no_main]

use std::sync::Arc;

use libfuzzer_sys::fuzz_target;
use vtium_config::frit::Hinting;
use vtium_config::outline::{OutlineRequest, rasterise};
use vtium_config::shaping::ask;
use vtium_testfont::ttf;

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

/// `key` as the key of a piece of a syllable whose text it is, when it
/// holds 2 to 15 scalar values: the piece's place among the syllable's
/// cells comes from its first two scalar values.
fn piece(key: &[u32]) -> Option<Vec<u32>> {
    use vtium_font::syllable::{Piece, SYLLABLE_SCALARS_MAX};
    let n = key.len();
    if !(2..=SYLLABLE_SCALARS_MAX).contains(&n) {
        return None;
    }
    let total = u8::try_from(n).ok()?.saturating_mul(2);
    let start = u8::try_from(key[0] % u32::from(total)).ok()?;
    let cells = 1 + u8::try_from(key[1] % 2).ok()?;
    let p = Piece::new(start, cells, total).or_else(|| Piece::new(start, 1, total))?;
    let mut out = key.to_vec();
    out.push(u32::from(p.mark()));
    Some(out)
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
            &['\u{c95}', '\u{ccd}', '\u{cb0}'][..],
        ]
        .iter()
        .map(|k| k.iter().map(|&c| u32::from(c)).collect())
        .collect();
        (rest.into(), keys)
    };
    let pieces: Vec<Vec<u32>> = keys.iter().filter_map(|k| piece(k)).collect();
    let keys: Vec<Vec<u32>> = keys.into_iter().chain(pieces).collect();
    let cover: Vec<(u32, u32)> = keys.iter().flatten().map(|&c| (c, c)).collect();
    let request = OutlineRequest {
        cluster_width: Default::default(),
        fit: false,
        font,
        family: String::new(),
        cells: vec![(0, 16), (8, 16), (12, 24)],
        cover,
        clusters: keys.iter().map(|k| ask(k)).collect(),
        baselines: Vec::new(),
        sizes: Vec::new(),
        reaches: Vec::new(),
        hinting: Hinting::Light,
        synth: vtium_font::Style {
            bold: pick & 2 != 0,
            italic: pick & 4 != 0,
        },
        script: match (pick >> 3) % 3 {
            0 => vtium_font::Script::Baseline,
            1 => vtium_font::Script::Super,
            _ => vtium_font::Script::Sub,
        },
    };
    let _ = rasterise(&request);
});
