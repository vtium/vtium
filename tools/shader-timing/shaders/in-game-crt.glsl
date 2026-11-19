// timing: reads 31
// timing: work a barrel curve, 24 reads of ghosting to one side, 6 reads of color fringing, scanlines, a vignette and a tint
// timing: origin a stand-in for in-game-crt.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

vec2 game_curve(vec2 uv) {
    uv = uv * 2.0 - 1.0;
    vec2 offset = abs(uv.yx) / 13.0;
    uv = uv + uv * offset * offset;
    return uv * 0.5 + 0.5;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = game_curve(fragCoord / iResolution.xy);
    vec2 pixel = 1.0 / iResolution.xy;
    vec3 color = texture(iChannel0, uv).rgb;
    vec3 fringe = vec3(0.0);
    for (int i = 1; i <= 2; i++) {
        float spread = float(i) * 0.1 * pixel.x * 10.0;
        fringe.r += texture(iChannel0, uv + vec2(spread, 0.0)).r;
        fringe.g += texture(iChannel0, uv + vec2(0.0, spread)).g;
        fringe.b += texture(iChannel0, uv - vec2(spread, 0.0)).b;
    }
    color = mix(color, fringe * 0.5, 0.5);
    vec3 ghost = vec3(0.0);
    for (int i = 0; i < 24; i++) {
        float t = float(i) / 24.0;
        ghost += texture(iChannel0, uv - vec2(t * 0.75 * pixel.x * 40.0, 0.0)).rgb * (1.0 - t);
    }
    color += ghost / 12.0 * 0.1;
    float scan = 0.85 + 0.15 * sin(fragCoord.y * 3.14159);
    vec2 v = uv * (1.0 - uv.yx);
    float vignette = pow(v.x * v.y * 20.0, 0.4);
    color *= scan * vignette * vec3(0.93, 1.0, 0.93);
    bool outside = any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)));
    fragColor = vec4(outside ? vec3(0.0) : color, 1.0);
}
