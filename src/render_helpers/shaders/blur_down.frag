// Ported from https://github.com/nferhat/fht-compositor/blob/main/src/renderer/shaders/blur-down.frag

precision highp float;

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

varying vec2 niri_v_coords;
uniform float radius;
uniform vec2 half_pixel;

void main() {
    vec2 uv = niri_v_coords * 2.0;

    vec4 sum = texture2D(tex, uv) * 4.0;
    sum += texture2D(tex, uv - half_pixel * radius);
    sum += texture2D(tex, uv + half_pixel * radius);
    sum += texture2D(tex, uv + vec2(half_pixel.x, -half_pixel.y) * radius);
    sum += texture2D(tex, uv - vec2(half_pixel.x, -half_pixel.y) * radius);

    gl_FragColor = sum / 8.0;
}
