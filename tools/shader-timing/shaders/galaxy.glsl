// timing: reads 1
// timing: work a volume of 7 steps a pixel, each folding its point 16 times, and a star field of 6 hashes, drawn behind the text
// timing: origin a stand-in for galaxy.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float galaxy_hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 text = texture(iChannel0, uv);
    vec2 p = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    float stars = 0.0;
    for (int i = 0; i < 6; ++i) {
        float scale = float(i + 1) * 40.0;
        vec2 cell = floor(p * scale);
        float h = galaxy_hash(cell + float(i) * 17.0);
        stars += step(0.995, h) * (1.0 - length(fract(p * scale) - 0.5) * 2.0);
    }
    vec3 ray = normalize(vec3(p, 1.0));
    vec3 origin = vec3(1.0, 0.5, 0.5 + iTime * 0.01);
    vec3 color = vec3(0.0);
    float travel = 0.24;
    const float STEP = 0.24;
    for (float s = 0.0; s < 7.0; ++s) {
        vec3 q = origin + travel * ray;
        q = abs(vec3(0.85) - mod(q, vec3(1.7)));
        float previous = 0.0;
        float energy = 0.0;
        for (int i = 0; i < 16; ++i) {
            q = abs(q) / dot(q, q) - 0.53;
            float l = length(q);
            energy += abs(l - previous);
            previous = l;
        }
        energy *= energy * energy * 0.002;
        color += vec3(travel, travel * travel, travel * travel * travel) * energy * 0.01;
        travel += STEP;
    }
    vec3 scene = color * 0.5 + stars * vec3(0.8);
    float ink = step(0.25, max(text.r, max(text.g, text.b)));
    fragColor = vec4(mix(scene, text.rgb, ink), 1.0);
}
