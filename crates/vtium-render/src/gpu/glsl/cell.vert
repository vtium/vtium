#version 300 es
// A cell's quad (ADR-1300 D6), drawn as an instance of a strip of four
// vertices, as quad.vert draws one. The fragment shader works in whole
// pixels, so this shader hands it the quad's fields unchanged.
precision highp float;
precision highp int;

layout(location = 0) in ivec4 quad_dst;
layout(location = 1) in uvec4 quad_src;
layout(location = 2) in uvec4 quad_params;

uniform vec2 target_size;

flat out ivec4 dst;
flat out uvec4 src;
flat out uvec4 params;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 at = vec2(quad_dst.xy) + corner * vec2(quad_dst.zw);
    gl_Position = vec4(at / target_size * 2.0 - 1.0, 0.0, 1.0);
    dst = quad_dst;
    src = quad_src;
    params = quad_params;
}
