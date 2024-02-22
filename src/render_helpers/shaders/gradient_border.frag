precision mediump float;
uniform float alpha;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform vec2 size;
varying vec2 v_coords;

uniform vec4 color_from;
uniform vec4 color_to;
uniform vec2 grad_offset;
uniform float grad_width;
uniform vec2 grad_vec;

void main() {
    vec2 coords = v_coords * size + grad_offset;

    if ((grad_vec.x < 0.0 && 0.0 <= grad_vec.y) || (0.0 <= grad_vec.x && grad_vec.y < 0.0))
        coords.x -= grad_width;

    float frac = dot(coords, grad_vec) / dot(grad_vec, grad_vec);

    if (grad_vec.y < 0.0)
        frac += 1.0;

    frac = clamp(frac, 0.0, 1.0);
    vec4 out_color = mix(color_from, color_to, frac);

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        out_color = vec4(0.0, 0.3, 0.0, 0.2) + out_color * 0.8;
#endif

    gl_FragColor = out_color;
}
