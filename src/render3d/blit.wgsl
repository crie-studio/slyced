// Composites the offscreen volume image into egui's render pass.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VsOut;
    o.pos = vec4<f32>(corners[i], 0.0, 1.0);
    o.ndc = corners[i];
    return o;
}

@group(0) @binding(0) var blit_src: texture_2d<f32>;
@group(0) @binding(1) var blit_samp: sampler;

@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.ndc.x * 0.5 + 0.5, 0.5 - in.ndc.y * 0.5);
    return vec4<f32>(textureSampleLevel(blit_src, blit_samp, uv, 0.0).rgb, 1.0);
}

// Variant for sRGB targets: the offscreen image is already gamma encoded.
@fragment
fn fs_blit_srgb(in: VsOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.ndc.x * 0.5 + 0.5, 0.5 - in.ndc.y * 0.5);
    let c = textureSampleLevel(blit_src, blit_samp, uv, 0.0);
    return vec4<f32>(pow(c.rgb, vec3<f32>(2.2)), 1.0);
}
