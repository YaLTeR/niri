
void main() {
    vec3 coords_curr_geo = niri_input_to_curr_geo * vec3(niri_v_coords, 1.0);
    vec3 size_curr_geo = vec3(niri_curr_geo_size, 1.0);

    vec4 color = resize_color(coords_curr_geo, size_curr_geo);

    gl_FragColor = color * niri_alpha;
}
