#version 100

precision mediump float;

uniform sampler2D tex_from;
uniform vec2 tex_from_loc;
uniform vec2 tex_from_size;

uniform sampler2D tex_to;
uniform vec2 tex_to_loc;
uniform vec2 tex_to_size;

uniform float alpha;
uniform float amount;

uniform vec2 size;
varying vec2 v_coords;

void main() {
    vec2 coords_from = (v_coords - tex_from_loc) / tex_from_size;
    vec2 coords_to = (v_coords - tex_to_loc) / tex_to_size;

    vec4 color_from = texture2D(tex_from, coords_from);
    vec4 color_to = texture2D(tex_to, coords_to);

    vec4 color = mix(color_from, color_to, amount);
    color = color * alpha;

    gl_FragColor = color;
}

