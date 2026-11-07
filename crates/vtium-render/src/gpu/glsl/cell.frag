// The program's source puts `#version 300 es` before this file, then for
// a cell's text layer or vtium's mark alone `#define OVER`, for the cells
// that receive ink from their neighbours' glyphs and for the text layers
// `#define INK`, and then `#define REACH_SIDE` and `#define REACH_ROWS`,
// how many cells a glyph's ink may reach past its own to each side and
// above and below (ADR-1306 D12), which bound the loops over a cell's
// neighbours. A cell that receives no ink is drawn by the program built
// without INK, which is the cell shader as it was before any ink reached
// another cell, so it costs what it did.
//
// A cell (ADR-1300 D6), with the CPU path's integer arithmetic
// (src/blend.rs): every value a whole number held in a highp float, which
// holds 255 * 255 exactly; div255 with its shifts written as floors of
// divisions by 256; texels read whole, with no filtering; no dithering;
// and each channel c written as (c + 0.25) / 255, which an 8-bit target
// stores as c whether the GPU rounds or truncates.
//
// A quad is one cell job: dst where it draws; src.xy the glyph's texel
// under its top left and src.zw the column and row of the job's first
// cell in the grid; and params its colors, 0xRRGGBB, each with a kind in
// its top byte: x the foreground and the glyph's kind (0 none, 1 a mask
// from masks, 2 a premultiplied color glyph from colors), y the
// background and the job's flags (RECEIVES, MARKED and DOWN below), z the
// underline's color and its sprite, w the color and sprite of the lines
// over the glyph (the strike, the overline or both),
// a sprite being its place in sprites plus one, or 0 for none.
//
// With INK, a job that RECEIVES ink from its neighbours' glyphs (ADR-1306
// D4, D9) reads, for each neighbour its glyph's reach could come from, that
// neighbour's entry in the ink grid: the reach byte the CPU path keeps for
// the cell, its span, the slot of its glyph among the boxes, and its ink's
// color. A neighbour whose reach covers the pixel lends the coverage of
// its glyph's box there, tinted, and the received ink is blended, in
// D4's order, into one premultiplied layer, as compose.rs's receive does.
//
// Without OVER the quad writes the cell, its text layer over its
// background. With OVER it writes the cell's text layer over the pixels
// beneath it, which pictures drew, with the CPU path's over_px
// (src/picture/mod.rs), reading them from `under`, a copy of them, since
// no draw reads the target it draws into (ADR-1100 D4). The GPU's own
// blending would turn its float result into a byte as the driver
// chooses, rounding or truncating, so the shader does the blend itself.
precision highp float;
precision highp int;

uniform highp sampler2D masks;
uniform highp sampler2D colors;
uniform highp sampler2D sprites;
// the cell's width and height
uniform ivec4 cell;
#ifdef INK
// the boxes of the glyphs whose ink passes their cells, each in a slot
// with its first cell REACH_SIDE cells from the slot's left and
// REACH_ROWS rows from its top
uniform highp sampler2D boxes;
// x, y: a slot's width and height; z: the slots across the boxes' texture
uniform ivec4 box_slot;
// the ink grid: for the cell at column k and row r, the texel at k, 2r
// holds its reach byte, its span and its slot's number, low byte first,
// and the texel at k, 2r + 1 its ink's color
uniform highp sampler2D ink;
#endif
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

#ifdef INK
// the job's flags, in the top byte of params.y: it receives ink from its
// neighbours; a block cursor or the selection marks it, so received ink
// takes its foreground (ADR-1306 D6); a display line runs down its column
// of the upright frame, on a display turned a quarter
const uint RECEIVES = 1u;
const uint MARKED = 2u;
const uint DOWN = 4u;

// The job being drawn: the column and row of its first cell, how many
// cells it spans, and the color marked ink takes.
struct Job {
    int col;
    int row;
    int cells;
    bool marked;
    vec3 fg;
};

// The ink of the glyph whose first cell is at column k in row r, tinted
// over p at `at`, a pixel of the job's cells counted from the grid's top
// left. Nothing when no glyph's ink reaches from there, when its block of
// cells (its own and its reach) misses the job's cells or the pixel, or,
// for a gap of 0 or more, when the glyph does not end that many cells
// before the job's first.
vec4 take(vec4 p, int k, int r, Job job, ivec2 at, int gap) {
    ivec2 grid = textureSize(ink, 0);
    if (k < 0 || r < 0 || k >= grid.x || r * 2 + 1 >= grid.y) {
        return p;
    }
    vec4 e = floor(texelFetch(ink, ivec2(k, r * 2), 0) * 255.0 + 0.5);
    int reach = int(e.r);
    int span = int(e.g);
    if (reach == 0 || span == 0) {
        return p;
    }
    if (gap >= 0 && job.col - (k + span) != gap) {
        return p;
    }
    // the reach byte's counts (src/reach.rs): left, right, up and down,
    // two bits each
    int c0 = k - (reach & 3);
    int c1 = k + span - 1 + ((reach >> 2) & 3);
    int r0 = r - ((reach >> 4) & 3);
    int r1 = r + ((reach >> 6) & 3);
    if (job.row < r0 || job.row > r1 || job.col + job.cells - 1 < c0 || job.col > c1) {
        return p;
    }
    if (at.x < c0 * cell.x || at.x >= (c1 + 1) * cell.x
            || at.y < r0 * cell.y || at.y >= (r1 + 1) * cell.y) {
        return p;
    }
    int slot = int(e.b) + int(e.a) * 256;
    ivec2 t = at - ivec2(k * cell.x, r * cell.y)
        + ivec2(REACH_SIDE * cell.x, REACH_ROWS * cell.y);
    if (t.x < 0 || t.y < 0 || t.x >= box_slot.x || t.y >= box_slot.y) {
        return p;
    }
    ivec2 from = ivec2(slot % box_slot.z * box_slot.x, slot / box_slot.z * box_slot.y);
    float a = byte(texelFetch(boxes, from + t, 0).r);
    vec3 c = job.marked ? job.fg : floor(texelFetch(ink, ivec2(k, r * 2 + 1), 0).rgb * 255.0 + 0.5);
    return tint_over(p, a, c);
}

// The ink the job's pixel `at` receives from its neighbours' glyphs,
// premultiplied, blended in ADR-1306 D4's order: the rows above, then the
// rows below, each from the column before the job's first to the column
// after its last; then the cells beside it in its row, the farthest first
// and, at each distance, the one before it first, so the nearer ink lies
// on top. A glyph beside the job reaches it only from fewer than
// REACH_SIDE cells away, since its reach is cut at REACH_SIDE cells.
vec4 received(Job job, ivec2 at) {
    vec4 p = vec4(0.0);
    int end = job.col + job.cells;
    for (int below = 0; below < 2; below++) {
        for (int k = job.col - REACH_SIDE - 1; k < end + REACH_SIDE; k++) {
            for (int i = 1; i <= REACH_ROWS; i++) {
                int r = below == 0 ? job.row - REACH_ROWS - 1 + i : job.row + i;
                p = take(p, k, r, job, at, -1);
            }
        }
    }
    for (int gap = REACH_SIDE - 1; gap >= 0; gap--) {
        for (int k = job.col - REACH_SIDE - 1; k < job.col; k++) {
            p = take(p, k, job.row, job, at, gap);
        }
        p = take(p, end + gap, job.row, job, at, -1);
    }
    return p;
}

// Whether any ink reaches the display line through the job's pixel `at`:
// the CPU path composes such a line whole as a cell of several layers
// (compose.rs's cell_line), whose rounding differs from a cell of one
// mask's where the mask's coverage is partial.
bool line_inked(Job job, ivec2 at, bool down) {
    ivec2 first = ivec2(job.col * cell.x, job.row * cell.y);
    if (down) {
        for (int y = 0; y < cell.y; y++) {
            if (received(job, ivec2(at.x, first.y + y)).a != 0.0) {
                return true;
            }
        }
    } else {
        for (int x = 0; x < job.cells * cell.x; x++) {
            if (received(job, ivec2(first.x + x, at.y)).a != 0.0) {
                return true;
            }
        }
    }
    return false;
}
#endif

void main() {
    ivec2 local = ivec2(gl_FragCoord.xy) - dst.xy;
    ivec2 at = ivec2(src.xy) + local;
    uint kind = params.x >> 24u;
    uint ul = params.z >> 24u;
    uint st = params.w >> 24u;
    vec3 fg = rgb(params.x);
    vec3 bg = rgb(params.y);
#ifdef OVER
    bool layer = true;
#else
    bool layer = false;
#endif
    float a = kind == 1u ? byte(texelFetch(masks, at, 0).r) : 0.0;
    bool one = !layer && ul == 0u && st == 0u;
    vec4 recv = vec4(0.0);
#ifdef INK
    uint flags = params.y >> 24u;
    if ((flags & RECEIVES) != 0u) {
        Job job = Job(int(src.z), int(src.w), dst.z / max(cell.x, 1), (flags & MARKED) != 0u, fg);
        ivec2 pixel = ivec2(job.col * cell.x, job.row * cell.y) + local;
        recv = received(job, pixel);
        one = one && recv.a == 0.0;
        if (one && a != 0.0 && a != 255.0) {
            one = !line_inked(job, pixel, (flags & DOWN) != 0u);
        }
    }
#endif
    vec3 c;
    if (one) {
        if (kind == 1u) {
            // blend.rs's mask: fg over bg through the coverage
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
        // the received ink, one layer over the underline and under the
        // cell's own glyph (compose.rs's received_over)
        if (recv.a != 0.0) {
            p = over(p, recv);
        }
        if (kind == 1u) {
            p = tint_over(p, a, fg);
        } else if (kind == 2u) {
            vec4 s = floor(texelFetch(colors, at, 0) * 255.0 + 0.5);
            if (s.a != 0.0) {
                p = over(p, s);
            }
        }
        if (st != 0u) {
            p = tint_over(p, sprite(st, pos), rgb(params.w));
        }
#ifdef OVER
        // over_px: p over the opaque pixel beneath, at most 255
        ivec2 under_texel = ivec2(gl_FragCoord.xy) - under_at.xy;
        vec3 d = floor(texelFetch(under, under_texel, 0).rgb * 255.0 + 0.5);
        c = p.a == 0.0 ? d : min(p.rgb + div255(d * (255.0 - p.a)), 255.0);
#else
        c = p.a == 0.0 ? bg : p.rgb + div255(bg * (255.0 - p.a));
#endif
    }
    color = vec4((c + 0.25) / 255.0, 1.0);
}
