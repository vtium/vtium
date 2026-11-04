#version 300 es
// The selection's tint over a cell that pictures reach (ADR-1100 D4): the
// cell's pixels as the frame composed them, from a copy of them, and the
// selection's color over each one at its strength, with the CPU path's
// arithmetic (blend.rs's mask): each channel (c * t + p * (255 - t)) / 255,
// rounded. The GPU's blending would round the selection's share first, so
// the pass reads the pixels beneath from `under`, a copy, since no draw
// reads the target it draws into. The same pass with no tint makes that
// copy.
//
// A quad is one cell: dst where it draws, src the texel of `under` under
// its top left, params.x the selection's color, 0xRRGGBB, with 1 in its top
// byte to tint and 0 to copy, and params.y the tint's strength out of 255.
precision highp float;
precision highp int;

uniform highp sampler2D under;

flat in ivec4 dst;
flat in uvec4 src;
flat in uvec4 params;

out vec4 color;

// t / 255 rounded, as blend.rs's div255
vec3 div255(vec3 t) {
    vec3 u = t + 128.0;
    return floor((u + floor(u / 256.0)) / 256.0);
}

void main() {
    ivec2 at = ivec2(src.xy) + ivec2(gl_FragCoord.xy) - dst.xy;
    vec3 p = floor(texelFetch(under, at, 0).rgb * 255.0 + 0.5);
    if ((params.x >> 24u) == 1u) {
        uint c = params.x;
        vec3 sel = vec3(float((c >> 16u) & 255u), float((c >> 8u) & 255u), float(c & 255u));
        float t = float(params.y);
        p = div255(sel * t + p * (255.0 - t));
    }
    color = vec4((p + 0.25) / 255.0, 1.0);
}
