//! The GPU path's side of tier 2 (ADR-1303 D1): each screen the GPU path
//! draws has a sidecar of its own, exact for now, and the
//! judge sees a GPU path that draws nothing. The pixels are judged on a
//! real GL; here the stand-in GL draws through the whole GPU path and reads
//! back zeros.
#![cfg(feature = "gpu")]

use std::fs;
use std::path::{Path, PathBuf};

use vtium_render::gpu::StandIn;
use vtium_tier2::compare::Verdict;
use vtium_tier2::gpu;
use vtium_tier2::screens::CASES;
use vtium_tier2::tol::Tolerance;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The screens the GPU path draws today.
fn drawn() -> Vec<&'static str> {
    CASES
        .iter()
        .filter(|c| gpu::waits(&c.screen).is_none())
        .map(|c| c.screen.name)
        .collect()
}

#[test]
fn every_screen_the_gpu_path_draws_has_its_own_exact_sidecar() {
    // the maintainer's decision of 2026-11-03: the GPU path is judged
    // exactly for now, each sidecar saying so in both limits
    let names = drawn();
    assert!(names.len() > 40, "{names:?}");
    let mut wrong = Vec::new();
    for name in &names {
        let path = root().join("gpu").join(format!("{name}.tol"));
        let text = fs::read_to_string(&path).unwrap_or_default();
        let states = text.lines().any(|l| l.trim() == "max_delta=0")
            && text.lines().any(|l| l.trim() == "max_pixels=0");
        match gpu::sidecar(&root(), name) {
            Ok(tol) if tol == Tolerance::default() && states => {}
            other => wrong.push(format!(
                "{name}: {other:?}, want max_delta=0 and max_pixels=0"
            )),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // and no sidecar names a screen the GPU path does not draw
    let dir = root().join("gpu");
    for entry in fs::read_dir(&dir).unwrap() {
        let file = entry.unwrap().file_name().into_string().unwrap();
        let name = file.strip_suffix(".tol").unwrap_or(&file);
        assert!(
            names.contains(&name),
            "{file}: no screen the GPU path draws"
        );
    }
}

#[test]
fn the_screens_the_gpu_path_does_not_draw_yet_say_why() {
    let waiting: Vec<(&str, &str)> = CASES
        .iter()
        .filter_map(|c| gpu::waits(&c.screen).map(|w| (c.screen.name, w)))
        .collect();
    assert!(waiting.iter().any(|(n, _)| *n == "edges-right"));
    assert!(waiting.iter().any(|(n, _)| *n == "small-320x240-1bit"));
    assert!(!waiting.iter().any(|(n, _)| *n == "edges"));
}

#[test]
fn a_gpu_path_that_draws_nothing_fails_the_judge() {
    // the stand-in draws the whole GPU path, two buffers, a second part and
    // the console's overlay included, and reads back zeros: the comparison
    // with the CPU path's drawing sees it
    for name in [
        "redraw-unifont",
        "osd-full-800x480",
        "emoji-rgba",
        "small-640x480",
    ] {
        let case = CASES.iter().find(|c| c.screen.name == name).unwrap();
        let mut gl = StandIn::new();
        let judged = gpu::judge(&case.screen, &root(), &mut gl).unwrap();
        let c = &judged.comparison;
        assert_eq!(c.verdict, Verdict::Fail, "{name}");
        assert!(c.max_delta > 1 && judged.share() > 0.0, "{name}: {c:?}");
        assert_eq!(gl.live(), (0, 0, 0), "{name}: the run frees what it made");
    }
}

#[test]
fn a_screen_with_no_sidecar_is_not_judged() {
    let case = CASES.iter().find(|c| c.screen.name == "edges").unwrap();
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-sidecars");
    let mut gl = StandIn::new();
    let e = gpu::judge(&case.screen, &empty, &mut gl).unwrap_err();
    assert!(e.contains("edges.tol"), "{e}");
}

// ADR-1303 D1: on Mesa's llvmpipe, through vtium-platform's context and the
// binary's adapter, the GPU path draws a screen of text, decorations,
// colors and a notice with its focus mark within its sidecar of the CPU
// path's drawing, and the report names the case and what waits
// covers: platform.gpu
#[test]
fn the_gpu_path_on_llvmpipe_draws_within_its_sidecar_of_the_cpu_paths_pixels() {
    let screens: Vec<_> = CASES
        .iter()
        .filter(|c| {
            ["small-640x480", "osd-minimal-notice-800x480", "edges-left"].contains(&c.screen.name)
        })
        .map(|c| c.screen)
        .collect();
    assert_eq!(screens.len(), 3);
    let mut gl = gpu::llvmpipe().unwrap();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gpu-llvmpipe");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap();
    let reports = gpu::reports(&screens, &root(), &mut gl, (&out, &out));
    let got: Vec<(String, &str)> = reports
        .iter()
        .map(|r| (r.name.clone(), r.status.name()))
        .collect();
    assert_eq!(
        got,
        [
            ("edges-left@gpu".to_owned(), "pending"),
            ("small-640x480@gpu".to_owned(), "pass"),
            ("osd-minimal-notice-800x480@gpu".to_owned(), "pass"),
        ],
        "{reports:?}"
    );
    assert!(out.join("small-640x480@gpu.ppm").exists());
}
