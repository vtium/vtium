// timing: reads 1
// timing: work 50 parallax layers a pixel, each finding its cell's flake from a hash and its distance, drawn over the text
// timing: origin a stand-in for just-snow.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

vec3 snow_hash(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.xxy + q.yzz) * q.zyx);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 text = texture(iChannel0, uv);
    vec2 p = fragCoord / iResolution.y;
    float snow = 0.0;
    for (int i = 0; i < 50; i++) {
        float layer = float(i);
        float depth = 1.0 + layer * 0.5;
        vec2 q = p * depth + vec2(sin(layer * 1.7 + iTime * 0.3) * 0.3, iTime * 0.6 / (1.0 + layer * 0.1));
        vec2 cell = floor(q * 4.0);
        vec3 h = snow_hash(cell + layer * 13.0);
        vec2 flake = (cell + 0.2 + h.xy * 0.6) / 4.0;
        float d = length(q - flake) * 4.0;
        snow += smoothstep(0.08 * h.z + 0.02, 0.0, d) * (1.0 - layer / 60.0);
    }
    fragColor = vec4(text.rgb + vec3(snow), 1.0);
}
