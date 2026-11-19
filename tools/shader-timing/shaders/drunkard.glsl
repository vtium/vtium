// timing: reads 3
// timing: work two noise sums of 10 octaves each that sway the place, then one read for each of red, green and blue a little apart
// timing: origin a stand-in for drunkard.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float drunk_hash(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

float drunk_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    float a = drunk_hash(i);
    float b = drunk_hash(i + vec2(1.0, 0.0));
    float c = drunk_hash(i + vec2(0.0, 1.0));
    float d = drunk_hash(i + vec2(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

float drunk_sum(vec2 p) {
    float total = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 10; i++) {
        total += drunk_noise(p) * amplitude;
        p *= 2.0;
        amplitude *= 0.5;
    }
    return total;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float t = iTime * 0.4;
    vec2 sway = vec2(drunk_sum(uv * 3.0 + t), drunk_sum(uv * 3.0 - t + 5.2)) - 0.5;
    vec2 at = uv + sway * 0.02;
    vec2 apart = vec2(0.002, 0.0);
    float r = texture(iChannel0, at + apart).r;
    float g = texture(iChannel0, at).g;
    float b = texture(iChannel0, at - apart).b;
    fragColor = vec4(r, g, b, 1.0);
}
