// timing: reads 1
// timing: work one subtraction a channel
// timing: origin a stand-in for negative.glsl, written for this harness from the read count and loop shape the survey of hackrdotsh/ghostty-shaders at 6b3e4d1c records, with no line of that file; MIT, vtium

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 c = texture(iChannel0, uv);
    fragColor = vec4(1.0 - c.rgb, c.a);
}
