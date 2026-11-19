// timing: reads 1
// timing: work a luminance, a 4x4 ordered-dither threshold from bit operations, and two levels a channel
// timing: origin a stand-in for dither.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float bayer4(ivec2 p) {
    int x = p.x & 3;
    int y = p.y & 3;
    int v = ((x ^ y) & 1) << 3 | (y & 1) << 2 | ((x ^ y) & 2) | (y & 2) >> 1;
    return (float(v) + 0.5) / 16.0;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float threshold = bayer4(ivec2(fragCoord));
    float levels = 2.0;
    vec3 q = floor(c * (levels - 1.0) + threshold) / (levels - 1.0);
    fragColor = vec4(q, 1.0);
}
