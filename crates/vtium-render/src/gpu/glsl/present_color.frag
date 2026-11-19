// The color step: each pixel's color is looked up in the head's color
// table, a look's color filters merged into one lattice of 33 nodes a
// side, which present samples as a 3D texture. The node at index i of a
// side holds what the filters give the color i/32, and its texel's center
// lies at (i + 0.5)/33, so a channel c from 0 to 1 samples at
// (c*32 + 0.5)/33. The GPU blends the eight nodes around the color
// linearly, as the CPU path's lookup does. Alpha passes through.
//
// References:
// - ADR-1304 D2: the table, where present samples it, and the one-level tolerance against the CPU path.
// - ADR-1303 D6: the color step runs after the spatial step and the turn.
#define COLOR_STEP

uniform highp sampler3D color_table;

vec4 color_step(vec4 c) {
    vec3 at = (clamp(c.rgb, 0.0, 1.0) * 32.0 + 0.5) / 33.0;
    return vec4(texture(color_table, at).rgb, c.a);
}

