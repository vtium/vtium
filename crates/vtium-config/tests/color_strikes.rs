//! Color bitmap strikes that pass the font check but are damaged or
//! unusual, drawn through `outline::cell_width` and `outline::rasterise`
//! as the font helper calls them. vtium-config reads the strikes itself
//! with read-fonts and decodes their PNG images with the png crate, so
//! none of these fonts hangs the helper, panics in it, or makes it
//! allocate past its memory limit.
//!
//! Each font is Test Emoji, its glyph 1 mapped from U+1F525, A and W and
//! glyphs 2 and 3 from B and C, with its CBLC and CBDT tables, or an sbix
//! table, written by the test. Each case runs in a child process of this
//! test binary under the font helper's own limit on its address space,
//! 512 MiB, so an allocation past that limit fails the case as it would
//! end the helper. The parent waits a bounded time, so a case that hangs
//! fails instead of hanging the test run.

use std::process::{Command, Stdio};
use std::time::Duration;

use vtium_config::child::PARSER_MEMORY;
use vtium_config::frit::Hinting;
use vtium_config::names::sound;
use vtium_config::outline::{Outline, OutlineRequest, cell_width, rasterise};
use vtium_testfont::ttf::{self, Glyph};

/// How long the parent waits for a case. Each case answers in well under
/// a second; the bound only turns a hang into a failure.
const CASE_WAIT: Duration = Duration::from_secs(30);

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([b[at], b[at + 1]]))
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

fn p16(v: &mut Vec<u8>, x: u16) {
    v.extend(x.to_be_bytes());
}

fn p32(v: &mut Vec<u8>, x: u32) {
    v.extend(x.to_be_bytes());
}

/// The tables of a font of one face, each with its tag.
fn tables(font: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    (0..u16_at(font, 4))
        .map(|i| {
            let r = 12 + 16 * i;
            let (o, l) = (u32_at(font, r + 8), u32_at(font, r + 12));
            (font[r..r + 4].try_into().unwrap(), font[o..o + l].to_vec())
        })
        .collect()
}

/// A font of one face holding `t`, sorted by tag and padded to four bytes.
/// The checksums are left 0, which no reader vtium-config uses checks.
fn sfnt(mut t: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    t.sort_by_key(|(tag, _)| *tag);
    let n = t.len();
    let mut out = vec![0, 1, 0, 0];
    p16(&mut out, n as u16);
    out.extend([0; 6]);
    let mut body = Vec::new();
    for (tag, data) in &t {
        out.extend(tag);
        out.extend([0; 4]);
        p32(&mut out, (12 + 16 * n + body.len()) as u32);
        p32(&mut out, data.len() as u32);
        body.extend(data);
        body.resize(body.len().next_multiple_of(4), 0);
    }
    out.extend(body);
    out
}

/// Test Emoji with glyph 1 mapped from U+1F525, A and W, and glyphs 2 and
/// 3 from B and C, its color bitmap tables replaced by `strikes`.
fn emoji_with(strikes: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let mut f = ttf::test_emoji();
    f.glyphs = vec![Glyph::empty(1200); 4];
    f.cmap = vec![(0x41, 1), (0x42, 2), (0x43, 3), (0x57, 1), (0x1f525, 1)];
    let t = tables(&f.build())
        .into_iter()
        .filter(|(tag, _)| tag != b"CBDT" && tag != b"CBLC")
        .chain(strikes)
        .collect();
    sfnt(t)
}

/// A CBLC of one strike of 40 pixels to the em over glyphs `first` to
/// `last`, with one index subtable of `index_format` whose images are in
/// `image_format` from `image_offset` in CBDT; `rest` follows the
/// subtable's header.
fn cblc(
    first: u16,
    last: u16,
    index_format: u16,
    image_format: u16,
    image_offset: u32,
    rest: &[u8],
) -> Vec<u8> {
    let mut sub = Vec::new();
    p16(&mut sub, index_format);
    p16(&mut sub, image_format);
    p32(&mut sub, image_offset);
    sub.extend(rest);
    let mut v = Vec::new();
    p16(&mut v, 3);
    p16(&mut v, 0);
    p32(&mut v, 1); // numSizes
    p32(&mut v, 56); // indexSubTableArrayOffset
    p32(&mut v, 8 + sub.len() as u32); // indexTablesSize
    p32(&mut v, 1); // numberOfIndexSubTables
    p32(&mut v, 0); // colorRef
    for _ in 0..2 {
        v.extend([40, 0, 40, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    p16(&mut v, first);
    p16(&mut v, last);
    v.extend([40, 40, 32, 1]); // ppemX, ppemY, bitDepth, flags
    p16(&mut v, first); // the one IndexSubTableArray record
    p16(&mut v, last);
    p32(&mut v, 8);
    v.extend(sub);
    v
}

/// A CBDT holding each of `pngs` in image format 17, small metrics then
/// the PNG, each 36 pixels wide and high and standing 30 above the
/// baseline; with each image's offset from the table's start and the end.
fn cbdt17(pngs: &[&[u8]]) -> (Vec<u8>, Vec<u32>) {
    let mut d = vec![0, 3, 0, 0];
    let mut offsets = Vec::new();
    for png in pngs {
        offsets.push(d.len() as u32);
        d.extend([36, 36, 0, 30, 48]);
        p32(&mut d, png.len() as u32);
        d.extend(*png);
    }
    offsets.push(d.len() as u32);
    (d, offsets)
}

/// The font of glyph 1's PNG in index format 1, as Noto Color Emoji
/// stores its strikes.
fn format1(png: &[u8]) -> Vec<u8> {
    let (d, o) = cbdt17(&[png]);
    let mut rest = Vec::new();
    for x in &o {
        p32(&mut rest, *x);
    }
    emoji_with(vec![(*b"CBDT", d), (*b"CBLC", cblc(1, 1, 1, 17, 0, &rest))])
}

/// A 36 by 36 PNG of opaque blue, 8 bits a sample, the size of Test
/// Emoji's own bitmap.
fn blue() -> Vec<u8> {
    ttf::png(36, 36, &[0, 0, 255, 255].repeat(36 * 36))
}

/// A 36 by 36 PNG of 16 bits a sample: color type 6 (RGBA) of opaque red,
/// or 0 (grey) of the middle grey, 0x8000.
fn sixteen_bit(color: u8) -> Vec<u8> {
    let pixel: &[u8] = if color == 6 {
        &[0xff, 0xff, 0, 0, 0, 0, 0xff, 0xff]
    } else {
        &[0x80, 0x00]
    };
    let row = [&[0u8][..], &pixel.repeat(36)].concat();
    let raw = row.repeat(36);
    ttf::png_of(
        36,
        36,
        16,
        color,
        &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6),
    )
}

/// A PNG whose header asks for `side` by `side` pixels of 8-bit RGBA, with
/// one short row of data.
fn header_only(side: u32) -> Vec<u8> {
    ttf::png_of(
        side,
        side,
        8,
        6,
        &miniz_oxide::deflate::compress_to_vec_zlib(&[0; 8], 6),
    )
}

/// The font of `case`, with what it shows.
fn case_font(case: &str) -> Vec<u8> {
    match case {
        "control" => format1(&blue()),
        // a valid index subtable of format 4: one glyph and the record
        // that ends its data, its offsets counted from imageDataOffset
        "format4" => {
            let png = blue();
            let (d, o) = cbdt17(&[&png]);
            let mut rest = Vec::new();
            p32(&mut rest, 1); // numGlyphs
            p16(&mut rest, 1);
            p16(&mut rest, 0);
            p16(&mut rest, 0);
            p16(&mut rest, (o[1] - o[0]) as u16);
            emoji_with(vec![
                (*b"CBDT", d),
                (*b"CBLC", cblc(1, 1, 4, 17, o[0], &rest)),
            ])
        }
        // the control's CBLC with numSizes 0xFFFFFFFF, of which one fits
        "many" => {
            let mut font = format1(&blue());
            let t = tables(&font);
            let mut c = t.iter().find(|(tag, _)| tag == b"CBLC").unwrap().1.clone();
            c[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
            font = emoji_with(vec![
                (
                    *b"CBDT",
                    t.iter().find(|(tag, _)| tag == b"CBDT").unwrap().1.clone(),
                ),
                (*b"CBLC", c),
            ]);
            font
        }
        "rgba16" => format1(&sixteen_bit(6)),
        "grey16" => format1(&sixteen_bit(0)),
        "big16k" => format1(&header_only(16_384)),
        "wrap32k" => format1(&header_only(32_768)),
        // a 1 by 1 PNG of opaque blue whose IDAT inflates to 600 MB
        "bomb" => format1(&ttf::png_of(
            1,
            1,
            8,
            6,
            &ttf::zlib_run(&[0, 0, 0, 255, 255], 600 << 20),
        )),
        // index format 2 over glyphs 1 to 3, each image 0x80000000 bytes:
        // glyph 3's image starts 0x1_0000_0004 bytes into CBDT, which
        // wraps to glyph 1's offset in 32 bits
        "format2" => {
            let png = blue();
            let (d, _) = cbdt17(&[&png]);
            let mut rest = Vec::new();
            p32(&mut rest, 0x8000_0000); // imageSize
            rest.extend([36, 36, 0, 30, 48, 0, 0, 0]); // bigMetrics
            emoji_with(vec![(*b"CBDT", d), (*b"CBLC", cblc(1, 3, 2, 17, 4, &rest))])
        }
        // an sbix strike of 40 pixels to the em holding glyph 1's PNG,
        // its origin offset 0, and no CBLC or CBDT
        "sbix" => {
            let png = blue();
            let mut s = vec![0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 12];
            let start = 4 + 4 * 5;
            let end = start + 8 + png.len() as u32;
            let mut strike = vec![0, 40, 0, 72];
            for o in [start, start, end, end, end] {
                p32(&mut strike, o);
            }
            strike.extend([0, 0, 0, 0]);
            strike.extend(b"png ");
            strike.extend(&png);
            s.extend(strike);
            emoji_with(vec![(*b"sbix", s)])
        }
        other => panic!("no case {other}"),
    }
}

/// The glyph mapped from `c` in strike 0: whether it is drawn in color,
/// whether any of its pixels is lit, and its most opaque pixel, four
/// bytes for color.
fn glyph(o: &Outline, c: u32) -> (bool, bool, [u8; 4]) {
    let s = &o.strikes[0];
    let (_, g) = s
        .map
        .iter()
        .find(|(cp, _)| *cp == c)
        .expect("the character is mapped");
    let glyph = &s.glyphs[*g as usize];
    let n = if glyph.color { 4 } else { 1 };
    let mut most = [0; 4];
    for p in glyph.pixels.chunks(n) {
        if p[n - 1] > most[n - 1] {
            most[..n].copy_from_slice(p);
        }
    }
    (glyph.color, glyph.pixels.iter().any(|&p| p != 0), most)
}

/// Whether `got` is `want` within 2 in each channel, as a scaled bitmap of
/// one color keeps it.
fn near(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 2)
}

/// Runs `case` in a child process of this test binary and fails unless
/// the child passes within [`CASE_WAIT`].
fn run(case: &str) {
    let child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "--exact",
            "case_in_a_child",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("VTIUM_STRIKE_CASE", case)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the child starts");
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let Ok(out) = rx.recv_timeout(CASE_WAIT) else {
        let pid = rustix::process::Pid::from_raw(pid as i32).expect("a pid");
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        panic!("case {case} did not answer within {CASE_WAIT:?}");
    };
    let out = out.expect("the child's output");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "case {case} failed: {}\n{err}",
        out.status
    );
}

/// One case, in a child process that [`run`] starts: the font goes
/// through the font check, the cell width at a cell 16 pixels high, and
/// the rasteriser at the same cell with glyphs 1 to 3 in its cover, under
/// the font helper's limit on its address space.
#[test]
#[ignore = "a case's child process; the tests below start it"]
fn case_in_a_child() {
    let Ok(case) = std::env::var("VTIUM_STRIKE_CASE") else {
        return;
    };
    let limit = rustix::process::Rlimit {
        current: Some(PARSER_MEMORY),
        maximum: Some(PARSER_MEMORY),
    };
    rustix::process::setrlimit(rustix::process::Resource::As, limit).expect("the limit");
    let font = case_font(&case);
    assert_eq!(sound(&font), Ok(()));
    let width = cell_width(&[(&font, "")], 16).expect("a cell width");
    assert!(width > 0);
    let request = OutlineRequest {
        font: font.into(),
        cells: vec![(0, 16)],
        cover: vec![(0x1f525, 0x1f525), (0x41, 0x43)],
        hinting: Hinting::Light,
        ..Default::default()
    };
    let o = rasterise(&request).expect("rasterised");
    let fire = glyph(&o, 0x1f525);
    let c = glyph(&o, 0x43);
    match case.as_str() {
        "control" | "format4" | "many" | "bomb" | "sbix" => {
            assert!(
                fire.0 && fire.1 && near(fire.2, [0, 0, 255, 255]),
                "glyph 1 draws its blue bitmap: {fire:?}"
            );
        }
        "rgba16" => assert!(
            fire.0 && fire.1 && near(fire.2, [255, 0, 0, 255]),
            "glyph 1 draws its red bitmap: {fire:?}"
        ),
        "grey16" => assert!(
            fire.0 && fire.1 && near(fire.2, [128, 128, 128, 255]),
            "glyph 1 draws its grey bitmap: {fire:?}"
        ),
        "big16k" | "wrap32k" => assert!(!fire.0 && !fire.1, "glyph 1 draws nothing: {fire:?}"),
        "format2" => {
            assert!(
                !c.0 && !c.1,
                "glyph 3 draws nothing, not glyph 1's bitmap: {c:?}"
            );
        }
        other => panic!("no case {other}"),
    }
}

// The index subtable format 4 lists each glyph with its offset, then one
// more record whose offset ends the last glyph's data. A reader that
// halves its search range wrongly never ends on such a subtable; the
// glyph draws from the offsets counted from imageDataOffset
#[test]
fn a_strike_indexed_in_format_4_draws() {
    run("format4");
}

// numSizes 0xFFFFFFFF names four billion strikes in a table that holds
// one: vtium-config reads only the strikes the table holds
#[test]
fn a_strike_count_past_the_table_reads_only_the_strikes_it_holds() {
    run("many");
}

// PNG allows 16 bits a sample; vtium-config keeps the high byte of each
#[test]
fn a_16_bit_rgba_png_draws() {
    run("rgba16");
}

#[test]
fn a_16_bit_grey_png_draws() {
    run("grey16");
}

// A PNG header of 16,384 × 16,384 asks for 1 GiB of pixels, and one of
// 32,768 × 32,768 for 4 GiB, a size that wraps to 0 in 32 bits. Both are
// past the glyph bound of 4,096 pixels a side, so neither is allocated
#[test]
fn a_png_header_past_the_glyph_bound_is_refused_before_allocating() {
    run("big16k");
    run("wrap32k");
}

// The PNG holds a 1 × 1 image whose compressed data inflates to 600 MB.
// vtium-config inflates only the bytes the image's one row needs
#[test]
fn a_png_whose_data_inflates_far_past_its_image_draws_its_image_alone() {
    run("bomb");
}

// Index format 2 gives every glyph the same image size, here 0x80000000
// bytes, so glyph 3's offset passes 4 GiB. It is past the end of CBDT, so
// glyph 3 draws nothing; in 32 bits it wraps to glyph 1's offset
#[test]
fn an_image_offset_past_4_gib_draws_nothing_rather_than_another_glyph() {
    run("format2");
}

// The two controls: the strike as Noto Color Emoji stores it, and an
// sbix strike, as Apple's fonts store theirs
#[test]
fn a_png_strike_draws() {
    run("control");
    run("sbix");
}
