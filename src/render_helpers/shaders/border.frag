precision mediump float;

#if defined(DEBUG_FLAGS)
uniform float niri_tint;
#endif

uniform float niri_alpha;
uniform float niri_scale;

uniform vec2 niri_size;
varying vec2 niri_v_coords;

uniform float colorspace;
uniform float hue_interpolation;
uniform vec4 color_from;
uniform vec4 color_to;
uniform vec2 grad_offset;
uniform float grad_width;
uniform vec2 grad_vec;

uniform mat3 input_to_geo;
uniform vec2 geo_size;
uniform vec4 outer_radius;
uniform float border_width;

float srgb_to_linear(float color) {
  return 
    color <= 0.04045 ?
    color / 12.92 :
    pow((color + 0.055) / 1.055, 2.4) ;
}

float linear_to_srgb(float color) {
  return 
    color <= 0.0031308 ?
    color * 12.92 :
    pow(color * 1.055, 1.0 / 2.4) - 0.055 ;
} 

vec4 color_mix(vec4 color1, vec4 color2, float color_ratio) {

  if (colorspace == 0.0) { //  srgb
    return mix(color1, color2, color_ratio);
  }
  
  vec4 color_out;

  color1.rgb = color1.rgb / color1.a;
  color2.rgb = color2.rgb / color2.a;
  
  color1.rgb = vec3(
    srgb_to_linear(color1.r),
    srgb_to_linear(color1.g),
    srgb_to_linear(color1.b));
 
  color2.rgb = vec3(
    srgb_to_linear(color2.r),
    srgb_to_linear(color2.g),
    srgb_to_linear(color2.b));

  if (colorspace == 1.0) { // srgb-linear
    color_out = mix(
      color1,
      color2,
      color_ratio
    );
  } else {
    color_out = vec4(
      1.0,
      0.0,
      0.0,
      1.0
    );
  }

  return vec4(
    linear_to_srgb(color_out.r) * color_out.a,
    linear_to_srgb(color_out.g) * color_out.a,
    linear_to_srgb(color_out.b) * color_out.a,
    color_out.a
  );
}

vec4 gradient_color(vec2 coords) {
    coords = coords + grad_offset;

    if ((grad_vec.x < 0.0 && 0.0 <= grad_vec.y) || (0.0 <= grad_vec.x && grad_vec.y < 0.0))
        coords.x -= grad_width;

    float frac = dot(coords, grad_vec) / dot(grad_vec, grad_vec);

    if (grad_vec.y < 0.0)
        frac += 1.0;

    frac = clamp(frac, 0.0, 1.0);
    return color_mix(color_from, color_to, frac);
}

float rounding_alpha(vec2 coords, vec2 size, vec4 corner_radius) {
    vec2 center;
    float radius;

    if (coords.x < corner_radius.x && coords.y < corner_radius.x) {
        radius = corner_radius.x;
        center = vec2(radius, radius);
    } else if (size.x - corner_radius.y < coords.x && coords.y < corner_radius.y) {
        radius = corner_radius.y;
        center = vec2(size.x - radius, radius);
    } else if (size.x - corner_radius.z < coords.x && size.y - corner_radius.z < coords.y) {
        radius = corner_radius.z;
        center = vec2(size.x - radius, size.y - radius);
    } else if (coords.x < corner_radius.w && size.y - corner_radius.w < coords.y) {
        radius = corner_radius.w;
        center = vec2(radius, size.y - radius);
    } else {
        return 1.0;
    }

    float dist = distance(coords, center);
    float half_px = 0.5 / niri_scale;
    return 1.0 - smoothstep(radius - half_px, radius + half_px, dist);
}

void main() {
    vec3 coords_geo = input_to_geo * vec3(niri_v_coords, 1.0);
    vec4 color = gradient_color(coords_geo.xy);
    color = color * rounding_alpha(coords_geo.xy, geo_size, outer_radius);

    if (border_width > 0.0) {
        coords_geo -= vec3(border_width);
        vec2 inner_geo_size = geo_size - vec2(border_width * 2.0);
        if (0.0 <= coords_geo.x && coords_geo.x <= inner_geo_size.x
                && 0.0 <= coords_geo.y && coords_geo.y <= inner_geo_size.y)
        {
            vec4 inner_radius = max(outer_radius - vec4(border_width), 0.0);
            color = color * (1.0 - rounding_alpha(coords_geo.xy, inner_geo_size, inner_radius));
        }
    }

    color = color * niri_alpha;

#if defined(DEBUG_FLAGS)
    if (niri_tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
