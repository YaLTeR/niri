
vec4 linear_color_mix(vec4 color1, vec4 color2, float color_ratio) {
  vec4 lin_color1 = vec4(
    pow(color1.r, 2.0),
    pow(color1.g, 2.0),
    pow(color1.b, 2.0),
    color1.a
  );

  vec4 lin_color2 = vec4(
    pow(color2.r, 2.0),
    pow(color2.g, 2.0),
    pow(color2.b, 2.0),
    color2.a
  );

  vec4 color_out = mix(
    lin_color1,
    lin_color2,
    color_ratio
  );

  return vec4(sqrt(color_out.r),
    sqrt(color_out.g),
    sqrt(color_out.b),
    color_out.a
  );
}


vec4 resize_color(vec3 coords_curr_geo, vec3 size_curr_geo) {
    vec3 coords_tex_prev = niri_geo_to_tex_prev * coords_curr_geo;
    vec4 color_prev = texture2D(niri_tex_prev, coords_tex_prev.st);

    vec3 coords_tex_next = niri_geo_to_tex_next * coords_curr_geo;
    vec4 color_next = texture2D(niri_tex_next, coords_tex_next.st);

    vec4 color = linear_color_mix(color_prev, color_next, niri_clamped_progress);
    return color;
}
