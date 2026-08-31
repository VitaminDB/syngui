// Text shader - samples from RGBA font atlas texture
// Supports both mono glyphs (tinted by vertex color) and color emoji

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    _padding: f32,
    clip_rect: vec4<f32>,
    clip_corner_radius: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0) var font_atlas: texture_2d<f32>;
@group(1) @binding(1) var font_sampler: sampler;

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

// Покрытие глифа с поправкой на линейный блендинг.
//
// Кадр собирается в линейном пространстве (surface — sRGB-формат), поэтому
// GPU смешивает глиф с фоном по линейным величинам: полупокрытый пиксель
// чёрного текста на белом листе даёт линейные 0.5 — это sRGB 0.73, а не 0.5,
// какие дал бы перцептивный блендинг. Штрихи мелкого кегля почти целиком
// состоят из таких пикселей, и текст выцветает до серой каши.
//
// Возвращаем покрытию перцептивный смысл: для тёмного текста подтягиваем
// альфу вверх (эмуляция смешения по светлому фону), для светлого — вниз (по
// тёмному). Смешиваем оба края по яркости самого текста: фон шейдеру не
// виден, но текст почти всегда контрастен фону, и знак поправки от этого не
// зависит. Покрытие 0 и 1 обе ветви оставляют на месте — сплошная заливка
// глифа не меняется, правится только антиалиасинг.
fn gamma_coverage(a: f32, color: vec3<f32>) -> f32 {
    let luma = clamp(dot(color, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let t = pow(luma, 1.0 / 2.2);
    let dark = 1.0 - pow(1.0 - a, 2.2);
    let light = pow(a, 2.2);
    return mix(dark, light, t);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.data.x > 1.5) {
        // Image: multiply texture by vertex color (tint)
        let texel = textureSample(font_atlas, font_sampler, in.uv);
        return apply_rounded_clip(texel * in.color, in.logical_pos);
    } else if (in.data.x > 0.5) {
        // Color glyph (emoji): use texture RGBA directly (blur не применяется
        // для emoji в v1 — gaussian работает только по alpha-каналу)
        let texel = textureSample(font_atlas, font_sampler, in.uv);
        return apply_rounded_clip(texel, in.logical_pos);
    }

    // Mono glyph (или mono shadow-pass): alpha из атласа, tint vertex'ом.
    let blur = in.data.y;
    if (blur <= 0.0) {
        // Coverage из растеризатора берётся без S-кривой (smoothstep искажал
        // AA), но с поправкой на линейный блендинг — см. `gamma_coverage`.
        let texel = textureSample(font_atlas, font_sampler, in.uv);
        let alpha = gamma_coverage(texel.a, in.color.rgb);
        return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * alpha), in.logical_pos);
    }

    // Gaussian shadow blur: многотаповая выборка alpha-канала из глиф-bbox
    // с clamp'ом UV в `data2.xy..data2.zw` — соседи по атласу не читаются.
    let uv_min = in.data2.xy;
    let uv_max = in.data2.zw;
    let duv = vec2<f32>(
        max(abs(dpdx(in.uv.x)), abs(dpdy(in.uv.x))),
        max(abs(dpdx(in.uv.y)), abs(dpdy(in.uv.y))),
    );
    let sigma = max(blur / 3.0, 0.5);
    // Количество tap'ов по каждой полуоси (всего (2k+1)^2). Capped at 6.
    let k = i32(clamp(ceil(blur), 1.0, 6.0));
    var alpha_sum = 0.0;
    var w_sum = 0.0;
    for (var j = -6; j <= 6; j = j + 1) {
        if (j < -k || j > k) { continue; }
        for (var i = -6; i <= 6; i = i + 1) {
            if (i < -k || i > k) { continue; }
            let off = vec2<f32>(f32(i), f32(j)) * duv;
            let w = exp(-(f32(i * i + j * j)) / (2.0 * sigma * sigma));
            let sample_uv = clamp(in.uv + off, uv_min, uv_max);
            // textureSampleLevel: derivatives нам не нужны (атлас без mip'ов),
            // и работает в non-uniform control flow (внутри continue-цикла).
            let a = textureSampleLevel(font_atlas, font_sampler, sample_uv, 0.0).a;
            alpha_sum = alpha_sum + a * w;
            w_sum = w_sum + w;
        }
    }
    let alpha = alpha_sum / max(w_sum, 1.0e-6);
    return apply_rounded_clip(
        vec4<f32>(in.color.rgb, in.color.a * smoothstep(0.0, 1.0, alpha)),
        in.logical_pos,
    );
}
