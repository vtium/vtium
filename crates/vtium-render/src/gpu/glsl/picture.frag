// The program's source puts `#version 300 es` before this file, and for
// a picture that is not opaque `#define OVER` after it.
//
// A picture drawn at another size (ADR-1303 D2), with the CPU path's own
// filter (src/picture/scale.rs) in whole numbers: a box filter along an
// axis where the picture shrinks, a bilinear blend where it grows, and the
// source pixel itself where it keeps its size. Each weight is 16-bit fixed
// point, each axis's weights sum to 65,536, and each output channel is
// rounded once, so the pixel is the CPU path's byte for byte. The sum of a
// channel reaches 2^40, past a 32-bit integer, so it is kept in two parts.
//
// The texture holds the source, or the tile of it, from the source pixel
// `origin.zw` on: the picture's own pixels, or the CPU path's copy at the
// drawn size, which keeps its size and is read one texel a pixel.
//
// A quad draws part of the picture: dst where. An opaque picture
// replaces what lies beneath it. A picture that is not opaque goes over
// the pixels beneath it with the CPU path's over_px (src/picture/mod.rs)
// in whole numbers, reading them from `under`, a copy of them, since no
// draw reads the target it draws into. The GPU's own blending would turn
// its float result into a byte as the driver chooses, rounding or
// truncating, so the shader does the blend itself and writes every pixel
// as an opaque one.
precision highp float;
precision highp int;

uniform highp sampler2D picture;
// x, y: the source's width and height; z, w: the size it is drawn at
uniform uvec4 scale;
// x, y: the drawn picture's top left in the target; z, w: the source
// pixel the texture's first texel holds
uniform ivec4 origin;
#ifdef OVER
// the pixels beneath, copied, and x, y: the target's pixel the copy's
// first texel holds
uniform highp sampler2D under;
uniform ivec4 under_at;
#endif

flat in ivec4 dst;
flat in uvec4 src;
flat in uvec4 params;

out vec4 color;

const uint ONE = 65536u;

// One axis: output o of s source pixels drawn as d reads `count` source
// pixels from `first`. For the box, `big` is the heaviest weight's place,
// which takes the rounding left over, `left`; for the blend, `left` is the
// fraction of the second pixel.
struct Axis {
    uint s;
    uint d;
    uint o;
    uint first;
    uint count;
    uint big;
    uint left;
};

// The box's weight of source pixel `i` for output o: how much of it the
// output's span covers, out of ONE.
uint box_weight(Axis a, uint i) {
    uint lo = a.o * a.s;
    uint hi = lo + a.s;
    uint p0 = i * a.d;
    uint p1 = p0 + a.d;
    uint ov = min(hi, p1) - max(lo, p0);
    return ov * ONE / a.s;
}

Axis axis(uint s, uint d, uint o) {
    Axis a = Axis(s, d, o, o, 1u, 0u, 0u);
    if (s > d) {
        uint lo = o * s;
        uint hi = lo + s;
        a.first = lo / d;
        uint sum = 0u;
        uint heaviest = 0u;
        uint n = 0u;
        for (uint i = a.first; i < s && i * d < hi; i++) {
            uint v = box_weight(a, i);
            sum += v;
            if (v > heaviest) {
                heaviest = v;
                a.big = n;
            }
            n++;
        }
        a.count = n;
        a.left = ONE - sum;
    } else if (s < d) {
        // the output pixel's centre in source pixels, out of ONE:
        // (2o + 1) * s * ONE / (2d), taken apart so no step passes 2^32
        uint num = (2u * o + 1u) * s;
        uint q = num / (2u * d);
        uint r = num % (2u * d);
        uint centre = q * ONE + r * ONE / (2u * d);
        uint p = centre >= ONE / 2u ? centre - ONE / 2u : 0u;
        uint i = p >> 16;
        uint f = p & 0xffffu;
        if (i >= s - 1u) {
            i = s - 1u;
            f = 0u;
        }
        a.first = i;
        a.left = f;
        a.count = f == 0u ? 1u : 2u;
    }
    return a;
}

// The weight of the k-th source pixel output a.o reads.
uint weight(Axis a, uint k) {
    if (a.s > a.d) {
        uint v = box_weight(a, a.first + k);
        return k == a.big ? v + a.left : v;
    }
    if (a.s < a.d) {
        return k == 0u ? ONE - a.left : a.left;
    }
    return ONE;
}

// Source pixel (x, y)'s bytes, red, green, blue and alpha.
uvec4 source(uint x, uint y) {
    ivec2 at = ivec2(int(x) - origin.z, int(y) - origin.w);
    return uvec4(floor(texelFetch(picture, at, 0) * 255.0 + 0.5));
}

void main() {
    ivec2 px = ivec2(gl_FragCoord.xy) - origin.xy;
    Axis ax = axis(scale.x, scale.z, uint(px.x));
    Axis ay = axis(scale.y, scale.w, uint(px.y));
    // each channel's sum of weights times bytes, as lo + hi * 2^16
    uvec4 lo = uvec4(0u);
    uvec4 hi = uvec4(0u);
    for (uint ky = 0u; ky < ay.count; ky++) {
        uint wy = weight(ay, ky);
        uvec4 h = uvec4(0u);
        for (uint kx = 0u; kx < ax.count; kx++) {
            h += weight(ax, kx) * source(ax.first + kx, ay.first + ky);
        }
        lo += wy * (h & 0xffffu);
        hi += wy * (h >> 16);
    }
    // (sum + 2^31) >> 32, rounded once, as scale.rs's round
    uvec4 c = min((hi + (lo >> 16) + 32768u) >> 16, uvec4(255u));
    uvec3 out_rgb = c.rgb;
#ifdef OVER
    // over_px: the premultiplied c over the opaque pixel d,
    // each channel c + div255(d * (255 - a)), at most 255
    ivec2 at = ivec2(gl_FragCoord.xy) - under_at.xy;
    uvec3 d = uvec3(floor(texelFetch(under, at, 0).rgb * 255.0 + 0.5));
    uvec3 t = d * (255u - c.a) + 128u;
    t = (t + (t >> 8)) >> 8;
    out_rgb = c.a == 0u ? d : min(c.rgb + t, uvec3(255u));
#endif
    // each channel v written as (v + 0.25) / 255, which an 8-bit target
    // stores as v whether the GPU rounds or truncates
    color = vec4((vec3(out_rgb) + 0.25) / 255.0, 1.0);
}
