// Gaussian blur shader - two-pass (horizontal + vertical) fragment-based blur
// Uses a 9-tap Gaussian kernel for quality/performance balance.
// direction uniform: (1,0) for horizontal pass, (0,1) for vertical pass.

struct BlurUniforms {
    resolution: vec2<f32>,
    direction: vec2<f32>,  // (1,0) or (0,1)
    radius: f32,
    _padding: f32,
    _padding2: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> blur_uniforms: BlurUniforms;

@group(1) @binding(0)
var input_texture: texture_2d<f32>;

@group(1) @binding(1)
var input_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Positions are in NDC already for fullscreen quad (-1..1)
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sigma = max(blur_uniforms.radius / 3.0, 0.5);
    let pixel_size = 1.0 / blur_uniforms.resolution;
    let dir = blur_uniforms.direction * pixel_size;

    let radius_i = i32(ceil(blur_uniforms.radius));
    let max_radius = min(radius_i, 32); // Cap at 32 taps per side

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var i = -max_radius; i <= max_radius; i++) {
        let offset = dir * f32(i);
        let sample_uv = in.uv + offset;
        let w = gaussian_weight(f32(i), sigma);
        color += textureSample(input_texture, input_sampler, sample_uv) * w;
        weight_sum += w;
    }

    return color / weight_sum;
}
