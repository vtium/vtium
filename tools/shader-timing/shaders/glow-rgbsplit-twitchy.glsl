// timing: reads 27
// timing: work a twitch picked from 4 hashes of the time, 24 reads of glow on a ring, and one read for each of red, green and blue a twitch apart
// timing: origin a stand-in for glow-rgbsplit-twitchy.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float twitch_hash(float n) {
    return fract(sin(n * 91.345) * 47453.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float twitch = 0.0;
    for (int i = 0; i < 4; i++) {
        float scale = exp2(float(i));
        twitch += step(0.9, twitch_hash(floor(iTime * 10.0 * scale) + float(i))) / scale;
    }
    vec2 apart = vec2(0.003 * twitch + 0.0005, 0.0);
    vec3 split = vec3(
        texture(iChannel0, uv + apart).r,
        texture(iChannel0, uv).g,
        texture(iChannel0, uv - apart).b);
    vec2 pixel = 1.0 / iResolution.xy;
    vec3 glow = vec3(0.0);
    for (int i = 0; i < 24; i++) {
        float angle = float(i) * 0.2617994;
        float radius = 2.0 + float(i % 3) * 2.0;
        glow += texture(iChannel0, uv + vec2(cos(angle), sin(angle)) * radius * pixel).rgb;
    }
    fragColor = vec4(split + glow / 24.0 * 0.5, 1.0);
}
