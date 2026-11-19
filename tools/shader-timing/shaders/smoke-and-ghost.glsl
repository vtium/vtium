// timing: reads 46
// timing: work 45 reads in a 9 by 5 neighbourhood below the pixel that look for text, a smoke of 3 noise octaves rising from it, and a ghost face's shape
// timing: origin a stand-in for smoke-and-ghost.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float smoke_hash(vec2 p) {
    return fract(sin(dot(p, vec2(41.7, 289.3))) * 23758.5453);
}

float smoke_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(smoke_hash(i), smoke_hash(i + vec2(1.0, 0.0)), u.x),
               mix(smoke_hash(i + vec2(0.0, 1.0)), smoke_hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 here = texture(iChannel0, uv);
    const float RADIUS = 0.011;
    const float STEP = RADIUS / 4.0;
    float source = 0.0;
    for (float x = -RADIUS; x <= RADIUS + 0.0001; x += STEP) {
        for (float y = -RADIUS; y <= 0.0001; y += STEP) {
            vec3 c = texture(iChannel0, uv + vec2(x, y)).rgb;
            float lit = step(0.25, max(c.r, max(c.g, c.b)));
            source += lit * (1.0 - smoothstep(0.0, RADIUS, length(vec2(x, y))));
        }
    }
    vec2 q = uv * 25.0 + vec2(0.0, -iTime * 0.5);
    float smoke = smoke_noise(q) * 0.5 + smoke_noise(q * 2.0) * 0.25 + smoke_noise(q * 4.0) * 0.125;
    float presence = clamp(source / 20.0, 0.0, 1.0) * smoke;
    vec2 face = (uv - vec2(0.5 + 0.1 * sin(iTime * 0.3), 0.5)) * vec2(1.0, 1.5) * 4.0;
    float body = smoothstep(1.0, 0.8, length(face));
    float eyes = smoothstep(0.12, 0.1, length(face - vec2(-0.3, 0.2))) + smoothstep(0.12, 0.1, length(face - vec2(0.3, 0.2)));
    float ghost = clamp(body - eyes, 0.0, 1.0) * smoothstep(0.2, 0.4, presence);
    vec3 color = here.rgb + vec3(presence * 0.2) + vec3(0.8, 0.9, 1.0) * ghost * 0.5;
    fragColor = vec4(color, 1.0);
}
