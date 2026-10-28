//! vtium-font's sprites at 1 bit (vtium/vtium#135): drawn as the renderer
//! draws them there, their coverage cut at half or vtium-font's 1-bit
//! shape where it has one, each keeps as many pieces as its coverage has.
//! The hatches, U+1FB98 and U+1FB99, are diagonal stripes whose
//! antialiased edges touch: at 1 bit they are their separate stripes. The
//! shades, U+2591 to U+2593, are ordered patterns at 1 bit, the light one
//! dots (`vtium-font`'s `tests/shade.rs`).

use vtium_font::sprite;

/// The 8-connected pieces of the `w` by `h` pixels `lit` says are set.
fn pieces(w: u32, h: u32, lit: impl Fn(u32, u32) -> bool) -> usize {
    let (w, h) = (w as usize, h as usize);
    let mut seen = vec![false; w * h];
    let mut n = 0;
    for start in 0..w * h {
        if seen[start] || !lit((start % w) as u32, (start / w) as u32) {
            continue;
        }
        n += 1;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !seen[j] && lit(nx as u32, ny as u32) {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
    }
    n
}

/// Every character vtium-font draws as a sprite.
fn sprites() -> impl Iterator<Item = char> {
    [
        0x2500..0x2600u32,
        0x2800..0x2900,
        0x1cc00..0x1cf00,
        0x1fb00..0x1fc00,
    ]
    .into_iter()
    .flatten()
    .filter_map(char::from_u32)
    .filter(|&c| sprite::has(c))
}

#[test]
fn every_sprite_keeps_its_pieces_at_1_bit() {
    let mut bad = Vec::new();
    let mut checked = 0;
    for (w, h) in [(8, 8), (8, 16), (10, 20), (12, 24), (16, 32)] {
        for c in sprites() {
            let Some(g) = sprite::draw(c, w, h, 1) else {
                continue;
            };
            let px = g.pixels();
            let at = |x: u32, y: u32| px[(y * w + x) as usize];
            if !px.iter().any(|&p| p != 0 && p != 0xff) {
                continue;
            }
            if matches!(c, '\u{1fb98}' | '\u{1fb99}' | '\u{2591}'..='\u{2593}') {
                continue;
            }
            checked += 1;
            let covered = pieces(w, h, |x, y| at(x, y) != 0);
            let cut = match sprite::one_bit(c, w, h, 1) {
                Some(b) => {
                    let bits = b.pixels();
                    assert!(
                        bits.iter().all(|&p| p == 0 || p == 0xff),
                        "U+{:04X}",
                        c as u32
                    );
                    pieces(w, h, |x, y| bits[(y * w + x) as usize] == 0xff)
                }
                None => pieces(w, h, |x, y| at(x, y) >= 128),
            };
            if cut > covered {
                bad.push(format!("U+{:04X} {w}x{h}: {covered} -> {cut}", c as u32));
            }
        }
    }
    assert!(checked > 100, "{checked} sprites of partial coverage");
    assert!(bad.is_empty(), "{} break: {bad:#?}", bad.len());
}
