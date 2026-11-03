#version 300 es
// One quad of a batch, drawn as an instance of a strip of four vertices.
// The adapter gives each quad's fields at fixed locations (`Quad` in
// gl.rs), and the corner comes from the vertex's index. Row 0 of every
// target is its top: the renderer never draws to a window, so GL's
// bottom-left origin is the first row in memory, as a display buffer's is.
precision highp float;
precision highp int;

layout(location = 0) in ivec4 quad_dst;
layout(location = 1) in uvec4 quad_src;
layout(location = 2) in uvec4 quad_params;

uniform vec2 target_size;

out vec2 src_at;
flat out uvec4 params;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 at = vec2(quad_dst.xy) + corner * vec2(quad_dst.zw);
    gl_Position = vec4(at / target_size * 2.0 - 1.0, 0.0, 1.0);
    src_at = vec2(quad_src.xy) + corner * vec2(quad_src.zw);
    params = quad_params;
}
