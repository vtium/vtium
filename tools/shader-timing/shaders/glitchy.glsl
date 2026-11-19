// timing: reads 3
// timing: work bands and blocks picked from hashes of the time, which shift rows sideways, then one read for each of red, green and blue
// timing: origin a stand-in for glitchy.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float glitch_hash(float n) {
    return fract(sin(n) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    float slice = floor(iTime * 8.0);
    float band = floor(uv.y * 24.0);
    float block = floor(uv.x * 12.0) + band * 12.0;
    float on = step(0.85, glitch_hash(slice + band * 3.1));
    float shift = (glitch_hash(slice * 1.7 + block) - 0.5) * 0.1 * on;
    vec2 at = vec2(uv.x + shift, uv.y);
    float split = 0.004 * on + 0.001;
    float r = texture(iChannel0, at + vec2(split, 0.0)).r;
    float g = texture(iChannel0, at).g;
    float b = texture(iChannel0, at - vec2(split, 0.0)).b;
    float noise = glitch_hash(dot(fragCoord, vec2(1.0, 57.0)) + slice) * 0.08 * on;
    fragColor = vec4(vec3(r, g, b) + noise, 1.0);
}
