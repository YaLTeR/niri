
vec4 color_linear(vec4 color) {
  return vec4(
    pow(color.r, 2.0),
    pow(color.g, 2.0),
    pow(color.b, 2.0),
    color.a
  );
}

vec4 color_root(vec4 color) {
  return vec4(
    sqrt(color.r),
    sqrt(color.g),
    sqrt(color.b),
    color.a
  );
}

vec4 color_mix(vec4 color1, vec4 color2, float color_ratio) {
  
  float gradient_type = grad_format;

  if(gradient_type == 0.0) { //   CssLinear
    return mix(color1, color2, color_ratio);
  }
  
  vec4 color_out;

  color1 = color_linear(color1);
  color2 = color_linear(color2);

  if (gradient_type == 1.0) {
    color_out = mix(
      color1,
      color2,
      color_ratio
    );
  }else{
    color_out = vec4(255.0,0.0,0.0,1.0);
  }

  return color_root(color_out);
}
