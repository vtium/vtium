//! Each malformed input the tools take is refused, never judged as if it
//! passed (aaronsb/kmscon#10): PPMs, tolerance sidecars and their masks,
//! and the run's lists. `tier2 compare` exits 2 on each.

use std::path::PathBuf;
use std::process::Command;

use vtium_tier2::compare::compare;
use vtium_tier2::ppm::{self, Image, PpmError};
use vtium_tier2::runner;
use vtium_tier2::tol::Tolerance;

fn black(w: u32, h: u32) -> Image {
    Image::new(w, h, vec![0; (w * h * 3) as usize]).unwrap()
}

#[test]
fn malformed_ppms_are_refused() {
    let cases: &[(&[u8], PpmError)] = &[
        (b"", PpmError::Magic),
        (b"P3 1 1 255\n0 0 0", PpmError::Magic),
        (b"P6", PpmError::Truncated),
        (b"P6 1 1", PpmError::Truncated),
        (b"P6 1 1 # no maxval", PpmError::Truncated),
        (b"P6 -1 1 255\n\0\0\0", PpmError::Field("width")),
        (b"P6 1x 1 255\n\0\0\0", PpmError::Field("width")),
        (b"P6 1 1 +255\n\0\0\0", PpmError::Field("maxval")),
        (b"P6 9999999999 1 255\n", PpmError::Field("width")),
        (b"P6 0 1 255\n", PpmError::Size(0, 1)),
        (b"P6 1 16385 255\n", PpmError::Size(1, 16385)),
        (b"P6 1 1 65535\n\0\0\0\0\0\0", PpmError::Maxval(65535)),
        (b"P6 1 1 15\n\0\0\0", PpmError::Maxval(15)),
        (b"P6 1 1 255", PpmError::HeaderEnd),
        (b"P6 1 1 255#\0\0\0", PpmError::HeaderEnd),
        (b"P6 1 1 255\n\0\0", PpmError::Raster { got: 2, want: 3 }),
        (
            b"P6 1 1 255\n\0\0\0\0",
            PpmError::Raster { got: 4, want: 3 },
        ),
        // two images in one file
        (
            b"P6 1 1 255\n\0\0\0P6 1 1 255\n\0\0\0",
            PpmError::Raster { got: 17, want: 3 },
        ),
    ];
    for (bytes, want) in cases {
        assert_eq!(
            ppm::parse(bytes).as_ref().err(),
            Some(want),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn malformed_sidecars_are_refused() {
    let cases = [
        "max_delta 2",
        "max_delta=",
        "max_delta=-1\nmax_pixels=1",
        "max_delta=0x10\nmax_pixels=1",
        "max_delta=256\nmax_pixels=1",
        "max_delta=1\nmax_delta=2\nmax_pixels=1",
        "max_pixels=1\nmax_pixels=2\nmax_delta=1",
        "maxdelta=1",
        "tolerance=exact",
        // one limit at 0 allows nothing
        "max_delta=4",
        "max_pixels=40",
        "max_delta=4\nmax_pixels=0",
        // masks
        "mask=1,2,3",
        "mask=1,2,3,4,5",
        "mask=a,b,c,d",
        "mask=1,2,-3,4",
        "mask=5,0,5,4",
        "mask=0,5,4,5",
        "mask=6,0,5,4",
        "mask=0,6,4,5",
    ];
    for text in cases {
        assert!(
            Tolerance::parse(text, "t.tol").is_err(),
            "{text:?} was taken"
        );
    }
}

#[test]
fn a_mask_past_the_image_is_refused_when_compared() {
    let img = black(10, 8);
    for mask in ["mask=0,0,11,8", "mask=0,0,10,9", "mask=20,20,30,30"] {
        let tol = Tolerance::parse(mask, "t.tol").unwrap();
        assert!(compare(&img, &img, &tol).is_err(), "{mask} was taken");
    }
    let tol = Tolerance::parse("mask=0,0,10,8", "t.tol").unwrap();
    assert!(compare(&img, &img, &tol).is_ok());
}

#[test]
fn malformed_lists_are_refused() {
    for text in [
        "kmscon\tb1f7a7c\thttps://example.org/k.git",
        "kmscon\tb1f7a7c5df563f3378584db7298753848a7a9a1g\thttps://example.org/k.git",
        "kmscon b1f7a7c5df563f3378584db7298753848a7a9a13 https://example.org/k.git",
    ] {
        assert!(runner::pins(text).is_err(), "pin {text:?} was taken");
    }
    for text in [
        "edges\tA wide glyph",
        "edges\tA wide glyph\t0,0,1",
        "edges\tA wide glyph\t0,0,1,1;2,2,1,1",
        "edges\tT\t0,0,1,1\nedges\tT\t0,0,1,1",
    ] {
        assert!(
            runner::departures(text).is_err(),
            "departure {text:?} was taken"
        );
    }
    for text in [
        "ascii\t#24\toutline font",
        "ascii\tvtium/vtium#24",
        "ascii\tvtium/vtium#24\ta\nascii\tvtium/vtium#24\tb",
    ] {
        assert!(runner::pending(text).is_err(), "pending {text:?} was taken");
    }
}

fn tier2(args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_tier2"))
        .args(args)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("malformed");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn compare_exits_2_on_each_malformed_input() {
    let good = scratch("good.ppm");
    std::fs::write(&good, black(4, 4).to_ppm()).unwrap();
    let bad = scratch("bad.ppm");
    std::fs::write(&bad, b"P6 4 4 255\n\0").unwrap();
    let tol = scratch("bad.tol");
    std::fs::write(&tol, "max_delta=3\n").unwrap();
    let off = scratch("off.tol");
    std::fs::write(&off, "mask=0,0,5,4\n").unwrap();
    let (g, b) = (good.to_str().unwrap(), bad.to_str().unwrap());
    assert_eq!(tier2(&["compare", g, g]), 0);
    assert_eq!(tier2(&["compare", b, g]), 2);
    assert_eq!(tier2(&["compare", g, b]), 2);
    assert_eq!(tier2(&["compare", g, g, "--tol", tol.to_str().unwrap()]), 2);
    assert_eq!(tier2(&["compare", g, g, "--tol", off.to_str().unwrap()]), 2);
    assert_eq!(tier2(&["compare", g, "missing.ppm"]), 2);
}

#[test]
fn compare_exits_1_on_a_difference() {
    let a = scratch("a.ppm");
    std::fs::write(&a, black(4, 4).to_ppm()).unwrap();
    let mut rgb = vec![0; 48];
    rgb[0] = 9;
    let b = scratch("b.ppm");
    std::fs::write(&b, Image::new(4, 4, rgb).unwrap().to_ppm()).unwrap();
    let near = scratch("near.tol");
    std::fs::write(&near, "max_delta=9\nmax_pixels=1\n").unwrap();
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    assert_eq!(tier2(&["compare", a, b]), 1);
    assert_eq!(
        tier2(&["compare", a, b, "--tol", near.to_str().unwrap()]),
        0
    );
}
