// timing: reads 1
// timing: work a red, green and blue stripe mask across each 3 pixels, and a dark line every 3 rows
// timing: origin a stand-in for tft.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec3 c = texture(iChannel0, uv).rgb;
    float column = mod(floor(fragCoord.x), 3.0);
    vec3 stripe = vec3(column == 0.0, column == 1.0, column == 2.0) * 0.6 + 0.4;
    float row = mod(floor(fragCoord.y), 3.0) == 0.0 ? 0.7 : 1.0;
    fragColor = vec4(c * stripe * row * 1.4, 1.0);
}
