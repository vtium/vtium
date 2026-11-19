//! The shaders the harness times, and the fragment shaders it builds from
//! them.
//!
//! Each shader is a Ghostty-style file: a `mainImage(out vec4, in vec2)`
//! that reads the frame through `texture(iChannel0, uv)`. A header of
//! `// timing:` lines states the shader's reads for each pixel, the work it
//! does besides reading, and where the file comes from.
//!
//! The harness rewrites each read of `iChannel0` to a function of its own,
//! whose body depends on the read mode. That lets one file be timed with
//! each way of reading the frame that vtium has weighed: a plain read, a
//! smooth read blended in the shader or by the GPU's own filter, and a
//! smooth read in linear light.
//!
//! References:
//! - ADR-1304 D18: the list of shaders the measurement runs, and the smooth
//!   read, which uses the GPU's own linear filter since the maintainer's
//!   decision of 2026-11-18. The harness also times the blend in the
//!   shader, which the record proposed before that decision.
//! - ADR-1304 D20: a Ghostty shader's `texture` read is a smooth read in
//!   linear light, and its output is encoded before the next pass reads it.
//! - ADR-1304 D3: a filter that reads pixels other than its own runs as a
//!   pass of its own. The harness also times crt then bloom sharing one
//!   pass, where each read of bloom runs crt again, to price that choice.

/// One shader file and what its header states.
#[derive(Clone, Debug, PartialEq)]
pub struct Shader {
    pub name: &'static str,
    pub text: &'static str,
    /// Reads of the frame for each pixel, as the header states them.
    pub reads: u32,
    /// The work the shader does besides reading, in a phrase.
    pub work: String,
    /// Where the file comes from and its license.
    pub origin: String,
}

/// The shaders, from cheap to expensive, in the order ADR-1304 D18 lists.
const FILES: [(&str, &str); 16] = [
    ("negative", include_str!("../shaders/negative.glsl")),
    ("tft", include_str!("../shaders/tft.glsl")),
    ("dither", include_str!("../shaders/dither.glsl")),
    ("cubes", include_str!("../shaders/cubes.glsl")),
    ("galaxy", include_str!("../shaders/galaxy.glsl")),
    (
        "fireworks-rockets",
        include_str!("../shaders/fireworks-rockets.glsl"),
    ),
    ("just-snow", include_str!("../shaders/just-snow.glsl")),
    (
        "inside-the-matrix",
        include_str!("../shaders/inside-the-matrix.glsl"),
    ),
    ("drunkard", include_str!("../shaders/drunkard.glsl")),
    ("glitchy", include_str!("../shaders/glitchy.glsl")),
    ("vhs", include_str!("../shaders/vhs.glsl")),
    ("crt", include_str!("../shaders/crt.glsl")),
    ("bloom", include_str!("../shaders/bloom.glsl")),
    (
        "glow-rgbsplit-twitchy",
        include_str!("../shaders/glow-rgbsplit-twitchy.glsl"),
    ),
    ("in-game-crt", include_str!("../shaders/in-game-crt.glsl")),
    (
        "smoke-and-ghost",
        include_str!("../shaders/smoke-and-ghost.glsl"),
    ),
];

/// Every shader the harness carries, parsed.
pub fn catalog() -> Result<Vec<Shader>, String> {
    FILES
        .iter()
        .map(|&(name, text)| parse(name, text))
        .collect()
}

/// Reads a shader's `// timing:` header.
pub fn parse(name: &'static str, text: &'static str) -> Result<Shader, String> {
    let mut reads = None;
    let mut work = None;
    let mut origin = None;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("// timing:") else {
            continue;
        };
        let (key, value) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
        let value = value.trim();
        match key {
            "reads" => {
                reads = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("{name}: reads is not a whole number: {value}"))?,
                )
            }
            "work" => work = Some(value.to_owned()),
            "origin" => origin = Some(value.to_owned()),
            other => return Err(format!("{name}: unknown timing key {other}")),
        }
    }
    let missing = |key: &str| format!("{name}: the header has no `// timing: {key}` line");
    if !text.contains("mainImage") {
        return Err(format!("{name}: the file defines no mainImage"));
    }
    Ok(Shader {
        name,
        text,
        reads: reads.ok_or_else(|| missing("reads"))?,
        work: work.ok_or_else(|| missing("work"))?,
        origin: origin.ok_or_else(|| missing("origin"))?,
    })
}

/// How a read of `iChannel0` reads the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One `texelFetch` of the pixel whose square holds the place: vtium's
    /// plain read, `SPATIAL_IN`.
    Plain,
    /// Four `texelFetch`es blended in the shader in encoded color: the
    /// smooth read as ADR-1304 D18 proposed it before 2026-11-18.
    Smooth,
    /// The smooth read blended in the shader, with each pixel decoded to
    /// linear light first and the output encoded again: Ghostty's dialect
    /// as ADR-1304 D20 proposed it before 2026-11-18.
    Ghostty,
    /// GL's own linear filtering through `texture`, as Ghostty reads and
    /// as vtium's `SPATIAL_SMOOTH` reads.
    GlLinear,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Plain, Mode::Smooth, Mode::Ghostty, Mode::GlLinear];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Plain => "plain",
            Mode::Smooth => "smooth",
            Mode::Ghostty => "ghostty",
            Mode::GlLinear => "gl-linear",
        }
    }

    pub fn parse(text: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == text)
    }

    /// Whether the frame's texture is sampled with GL_LINEAR.
    pub fn linear_sampler(self) -> bool {
        self == Mode::GlLinear
    }
}

/// The uniforms Ghostty declares, as plain uniforms. The harness sets the
/// size, time and frame count; the rest stay 0.
const HEAD: &str = r"#version 300 es
precision highp float;
precision highp int;
precision highp sampler2D;
uniform vec3 iResolution;
uniform float iTime;
uniform float iTimeDelta;
uniform float iFrameRate;
uniform int iFrame;
uniform float iChannelTime[4];
uniform vec3 iChannelResolution[4];
uniform vec4 iMouse;
uniform vec4 iDate;
uniform float iSampleRate;
uniform vec4 iCurrentCursor;
uniform vec4 iPreviousCursor;
uniform vec4 iCurrentCursorColor;
uniform vec4 iPreviousCursorColor;
uniform int iCurrentCursorStyle;
uniform int iPreviousCursorStyle;
uniform int iCursorVisible;
uniform float iTimeCursorChange;
uniform float iTimeFocus;
uniform int iFocus;
uniform sampler2D iChannel0;
// The reach clamp's half width in pixels: the cap on reach.
uniform float vt_reach;
out vec4 vt_out;

vec3 vtToLinear(vec3 c) {
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}
vec3 vtToEncoded(vec3 c) {
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}
// A place in the upright frame, from Ghostty's uv, held within the reach
// of the pixel being written.
vec2 vt_place(vec2 uv) {
    vec2 at = vec2(uv.x, 1.0 - uv.y) * iResolution.xy;
    vec2 here = vec2(gl_FragCoord.x, iResolution.y - gl_FragCoord.y);
    return clamp(at, here - vt_reach, here + vt_reach);
}
vec4 vt_texel(sampler2D s, ivec2 p) {
    return texelFetch(s, clamp(p, ivec2(0), ivec2(iResolution.xy) - 1), 0);
}
";

/// The body of `vt_read` for each mode.
fn read_function(mode: Mode) -> &'static str {
    match mode {
        Mode::Plain => {
            r"vec4 vt_read(sampler2D s, vec2 uv) {
    return vt_texel(s, ivec2(floor(vt_place(uv))));
}
"
        }
        Mode::Smooth => {
            r"vec4 vt_read(sampler2D s, vec2 uv) {
    vec2 at = vt_place(uv) - 0.5;
    ivec2 p = ivec2(floor(at));
    vec2 f = at - floor(at);
    vec4 a = vt_texel(s, p);
    vec4 b = vt_texel(s, p + ivec2(1, 0));
    vec4 c = vt_texel(s, p + ivec2(0, 1));
    vec4 d = vt_texel(s, p + ivec2(1, 1));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
"
        }
        Mode::Ghostty => {
            r"vec4 vt_read(sampler2D s, vec2 uv) {
    vec2 at = vt_place(uv) - 0.5;
    ivec2 p = ivec2(floor(at));
    vec2 f = at - floor(at);
    vec4 a = vt_texel(s, p);
    vec4 b = vt_texel(s, p + ivec2(1, 0));
    vec4 c = vt_texel(s, p + ivec2(0, 1));
    vec4 d = vt_texel(s, p + ivec2(1, 1));
    a.rgb = vtToLinear(a.rgb);
    b.rgb = vtToLinear(b.rgb);
    c.rgb = vtToLinear(c.rgb);
    d.rgb = vtToLinear(d.rgb);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
"
        }
        Mode::GlLinear => {
            r"vec4 vt_read(sampler2D s, vec2 uv) {
    // the texture's first row is the frame's top, as the upright place
    // counts it
    return texture(s, vt_place(uv) / iResolution.xy);
}
"
        }
    }
}

/// Replaces each `texture(iChannel0` and `texture2D(iChannel0`, spaces
/// allowed before and after the parenthesis, with `name(iChannel0`, and
/// counts the call sites it replaced.
pub fn rewrite_reads(text: &str, name: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut count = 0;
    while let Some(i) = rest.find("texture") {
        let before_ok = rest[..i]
            .chars()
            .next_back()
            .or_else(|| out.chars().next_back())
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        let after = &rest[i + "texture".len()..];
        let after = after.strip_prefix("2D").unwrap_or(after);
        let tail = after.trim_start();
        let call = tail
            .strip_prefix('(')
            .map(str::trim_start)
            .and_then(|t| t.strip_prefix("iChannel0"))
            .filter(|t| !t.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'));
        match (before_ok, call) {
            (true, Some(t)) => {
                out.push_str(&rest[..i]);
                out.push_str(name);
                out.push_str("(iChannel0");
                rest = t;
                count += 1;
            }
            _ => {
                out.push_str(&rest[..i + "texture".len()]);
                rest = &rest[i + "texture".len()..];
            }
        }
    }
    out.push_str(rest);
    (out, count)
}

/// Replaces the identifier `from` with `to` wherever it stands as a whole
/// word.
fn rename(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(from) {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let before = rest[..i]
            .chars()
            .next_back()
            .or_else(|| out.chars().next_back());
        let after = rest[i + from.len()..].chars().next();
        out.push_str(&rest[..i]);
        if before.is_some_and(word) || after.is_some_and(word) {
            out.push_str(from);
        } else {
            out.push_str(to);
        }
        rest = &rest[i + from.len()..];
    }
    out.push_str(rest);
    out
}

/// `main`, which calls the last filter's `mainImage` with Ghostty's
/// `fragCoord` and writes its color, encoded under Ghostty's dialect.
fn main_function(mode: Mode, image: &str) -> String {
    let encode = if mode == Mode::Ghostty {
        "    c.rgb = vtToEncoded(clamp(c.rgb, 0.0, 1.0));\n"
    } else {
        ""
    };
    format!(
        "void main() {{\n    vec4 c = vec4(0.0);\n    {image}(c, gl_FragCoord.xy);\n{encode}    vt_out = c;\n}}\n"
    )
}

/// One shader as a whole fragment shader, its reads in `mode`.
pub fn single(shader: &Shader, mode: Mode) -> Result<String, String> {
    let (body, sites) = rewrite_reads(shader.text, "vt_read");
    if sites == 0 {
        return Err(format!("{}: the file never reads iChannel0", shader.name));
    }
    Ok(format!(
        "{HEAD}{}#line 1\n{body}\n{}",
        read_function(mode),
        main_function(mode, "mainImage")
    ))
}

/// Two shaders chained in one pass: each read the second makes runs the
/// first at that place. The first reads the frame in `mode`.
pub fn chain(first: &Shader, second: &Shader, mode: Mode) -> Result<String, String> {
    let (a, sites_a) = rewrite_reads(first.text, "vt_read");
    let (b, sites_b) = rewrite_reads(second.text, "vt_read_first");
    if sites_a == 0 || sites_b == 0 {
        return Err(format!(
            "{} then {}: a file never reads iChannel0",
            first.name, second.name
        ));
    }
    let a = rename(&a, "mainImage", "vt_first_image");
    // Under Ghostty's dialect each filter hands the next encoded color,
    // and the next decodes what it reads.
    let hand_over = if mode == Mode::Ghostty {
        "    c.rgb = vtToLinear(vtToEncoded(clamp(c.rgb, 0.0, 1.0)));\n"
    } else {
        ""
    };
    // A smooth read of the first filter's output runs the first filter at
    // the four pixel centers around the place, and blends them.
    let read = match mode {
        Mode::Plain | Mode::GlLinear => "    return vt_first_at(floor(vt_place(uv)) + 0.5);\n",
        Mode::Smooth | Mode::Ghostty => {
            "    vec2 at = vt_place(uv) - 0.5;\n    vec2 p = floor(at);\n    vec2 f = at - p;\n    \
             vec4 a = vt_first_at(p + vec2(0.5, 0.5));\n    vec4 b = vt_first_at(p + vec2(1.5, 0.5));\n    \
             vec4 c = vt_first_at(p + vec2(0.5, 1.5));\n    vec4 d = vt_first_at(p + vec2(1.5, 1.5));\n    \
             return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);\n"
        }
    };
    let between = format!(
        "\nvec4 vt_first_at(vec2 at) {{\n    vec4 c = vec4(0.0);\n    vt_first_image(c, vec2(at.x, iResolution.y - at.y));\n{hand_over}    return c;\n}}\n\
         vec4 vt_read_first(sampler2D s, vec2 uv) {{\n{read}}}\n"
    );
    Ok(format!(
        "{HEAD}{}#line 1\n{a}\n{between}#line 1\n{b}\n{}",
        read_function(mode),
        main_function(mode, "mainImage")
    ))
}

/// The vertex shader: one triangle that covers the target.
pub const VERTEX: &str = r"#version 300 es
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_replaces_reads_of_the_channel_only() {
        let text = "a = texture(iChannel0, uv); b = texture2D( iChannel0 ,uv); \
                    c = texture(iChannel1, uv); d = mytexture(iChannel0, uv); \
                    e = texture(iChannel0x, uv); f = textureSize(iChannel0, 0);";
        let (out, count) = rewrite_reads(text, "vt_read");
        assert_eq!(count, 2);
        assert_eq!(
            out,
            "a = vt_read(iChannel0, uv); b = vt_read(iChannel0 ,uv); \
             c = texture(iChannel1, uv); d = mytexture(iChannel0, uv); \
             e = texture(iChannel0x, uv); f = textureSize(iChannel0, 0);"
        );
    }

    #[test]
    fn rename_touches_whole_words_only() {
        let out = rename(
            "void mainImage(); mainImage2(); x_mainImage(); mainImage();",
            "mainImage",
            "f",
        );
        assert_eq!(out, "void f(); mainImage2(); x_mainImage(); f();");
    }

    #[test]
    fn every_shader_has_a_header_and_reads_the_frame() {
        let shaders = catalog().unwrap();
        assert_eq!(shaders.len(), FILES.len());
        for s in &shaders {
            assert!(s.reads >= 1, "{}", s.name);
            for mode in Mode::ALL {
                single(s, mode).unwrap();
            }
        }
    }

    #[test]
    fn a_header_without_its_reads_is_refused() {
        let e = parse(
            "x",
            "// timing: work w\n// timing: origin o\nvoid mainImage(out vec4 c, in vec2 p) {}",
        )
        .unwrap_err();
        assert!(e.contains("reads"), "{e}");
        let e = parse("x", "// timing: reads many\n").unwrap_err();
        assert!(e.contains("whole number"), "{e}");
    }

    #[test]
    fn a_chain_routes_the_second_filters_reads_through_the_first() {
        let shaders = catalog().unwrap();
        let crt = shaders.iter().find(|s| s.name == "crt").unwrap();
        let bloom = shaders.iter().find(|s| s.name == "bloom").unwrap();
        let text = chain(crt, bloom, Mode::Plain).unwrap();
        assert!(text.contains("vt_first_image"));
        assert!(text.contains("vt_read_first(iChannel0"));
        assert_eq!(text.matches("void mainImage").count(), 1);
    }
}
