#version 300 es
// Present (ADR-1303 D6): each pixel of the display's buffer reads its texel
// of the upright frame's tile by integer coordinates, with no filtering,
// turned as the CPU path turns a pixel. The spatial and color steps that
// are switched on are joined in between this head and present_main.frag,
// each defining its macro and its function; an empty slot joins nothing,
// so it adds no instruction.
precision highp float;
precision highp int;

uniform highp sampler2D frame;

// The quad, from cell.vert: dst the display's rectangle it draws, src the
// tile's texel under the upright top left of that rectangle, and params.x
// the turn: 0 upright, 1 a quarter clockwise, 2 a half, 3 a quarter
// anticlockwise.
flat in ivec4 dst;
flat in uvec4 src;
flat in uvec4 params;

out vec4 color;

// The tile's texel the display pixel p draws. A display turned a quarter
// clockwise draws the upright pixel x, y at width - 1 - y, x, so within the
// quad the pixel d reads the upright pixel d.y, w - 1 - d.x.
ivec2 texel(ivec2 p) {
    ivec2 d = p - dst.xy;
    ivec2 last = dst.zw - 1;
    ivec2 u;
    if (params.x == 1u) {
        u = ivec2(d.y, last.x - d.x);
    } else if (params.x == 2u) {
        u = last - d;
    } else if (params.x == 3u) {
        u = ivec2(last.y - d.y, d.x);
    } else {
        u = d;
    }
    return ivec2(src.xy) + u;
}
