#version 300 es
// A cell (ADR-1300 D6), with the CPU path's integer arithmetic
// (src/blend.rs): every value a whole number held in a highp float, which
// holds 255 * 255 exactly; div255 with its shifts written as floors of
// divisions by 256; texels read whole, with no filtering; no dithering;
// and each channel c written as (c + 0.25) / 255, which an 8-bit target
// stores as c whether the GPU rounds or truncates.
//
// A quad is one cell job: dst where it draws, src the glyph's texel under
// its top left, and params its colors, 0xRRGGBB, each with a kind in its
// top byte: x the foreground and the glyph's kind (0 none, 1 a mask from
// masks, 2 a premultiplied color glyph from colors), y the background, z
// the underline's color and its sprite, w the strike's color and its
// sprite, a sprite being its place in sprites plus one, or 0 for none.
precision highp float;
precision highp int;

uniform highp sampler2D masks;
uniform highp sampler2D colors;
uniform highp sampler2D sprites;
// the cell's width and height
uniform ivec4 cell;

flat in ivec4 dst;
flat in uvec4 src;
flat in uvec4 params;

out vec4 color;

// t / 255 rounded, as blend.rs's div255: (t + 128 + ((t + 128) >> 8)) >> 8
vec3 div255(vec3 t) {
    vec3 u = t + 128.0;
    return floor((u + floor(u / 256.0)) / 256.0);
}

vec4 div255(vec4 t) {
    vec4 u = t + 128.0;
    return floor((u + floor(u / 256.0)) / 256.0);
}

// a normalized texel's channels as the bytes it holds
float byte(float v) {
    return floor(v * 255.0 + 0.5);
}

vec3 rgb(uint c) {
    return vec3(float((c >> 16u) & 255u), float((c >> 8u) & 255u), float(c & 255u));
}

// the premultiplied s over the premultiplied p, rgb then alpha: blend.rs's over
vec4 over(vec4 p, vec4 s) {
    return s + div255(p * (255.0 - s.a));
}

// coverage a tinted c over p: blend.rs's tint_over
vec4 tint_over(vec4 p, float a, vec3 c) {
    if (a == 0.0) {
        return p;
    }
    return over(p, vec4(div255(c * a), a));
}

// a sprite's coverage at pos within the cell
float sprite(uint k, ivec2 pos) {
    ivec2 at = ivec2(int(k - 1u) * cell.x + pos.x, pos.y);
    return byte(texelFetch(sprites, at, 0).r);
}

void main() {
    ivec2 local = ivec2(gl_FragCoord.xy) - dst.xy;
    ivec2 at = ivec2(src.xy) + local;
    uint kind = params.x >> 24u;
    uint ul = params.z >> 24u;
    uint st = params.w >> 24u;
    vec3 fg = rgb(params.x);
    vec3 bg = rgb(params.y);
    vec3 c;
    if (ul == 0u && st == 0u) {
        if (kind == 1u) {
            // blend.rs's mask: fg over bg through the coverage
            float a = byte(texelFetch(masks, at, 0).r);
            c = div255(fg * a + bg * (255.0 - a));
        } else if (kind == 2u) {
            // blend.rs's over_bg
            vec4 s = floor(texelFetch(colors, at, 0) * 255.0 + 0.5);
            c = s.a == 0.0 ? bg : s.rgb + div255(bg * (255.0 - s.a));
        } else {
            c = bg;
        }
    } else {
        // a cell of several layers: its text layer premultiplied, from
        // the bottom, then over its background
        vec4 p = vec4(0.0);
        ivec2 pos = ivec2(local.x % cell.x, local.y);
        if (ul != 0u) {
            p = tint_over(p, sprite(ul, pos), rgb(params.z));
        }
        if (kind == 1u) {
            p = tint_over(p, byte(texelFetch(masks, at, 0).r), fg);
        } else if (kind == 2u) {
            vec4 s = floor(texelFetch(colors, at, 0) * 255.0 + 0.5);
            if (s.a != 0.0) {
                p = over(p, s);
            }
        }
        if (st != 0u) {
            p = tint_over(p, sprite(st, pos), rgb(params.w));
        }
        c = p.a == 0.0 ? bg : p.rgb + div255(bg * (255.0 - p.a));
    }
    color = vec4((c + 0.25) / 255.0, 1.0);
}
