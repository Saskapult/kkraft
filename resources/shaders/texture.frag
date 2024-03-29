#version 450

layout(location = 0) in vec2 tc;
layout(location = 0) out vec4 outAttatchment0;

layout(set = 1, binding = 0) uniform texture2D ttexture;
layout(set = 1, binding = 1) uniform sampler ssampler;

void main() {
    outAttatchment0 = vec4(texture(sampler2D(ttexture, ssampler), tc).rgb, 1.0);
    // outAttatchment0 = vec4(1.0, 1.0, 1.0, 1.0);
}
