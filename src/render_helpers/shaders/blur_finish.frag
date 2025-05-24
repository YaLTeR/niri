// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/shaders/blur-finish.frag

precision highp float;

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
uniform samplerExternalOES text;
#else
uniform sampler2D text;
#endif

uniform float alpha;
varying vec2 v_coords;

uniform vec4 geo;           // x, y, width, height
uniform float corner_radius;
uniform float noise;

float rounding_alpha(vec2 coords, vec2 size, float radius) {
    vec2 center;

    if (coords.x < radius && coords.y < radius) {
        center = vec2(radius, radius);
    } else if (coords.x > size.x - radius && coords.y < radius) {
        center = vec2(size.x - radius, radius);
    } else if (coords.x > size.x - radius && coords.y > size.y - radius) {
        center = vec2(size.x - radius, size.y - radius);
    } else if (coords.x < radius && coords.y > size.y - radius) {
        center = vec2(radius, size.y - radius);
    } else {
        return 1.0;
    }

    float dist = distance(coords, center);
    float half_px = 0.5;
    return 1.0 - smoothstep(radius - half_px, radius + half_px, dist);
}

float hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 727.727);
    p3 += dot(p3, p3.xyz + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

void main() {
    vec4 color = texture2D(text, v_coords);

#if defined(NO_ALPHA)
    color.a = 1.0;
#endif

    vec2 size = geo.zw;
    vec2 loc = gl_FragCoord.xy - geo.xy;

    float noiseHash = hash(loc / size);
    float noiseAmount = noiseHash - 0.5;
    color.rgb += noiseAmount * noise;

    color *= rounding_alpha(loc, size, corner_radius);

    color *= alpha;

#if defined(DEBUG_FLAGS)
    // Optional debug tint omitted to match example
#endif

    gl_FragColor = color;
}
