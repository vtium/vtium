//! The drawn-glyph cache's memory on the minspec display, 1366x768
//! (ADR-1300, "Memory"). Each figure is measured by filling a renderer's
//! cache to its budgets with distinct glyphs and reading the bytes it holds,
//! and pinned, so a change to the storage changes the record.
//!
//! `cargo test -p vtium-render --test memory -- --nocapture` prints the
//! table.

mod common;

use common::{Screen, rgb, style};
use vtium_font::{Font, Unifont};
use vtium_render::{Canvas, Config, DrawState, Profile, Renderer, Storage};
use vtium_term::Age;

const W: u32 = 1366;
const H: u32 = 768;
const MIB: f64 = 1024.0 * 1024.0;

struct Filled {
    cols: usize,
    rows: usize,
    narrow: u32,
    wide: u32,
    bytes: u64,
    scratch: usize,
}

/// A renderer at `profile` with unifont `height` pixels high, its narrow
/// and wide pools filled with distinct glyphs.
fn fill(height: u32, profile: Profile, storage: Storage) -> Filled {
    let config = Config {
        profile,
        storage,
        ..Config::default()
    };
    let mut r = Renderer::new(Font::new(Unifont::new(height)), W, H, config);
    let (cols, rows) = r.grid_size();
    let cells = (cols * rows) as u32;
    let (narrow, wide) = match profile {
        Profile::Minspec => (2 * cells, cells),
        Profile::Performance => (4 * cells, 2 * cells),
    };
    let st = style(rgb("ffffff"), rgb("000000"));
    let mut next = 0u32;
    let frame = |r: &mut Renderer<Unifont>, wide: bool, n: &mut u32| {
        let mut s = Screen::new(cols, rows, rgb("ffffff"), rgb("000000"));
        for row in 0..rows {
            let mut col = 0;
            while col + usize::from(wide) < cols {
                // ideographs drawn narrow, then Hangul and ideographs wide:
                // each a glyph the cache has not seen
                let base = if wide { 0x2_0000 } else { 0x4e00 };
                s.put(col, row, char::from_u32(base + *n).unwrap(), wide, st);
                *n += 1;
                col += 1 + usize::from(wide);
            }
        }
        let mut c = Canvas::new(W, H, 0);
        r.draw(&s.frame(Age::NEVER), &mut c, 0, &DrawState::default());
    };
    while next < narrow {
        frame(&mut r, false, &mut next);
    }
    let mut next = 0u32;
    while next < wide {
        frame(&mut r, true, &mut next);
    }
    Filled {
        cols,
        rows,
        narrow,
        wide,
        bytes: r.cache_bytes(),
        scratch: r.row_scratch_bytes(),
    }
}

fn mib(n: u64) -> String {
    format!("{:.2} MiB", n as f64 / MIB)
}

#[test]
fn the_cache_holds_what_adr_1300_records() {
    let bits = Storage::default();
    let eight = Storage {
        bits: false,
        unscaled: true,
    };
    let min = fill(16, Profile::Minspec, bits);
    let perf = fill(16, Profile::Performance, bits);
    let min2 = fill(32, Profile::Minspec, bits);
    let perf2 = fill(32, Profile::Performance, bits);
    let min_aa = fill(16, Profile::Minspec, eight);
    let perf_aa = fill(16, Profile::Performance, eight);

    // the C reference's bbulk: twice the cells, each glyph held 8-bit at
    // its drawn size, at its worst every glyph wide
    let c_ref = |f: &Filled, cw: u64, ch: u64| 2 * (f.cols * f.rows) as u64 * 2 * cw * ch;

    println!("| | C reference, bbulk | vtium minspec | vtium performance |");
    println!("|---|---|---|---|");
    println!(
        "| Glyphs held | {} | {} narrow, {} wide | {} narrow, {} wide |",
        2 * min.cols * min.rows,
        min.narrow,
        min.wide,
        perf.narrow,
        perf.wide
    );
    println!(
        "| A bitmap font | {} | {} | {} |",
        mib(c_ref(&min, 8, 16)),
        mib(min.bytes),
        mib(perf.bytes)
    );
    println!(
        "| A bitmap font at scale 2, in 16x32 cells | {} | {} | {} |",
        mib(c_ref(&min2, 16, 32)),
        mib(min2.bytes),
        mib(perf2.bytes)
    );
    println!(
        "| An anti-aliased font | {} | {} | {} |",
        mib(c_ref(&min, 8, 16)),
        mib(min_aa.bytes),
        mib(perf_aa.bytes)
    );
    println!("Row scratch: {} bytes", min.scratch);

    assert_eq!((min.cols, min.rows), (170, 48));
    assert_eq!((min.narrow, min.wide), (16_320, 8_160));
    assert_eq!((perf.narrow, perf.wide), (32_640, 16_320));
    assert_eq!(c_ref(&min, 8, 16), 4_177_920);
    assert_eq!(c_ref(&min2, 16, 32), 4_177_920);
    // 16 bytes a narrow 8x16 glyph at 1 bit, 32 a wide one
    assert_eq!(min.bytes, 522_240);
    assert_eq!(perf.bytes, 1_044_480);
    // stored at the font's own size: a quarter the glyphs, the same bytes
    assert_eq!((min2.cols, min2.rows), (85, 24));
    assert_eq!(min2.bytes, 130_560);
    assert_eq!(perf2.bytes, 261_120);
    // 128 bytes a narrow glyph at 8 bits, 256 a wide one
    assert_eq!(min_aa.bytes, 4_177_920);
    assert_eq!(perf_aa.bytes, 8_355_840);
    assert_eq!(min.scratch, 87_424);
}
