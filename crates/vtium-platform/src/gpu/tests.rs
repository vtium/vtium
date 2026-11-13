//! The safe GL type on Mesa's llvmpipe, through EGL's software device, and
//! the import of a dma-buf on a render node.
//!
//! Every test but the render node's needs llvmpipe and Mesa's EGL. Where
//! they are missing, each test fails and names the packages; none passes
//! without having drawn. The render node's test needs a GPU, so it runs
//! only when asked: `cargo test -p vtium-platform --features gpu --
//! --ignored`.

#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use std::os::fd::OwnedFd;
use std::path::Path;

use super::*;
use crate::display::Rect;

/// The quad contract's vertex shader, as vtium-render's `quad.vert` keeps
/// it: each quad's `dst` at location 0, `src` and `params` at 1 and 2.
const VERT: &str = "#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in ivec4 quad_dst;
layout(location = 1) in uvec4 quad_src;
layout(location = 2) in uvec4 quad_params;
uniform vec2 target_size;
out vec2 src_at;
flat out uvec4 params;
void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 at = vec2(quad_dst.xy) + corner * vec2(quad_dst.zw);
    gl_Position = vec4(at / target_size * 2.0 - 1.0, 0.0, 1.0);
    src_at = vec2(quad_src.xy) + corner * vec2(quad_src.zw);
    params = quad_params;
}
";

/// Copies texels one to a pixel; an R8 texture's red goes to every
/// channel. `unused` is declared and never read, so the driver drops it.
const COPY: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D tex;
uniform float unused;
uniform float gray;
in vec2 src_at;
out vec4 color;
void main() {
    vec4 t = texelFetch(tex, ivec2(src_at), 0);
    color = gray > 0.5 ? vec4(t.rrr, 1.0) : t;
}
";

/// Samples its texture at each pixel's place, through the draw's filter.
const SAMPLE: &str = "#version 300 es
precision highp float;
uniform highp sampler2D tex;
in vec2 src_at;
out vec4 color;
void main() {
    color = texture(tex, src_at / vec2(textureSize(tex, 0)));
}
";

/// A software context, or a failure that names what is missing.
fn software() -> Context {
    match Context::software() {
        Ok(gl) => gl,
        Err(e) => panic!(
            "{e}. The GPU path's tests draw on Mesa's llvmpipe through EGL: install \
             libegl1, libegl-mesa0 and libgl1-mesa-dri (Debian, Ubuntu) or mesa (Arch)"
        ),
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

/// Draws `quads` of `texture` into `fb`, a `size` target, through COPY.
#[expect(
    clippy::too_many_arguments,
    reason = "each test sets every part of the draw"
)]
fn copy(
    gl: &mut Context,
    program: u32,
    fb: u32,
    size: (u32, u32),
    texture: u32,
    gray: bool,
    quads: &[Instance],
    blend: Blend,
    scissor: Option<Rect>,
) {
    let at = |gl: &mut Context, n: &str| gl.uniform_location(program, n).unwrap();
    let uniforms = [
        (
            at(gl, "target_size"),
            Uniform::Vec2([size.0 as f32, size.1 as f32]),
        ),
        (at(gl, "tex"), Uniform::Texture(texture, Filter::Nearest)),
        (at(gl, "unused"), Uniform::Float(1.0)),
        (at(gl, "gray"), Uniform::Float(if gray { 1.0 } else { 0.0 })),
    ];
    gl.draw(&DrawCall {
        framebuffer: fb,
        size,
        program,
        uniforms: &uniforms,
        instances: quads,
        blend,
        scissor,
    })
    .unwrap();
}

fn quad(dst: [i32; 4], src: [u32; 4]) -> Instance {
    Instance {
        dst,
        src,
        params: [0; 4],
    }
}

#[test]
fn the_software_context_is_llvmpipe_with_opengl_es_3_and_highp() {
    let gl = software();
    assert!(gl.renderer().contains("llvmpipe"), "{}", gl.renderer());
    assert!(gl.version().starts_with("OpenGL ES 3."), "{}", gl.version());
    let l = gl.limits();
    assert!(l.texture >= 2048 && l.renderbuffer >= 2048, "{l:?}");
    assert!(l.viewport.0 >= 2048 && l.viewport.1 >= 2048, "{l:?}");
    assert!(l.fragment_highp);
    assert!(l.texture_units >= 16, "{l:?}");
}

#[test]
fn a_quad_copies_its_texels_row_for_row_from_a_padded_upload() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    // 3x2 pixels, each row padded to 16 bytes: the row length carries the
    // stride, and row 0 is the first row in memory
    let mut pixels = vec![0xEEu8; 32];
    for (i, px) in [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]
        .iter()
        .enumerate()
    {
        pixels[i * 4..i * 4 + 4].copy_from_slice(px);
        pixels[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&px.map(|c| c + 100));
    }
    let texture = gl.create_texture(TextureFormat::Rgba8, 3, 2).unwrap();
    gl.fill_texture(texture, rect(0, 0, 3, 2), &pixels, 16)
        .unwrap();
    let (frame_tex, fb) = gl.create_frame(3, 2).unwrap();
    gl.clear(fb, (3, 2), rect(0, 0, 3, 2), 0).unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (3, 2),
        texture,
        false,
        &[quad([0, 0, 3, 2], [0, 0, 3, 2])],
        Blend::Replace,
        None,
    );
    gl.check().unwrap();
    let mut out = vec![0u8; 24];
    gl.read(fb, rect(0, 0, 3, 2), &mut out).unwrap();
    let want: Vec<u8> = (1..=12).chain(101..=112).collect();
    assert_eq!(out, want);
    gl.free_frame(fb, frame_tex).unwrap();
    gl.free_texture(texture).unwrap();
    gl.free_program(program).unwrap();
    gl.check().unwrap();
}

#[test]
fn an_r8_texture_of_odd_width_uploads_with_no_row_alignment() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::R8, 3, 3).unwrap();
    // rows of 3 bytes, tightly packed: GL's default alignment of 4 would
    // read each row after the first from the wrong place
    let pixels: Vec<u8> = (1..=9).map(|v| v * 20).collect();
    gl.fill_texture(texture, rect(0, 0, 3, 3), &pixels, 3)
        .unwrap();
    let (frame_tex, fb) = gl.create_frame(3, 3).unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (3, 3),
        texture,
        true,
        &[quad([0, 0, 3, 3], [0, 0, 3, 3])],
        Blend::Replace,
        None,
    );
    let mut out = vec![0u8; 36];
    gl.read(fb, rect(0, 0, 3, 3), &mut out).unwrap();
    let red: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
    assert_eq!(red, pixels);
    gl.free_frame(fb, frame_tex).unwrap();
    gl.check().unwrap();
}

#[test]
fn clear_fills_its_rectangle_opaque_and_a_scissor_cuts_a_draw() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let (frame_tex, fb) = gl.create_frame(4, 4).unwrap();
    gl.clear(fb, (4, 4), rect(0, 0, 4, 4), 0x10_2030).unwrap();
    gl.clear(fb, (4, 4), rect(1, 1, 2, 2), 0xA0_B0C0).unwrap();
    let mut out = vec![0u8; 64];
    gl.read(fb, rect(0, 0, 4, 4), &mut out).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let inside = (1..3).contains(&x) && (1..3).contains(&y);
            let want = if inside {
                [0xA0, 0xB0, 0xC0, 0xFF]
            } else {
                [0x10, 0x20, 0x30, 0xFF]
            };
            assert_eq!(out[(y * 4 + x) * 4..][..4], want, "at {x},{y}");
        }
    }
    // a white texel drawn over the whole target, cut to its first row
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    gl.fill_texture(texture, rect(0, 0, 1, 1), &[255; 4], 4)
        .unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (4, 4),
        texture,
        false,
        &[quad([0, 0, 4, 4], [0, 0, 1, 1])],
        Blend::Replace,
        Some(rect(0, 0, 4, 1)),
    );
    let mut row = vec![0u8; 32];
    gl.read(fb, rect(0, 0, 4, 2), &mut row).unwrap();
    assert!(row[..16].iter().all(|&c| c == 255), "{row:?}");
    assert_eq!(row[16..20], [0x10, 0x20, 0x30, 0xFF]);
    gl.free_frame(fb, frame_tex).unwrap();
    gl.check().unwrap();
}

#[test]
fn over_blends_a_premultiplied_texel_on_what_the_target_holds() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let (frame_tex, fb) = gl.create_frame(1, 1).unwrap();
    gl.clear(fb, (1, 1), rect(0, 0, 1, 1), 0xC8_6400).unwrap();
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    // half covered blue, premultiplied: src + dst * (1 - 0.5)
    gl.fill_texture(texture, rect(0, 0, 1, 1), &[0, 0, 128, 128], 4)
        .unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (1, 1),
        texture,
        false,
        &[quad([0, 0, 1, 1], [0, 0, 1, 1])],
        Blend::Over,
        None,
    );
    let mut out = [0u8; 4];
    gl.read(fb, rect(0, 0, 1, 1), &mut out).unwrap();
    for (got, want) in out.iter().zip([100, 50, 128, 255]) {
        assert!(got.abs_diff(want) <= 1, "{out:?}");
    }
    gl.free_frame(fb, frame_tex).unwrap();
}

#[test]
fn a_draws_filter_chooses_how_its_texture_is_sampled() {
    let mut gl = software();
    let program = gl.compile(VERT, SAMPLE).unwrap();
    let size = gl.uniform_location(program, "target_size").unwrap();
    let tex = gl.uniform_location(program, "tex").unwrap();
    // black then red, stretched over four pixels
    let texture = gl.create_texture(TextureFormat::Rgba8, 2, 1).unwrap();
    gl.fill_texture(
        texture,
        rect(0, 0, 2, 1),
        &[0, 0, 0, 255, 255, 0, 0, 255],
        8,
    )
    .unwrap();
    let (frame_tex, fb) = gl.create_frame(4, 1).unwrap();
    for (filter, want) in [
        (Filter::Nearest, [0, 0, 255, 255]),
        (Filter::Linear, [0, 64, 191, 255]),
    ] {
        gl.draw(&DrawCall {
            framebuffer: fb,
            size: (4, 1),
            program,
            uniforms: &[
                (size, Uniform::Vec2([4.0, 1.0])),
                (tex, Uniform::Texture(texture, filter)),
            ],
            instances: &[quad([0, 0, 4, 1], [0, 0, 2, 1])],
            blend: Blend::Replace,
            scissor: None,
        })
        .unwrap();
        let mut out = vec![0u8; 16];
        gl.read(fb, rect(0, 0, 4, 1), &mut out).unwrap();
        let red: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        for (got, want) in red.iter().zip(want) {
            assert!(got.abs_diff(want) <= 1, "{filter:?}: {red:?}");
        }
    }
    gl.free_frame(fb, frame_tex).unwrap();
    gl.check().unwrap();
}

#[test]
fn many_quads_draw_as_instances_of_one_call() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::Rgba8, 2, 1).unwrap();
    gl.fill_texture(
        texture,
        rect(0, 0, 2, 1),
        &[255, 0, 0, 255, 0, 0, 255, 255],
        8,
    )
    .unwrap();
    let (frame_tex, fb) = gl.create_frame(8, 1).unwrap();
    gl.clear(fb, (8, 1), rect(0, 0, 8, 1), 0).unwrap();
    // red at the even pixels, blue at the odd, one quad each
    let quads: Vec<Instance> = (0..8)
        .map(|x| quad([x, 0, 1, 1], [u32::try_from(x % 2).unwrap(), 0, 1, 1]))
        .collect();
    copy(
        &mut gl,
        program,
        fb,
        (8, 1),
        texture,
        false,
        &quads,
        Blend::Replace,
        None,
    );
    let mut out = vec![0u8; 32];
    gl.read(fb, rect(0, 0, 8, 1), &mut out).unwrap();
    for (x, px) in out.chunks(4).enumerate() {
        let want = if x % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        assert_eq!(px, want, "at {x}");
    }
    gl.free_frame(fb, frame_tex).unwrap();
    gl.check().unwrap();
}

#[test]
fn a_uniform_the_driver_dropped_has_no_location_and_setting_it_does_nothing() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    assert_eq!(gl.uniform_location(program, "unused").unwrap(), None);
    assert!(gl.uniform_location(program, "tex").unwrap().is_some());
    // `copy` sets it at no location, and GL raises nothing
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    let (frame_tex, fb) = gl.create_frame(1, 1).unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (1, 1),
        texture,
        false,
        &[quad([0, 0, 1, 1], [0, 0, 1, 1])],
        Blend::Replace,
        None,
    );
    gl.check().unwrap();
    gl.free_frame(fb, frame_tex).unwrap();
}

/// The row length is set at every upload, so an upload of rows packed
/// tight after one of padded rows reads its own rows only. Left from the
/// first upload, a row length of 16 would read 35 bytes of the second's 9.
#[test]
fn each_upload_sets_its_own_row_length() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::R8, 3, 3).unwrap();
    let padded: Vec<u8> = (0..48).map(|i| if i % 16 < 3 { 200 } else { 0 }).collect();
    gl.fill_texture(texture, rect(0, 0, 3, 3), &padded, 16)
        .unwrap();
    // the 9 bytes uploaded sit in a longer allocation of other values, so a
    // stale row length reads them, not memory past the allocation
    let backing: Vec<u8> = (1..=9).map(|v| v * 10).chain([255; 32]).collect();
    gl.fill_texture(texture, rect(0, 0, 3, 3), &backing[..9], 3)
        .unwrap();
    let (frame_tex, fb) = gl.create_frame(3, 3).unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (3, 3),
        texture,
        true,
        &[quad([0, 0, 3, 3], [0, 0, 3, 3])],
        Blend::Replace,
        None,
    );
    let mut out = vec![0u8; 36];
    gl.read(fb, rect(0, 0, 3, 3), &mut out).unwrap();
    let red: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
    assert_eq!(red, backing[..9]);
    gl.free_frame(fb, frame_tex).unwrap();
    gl.check().unwrap();
}

/// An upload is checked against the texture the context keeps: its format
/// sizes the slice, and the rectangle lies inside it. A texture the context
/// did not make, or freed, is refused, and none of it reaches GL.
#[test]
fn an_upload_is_checked_against_the_texture_itself() {
    let mut gl = software();
    let rgba = gl.create_texture(TextureFormat::Rgba8, 4, 4).unwrap();
    // sixteen bytes would fill a 4x4 R8 texture; this one takes four a pixel
    let e = gl
        .fill_texture(rgba, rect(0, 0, 4, 4), &[0; 16], 4)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    for outside in [rect(1, 0, 4, 4), rect(0, 3, 1, 2), rect(u32::MAX, 0, 2, 1)] {
        let e = gl.fill_texture(rgba, outside, &[0; 256], 64).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Misuse, "{outside:?}: {e}");
    }
    gl.fill_texture(rgba, rect(0, 0, 4, 4), &[0; 64], 16)
        .unwrap();
    let e = gl
        .fill_texture(9999, rect(0, 0, 1, 1), &[0; 4], 4)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    gl.free_texture(rgba).unwrap();
    let e = gl
        .fill_texture(rgba, rect(0, 0, 1, 1), &[0; 4], 4)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    assert_eq!(gl.free_texture(rgba).unwrap_err().kind, ErrorKind::Misuse);
    gl.check().unwrap();
}

#[test]
fn a_uniform_name_with_a_nul_is_refused() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let e = gl.uniform_location(program, "tex\0").unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
}

#[test]
fn only_opengl_es_versions_are_read() {
    use super::context::es_version;
    assert_eq!(es_version("OpenGL ES 3.2 Mesa 26.2.6"), Some((3, 2)));
    assert_eq!(es_version("OpenGL ES 3.0"), Some((3, 0)));
    assert_eq!(es_version("OpenGL ES 2.0 Mesa"), Some((2, 0)));
    assert_eq!(es_version("4.6 (Core Profile) Mesa"), None);
    assert_eq!(es_version("OpenGL ES 3"), None);
    assert_eq!(es_version(""), None);
}

#[test]
fn an_error_a_test_raises_fails_the_next_call_alone() {
    let mut gl = software();
    gl.raise(ErrorKind::ContextLost);
    assert_eq!(gl.check().unwrap_err().kind, ErrorKind::ContextLost);
    gl.check().unwrap();
    // any call that reaches GL, not only a check
    gl.raise(ErrorKind::OutOfMemory);
    let e = gl.create_texture(TextureFormat::R8, 1, 1).unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    gl.create_texture(TextureFormat::R8, 1, 1).unwrap();
    gl.check().unwrap();
}

/// Binding an import's image keeps a lost context and out of memory, which
/// leave the GPU path; any other error fails the import alone.
#[test]
fn an_imports_bind_keeps_a_lost_context_and_out_of_memory() {
    use super::context::bound;
    const LOST: u32 = 0x0507;
    const OOM: u32 = 0x0505;
    const INVALID_OPERATION: u32 = 0x0502;
    let kind = |errors: &[u32], reset| bound(errors, reset).unwrap_err().kind;
    assert_eq!(bound(&[], 0), Ok(()));
    assert_eq!(kind(&[INVALID_OPERATION], 0), ErrorKind::Import);
    assert_eq!(kind(&[OOM], 0), ErrorKind::OutOfMemory);
    assert_eq!(kind(&[INVALID_OPERATION, OOM], 0), ErrorKind::OutOfMemory);
    assert_eq!(kind(&[LOST], 0), ErrorKind::ContextLost);
    assert_eq!(kind(&[], 0x8253), ErrorKind::ContextLost);
}

#[test]
fn an_upload_or_a_read_whose_slice_is_short_is_refused_before_gl() {
    let mut gl = software();
    let texture = gl.create_texture(TextureFormat::Rgba8, 4, 4).unwrap();
    let r = rect(0, 0, 4, 4);
    // the last row needs 16 bytes after three strides of 20
    let e = gl.fill_texture(texture, r, &[0; 75], 20).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    gl.fill_texture(texture, r, &[0; 76], 20).unwrap();
    // a stride shorter than a row, or not of whole pixels
    for stride in [15, 18] {
        let e = gl.fill_texture(texture, r, &[0; 128], stride).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    }
    let (frame_tex, fb) = gl.create_frame(4, 4).unwrap();
    for len in [63, 65] {
        let e = gl.read(fb, r, &mut vec![0; len]).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    }
    // none of them reached GL
    gl.check().unwrap();
    gl.free_frame(fb, frame_tex).unwrap();
}

#[test]
fn check_reports_a_gl_error_once_and_then_nothing() {
    let mut gl = software();
    // past the largest texture: GL_INVALID_VALUE, which only check reads
    let past = gl.limits().texture + 1;
    gl.create_texture(TextureFormat::R8, past, 1).unwrap();
    let e = gl.check().unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Gl(_)), "{e}");
    gl.check().unwrap();
}

#[test]
fn a_lost_context_and_out_of_memory_come_before_other_errors() {
    use super::context::sort;
    const LOST: u32 = 0x0507;
    const OOM: u32 = 0x0505;
    const INVALID_VALUE: u32 = 0x0501;
    const GUILTY_RESET: u32 = 0x8253;
    let kind = |errors: &[u32], reset| sort(errors, reset).unwrap_err().kind;
    assert_eq!(sort(&[], 0), Ok(()));
    assert_eq!(
        kind(&[INVALID_VALUE, 0x0502], 0),
        ErrorKind::Gl(INVALID_VALUE)
    );
    assert_eq!(kind(&[INVALID_VALUE, OOM], 0), ErrorKind::OutOfMemory);
    assert_eq!(kind(&[OOM, LOST], 0), ErrorKind::ContextLost);
    assert_eq!(kind(&[], GUILTY_RESET), ErrorKind::ContextLost);
}

#[test]
fn a_shader_that_does_not_compile_gives_the_drivers_log() {
    let mut gl = software();
    let e = gl
        .compile(VERT, "#version 300 es\nvoid main() { undeclared = 1; }\n")
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Compile);
    assert!(
        e.detail.contains("fragment shader") && e.detail.contains("undeclared"),
        "{e}"
    );
    // both shaders refused: both logs
    let e = gl
        .compile(
            "#version 300 es\nvoid main() { first_wrong = 1; }\n",
            "#version 300 es\nvoid main() { second_wrong = 1; }\n",
        )
        .unwrap_err();
    assert!(
        e.detail.contains("first_wrong") && e.detail.contains("second_wrong"),
        "{e}"
    );
    // the inputs do not match the outputs: it compiles and does not link
    let e = gl
        .compile(VERT, "#version 300 es\nprecision highp float;\nin vec3 nowhere;\nout vec4 c;\nvoid main() { c = vec4(nowhere, 1.0); }\n")
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Compile);
    assert!(e.detail.contains("does not link"), "{e}");
}

#[test]
fn a_draw_of_more_textures_than_units_is_refused() {
    let mut gl = software();
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    let (frame_tex, fb) = gl.create_frame(1, 1).unwrap();
    let units = gl.limits().texture_units as usize;
    let uniforms = vec![(None, Uniform::Texture(texture, Filter::Nearest)); units + 1];
    let e = gl
        .draw(&DrawCall {
            framebuffer: fb,
            size: (1, 1),
            program,
            uniforms: &uniforms,
            instances: &[quad([0, 0, 1, 1], [0, 0, 1, 1])],
            blend: Blend::Replace,
            scissor: None,
        })
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse, "{e}");
    gl.free_frame(fb, frame_tex).unwrap();
}

#[test]
fn a_set_up_ends_at_the_first_step_that_fails() {
    let e = Context::on_render_node(Path::new("/nonexistent/renderD999")).unwrap_err();
    assert_eq!(e.step, Step::RenderNode, "{e}");
    // a node that opens and is no DRM device: libgbm falls back to Mesa's
    // software renderer, which is no GPU path
    let e = Context::on_render_node(Path::new("/dev/null")).unwrap_err();
    assert_eq!(e.step, Step::EglDisplay, "{e}");
    assert!(e.detail.contains("software"), "{e}");
}

/// The DRM nodes in `/dev/dri` whose names start with `prefix`, each with
/// the device sysfs's `/sys/class/drm` names it by, which is a different route
/// from the one [`render_node`] takes (the card's device number).
fn nodes_by_device(prefix: &str) -> Vec<(std::path::PathBuf, Option<std::path::PathBuf>)> {
    let Ok(entries) = std::fs::read_dir("/dev/dri") else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .map(|p| {
            let name = p.file_name().unwrap().to_owned();
            let device =
                std::fs::canonicalize(Path::new("/sys/class/drm").join(&name).join("device")).ok();
            (p, device)
        })
        .collect()
}

/// Each card's render node is the one on the card's own device, and a card
/// whose device has none, such as a display-only card, has none.
#[test]
fn a_cards_render_node_is_found_without_opening_the_card() {
    let renders = nodes_by_device("renderD");
    for (card, device) in nodes_by_device("card") {
        let own: Vec<_> = renders
            .iter()
            .filter(|(_, d)| d.is_some() && *d == device)
            .map(|(node, _)| node.clone())
            .collect();
        match render_node(&card) {
            Ok(node) => assert!(
                own.contains(&node),
                "{} gives {}, not one of its device's {own:?}",
                card.display(),
                node.display()
            ),
            Err(e) => {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::NotFound,
                    "{}: {e}",
                    card.display()
                );
                assert!(
                    own.is_empty(),
                    "{} has render nodes {own:?} but none was found: {e}",
                    card.display()
                );
            }
        }
    }
}

/// The first render node, opened.
fn render_node_fd() -> (std::path::PathBuf, OwnedFd) {
    let node = std::fs::read_dir("/dev/dri")
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("renderD"))
        })
        .min()
        .expect("a render node");
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&node)
        .unwrap()
        .into();
    (node, fd)
}

/// A buffer GBM allocates on the render node, imported into a context on
/// the same node, cleared and read back. A render node refuses dumb buffers
/// but allocates GBM buffers for rendering, so this proves the import here;
/// a buffer for scanout needs the card and only tier 3 proves it.
#[test]
#[ignore = "needs a GPU's render node: cargo test -p vtium-platform --features gpu -- --ignored"]
fn a_gbm_buffer_imports_as_a_target_that_draws() {
    let (node, fd) = render_node_fd();
    let mut gl = Context::on_render_node(&node).unwrap();
    let gbm = Gbm::new(fd).unwrap();
    let xrgb8888 = u32::from_le_bytes(*b"XR24");
    let bo = gbm.allocate(64, 32, xrgb8888, Usage::Rendering).unwrap();
    let buffer = bo.export().unwrap();
    assert_eq!(buffer.size(), (64, 32));
    // an error an earlier call left fails no import, and the next check
    // still reports it
    let past = gl.limits().texture + 1;
    gl.create_texture(TextureFormat::R8, past, 1).unwrap();
    let fb = gl.import(&buffer).unwrap();
    assert!(matches!(gl.check().unwrap_err().kind, ErrorKind::Gl(_)));
    gl.clear(fb, (64, 32), rect(0, 0, 64, 32), 0x12_3456)
        .unwrap();
    // a quad through a program into the last 4x2 pixels
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    gl.fill_texture(texture, rect(0, 0, 1, 1), &[0xAB, 0xCD, 0xEF, 0xFF], 4)
        .unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (64, 32),
        texture,
        false,
        &[quad([60, 30, 4, 2], [0, 0, 1, 1])],
        Blend::Replace,
        None,
    );
    let fence = gl.submit().unwrap();
    gl.wait(fence).unwrap();
    gl.check().unwrap();
    let mut out = vec![0u8; 64 * 32 * 4];
    gl.read(fb, rect(0, 0, 64, 32), &mut out).unwrap();
    assert_eq!(out[..4], [0x12, 0x34, 0x56, 0xFF]);
    assert_eq!(out[(29 * 64 + 63) * 4..][..4], [0x12, 0x34, 0x56, 0xFF]);
    assert_eq!(out[(30 * 64 + 60) * 4..][..4], [0xAB, 0xCD, 0xEF, 0xFF]);
    assert_eq!(out[(31 * 64 + 63) * 4..][..4], [0xAB, 0xCD, 0xEF, 0xFF]);
    gl.free_buffer(fb).unwrap();
    gl.check().unwrap();
}

// ADR-1303 D9: the frame a head draws into for the staging copy is a
// renderbuffer in GL's memory. A clear and a draw reach it, a read gives
// them back, and a free takes it once; a buffer left unfreed goes with
// its context
#[test]
fn a_buffer_in_gl_memory_draws_and_reads_back() {
    let mut gl = software();
    let fb = gl.create_buffer(64, 32).unwrap();
    gl.clear(fb, (64, 32), rect(0, 0, 64, 32), 0x12_3456)
        .unwrap();
    let program = gl.compile(VERT, COPY).unwrap();
    let texture = gl.create_texture(TextureFormat::Rgba8, 1, 1).unwrap();
    gl.fill_texture(texture, rect(0, 0, 1, 1), &[0xAB, 0xCD, 0xEF, 0xFF], 4)
        .unwrap();
    copy(
        &mut gl,
        program,
        fb,
        (64, 32),
        texture,
        false,
        &[quad([60, 30, 4, 2], [0, 0, 1, 1])],
        Blend::Replace,
        None,
    );
    let fence = gl.submit().unwrap();
    gl.wait(fence).unwrap();
    gl.check().unwrap();
    let mut out = vec![0u8; 64 * 32 * 4];
    gl.read(fb, rect(0, 0, 64, 32), &mut out).unwrap();
    assert_eq!(out[..4], [0x12, 0x34, 0x56, 0xFF]);
    assert_eq!(out[(29 * 64 + 63) * 4..][..4], [0x12, 0x34, 0x56, 0xFF]);
    assert_eq!(out[(30 * 64 + 60) * 4..][..4], [0xAB, 0xCD, 0xEF, 0xFF]);
    assert_eq!(out[(31 * 64 + 63) * 4..][..4], [0xAB, 0xCD, 0xEF, 0xFF]);
    gl.free_buffer(fb).unwrap();
    assert_eq!(gl.free_buffer(fb).unwrap_err().kind, ErrorKind::Misuse);
    gl.free_texture(texture).unwrap();
    gl.free_program(program).unwrap();
    gl.check().unwrap();
    gl.create_buffer(8, 8).unwrap();
    drop(gl);
}

// the issue on the EGL sync before the flip: a frame ends with a fence
// after its commands, an EGL fence sync on llvmpipe, which a wait takes
// once, in any order; a name no submit gave, or one a wait took, is
// refused before it reaches EGL
#[test]
fn a_frame_ends_with_a_fence_sync_that_a_wait_takes_once() {
    let mut gl = software();
    assert!(gl.fence_syncs(), "llvmpipe's EGL makes fence syncs");
    let (fb, texture) = gl.create_frame(8, 8).unwrap();
    gl.clear(fb, (8, 8), rect(0, 0, 8, 8), 0x20_4060).unwrap();
    let first = gl.submit().unwrap();
    gl.clear(fb, (8, 8), rect(0, 0, 4, 4), 0x80_A0C0).unwrap();
    let second = gl.submit().unwrap();
    assert_ne!(first, second);
    gl.wait(second).unwrap();
    gl.wait(first).unwrap();
    for name in [first, second, 0, second + 1] {
        assert_eq!(gl.wait(name).unwrap_err().kind, ErrorKind::Misuse, "{name}");
    }
    gl.check().unwrap();
    let mut out = vec![0u8; 8 * 8 * 4];
    gl.read(fb, rect(0, 0, 8, 8), &mut out).unwrap();
    assert_eq!(out[..4], [0x80, 0xA0, 0xC0, 0xFF]);
    assert_eq!(out[(7 * 8 + 7) * 4..][..4], [0x20, 0x40, 0x60, 0xFF]);
    gl.free_frame(fb, texture).unwrap();
    // every fence a wait took is destroyed
    let live = gl.live_syncs();
    assert_eq!(live.get(), 0);
    // so is one whose wait failed before it reached EGL
    let fence = gl.submit().unwrap();
    assert_eq!(live.get(), 1);
    gl.raise(ErrorKind::ContextLost);
    assert_eq!(gl.wait(fence).unwrap_err().kind, ErrorKind::ContextLost);
    assert_eq!(live.get(), 0);
    // and one no wait took goes with its context
    gl.submit().unwrap();
    assert_eq!((live.get(), gl.pending_fences()), (1, 1));
    drop(gl);
    assert_eq!(live.get(), 0);
}

// ADR-1303 D9: a GPU that does not pass the frame's fence within
// GPU_FENCE_WAIT_MS, or that was reset during the wait, fails the wait as
// a lost context, and the fence is destroyed either way
#[test]
fn a_wait_that_times_out_or_finds_a_reset_is_a_lost_context() {
    let mut gl = software();
    assert!(gl.fence_syncs(), "llvmpipe's EGL makes fence syncs");
    let live = gl.live_syncs();
    let (fb, texture) = gl.create_frame(8, 8).unwrap();
    for force in [Context::time_out_next_wait, Context::reset_in_next_wait] {
        gl.clear(fb, (8, 8), rect(0, 0, 8, 8), 0x20_4060).unwrap();
        let fence = gl.submit().unwrap();
        force(&mut gl);
        let e = gl.wait(fence).unwrap_err();
        assert_eq!(e.kind, ErrorKind::ContextLost, "{e}");
        assert_eq!((live.get(), gl.pending_fences()), (0, 0), "{e}");
        // the next wait is a plain one
        let fence = gl.submit().unwrap();
        gl.wait(fence).unwrap();
    }
    gl.free_frame(fb, texture).unwrap();
}

// A fault a test sets for the next wait ends with that wait, whichever way
// the wait returns: a wait that cannot make the context current leaves no
// fault armed for the next frame's wait
#[test]
fn a_wait_fault_ends_with_the_wait_that_fails_first() {
    let mut gl = software();
    let (fb, texture) = gl.create_frame(8, 8).unwrap();
    gl.clear(fb, (8, 8), rect(0, 0, 8, 8), 0x20_4060).unwrap();
    let fence = gl.submit().unwrap();
    gl.time_out_next_wait();
    gl.raise(ErrorKind::Gl(0x0502));
    let e = gl.wait(fence).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Gl(0x0502), "{e}");
    gl.clear(fb, (8, 8), rect(0, 0, 8, 8), 0x20_4060).unwrap();
    let fence = gl.submit().unwrap();
    gl.wait(fence).unwrap();
    gl.free_frame(fb, texture).unwrap();
}

// Where the display or GL makes no fence sync, a frame ends with glFinish:
// the submit still gives a name, a wait takes it once, and the frame is
// done
#[test]
fn without_fence_syncs_a_frame_ends_with_glfinish_and_its_name() {
    let mut gl = software();
    gl.force_finish();
    let live = gl.live_syncs();
    let (fb, texture) = gl.create_frame(4, 4).unwrap();
    gl.clear(fb, (4, 4), rect(0, 0, 4, 4), 0x10_2030).unwrap();
    let fence = gl.submit().unwrap();
    assert_eq!((live.get(), gl.pending_fences()), (0, 1));
    gl.wait(fence).unwrap();
    assert_eq!(gl.wait(fence).unwrap_err().kind, ErrorKind::Misuse);
    assert_eq!(gl.pending_fences(), 0);
    let mut out = vec![0u8; 4 * 4 * 4];
    gl.read(fb, rect(0, 0, 4, 4), &mut out).unwrap();
    assert!(out.chunks(4).all(|p| p == [0x10, 0x20, 0x30, 0xFF]));
    gl.free_frame(fb, texture).unwrap();
    gl.check().unwrap();
}
