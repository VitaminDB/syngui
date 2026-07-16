// Inner shadow shader — SDF-based inset shadow.
// The shadow is cast from a contracted inner rectangle, creating a gradient
// that extends inward by blur_radius pixels from the element edges.
// Vertex data: [blur_radius, corner_radius, rect_width, rect_height]
// data2: [offset_x, offset_y, 0, 0]

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
    @location(3) data2: vec4<f32>,
    @location(4) logical_pos: vec2<f32>,
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
    out.data2 = in.data2;
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
    let offset_x = in.data2.x;
    let offset_y = in.data2.y;

    // Map UV [0,1] to pixel coords relative to rect center
    let p = (in.uv - 0.5) * vec2<f32>(rect_w, rect_h);

    // Outer boundary SDF — clip to element bounds
    let half_size = vec2<f32>(rect_w, rect_h) * 0.5;
    let d_outer = rounded_box_sdf(p, half_size, corner_radius);

    // Discard pixels outside the element boundary
    if d_outer > 0.0 {
        discard;
    }

    // Inner shadow: contract the rect by blur_radius, then shift by offset.
    // Shadow appears in the gap between the outer and contracted rects.
    let contract = blur_radius * 0.5;
    let inner_half = max(half_size - vec2<f32>(contract), vec2<f32>(0.0));
    let inner_radius = max(corner_radius - contract, 0.0);
    let shadow_center = vec2<f32>(offset_x, offset_y);
    let d_inner = rounded_box_sdf(p - shadow_center, inner_half, inner_radius);

    // d_inner > 0: outside contracted rect → shadow visible
    // d_inner <= 0: inside contracted rect → no shadow
    // smoothstep creates a soft gradient over blur_radius
    let alpha = smoothstep(0.0, blur_radius, d_inner);

    if alpha < 0.001 {
        discard;
    }

    return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * alpha), in.logical_pos);
}
