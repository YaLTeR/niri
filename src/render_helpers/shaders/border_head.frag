precision mediump float;

#if defined(DEBUG_FLAGS)
uniform float niri_tint;
#endif

uniform float niri_alpha;
uniform float niri_scale;

uniform vec2 niri_size;
varying vec2 niri_v_coords;

uniform float grad_format;
uniform vec4 color_from;
uniform vec4 color_to;
uniform vec2 grad_offset;
uniform float grad_width;
uniform vec2 grad_vec;

uniform mat3 input_to_geo;
uniform vec2 geo_size;
uniform vec4 outer_radius;
uniform float border_width;


// FIXME this is a terrible solution however,
// I need to insert a different file with 
// functions after this part as adding them ahead will cause errors at runtime
// and havent found a clean way to do this
// this is prob super simple and im just too stupid
