// timing: reads 1
// timing: work 128 particles a pixel, each placed from a hash under gravity and lit by its distance, drawn behind the text
// timing: origin a stand-in for fireworks-rockets.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

vec2 fireworks_hash(float n) {
    return fract(sin(vec2(n, n + 1.0)) * vec2(43758.5453, 22578.1459));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 text = texture(iChannel0, uv);
    vec2 p = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    float cycle = 3.0;
    float t = mod(iTime, cycle);
    float burst = floor(iTime / cycle);
    vec2 center = (fireworks_hash(burst) - 0.5) * vec2(1.2, 0.6);
    vec3 color = vec3(0.0);
    for (float i = 1.0; i <= 128.0; i++) {
        vec2 h = fireworks_hash(i * 7.31 + burst);
        float angle = h.x * 6.2831853;
        float speed = 0.2 + h.y * 0.4;
        vec2 velocity = vec2(cos(angle), sin(angle)) * speed;
        vec2 place = center + velocity * t - vec2(0.0, 0.15) * t * t;
        vec2 d = p - place;
        float light = 0.00002 / (dot(d, d) + 0.00001);
        color += light * vec3(1.0, 0.6 + h.x * 0.4, 0.3 + h.y * 0.7);
    }
    color *= smoothstep(0.0, 1.0, (cycle - t) / cycle);
    float ink = step(0.25, max(text.r, max(text.g, text.b)));
    fragColor = vec4(mix(color, text.rgb, ink), 1.0);
}
