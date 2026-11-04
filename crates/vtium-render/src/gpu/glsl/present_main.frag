
void main() {
    ivec2 at = texel(ivec2(gl_FragCoord.xy));
#ifdef SPATIAL_STEP
    vec4 c = spatial_step(at);
#else
    vec4 c = texelFetch(frame, at, 0);
#endif
#ifdef COLOR_STEP
    c = color_step(c);
#endif
    color = c;
}
