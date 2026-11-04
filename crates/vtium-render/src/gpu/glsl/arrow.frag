#version 300 es
// The pointer's arrow (ADR-1303 D3): its premultiplied texel over the
// pixel beneath it, with the CPU path's arithmetic (src/blend.rs's
// over_bg), each channel s + div255(d * (255 - a)), where s is the arrow's
// channel, a its alpha and d the pixel beneath. No draw reads the target
// it draws into, so the pixels beneath are read from `under`, a copy of
// them. The GPU's own blending would turn its float result into a byte as
// the driver chooses, rounding or truncating, so the shader blends itself
// and writes each channel c as (c + 0.25) / 255, which an 8-bit target
// stores as c either way.
precision highp float;
precision highp int;

uniform highp sampler2D arrow;
// the pixels beneath, copied, and x, y: the target's pixel the copy's
// first texel holds
uniform highp sampler2D under;
uniform ivec4 under_at;

in vec2 src_at;

out vec4 color;

// t / 255 rounded, as blend.rs's div255
vec3 div255(vec3 t) {
    vec3 u = t + 128.0;
    return floor((u + floor(u / 256.0)) / 256.0);
}

void main() {
    vec4 s = floor(texelFetch(arrow, ivec2(src_at), 0) * 255.0 + 0.5);
    ivec2 at = ivec2(gl_FragCoord.xy) - under_at.xy;
    vec3 d = floor(texelFetch(under, at, 0).rgb * 255.0 + 0.5);
    vec3 c = s.a == 0.0 ? d : s.rgb + div255(d * (255.0 - s.a));
    color = vec4((c + 0.25) / 255.0, 1.0);
}
