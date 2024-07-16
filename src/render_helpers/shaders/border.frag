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
  return pow(color, 2.2);
}

float linear_to_srgb(float color) {
  return pow(color, 1.0 / 2.2);
}

vec3 lab_to_lch(vec3 color) {
  float c = sqrt(pow(color.y, 2.0) + pow(color.z, 2.0));
  float h = degrees(atan(color.z, color.y)) ;
  h += h <= 0.0 ?
    360.0 :
    0.0 ;
  return vec3(
    color.x,
    c,
    h
  );
}

vec3 lch_to_lab(vec3 color) {
  float a = color.y * clamp(cos(radians(color.z)), -1.0, 1.0);
  float b = color.y * clamp(sin(radians(color.z)), -1.0, 1.0);
  return vec3(
    color.x,
    a,
    b
  ); 
}

vec3 linear_to_oklab(vec3 color){
  mat3 rgb_to_lms = mat3(
    vec3(0.4122214708, 0.5363325363, 0.0514459929),
    vec3(0.2119034982, 0.6806995451, 0.1073969566),
    vec3(0.0883024619, 0.2817188376, 0.6299787005)
  );
  mat3 lms_to_oklab = mat3( 
    vec3(0.2104542553, 0.7936177850, -0.0040720468),
    vec3(1.9779984951, -2.4285922050, 0.4505937099),
    vec3(0.0259040371, 0.7827717662, -0.8086757660)
  );
  vec3 lms = color * rgb_to_lms;

  lms = vec3(
    pow(lms.x, 1.0 / 3.0),
    pow(lms.y, 1.0 / 3.0),
    pow(lms.z, 1.0 / 3.0)
  );
  return lms * lms_to_oklab;
}

vec3 oklab_to_linear(vec3 color){
  mat3 oklab_to_lms = mat3(
    vec3(1.0, 0.3963377774, 0.2158037573),
    vec3(1.0, -0.1055613458, -0.0638541728),
    vec3(1.0, -0.0894841775, -1.2914855480)
  );
  mat3 lms_to_rgb = mat3(
    vec3(4.0767416621, -3.3077115913, 0.2309699292),
    vec3(-1.2684380046, 2.6097574011, -0.3413193965),
    vec3(-0.0041960863, -0.7034186147, 1.7076147010)
  );
  vec3 lms = color * oklab_to_lms;

  lms = vec3(
    pow(lms.x, 3.0),
    pow(lms.y, 3.0),
    pow(lms.z, 3.0)
  );
  return lms * lms_to_rgb;
}

vec4 color_mix(vec4 color1, vec4 color2, float color_ratio) {
  //  srgb

  vec4 color_out;
  
  if (colorspace == 0.0) {
    color_out = mix(color1, color2, color_ratio);
    color_out.rgb = color_out.rgb * color_out.a;
    return color_out;
  }

  color1.rgb = vec3(
    srgb_to_linear(color1.r),
    srgb_to_linear(color1.g),
    srgb_to_linear(color1.b)
  ); 
  color2.rgb = vec3(
    srgb_to_linear(color2.r),
    srgb_to_linear(color2.g),
    srgb_to_linear(color2.b)
  );
  //  srgb-linear
  if (colorspace == 1.0) {
    color_out = mix(
      color1,
      color2,
      color_ratio
    );
  //  oklab
  } else if (colorspace == 2.0) {
    color1.xyz = linear_to_oklab(color1.rgb);
    color2.xyz = linear_to_oklab(color2.rgb);
    color_out = mix(
      color1,
      color2,
      color_ratio
    );
    color_out.rgb = oklab_to_linear(color_out.xyz);  
  //  oklch
  } else if (colorspace == 3.0) {
    color1.xyz = lab_to_lch(linear_to_oklab(color1.rgb));
    color2.xyz = lab_to_lch(linear_to_oklab(color2.rgb));
    color_out = mix(color1, color2, color_ratio);

    float min_hue = min(color1.z, color2.z);
    float max_hue = max(color1.z, color2.z);
    float path_direct_distance = (max_hue - min_hue) * color_ratio;
    float path_mod_distance = (360.0 - max_hue + min_hue) * color_ratio;

    float path_mod =
      color1.z == min_hue ?
        mod(color1.z - path_mod_distance, 360.0) :
        mod(color1.z + path_mod_distance, 360.0) ;
    float path_direct = 
      color1.z == min_hue ?
        color1.z + path_direct_distance :
        color1.z - path_direct_distance ;

    //  shorter
    if (hue_interpolation == 0.0) {
      color_out.z = 
        max_hue - min_hue > 360.0 - max_hue + min_hue ?
          path_mod :
          path_direct ;
    //  longer
    } else if (hue_interpolation == 1.0) { 
      color_out.z = 
        max_hue - min_hue <= 360.0 - max_hue + min_hue ?
          path_mod : 
          path_direct ;
    //  increasing
    } else if (hue_interpolation == 2.0) {
      color_out.z =
        color1.z > color2.z ?
          path_mod :
          path_direct ;
    //  decreasing
    } else if (hue_interpolation == 3.0) {
      color_out.z =
        color1.z <= color2.z ?
          path_mod :
          path_direct ;
    }
    color_out.rgb = clamp(oklab_to_linear(lch_to_lab(color_out.xyz)), 0.0, 1.0);
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
