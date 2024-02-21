precision mediump float;
uniform float alpha;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform vec2 size;
varying vec2 v_coords;

uniform vec4 color_from;
uniform vec4 color_to;
uniform float angle;
uniform vec2 gradient_offset;
uniform float gradient_width;
uniform float gradient_total;

#define FRAC_PI_2   1.57079632679
#define PI          3.14159265359
#define FRAC_3_PI_2 4.71238898038
#define TAU         6.28318530718

void main() {
    vec2 coords = v_coords * size + gradient_offset;

    if ((FRAC_PI_2 <= angle && angle < PI) || (FRAC_3_PI_2 <= angle && angle < TAU))
        coords.x -= gradient_width;

    float frag_angle = FRAC_PI_2;
    if (coords.x != 0.0)
        frag_angle = atan(coords.y, coords.x);

    float angle_frag_to_grad = frag_angle - angle;

    float frac = cos(angle_frag_to_grad) * length(coords) / gradient_total;
    if (PI <= angle)
        frac += 1.0;
    frac = clamp(frac, 0.0, 1.0);

    vec4 out_color = mix(color_from, color_to, frac);

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        out_color = vec4(0.0, 0.3, 0.0, 0.2) + out_color * 0.8;
#endif

    gl_FragColor = out_color;
}
