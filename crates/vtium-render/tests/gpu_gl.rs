//! The `Gl` trait's contract on the stand-in GL (ADR-1303 D7): what the
//! stand-in refuses, as the real GL's adapter does through the same ledger.
#![cfg(feature = "gpu")]

use vtium_render::Rect;
use vtium_render::gpu::{
    Batch, Blend, Call, Draw, ErrorKind, Filter, Format, Gl, Input, InputKind, Limits, Op, Quad,
    Size, Source, StandIn, Value,
};

const VERT: &str = "#version 300 es\nuniform vec2 target_size;\nvoid main() {}\n";
const FRAG: &str = "#version 300 es\nprecision highp float;\nuniform highp sampler2D picture;\nout vec4 color;\nvoid main() {}\n";
const INPUTS: [Input; 2] = [
    Input {
        name: "target_size",
        kind: InputKind::Vec2,
    },
    Input {
        name: "picture",
        kind: InputKind::Texture,
    },
];

fn source(inputs: &[Input]) -> Source<'_> {
    Source {
        vertex: &[VERT],
        fragment: &[FRAG],
        inputs,
    }
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn a_handle_from_another_context_is_refused() {
    let mut a = StandIn::new();
    let mut b = StandIn::new();
    let t = a.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    // b's own first texture: a's handle must not name it
    let _own = b.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    b.clear_calls();
    let e = b
        .fill_texture(&t, rect(0, 0, 1, 1), &[0; 4], 4)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Handle);
    let e = b.free_texture(t).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Handle);
    assert_eq!(b.calls(), &[]);
}

#[test]
fn a_texture_must_fit_the_limits_the_context_reports() {
    let mut gl = StandIn::with_limits(Limits {
        texture: 64,
        ..StandIn::LIMITS
    });
    assert!(gl.create_texture(Format::R8, Size::new(64, 64)).is_ok());
    for size in [Size::new(65, 1), Size::new(1, 65), Size::new(0, 8)] {
        let e = gl.create_texture(Format::R8, size).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Limit, "{size:?}");
    }
    assert_eq!(
        gl.calls(),
        &[Call::CreateTexture(Format::R8, Size::new(64, 64))]
    );
}

#[test]
fn an_upload_must_lie_inside_its_texture_and_its_bytes() {
    let mut gl = StandIn::new();
    let t = gl.create_texture(Format::Rgba8, Size::new(8, 4)).unwrap();
    // two rows of two pixels, a row 12 bytes apart: 12 + 8 bytes
    assert!(gl.fill_texture(&t, rect(6, 2, 2, 2), &[0; 20], 12).is_ok());
    let refused = [
        (rect(7, 0, 2, 1), 8, 8),   // past the right edge
        (rect(0, 3, 1, 2), 8, 4),   // past the bottom
        (rect(0, 0, 2, 2), 19, 12), // one byte short
        (rect(0, 0, 2, 1), 8, 6),   // a stride shorter than the row
        (rect(0, 0, 1, 1), 6, 6),   // a stride of part of a pixel
    ];
    for (r, len, stride) in refused {
        let e = gl.fill_texture(&t, r, &vec![0; len], stride).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Misuse, "{r:?} {len} {stride}");
    }
    assert_eq!(gl.calls().len(), 2);
}

#[test]
fn an_upright_frame_must_fit_the_largest_texture_and_viewport() {
    let mut gl = StandIn::with_limits(Limits {
        texture: 64,
        viewport: Size::new(32, 16),
        ..StandIn::LIMITS
    });
    for size in [Size::new(33, 16), Size::new(8, 17)] {
        assert_eq!(gl.create_frame(size).unwrap_err().kind, ErrorKind::Limit);
    }
    let frame = gl.create_frame(Size::new(32, 16)).unwrap();
    assert!(frame.texture().is_some());
    assert_eq!(gl.live(), (1, 1, 0));
    // freeing the frame frees its texture
    gl.free_target(frame).unwrap();
    assert_eq!(gl.live(), (0, 0, 0));
    assert_eq!(
        gl.calls(),
        &[Call::CreateFrame(Size::new(32, 16)), Call::FreeTarget]
    );
}

#[test]
fn a_display_buffer_must_fit_the_largest_renderbuffer_and_viewport() {
    // ADR-1303 D9: such a display draws on the CPU path while its mode lasts
    let mut gl = StandIn::with_limits(Limits {
        renderbuffer: 4096,
        viewport: Size::new(8192, 4096),
        ..StandIn::LIMITS
    });
    assert!(gl.import_buffer(&Size::new(3840, 2160)).is_ok());
    for size in [Size::new(7680, 2160), Size::new(2160, 4097)] {
        assert_eq!(
            gl.import_buffer(&size).unwrap_err().kind,
            ErrorKind::Limit,
            "{size:?}"
        );
        assert!(!gl.limits().fits_buffer(size));
    }
}

#[test]
fn a_program_declares_each_input_it_lists() {
    let mut gl = StandIn::new();
    assert!(gl.compile(&source(&INPUTS)).is_ok());
    let missing = [Input {
        name: "palette",
        kind: InputKind::Vec4,
    }];
    assert_eq!(
        gl.compile(&source(&missing)).unwrap_err().kind,
        ErrorKind::Misuse
    );
    let twice = [INPUTS[0], INPUTS[0]];
    assert_eq!(
        gl.compile(&source(&twice)).unwrap_err().kind,
        ErrorKind::Misuse
    );
}

#[test]
fn no_program_compiles_without_highp() {
    let mut gl = StandIn::with_limits(Limits {
        fragment_highp: false,
        ..StandIn::LIMITS
    });
    assert_eq!(
        gl.compile(&source(&INPUTS)).unwrap_err().kind,
        ErrorKind::Limit
    );
}

#[test]
fn a_draw_gives_each_input_once_with_a_value_of_its_kind() {
    let mut gl = StandIn::new();
    let program = gl.compile(&source(&INPUTS)).unwrap();
    let picture = gl.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    let target = gl.create_frame(Size::new(16, 16)).unwrap();
    let frame = target.texture().unwrap();
    let size = ("target_size", Value::Vec2([16.0, 16.0]));
    let pic = ("picture", Value::Texture(&picture, Filter::Linear));
    let quads = [Quad::default()];
    let draw = |gl: &mut StandIn, inputs: &[(&'static str, Value<'_>)]| {
        gl.draw(&Draw {
            target: &target,
            program: &program,
            inputs,
            quads: &quads,
            blend: Blend::Over,
            scissor: None,
        })
    };
    draw(&mut gl, &[size, pic]).unwrap();
    let wrong = ("picture", Value::Vec4([0.0; 4]));
    let extra = ("palette", Value::Float(1.0));
    let own = ("picture", Value::Texture(frame, Filter::Nearest));
    for inputs in [
        &[size][..],
        &[size, pic, pic],
        &[size, wrong],
        &[size, pic, extra],
        &[size, own],
    ] {
        assert_eq!(
            draw(&mut gl, inputs).unwrap_err().kind,
            ErrorKind::Misuse,
            "{inputs:?}"
        );
    }
    assert_eq!(
        gl.calls().last(),
        Some(&Call::Draw {
            quads: vec![Quad::default()],
            blend: Blend::Over,
            inputs: vec!["target_size", "picture"],
        })
    );
}

#[test]
fn a_read_lies_inside_its_target_and_fills_its_bytes() {
    let mut gl = StandIn::new();
    let target = gl.import_buffer(&Size::new(8, 8)).unwrap();
    let mut out = vec![1; 2 * 3 * 4];
    gl.read(&target, rect(6, 5, 2, 3), &mut out).unwrap();
    assert!(out.iter().all(|&b| b == 0));
    assert_eq!(
        gl.read(&target, rect(7, 5, 2, 3), &mut out)
            .unwrap_err()
            .kind,
        ErrorKind::Misuse
    );
    assert_eq!(
        gl.read(&target, rect(0, 0, 2, 2), &mut out)
            .unwrap_err()
            .kind,
        ErrorKind::Misuse
    );
}

#[test]
fn a_gl_error_is_reported_by_the_next_check_with_its_batch() {
    let mut gl = StandIn::new();
    gl.raise(ErrorKind::OutOfMemory);
    let e = gl.check(Batch::Uploads).unwrap_err();
    assert_eq!(
        (e.kind, e.batch),
        (ErrorKind::OutOfMemory, Some(Batch::Uploads))
    );
    // the flag is read once
    assert!(gl.check(Batch::Draws).is_ok());
}

#[test]
fn a_lost_context_refuses_every_later_call() {
    let mut gl = StandIn::new();
    let texture = gl.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    let frame = gl.create_frame(Size::new(4, 4)).unwrap();
    let buffer = gl.import_buffer(&Size::new(4, 4)).unwrap();
    let program = gl.compile(&source(&INPUTS)).unwrap();
    let fence = gl.submit(&buffer).unwrap();
    gl.raise(ErrorKind::ContextLost);
    assert_eq!(
        gl.check(Batch::Frame).unwrap_err().kind,
        ErrorKind::ContextLost
    );
    gl.clear_calls();
    let lost = |r: Result<(), vtium_render::gpu::Error>| r.unwrap_err().kind;
    let mut out = [0; 4];
    let size = ("target_size", Value::Vec2([4.0, 4.0]));
    let pic = ("picture", Value::Texture(&texture, Filter::Nearest));
    let draw = Draw {
        target: &buffer,
        program: &program,
        inputs: &[size, pic],
        quads: &[],
        blend: Blend::Replace,
        scissor: None,
    };
    let kinds = [
        lost(gl.create_texture(Format::R8, Size::new(1, 1)).map(drop)),
        lost(gl.fill_texture(&texture, rect(0, 0, 1, 1), &[0; 4], 4)),
        lost(gl.create_frame(Size::new(4, 4)).map(drop)),
        lost(gl.import_buffer(&Size::new(4, 4)).map(drop)),
        lost(gl.compile(&source(&INPUTS)).map(drop)),
        lost(gl.clear(&buffer, rect(0, 0, 1, 1), 0)),
        lost(gl.draw(&draw)),
        lost(gl.submit(&buffer).map(drop)),
        lost(gl.wait(fence)),
        lost(gl.read(&buffer, rect(0, 0, 1, 1), &mut out)),
        lost(gl.check(Batch::Frame)),
        lost(gl.free_program(program)),
        lost(gl.free_target(frame)),
        lost(gl.free_texture(texture)),
    ];
    assert!(
        kinds.iter().all(|&k| k == ErrorKind::ContextLost),
        "{kinds:?}"
    );
    assert_eq!(gl.calls(), &[]);
}

#[test]
fn an_input_must_be_declared_with_its_type() {
    let mut gl = StandIn::new();
    // `picture` is a sampler2D: real GL refuses a vec4 for it
    let wrong = [
        INPUTS[0],
        Input {
            name: "picture",
            kind: InputKind::Vec4,
        },
    ];
    assert_eq!(
        gl.compile(&source(&wrong)).unwrap_err().kind,
        ErrorKind::Misuse
    );
    let frag: &str = "#version 300 es\nprecision highp float;\nuniform vec2 a, b; // a list\nuniform vec4 palette[16];\nout vec4 color;\nvoid main() {}\n";
    let frags = [frag];
    let lists = |inputs| Source {
        vertex: &[VERT],
        fragment: &frags,
        inputs,
    };
    let a_b = [
        Input {
            name: "a",
            kind: InputKind::Vec2,
        },
        Input {
            name: "b",
            kind: InputKind::Vec2,
        },
    ];
    assert!(gl.compile(&lists(&a_b)).is_ok());
    // no value fills an array
    let palette = [Input {
        name: "palette",
        kind: InputKind::Vec4,
    }];
    assert_eq!(
        gl.compile(&lists(&palette)).unwrap_err().kind,
        ErrorKind::Misuse
    );
}

#[test]
fn a_draw_that_samples_another_contexts_texture_is_refused() {
    let mut a = StandIn::new();
    let mut b = StandIn::new();
    let theirs = a.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    // b's own first texture, which a's handle must not name
    let _own = b.create_texture(Format::Rgba8, Size::new(4, 4)).unwrap();
    let program = b.compile(&source(&INPUTS)).unwrap();
    let target = b.import_buffer(&Size::new(4, 4)).unwrap();
    b.clear_calls();
    let e = b
        .draw(&Draw {
            target: &target,
            program: &program,
            inputs: &[
                ("target_size", Value::Vec2([4.0, 4.0])),
                ("picture", Value::Texture(&theirs, Filter::Nearest)),
            ],
            quads: &[],
            blend: Blend::Replace,
            scissor: None,
        })
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Handle);
    assert_eq!(b.calls(), &[]);
}

#[test]
fn a_submit_is_recorded_for_a_live_target_of_its_own() {
    let mut a = StandIn::new();
    let mut b = StandIn::new();
    let target = a.import_buffer(&Size::new(4, 4)).unwrap();
    let _own = b.import_buffer(&Size::new(4, 4)).unwrap();
    let fence = a.submit(&target).unwrap();
    assert_eq!(a.calls().last(), Some(&Call::Submit));
    assert_eq!(a.fences(), 1);
    b.clear_calls();
    assert_eq!(b.submit(&target).unwrap_err().kind, ErrorKind::Handle);
    assert_eq!(b.calls(), &[]);
    a.wait(fence).unwrap();
    assert_eq!(a.calls().last(), Some(&Call::Wait));
    assert_eq!(a.fences(), 0);
}

// the issue on the EGL sync before the flip: a fence is waited on in the
// context that made it, as EGL waits on a sync only on its own display,
// and once
#[test]
fn a_fence_is_waited_on_once_in_its_own_context() {
    let mut a = StandIn::new();
    let mut b = StandIn::new();
    let target = a.import_buffer(&Size::new(4, 4)).unwrap();
    let fence = a.submit(&target).unwrap();
    assert_eq!(b.wait(fence).unwrap_err().kind, ErrorKind::Handle);
    assert_eq!(b.calls(), &[]);
    // refused by another context, the fence is still a's to wait on
    assert_eq!(a.fences(), 1);
    let fence = a.submit(&target).unwrap();
    a.wait(fence).unwrap();
    assert_eq!(a.fences(), 1);
}

#[test]
fn the_stand_in_fails_the_next_call_of_a_kind_as_a_driver_would() {
    let size = Size::new(4, 4);
    let cases = [
        (Op::CreateTexture, ErrorKind::OutOfMemory),
        (Op::CreateFrame, ErrorKind::OutOfMemory),
        (Op::ImportBuffer, ErrorKind::Import),
        (Op::Compile, ErrorKind::Compile),
    ];
    for (op, kind) in cases {
        let mut gl = StandIn::new();
        gl.fail_next(op, kind);
        let call = |gl: &mut StandIn| match op {
            Op::CreateTexture => gl.create_texture(Format::R8, size).map(|_| ()),
            Op::CreateFrame => gl.create_frame(size).map(|_| ()),
            Op::ImportBuffer => gl.import_buffer(&size).map(|_| ()),
            Op::Compile => gl.compile(&source(&INPUTS)).map(|_| ()),
            Op::Wait => unreachable!("a wait needs a fence: below"),
        };
        assert_eq!(call(&mut gl).unwrap_err().kind, kind, "{op:?}");
        assert_eq!((gl.live(), gl.calls()), ((0, 0, 0), &[][..]), "{op:?}");
        // only the next call fails
        assert!(call(&mut gl).is_ok(), "{op:?}");
    }
    // a wait the driver fails frees its fence all the same, and only the
    // next wait fails
    let mut gl = StandIn::new();
    let target = gl.import_buffer(&size).unwrap();
    gl.fail_next(Op::Wait, ErrorKind::Gl);
    let fence = gl.submit(&target).unwrap();
    assert_eq!(gl.wait(fence).unwrap_err().kind, ErrorKind::Gl);
    assert_eq!(gl.fences(), 0);
    let fence = gl.submit(&target).unwrap();
    assert!(gl.wait(fence).is_ok());
}
