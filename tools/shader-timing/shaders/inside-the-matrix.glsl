// timing: reads 1
// timing: work a march of 40 grid cells a pixel through a 3D field of glyph columns, each cell hashed twice and a hit drawn from 4 strokes, behind the text
// timing: origin a stand-in for inside-the-matrix.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

float matrix_hash(vec3 p) {
    p = fract(p * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float matrix_segment(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

float matrix_glyph(vec2 p, float seed) {
    float d = 1.0;
    for (int i = 0; i < 4; i++) {
        float s = seed * 13.1 + float(i) * 3.7;
        vec2 a = vec2(fract(sin(s) * 43758.5), fract(sin(s + 1.0) * 24634.6));
        vec2 b = vec2(fract(sin(s + 2.0) * 35746.3), fract(sin(s + 3.0) * 13546.7));
        d = min(d, matrix_segment(p, a, b));
    }
    return smoothstep(0.1, 0.0, d);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 text = texture(iChannel0, uv);
    vec2 p = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    vec3 ray = normalize(vec3(p, 1.0));
    vec3 origin = vec3(0.5, 0.5, iTime);
    vec3 cell = floor(origin);
    vec3 stride = sign(ray);
    vec3 delta = abs(1.0 / ray);
    vec3 side = (stride * (cell - origin) + stride * 0.5 + 0.5) * delta;
    vec3 color = vec3(0.0);
    for (int i = 0; i < 40; i++) {
        float h = 0.0;
        for (int j = 0; j < 2; j++) {
            h += matrix_hash(cell + float(j) * 7.0);
        }
        if (h > 1.6 && abs(cell.x) + abs(cell.y) > 1.0) {
            float t = min(side.x, min(side.y, side.z));
            vec3 hit = origin + ray * t;
            float g = matrix_glyph(fract(hit.xy + hit.z * 0.1), floor(h * 10.0 + iTime * 2.0));
            color = vec3(0.1, 1.0, 0.3) * g / (1.0 + t * 0.2);
            break;
        }
        vec3 m = step(side.xyz, side.yzx) * step(side.xyz, side.zxy);
        side += m * delta;
        cell += m * stride;
    }
    float ink = step(0.25, max(text.r, max(text.g, text.b)));
    fragColor = vec4(mix(color, text.rgb, ink), 1.0);
}
