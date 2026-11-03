#version 300 es
// The pointer's arrow (ADR-1303 D3): its premultiplied texel, which the
// GPU blends over the upright frame as GL_ONE, GL_ONE_MINUS_SRC_ALPHA.
precision highp float;
precision highp int;

uniform highp sampler2D arrow;

in vec2 src_at;

out vec4 color;

void main() {
    color = texelFetch(arrow, ivec2(src_at), 0);
}
