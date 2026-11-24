// Fullscreen blend shader — composites `src_texture` over `dst_texture`
// using W3C Compositing & Blending Level 1 modes. Both input textures and
// the render target use premultiplied alpha.

struct BlendUniforms {
    mode: u32,
    srgb_input: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0)
var src_texture: texture_2d<f32>;
@group(0) @binding(1)
var src_sampler: sampler;

@group(1) @binding(0)
var dst_texture: texture_2d<f32>;
@group(1) @binding(1)
var dst_sampler: sampler;

@group(2) @binding(0)
var<uniform> u: BlendUniforms;

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
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow(max((c + 0.055) / 1.055, vec3<f32>(0.0)), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn blend_hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let lo = 2.0 * b * s;
    let hi = 1.0 - 2.0 * (1.0 - b) * (1.0 - s);
    return select(hi, lo, s <= vec3<f32>(0.5));
}

fn blend_soft_light_ch(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    let d = select(sqrt(max(b, 0.0)), ((16.0 * b - 12.0) * b + 4.0) * b, b <= 0.25);
    return b + (2.0 * s - 1.0) * (d - b);
}

fn blend_rgb(mode: u32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 1u: { return b * s; }                                                              // Multiply
        case 2u: { return b + s - b * s; }                                                      // Screen
        case 3u: { return blend_hard_light(s, b); }                                             // Overlay
        case 4u: { return min(b, s); }                                                          // Darken
        case 5u: { return max(b, s); }                                                          // Lighten
        case 6u: {                                                                              // ColorDodge
            let res = min(vec3<f32>(1.0), b / max(vec3<f32>(1.0) - s, vec3<f32>(1e-5)));
            return select(res, vec3<f32>(0.0), b <= vec3<f32>(0.0));
        }
        case 7u: {                                                                              // ColorBurn
            let res = vec3<f32>(1.0) - min(vec3<f32>(1.0), (vec3<f32>(1.0) - b) / max(s, vec3<f32>(1e-5)));
            return select(res, vec3<f32>(1.0), b >= vec3<f32>(1.0));
        }
        case 8u: { return vec3<f32>(blend_soft_light_ch(b.r, s.r), blend_soft_light_ch(b.g, s.g), blend_soft_light_ch(b.b, s.b)); } // SoftLight
        case 9u: { return blend_hard_light(b, s); }                                             // HardLight
        case 10u: { return abs(b - s); }                                                        // Difference
        case 11u: { return b + s - 2.0 * b * s; }                                               // Exclusion
        case 12u: { return min(b + s, vec3<f32>(1.0)); }                                        // Add
        default: { return s; }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(src_texture, src_sampler, in.uv);
    let dst = textureSample(dst_texture, dst_sampler, in.uv);
    if src.a <= 1e-5 {
        return dst;
    }
    let src_lin = clamp(src.rgb / src.a, vec3<f32>(0.0), vec3<f32>(1.0));
    let dst_lin = select(vec3<f32>(0.0), clamp(dst.rgb / dst.a, vec3<f32>(0.0), vec3<f32>(1.0)), dst.a > 1e-5);
    let b = select(dst_lin, linear_to_srgb(dst_lin), u.srgb_input > 0.5);
    let s = select(src_lin, linear_to_srgb(src_lin), u.srgb_input > 0.5);
    let blended_srgb = clamp(blend_rgb(u.mode, b, s), vec3<f32>(0.0), vec3<f32>(1.0));
    let blended_lin = select(blended_srgb, srgb_to_linear(blended_srgb), u.srgb_input > 0.5);

    let out_rgb = src.a * (1.0 - dst.a) * src_lin + dst.a * (1.0 - src.a) * dst_lin + src.a * dst.a * blended_lin;
    let out_a = src.a + dst.a * (1.0 - src.a);
    return vec4<f32>(out_rgb, out_a);
}
