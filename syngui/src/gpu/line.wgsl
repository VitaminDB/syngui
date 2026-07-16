// Line segment shader with SDF anti-aliased rendering.
// Each line segment is a quad with endpoints encoded in data2.
// Fragment shader computes distance to the line segment for smooth AA.
//
// Vertex layout:
//   position: quad corner (expanded from segment endpoints)
//   uv:       unused
//   color:    line color RGBA
//   data:     [line_width, feather, 0, 0]
//   data2:    [A.x, A.y, B.x, B.y] — segment endpoints in screen pixels

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
    @location(1) world_pos: vec2<f32>,
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
    out.world_pos = in.position;
    out.data = in.data;
    out.data2 = in.data2;
    out.logical_pos = in.position;

    return out;
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
    let a = vec2<f32>(in.data2.x, in.data2.y);
    let b = vec2<f32>(in.data2.z, in.data2.w);
    let line_width = in.data.x;
    let feather = in.data.y;

    // SDF: distance from fragment to line segment AB
    let ba = b - a;
    let pa = in.world_pos - a;
    let ba_len_sq = dot(ba, ba);

    // Parameter along segment (clamped to [0, 1] for segment caps)
    let t = clamp(dot(pa, ba) / ba_len_sq, 0.0, 1.0);
    let closest = a + t * ba;
    let dist = length(in.world_pos - closest);

    // Anti-aliased edge via smoothstep
    let half_w = line_width * 0.5;
    let alpha = 1.0 - smoothstep(half_w - feather, half_w + feather, dist);

    if alpha < 0.004 {
        discard;
    }

    return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * alpha), in.logical_pos);
}
