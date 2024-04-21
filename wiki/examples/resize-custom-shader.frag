#version 100

precision mediump float;

// Coordinates of the current pixel.
//
// These range from 0 to 1 over the whole area of the shader. The location and
// the size of the area are unspecified, but niri will make it large enough to
// accomodate a crossfade.
//
// You very likely want to convert these coordinates to geometry coordinates
// before using them (see below).
varying vec2 v_coords;

// Pixel size of the whole area of the shader.
uniform vec2 size;

// Matrix that converts the input v_coords into coordinates inside the current
// window geometry.
//
// The window geometry is its "visible bounds" from the user's perspective.
// After applying this matrix, the 0 to 1 coordinate range will correspond to
// the current geometry (in the middle of a resize), and pixels outside the
// geometry will have coordinates below 0 or above 1.
uniform mat3 input_to_curr_geo;

// Matrix that converts the input v_coords into coordinates inside the previous
// (before resize) window geometry.
uniform mat3 input_to_prev_geo;

// Matrix that converts the input v_coords into coordinates inside the next
// (after resize) window geometry.
uniform mat3 input_to_next_geo;

// Previous (before resize) window texture.
uniform sampler2D tex_prev;

// Matrix that converts geometry coordinates into the previous window texture
// coordinates.
//
// The window texture can and will go outside the geometry (for client-side
// decoration shadows for example), which is why this matrix is necessary.
uniform mat3 geo_to_tex_prev;

// Next (after resize) window texture.
uniform sampler2D tex_next;

// Matrix that converts geometry coordinates into the next window texture
// coordinates.
uniform mat3 geo_to_tex_next;

// Unclamped progress of the resize.
//
// Goes from 0 to 1 but may overshoot and oscillate.
uniform float progress;

// Clamped progress of the resize.
//
// Goes from 0 to 1, but will stop at 1 as soon as it first reaches 1. Will not
// overshoot or oscillate.
uniform float clamped_progress;

// Additional opacity to apply to the final color.
uniform float alpha;

// Example: fill the current geometry with a solid vertical gradient.
vec4 solid_gradient() {
    vec3 coords = input_to_curr_geo * vec3(v_coords, 1.0);

    vec4 color = vec4(0.0);

    // Paint only the area inside the current geometry.
    if (0.0 <= coords.x && coords.x <= 1.0
            && 0.0 <= coords.y && coords.y <= 1.0)
    {
        vec4 from = vec4(1.0, 0.0, 0.0, 1.0);
        vec4 to = vec4(0.0, 1.0, 0.0, 1.0);
        color = mix(from, to, coords.y);
    }

    return color;
}

// Example: crossfade between previous and next texture, stretched to the
// current geometry.
vec4 crossfade() {
    vec3 coords_curr_geo = input_to_curr_geo * vec3(v_coords, 1.0);

    vec3 coords_tex_prev = geo_to_tex_prev * coords_curr_geo;
    vec4 color_prev = texture2D(tex_prev, vec2(coords_tex_prev));

    vec3 coords_tex_next = geo_to_tex_next * coords_curr_geo;
    vec4 color_next = texture2D(tex_next, vec2(coords_tex_next));

    vec4 color = mix(color_prev, color_next, clamped_progress);
    return color;
}

// Example: next texture, stretched to the current geometry.
vec4 stretch_next() {
    vec3 coords_curr_geo = input_to_curr_geo * vec3(v_coords, 1.0);
    vec3 coords_tex_next = geo_to_tex_next * coords_curr_geo;
    vec4 color = texture2D(tex_next, vec2(coords_tex_next));
    return color;
}

// Example: next texture, stretched to the current geometry if smaller, and
// cropped if bigger.
vec4 stretch_or_crop_next() {
    vec3 coords_curr_geo = input_to_curr_geo * vec3(v_coords, 1.0);
    vec3 coords_next_geo = input_to_next_geo * vec3(v_coords, 1.0);

    vec3 coords_stretch = geo_to_tex_next * coords_curr_geo;
    vec3 coords_crop = geo_to_tex_next * coords_next_geo;

    // If the crop coord is smaller than the stretch coord, then the next
    // texture size is bigger than the current geometry, which means that we
    // can crop.
    vec3 coords = coords_stretch;
    if (coords_crop.x < coords_stretch.x)
        coords.x = coords_crop.x;
    if (coords_crop.y < coords_stretch.y)
        coords.y = coords_crop.y;

    vec4 color = texture2D(tex_next, vec2(coords));

    // However, when we crop, we also want to crop out anything outside the
    // current geometry. This is because the area of the shader is unspecified
    // and usually bigger than the current geometry, so if we don't fill pixels
    // outside with transparency, the texture will leak out.
    //
    // When stretching, this is not an issue because the area outside will
    // correspond to client-side decoration shadows, which are already supposed
    // to be outside.
    if (coords_crop.x < coords_stretch.x
            && (coords_curr_geo.x < 0.0 || 1.0 < coords_curr_geo.x))
        color = vec4(0.0);
    if (coords_crop.y < coords_stretch.y
            && (coords_curr_geo.y < 0.0 || 1.0 < coords_curr_geo.y))
        color = vec4(0.0);

    return color;
}

// The main entry point of the shader.
void main() {
    // You can pick one of the example functions or write your own.
    vec4 color = stretch_or_crop_next();

    gl_FragColor = color * alpha;
}

