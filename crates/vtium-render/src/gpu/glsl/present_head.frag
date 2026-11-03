#version 300 es
// Present (ADR-1303 D6): each pixel of the display's buffer reads its texel
// of the upright frame's tile by integer coordinates, with no filtering. The
// spatial and color steps that are switched on are joined in between this
// head and present_main.frag, each defining its macro and its function; an
// empty slot joins nothing, so it adds no instruction.
precision highp float;
precision highp int;

uniform highp sampler2D frame;

// the texel of the tile under the pixel, with half a texel added
in vec2 src_at;

out vec4 color;
