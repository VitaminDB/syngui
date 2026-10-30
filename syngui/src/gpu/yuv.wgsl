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

// rgb = M · (yuv·hdr.x − off): строки M в r0..r2 (по Y, Cb, Cr), off.xyz — сдвиги,
// off.w = 1 — UV в одной плоскости (NV12, P010). hdr: x — множитель выборки (P010),
// y — 0 SDR / 1 PQ / 2 HLG, z — PQ-сигнал пика ролика, w — пик SDR в долях его (см.
// `YuvFrame::shader_uniform`).
struct YuvParams {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
    off: vec4<f32>,
    hdr: vec4<f32>,
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

// ─── HDR → SDR (то же, что `video::tonemap` на процессоре) ───────────────────

const PQ_M1: f32 = 0.1593017578125;
const PQ_M2: f32 = 78.84375;
const PQ_C1: f32 = 0.8359375;
const PQ_C2: f32 = 18.8515625;
const PQ_C3: f32 = 18.6875;
const SDR_WHITE: f32 = 203.0;

// PQ-сигнал → нит.
fn pq_eotf(e: vec3<f32>) -> vec3<f32> {
    let p = pow(clamp(e, vec3(0.0), vec3(1.0)), vec3(1.0 / PQ_M2));
    return 10000.0 * pow(max(p - PQ_C1, vec3(0.0)) / (PQ_C2 - PQ_C3 * p), vec3(1.0 / PQ_M1));
}

fn pq_eotf1(e: f32) -> f32 {
    return pq_eotf(vec3(e)).x;
}

// Нит → PQ-сигнал.
fn pq_inv(n: f32) -> f32 {
    let y = pow(clamp(n / 10000.0, 0.0, 1.0), PQ_M1);
    return pow((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y), PQ_M2);
}

// HLG: сигнал → нит (обратная OETF и OOTF BT.2100 для дисплея 1000 нит).
fn hlg_eotf(e: vec3<f32>) -> vec3<f32> {
    let a = 0.17883277;
    let b = 0.28466892;
    let c = 0.55991073;
    let lo = e * e / 3.0;
    let hi = (exp((e - c) / a) + b) / 12.0;
    let scene = select(hi, lo, e <= vec3(0.5));
    return 1000.0 * pow(max(scene, vec3(0.0)), vec3(1.2));
}

// BT.2390 EETF: нормированный PQ-сигнал источника → для приёмника с пиком max_lum.
fn bt2390(e: f32, max_lum: f32) -> f32 {
    let ks = 1.5 * max_lum - 0.5;
    if e < ks {
        return e;
    }
    let t = (e - ks) / (1.0 - ks);
    let t2 = t * t;
    let t3 = t2 * t;
    return (2.0 * t3 - 3.0 * t2 + 1.0) * ks + (t3 - 2.0 * t2 + t) * (1.0 - ks) + (-2.0 * t3 + 3.0 * t2) * max_lum;
}

// R'G'B' BT.2020 (PQ или HLG) → линейный BT.709 для SDR-вывода: белый SDR 203 нит → 1.0.
fn hdr_to_sdr(rgb: vec3<f32>) -> vec3<f32> {
    var nits = pq_eotf(rgb);
    if yuv.hdr.y > 1.5 {
        nits = hlg_eotf(rgb);
    }
    // тонмаппинг по максимуму RGB — оттенок не плывёт
    let m = max(nits.r, max(nits.g, nits.b));
    if m > 1e-4 {
        let e = min(pq_inv(m) / yuv.hdr.z, 1.0);
        let out = pq_eotf1(bt2390(e, yuv.hdr.w) * yuv.hdr.z);
        nits = nits * (out / m);
    }
    let l = nits / SDR_WHITE;
    var o = vec3(
        1.660491 * l.r - 0.587641 * l.g - 0.072850 * l.b,
        -0.124550 * l.r + 1.132900 * l.g - 0.008349 * l.b,
        -0.018151 * l.r - 0.100579 * l.g + 1.118730 * l.b,
    );
    // вне охвата BT.709: к яркости, пока не войдёт
    let mn = min(o.r, min(o.g, o.b));
    if mn < 0.0 {
        let lum = dot(o, vec3(0.2126, 0.7152, 0.0722));
        if lum > 0.0 {
            o = vec3(lum) + (o - vec3(lum)) * (lum / (lum - mn));
        } else {
            o = vec3(0.0);
        }
    }
    return clamp(o, vec3(0.0), vec3(1.0));
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
    let v = vec3(y, cbcr) * yuv.hdr.x - yuv.off.xyz;
    let rgb = clamp(vec3(dot(yuv.r0.xyz, v), dot(yuv.r1.xyz, v), dot(yuv.r2.xyz, v)), vec3(0.0), vec3(1.0));
    var lin = srgb_to_linear(rgb);
    if yuv.hdr.y > 0.5 {
        lin = hdr_to_sdr(rgb);
    }
    return apply_rounded_clip(vec4(lin, 1.0) * in.color, in.logical_pos);
}
