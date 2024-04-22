#version 100

precision mediump float;

varying vec2 v_coords;
uniform vec2 size;

uniform mat3 input_to_curr_geo;
uniform mat3 input_to_prev_geo;
uniform mat3 input_to_next_geo;

uniform sampler2D tex_prev;
uniform mat3 geo_to_tex_prev;

uniform sampler2D tex_next;
uniform mat3 geo_to_tex_next;

uniform float progress;
uniform float clamped_progress;

uniform float alpha;

vec4 crossfade() {
    vec3 coords_curr_geo = input_to_curr_geo * vec3(v_coords, 1.0);

    vec3 coords_tex_prev = geo_to_tex_prev * coords_curr_geo;
    vec4 color_prev = texture2D(tex_prev, vec2(coords_tex_prev));

    vec3 coords_tex_next = geo_to_tex_next * coords_curr_geo;
    vec4 color_next = texture2D(tex_next, vec2(coords_tex_next));

    vec4 color = mix(color_prev, color_next, clamped_progress);
    return color;
}

void main() {
    vec4 color = crossfade();

    gl_FragColor = color * alpha;
}

