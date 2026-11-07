//! The GPU path's frame on the stand-in GL (ADR-1303 D1, D3, D7, D9): the
//! calls each of the five slots makes. The stand-in draws no pixels, so these
//! tests judge calls only; pixels are judged on a real GL in tier 2, once the
//! composition and present draw.
#![cfg(feature = "gpu")]

use vtium_render::Rect;
use vtium_render::gpu::{
    Batch, Blend, Call, Color, Config, ErrorKind, Gl, Limits, Op, Pipeline, Quad, Size, Spatial,
    StandIn, Work,
};

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

const DISPLAY: Size = Size::new(64, 32);
const BORDER: u32 = 0x0010_2030;

fn set_up(gl: &mut StandIn) -> Pipeline {
    let p = Pipeline::new(gl, DISPLAY, Config::default()).unwrap();
    gl.clear_calls();
    p
}

/// Present's draw of `rects`: one quad for each, copying the upright
/// frame's pixels to the same place.
fn present_draw(rects: &[Rect]) -> Call {
    let quads = rects
        .iter()
        .map(|r| {
            let i = |v: u32| i32::try_from(v).unwrap();
            Quad {
                dst: [i(r.x), i(r.y), i(r.width), i(r.height)],
                src: [r.x, r.y, r.width, r.height],
                params: [0; 4],
            }
        })
        .collect();
    Call::Draw {
        quads,
        blend: Blend::Replace,
        inputs: vec!["target_size", "frame"],
    }
}

#[test]
fn set_up_makes_the_upright_frame_and_present_and_checks_once() {
    let mut gl = StandIn::new();
    let p = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap();
    let calls = gl.calls();
    // every program the composition draws with, so that no frame compiles
    // one: the cell shader without ink and with it, the arrow's, the under
    // shader, and in a build with pictures the picture shader without and
    // with OVER and the cell shader with OVER and ink; then present's
    let programs = if cfg!(feature = "images") { 8 } else { 5 };
    assert_eq!(calls.len(), programs + 2, "{calls:?}");
    assert_eq!(calls[0], Call::CreateFrame(DISPLAY));
    let defines = |c: &Call| match c {
        Call::Compile(t) => {
            let has = |d: &str| t.lines().any(|l| l == d);
            Some((has("#define INK"), has("#define OVER")))
        }
        _ => None,
    };
    let compiled: Vec<_> = calls[1..=programs].iter().map(defines).collect();
    assert!(compiled.iter().all(Option::is_some), "{calls:?}");
    let count = |d| compiled.iter().filter(|&&c| c == Some(d)).count();
    assert_eq!(count((true, false)), 1, "the cell shader with ink");
    assert_eq!(
        count((true, true)),
        usize::from(cfg!(feature = "images")),
        "the cell shader with ink over pictures"
    );
    assert_eq!(calls[programs + 1], Call::Check(Batch::Uploads));
    p.free(&mut gl).unwrap();
    assert_eq!(gl.live(), (0, 0, 0));
}

#[test]
fn empty_color_and_spatial_slots_join_no_source_and_make_no_call() {
    // ADR-1303 Verification: with slots 3 and 4 empty, present's shader
    // holds nothing of theirs, and a frame's calls are composition's and
    // present's alone
    let mut gl = StandIn::new();
    let config = Config::default();
    assert_eq!((config.color, config.spatial), (Color::None, Spatial::None));
    let mut p = Pipeline::new(&mut gl, DISPLAY, config).unwrap();
    // present's program is the last set up compiles
    let Some(fragment) = gl.calls().iter().rev().find_map(|c| match c {
        Call::Compile(t) => Some(t),
        _ => None,
    }) else {
        panic!("{:?}", gl.calls());
    };
    let bare = [
        include_str!("../src/gpu/glsl/present_head.frag"),
        include_str!("../src/gpu/glsl/present_main.frag"),
    ]
    .concat();
    assert_eq!(fragment, &bare);
    assert!(!fragment.contains("#define"));
    assert_eq!(Spatial::None.reach(), 0);
    assert!(!Spatial::None.presents_whole());

    gl.clear_calls();
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    gl.clear_calls();
    let work = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    assert_eq!(
        gl.calls(),
        &[
            Call::Clear(DISPLAY.rect(), BORDER),
            Call::Check(Batch::Draws),
            present_draw(&[DISPLAY.rect()]),
            Call::Check(Batch::Frame),
            Call::Submit,
            Call::Wait,
        ]
    );
}

#[test]
fn a_frame_checks_for_errors_after_the_composition_and_at_its_end() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let first = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &first, &buffer, 0).unwrap();
    gl.clear_calls();
    let cells = [rect(8, 0, 8, 16), rect(24, 16, 16, 16)];
    let work = Work::new(BORDER, &cells, &cells);
    let damage = p.frame(&mut gl, &work, &buffer, 1).unwrap();
    assert_eq!(damage, cells);
    assert_eq!(
        gl.calls(),
        &[
            Call::Clear(cells[0], BORDER),
            Call::Clear(cells[1], BORDER),
            Call::Check(Batch::Draws),
            present_draw(&cells),
            Call::Check(Batch::Frame),
            Call::Submit,
            Call::Wait,
        ]
    );
}

// the issue on the EGL sync before the flip: a frame that drew ends with a
// fence after its last draw, and waits on it before the buffer goes back
// to the display, so the display never shows a buffer the GPU has not
// finished; the fence is gone once waited on
#[test]
fn a_frame_that_draws_waits_on_a_fence_after_its_last_draw() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    gl.clear_calls();
    let work = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    let calls = gl.calls();
    let last_draw = calls
        .iter()
        .rposition(|c| matches!(c, Call::Draw { .. } | Call::Clear(..)))
        .unwrap();
    assert_eq!(
        &calls[last_draw + 1..],
        &[Call::Check(Batch::Frame), Call::Submit, Call::Wait]
    );
    assert_eq!(gl.fences(), 0, "a fence left behind");
    // a frame that draws nothing makes no fence
    gl.clear_calls();
    assert_eq!(p.frame(&mut gl, &work, &buffer, 1).unwrap(), []);
    assert_eq!(gl.calls(), &[]);
}

// a wait that fails fails its frame, which the head sorts by its kind
// (ADR-1303 D9), and leaves no fence behind
#[test]
fn a_wait_that_fails_fails_the_frame_and_the_next_one_draws_whole() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let work = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    gl.fail_next(Op::Wait, ErrorKind::Gl);
    let cells = [rect(8, 0, 8, 16)];
    let changed = Work::new(BORDER, &cells, &cells);
    assert_eq!(
        p.frame(&mut gl, &changed, &buffer, 1).unwrap_err().kind,
        ErrorKind::Gl
    );
    assert_eq!(gl.fences(), 0);
    assert_eq!(
        p.frame(&mut gl, &work, &buffer, 1).unwrap(),
        [DISPLAY.rect()]
    );
}

#[test]
fn a_frame_that_changes_nothing_makes_no_call() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let work = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    gl.clear_calls();
    // a rectangle wholly off the frame changes nothing either
    for redraw in [&[][..], &[rect(64, 0, 8, 8)]] {
        let work = Work::new(BORDER, redraw, redraw);
        assert_eq!(p.frame(&mut gl, &work, &buffer, 1).unwrap(), []);
    }
    assert_eq!(gl.calls(), &[]);
}

#[test]
fn present_draws_what_a_buffer_lacks_over_its_age() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let whole = vec![DISPLAY.rect()];
    let mut frame = |gl: &mut StandIn, redraw: &[Rect], age| {
        let work = Work::new(BORDER, redraw, redraw);
        p.frame(gl, &work, &buffer, age).unwrap()
    };
    assert_eq!(frame(&mut gl, &[], 0), whole);
    let a = rect(0, 0, 8, 16);
    let b = rect(8, 0, 8, 16);
    let c = rect(16, 0, 8, 16);
    assert_eq!(frame(&mut gl, &[a], 1), [a]);
    assert_eq!(frame(&mut gl, &[b], 2), [b, a]);
    assert_eq!(frame(&mut gl, &[c], 3), [c, b, a]);
    // a buffer of unknown contents, or older than the history, takes all
    assert_eq!(frame(&mut gl, &[a], 0), whole);
    assert_eq!(frame(&mut gl, &[a], 4), whole);
}

#[test]
fn a_buffer_that_lacks_something_is_presented_though_nothing_was_composed() {
    // the CPU path draws the whole picture into a buffer at age 0, after a
    // console switch for one, and what a buffer lacks over its age, whether
    // or not the frame changed anything (renderer.rs, `draw`)
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let a = [rect(0, 0, 8, 16)];
    let mut frame = |gl: &mut StandIn, redraw: &[Rect], age| {
        let work = Work::new(BORDER, redraw, redraw);
        p.frame(gl, &work, &buffer, age).unwrap()
    };
    frame(&mut gl, &[], 0);
    frame(&mut gl, &a, 1);
    gl.clear_calls();
    assert_eq!(frame(&mut gl, &[], 0), [DISPLAY.rect()]);
    assert_eq!(
        gl.calls(),
        &[
            present_draw(&[DISPLAY.rect()]),
            Call::Check(Batch::Frame),
            Call::Submit,
            Call::Wait,
        ]
    );
    // the frame drawn last composed nothing, so a buffer two frames old
    // lacks nothing, and one three frames old lacks the rectangle before it
    gl.clear_calls();
    assert_eq!(frame(&mut gl, &[], 2), []);
    assert_eq!(gl.calls(), &[]);
    assert_eq!(frame(&mut gl, &[], 3), a);
}

#[test]
fn a_redrawn_rectangle_partly_off_the_frame_is_trimmed_to_it() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let work = Work::new(BORDER, &[], &[]);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    gl.clear_calls();
    let r = [rect(56, 24, 16, 16)];
    let work = Work::new(BORDER, &r, &r);
    let trimmed = rect(56, 24, 8, 8);
    assert_eq!(p.frame(&mut gl, &work, &buffer, 1).unwrap(), [trimmed]);
    assert_eq!(gl.calls()[0], Call::Clear(trimmed, BORDER));
    assert_eq!(gl.calls()[2], present_draw(&[trimmed]));
}

#[test]
fn a_buffer_of_another_size_than_the_display_is_refused() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let work = Work::new(BORDER, &[], &[]);
    for size in [Size::new(64, 31), Size::new(32, 64)] {
        let buffer = gl.import_buffer(&size).unwrap();
        gl.clear_calls();
        let e = p.frame(&mut gl, &work, &buffer, 0).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Misuse);
        assert_eq!(gl.calls(), &[]);
    }
}

#[test]
fn a_set_up_whose_present_fails_leaves_nothing_behind() {
    let mut gl = StandIn::with_limits(Limits {
        fragment_highp: false,
        ..StandIn::LIMITS
    });
    let e = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Limit);
    assert_eq!(gl.live(), (0, 0, 0));
    let mut gl = StandIn::new();
    gl.fail_next(Op::Compile, ErrorKind::Compile);
    let e = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Compile);
    assert_eq!(gl.live(), (0, 0, 0));
}

#[test]
fn a_new_border_color_composes_the_whole_frame() {
    // ADR-1303 D3: as the CPU path redraws whole when the background changes
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let cell = [rect(8, 0, 8, 16)];
    for border in [BORDER, BORDER] {
        let work = Work::new(border, &cell, &cell);
        p.frame(&mut gl, &work, &buffer, 1).unwrap();
    }
    gl.clear_calls();
    let work = Work::new(0x00ff_ffff, &cell, &cell);
    p.frame(&mut gl, &work, &buffer, 1).unwrap();
    assert_eq!(gl.calls()[0], Call::Clear(DISPLAY.rect(), 0x00ff_ffff));
}

#[test]
fn a_frame_dropped_for_a_gl_error_is_followed_by_a_whole_one() {
    // ADR-1303 D9: what the failed frame left in the upright frame and the
    // buffer is unknown
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let cell = [rect(8, 0, 8, 16)];
    let work = Work::new(BORDER, &cell, &cell);
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    gl.raise(ErrorKind::Gl);
    let e = p.frame(&mut gl, &work, &buffer, 1).unwrap_err();
    assert_eq!((e.kind, e.batch), (ErrorKind::Gl, Some(Batch::Draws)));
    gl.clear_calls();
    assert_eq!(
        p.frame(&mut gl, &work, &buffer, 1).unwrap(),
        [DISPLAY.rect()]
    );
    assert_eq!(gl.calls()[0], Call::Clear(DISPLAY.rect(), BORDER));
}

#[test]
fn a_display_past_the_largest_texture_is_held_in_tiles() {
    // ADR-1303 D3: each tile within the largest texture, and one tile and
    // many take the same path
    let mut gl = StandIn::with_limits(Limits {
        texture: 24,
        ..StandIn::LIMITS
    });
    let mut p = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap();
    let frames: Vec<&Call> = gl
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::CreateFrame(_)))
        .collect();
    assert_eq!(
        frames,
        [
            &Call::CreateFrame(Size::new(24, 24)),
            &Call::CreateFrame(Size::new(24, 24)),
            &Call::CreateFrame(Size::new(16, 24)),
            &Call::CreateFrame(Size::new(24, 8)),
            &Call::CreateFrame(Size::new(24, 8)),
            &Call::CreateFrame(Size::new(16, 8)),
        ]
    );
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    p.frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 0)
        .unwrap();
    gl.clear_calls();
    // a rectangle across the first two tiles' seam is cleared in each, in
    // its own pixels, and presented from each into its own part
    let r = [rect(20, 4, 8, 8)];
    let damage = p
        .frame(&mut gl, &Work::new(BORDER, &r, &r), &buffer, 1)
        .unwrap();
    assert_eq!(damage, r);
    let quad = |x: u32, src_x: u32| Quad {
        dst: [x as i32, 4, 4, 8],
        src: [src_x, 4, 4, 8],
        params: [0; 4],
    };
    let draw = |q: Quad| Call::Draw {
        quads: vec![q],
        blend: Blend::Replace,
        inputs: vec!["target_size", "frame"],
    };
    assert_eq!(
        gl.calls(),
        &[
            Call::Clear(rect(20, 4, 4, 8), BORDER),
            Call::Clear(rect(0, 4, 4, 8), BORDER),
            Call::Check(Batch::Draws),
            draw(quad(20, 20)),
            draw(quad(24, 0)),
            Call::Check(Batch::Frame),
            Call::Submit,
            Call::Wait,
        ]
    );
    p.free(&mut gl).unwrap();
    gl.free_target(buffer).unwrap();
    assert_eq!(gl.live(), (0, 0, 0));
}

#[test]
fn a_display_no_tile_can_hold_is_refused_and_leaves_nothing_behind() {
    // the last tile's set-up fails: the ones made before it are freed
    let mut gl = StandIn::with_limits(Limits {
        texture: 32,
        ..StandIn::LIMITS
    });
    gl.fail_next(Op::CreateFrame, ErrorKind::OutOfMemory);
    let e = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    assert_eq!(gl.live(), (0, 0, 0));
    let mut gl = StandIn::new();
    let e = Pipeline::new(&mut gl, Size::new(0, 32), Config::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Limit);
    assert_eq!(gl.live(), (0, 0, 0));
}

#[test]
fn a_set_up_that_fails_its_check_leaves_nothing_behind() {
    let mut gl = StandIn::new();
    gl.raise(ErrorKind::OutOfMemory);
    let e = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap_err();
    assert_eq!(
        (e.kind, e.batch),
        (ErrorKind::OutOfMemory, Some(Batch::Uploads))
    );
    assert_eq!(gl.live(), (0, 0, 0));
}

/// The sizes of the upright frames the calls so far made.
fn frames_made(gl: &StandIn) -> Vec<Size> {
    gl.calls()
        .iter()
        .filter_map(|c| match c {
            Call::CreateFrame(s) => Some(*s),
            _ => None,
        })
        .collect()
}

// ADR-1303 D9: a quarter turn between a release and the next frame makes
// the upright frame once, at its new shape, and the frame after makes none
#[test]
fn a_quarter_turn_after_a_release_makes_the_upright_frame_once() {
    let mut gl = StandIn::new();
    let mut p = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap();
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    p.frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 1)
        .unwrap();
    p.release(&mut gl).unwrap();
    gl.clear_calls();
    p.turn(&mut gl, vtium_render::Orientation::Right).unwrap();
    p.frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 1)
        .unwrap();
    assert_eq!(frames_made(&gl), [Size::new(32, 64)]);
}

// ADR-1303 D9: a frame after a release whose upright frame cannot be made,
// out of memory, fails before it draws and leaves nothing made; the next
// frame makes the upright frame and draws whole
#[test]
fn a_restore_out_of_memory_fails_its_frame_cleanly_and_the_next_draws_whole() {
    let mut gl = StandIn::new();
    let mut p = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap();
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    p.frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 1)
        .unwrap();
    p.release(&mut gl).unwrap();
    let live = gl.live();
    gl.fail_next(Op::CreateFrame, ErrorKind::OutOfMemory);
    gl.clear_calls();
    let e = p
        .frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 1)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    assert_eq!(gl.live(), live);
    assert!(!gl.calls().iter().any(|c| matches!(c, Call::Draw { .. })));
    gl.clear_calls();
    let damage = p
        .frame(&mut gl, &Work::new(BORDER, &[], &[]), &buffer, 1)
        .unwrap();
    assert_eq!(frames_made(&gl), [DISPLAY]);
    assert_eq!(damage, [rect(0, 0, DISPLAY.width, DISPLAY.height)]);
}
