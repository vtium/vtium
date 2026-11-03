//! The GPU path's frame on the stand-in GL (ADR-1303 D1, D3, D7, D9): the
//! calls each of the five slots makes. The stand-in draws no pixels, so these
//! tests judge calls only; pixels are judged on a real GL in tier 2, once the
//! composition and present draw.
#![cfg(feature = "gpu")]

use vtium_render::Rect;
use vtium_render::gpu::{
    Batch, Blend, Call, Color, Config, ErrorKind, Gl, Limits, Pipeline, Size, Spatial, StandIn,
    Work,
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

fn present_draw(quads: usize) -> Call {
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
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(calls[0], Call::CreateFrame(DISPLAY));
    assert!(matches!(calls[1], Call::Compile(_)));
    assert_eq!(calls[2], Call::Check(Batch::Uploads));
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
    let Call::Compile(fragment) = &gl.calls()[1] else {
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
    let work = Work {
        redraw: &[],
        border: BORDER,
    };
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    assert_eq!(
        gl.calls(),
        &[
            Call::Clear(DISPLAY.rect(), BORDER),
            Call::Check(Batch::Draws),
            present_draw(1),
            Call::Check(Batch::Frame),
        ]
    );
}

#[test]
fn a_frame_checks_for_errors_after_the_composition_and_at_its_end() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let first = Work {
        redraw: &[],
        border: BORDER,
    };
    p.frame(&mut gl, &first, &buffer, 0).unwrap();
    gl.clear_calls();
    let cells = [rect(8, 0, 8, 16), rect(24, 16, 16, 16)];
    let work = Work {
        redraw: &cells,
        border: BORDER,
    };
    let damage = p.frame(&mut gl, &work, &buffer, 1).unwrap();
    assert_eq!(damage, cells);
    assert_eq!(
        gl.calls(),
        &[
            Call::Clear(cells[0], BORDER),
            Call::Clear(cells[1], BORDER),
            Call::Check(Batch::Draws),
            present_draw(2),
            Call::Check(Batch::Frame),
        ]
    );
}

#[test]
fn a_frame_that_changes_nothing_makes_no_call() {
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let work = Work {
        redraw: &[],
        border: BORDER,
    };
    p.frame(&mut gl, &work, &buffer, 0).unwrap();
    gl.clear_calls();
    // a rectangle wholly off the frame changes nothing either
    for redraw in [&[][..], &[rect(64, 0, 8, 8)]] {
        let work = Work {
            redraw,
            border: BORDER,
        };
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
        let work = Work {
            redraw,
            border: BORDER,
        };
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
fn a_new_border_color_composes_the_whole_frame() {
    // ADR-1303 D3: as the CPU path redraws whole when the background changes
    let mut gl = StandIn::new();
    let mut p = set_up(&mut gl);
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let cell = [rect(8, 0, 8, 16)];
    for border in [BORDER, BORDER] {
        let work = Work {
            redraw: &cell,
            border,
        };
        p.frame(&mut gl, &work, &buffer, 1).unwrap();
    }
    gl.clear_calls();
    let work = Work {
        redraw: &cell,
        border: 0x00ff_ffff,
    };
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
    let work = Work {
        redraw: &cell,
        border: BORDER,
    };
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
fn a_display_past_the_largest_texture_is_refused_until_the_frame_is_tiled() {
    // the tiles of ADR-1303 D3 come with the composition; until then the
    // set-up says why and leaves nothing behind
    let mut gl = StandIn::with_limits(Limits {
        texture: 32,
        ..StandIn::LIMITS
    });
    let e = Pipeline::new(&mut gl, DISPLAY, Config::default()).unwrap_err();
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
