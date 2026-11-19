// timing: reads 1
// timing: work a raymarch of 64 steps a pixel through a field of rotating rounded boxes, drawn behind the text
// timing: origin a stand-in for cubes.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

mat2 cubes_turn(float a) {
    float s = sin(a);
    float c = cos(a);
    return mat2(c, -s, s, c);
}

float cubes_field(vec3 p) {
    p.xy *= cubes_turn(iTime * 0.2 + p.z * 0.05);
    vec3 q = mod(p, 4.0) - 2.0;
    q.xz *= cubes_turn(iTime * 0.7);
    vec3 d = abs(q) - vec3(0.6);
    return length(max(d, 0.0)) + min(max(d.x, max(d.y, d.z)), 0.0) - 0.1;
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 text = texture(iChannel0, uv);
    vec2 p = (fragCoord * 2.0 - iResolution.xy) / iResolution.y;
    vec3 ray = normalize(vec3(p, 1.5));
    vec3 origin = vec3(0.0, 0.0, iTime);
    float t = 0.0;
    float glow = 0.0;
    const int MAX_STEPS = 64;
    for (int i = 0; i < MAX_STEPS; i++) {
        float d = cubes_field(origin + ray * t);
        glow += exp(-d * 4.0) * 0.02;
        if (d < 0.001 || t > 40.0) break;
        t += d;
    }
    vec3 scene = vec3(0.2, 0.5, 0.9) * glow / (1.0 + t * 0.05);
    float ink = step(0.25, max(text.r, max(text.g, text.b)));
    fragColor = vec4(mix(scene, text.rgb, ink), 1.0);
}
