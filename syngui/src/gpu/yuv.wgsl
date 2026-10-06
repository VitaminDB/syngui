// Видеокадр в YUV 4:2:0 (NV12 или I420) → RGB: перевод цвета вместо swscale
// на CPU. Вершины и обрезка — как у картинок (`text.wgsl`, ветка data.x = 2).

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _padding: f32,
    clip_rect: vec4<f32>,
    clip_corner_radius: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0) var plane_y: texture_2d<f32>;
// NV12: UV в каналах rg; I420: U здесь, V — в plane_c2.
@group(1) @binding(1) var plane_c1: texture_2d<f32>;
@group(1) @binding(2) var plane_c2: texture_2d<f32>;
@group(1) @binding(3) var yuv_sampler: sampler;

// rgb = M · (yuv − off): строки M в r0..r2 (по Y, Cb, Cr), off.xyz — сдвиги,
// off.w = 1 — NV12 (см. `YuvFrame::shader_uniform`).
struct YuvParams {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
    off: vec4<f32>,
};
@group(1) @binding(4) var<uniform> yuv: YuvParams;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) data: vec4<f32>,
    @location(4) data2: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) data: vec4<f32>,
    @location(3) logical_pos: vec2<f32>,
    // Для shadow-blur пути: bbox исходного глифа в UV (uv_min.xy, uv_max.xy).
    // Сэмплируем gaussian-taps с clamp'ом, чтобы не читать соседей по атласу.
    @location(4) data2: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Convert pixel coordinates to NDC
    let ndc_x = (in.position.x / uniforms.resolution.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / uniforms.resolution.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

    out.uv = in.uv;
    out.color = in.color;
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
    // Пустой clip_rect — обрезать нечего: границы легли на границы пикселей,
    // и всё сделали ножницы (см. `write_clip_uniform_slots`).
    let clip_size = uniforms.clip_rect.zw;
    if clip_size.x <= 0.0 || clip_size.y <= 0.0 {
        return color;
    }
    let cr = uniforms.clip_corner_radius;
    let d = rounded_clip_sdf(logical_pos, uniforms.clip_rect.xy, clip_size, cr);
    let aa = fwidth(d) * 0.75;
    let clip_alpha = 1.0 - smoothstep(-aa, aa, d);
    if color.a * clip_alpha < 0.001 {
        discard;
    }
    return vec4(color.rgb, color.a * clip_alpha);
}

// Кривая sRGB → линейный свет: RGB из YUV закодирован гаммой, а вывод идёт
// в sRGB-поверхность, как и выборка из sRGB-текстур картинок.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(plane_y, yuv_sampler, in.uv).r;
    let c1 = textureSample(plane_c1, yuv_sampler, in.uv);
    let c2 = textureSample(plane_c2, yuv_sampler, in.uv).r;
    var cbcr = vec2(c1.r, c2);
    if (yuv.off.w > 0.5) {
        cbcr = c1.rg;
    }
    let v = vec3(y, cbcr) - yuv.off.xyz;
    let rgb = clamp(vec3(dot(yuv.r0.xyz, v), dot(yuv.r1.xyz, v), dot(yuv.r2.xyz, v)), vec3(0.0), vec3(1.0));
    return apply_rounded_clip(vec4(srgb_to_linear(rgb), 1.0) * in.color, in.logical_pos);
}
