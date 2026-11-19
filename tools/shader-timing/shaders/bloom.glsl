// timing: reads 25
// timing: work the pixel, and 24 reads on a golden-angle spiral, each bright one adding light by its distance
// timing: origin a stand-in for bloom.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 color = texture(iChannel0, uv);
    vec2 pixel = 1.0 / iResolution.xy;
    vec3 light = vec3(0.0);
    for (int i = 0; i < 24; i++) {
        float n = float(i) + 0.5;
        float radius = sqrt(n / 24.0) * 6.0;
        float angle = n * 2.3999632;
        vec2 offset = vec2(cos(angle), sin(angle)) * radius * pixel;
        vec3 c = texture(iChannel0, uv + offset).rgb;
        float bright = max(c.r, max(c.g, c.b));
        light += c * smoothstep(0.2, 0.5, bright) * (1.0 - radius / 7.0);
    }
    fragColor = vec4(color.rgb + light * 0.08, color.a);
}
