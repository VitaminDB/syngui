// Shadow shader - SDF-based soft box shadow with Gaussian falloff
// Vertex data: [blur_radius, corner_radius, rect_width, rect_height]
// The quad is expanded by blur_radius on all sides beyond the inner rect.
// UV [0,1] maps over the expanded quad.

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _padding: f32,
    clip_rect: vec4<f32>,
    clip_corner_radius: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) data: vec4<f32>,
    @location(4) data2: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) data: vec4<f32>,
    @location(3) logical_pos: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let ndc_x = (in.position.x / uniforms.resolution.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / uniforms.resolution.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

    out.color = in.color;
    out.uv = in.uv;
    out.data = in.data;
    out.logical_pos = in.position;

    return out;
}

// SDF for axis-aligned rounded rectangle centered at origin
fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// SDF for rounded clip rectangle in logical pixel coordinates
fn rounded_clip_sdf(pos: vec2<f32>, rect_min: vec2<f32>, rect_size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let center = rect_min + rect_size * 0.5;
    let half = rect_size * 0.5;
    let p = pos - center;
    var r: f32;
    if p.x < 0.0 {
        if p.y < 0.0 { r = radius.x; } else { r = radius.w; }
    } else {
        if p.y < 0.0 { r = radius.y; } else { r = radius.z; }
    }
    let q = abs(p) - half + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - r;
}

fn apply_rounded_clip(color: vec4<f32>, logical_pos: vec2<f32>) -> vec4<f32> {
    let cr = uniforms.clip_corner_radius;
    if cr.x <= 0.0 && cr.y <= 0.0 && cr.z <= 0.0 && cr.w <= 0.0 {
        return color;
    }
    let d = rounded_clip_sdf(logical_pos, uniforms.clip_rect.xy, uniforms.clip_rect.zw, cr);
    let aa = fwidth(d) * 0.75;
    let clip_alpha = 1.0 - smoothstep(-aa, aa, d);
    if color.a * clip_alpha < 0.001 {
        discard;
    }
    return vec4(color.rgb, color.a * clip_alpha);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let blur_radius = in.data.x;
    let corner_radius = in.data.y;
    let rect_w = in.data.z;
    let rect_h = in.data.w;

    // Total quad size = inner rect + 2 * blur_radius padding on each side
    let quad_w = rect_w + blur_radius * 2.0;
    let quad_h = rect_h + blur_radius * 2.0;

    // Map UV to pixel coordinates relative to quad center
    let p = (in.uv - 0.5) * vec2<f32>(quad_w, quad_h);

    // Inner rect half-size
    let half_size = vec2<f32>(rect_w, rect_h) * 0.5;

    // SDF distance to the inner rounded rect
    let d = rounded_box_sdf(p, half_size, corner_radius);

    // Gaussian falloff: sigma = blur_radius / 3 (so 3-sigma ≈ edge of blur)
    let sigma = max(blur_radius / 3.0, 0.001);
    let falloff = exp(-(max(d, 0.0) * max(d, 0.0)) / (2.0 * sigma * sigma));

    // Inside the rect, full shadow; outside, Gaussian falloff
    var alpha: f32;
    if d <= 0.0 {
        alpha = 1.0;
    } else {
        alpha = falloff;
    }

    return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * alpha), in.logical_pos);
}
