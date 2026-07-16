// Rect shader with SDF rounded corners and border support
// Vertex attributes: position, uv, color, data(corner_radii), data2(border)

struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    scale_factor: f32,
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

    // Vertex placement:
    //   - В sharp per-side border mode (no-radius, data2.x ∈ [-51,-50])
    //     vertex'ы кладём ровно на integer device-pixel boundary (shift=0).
    //     Тогда pixel centers распределены симметрично: pixel 0 → uv=0.5/W,
    //     pixel W-1 → uv=(W-0.5)/W, edge_l = edge_r = 0.5 на крайних
    //     pixel'ах. smoothstep(0.5, 1.5, 0.5) = 0 → mask=1 одинаково на
    //     обеих сторонах. Это даёт точный 1 px бордер с равной плотностью.
    //   - В остальных режимах (rounded SDF, simple rect, uniform border)
    //     оставляем half-pixel shift: vertex corner align'ится с pixel
    //     center, что даёт корректную derivative-аппроксимацию dpdx/dpdy.
    let is_sharp_per_side = in.data2.x < -10.0;
    let shift = select(0.5 / uniforms.scale_factor, 0.0, is_sharp_per_side);
    let ndc_x = ((in.position.x + shift) / uniforms.resolution.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - ((in.position.y + shift) / uniforms.resolution.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

    out.color = in.color;
    out.uv = in.uv;
    out.data = in.data; // corner radii: [tl, tr, br, bl]
    out.data2 = in.data2; // border: [width, r, g, b]
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

// Apply rounded clip mask to output color
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

// SDF for a rounded rectangle
fn rounded_rect_sdf(uv: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    // Select corner radius based on quadrant
    var r: f32;
    if uv.x < 0.5 {
        if uv.y < 0.5 {
            r = radius.x; // top-left
        } else {
            r = radius.w; // bottom-left
        }
    } else {
        if uv.y < 0.5 {
            r = radius.y; // top-right
        } else {
            r = radius.z; // bottom-right
        }
    }

    // Map UV to centered coordinates in pixel space
    let half_size = size * 0.5;
    let p = (uv - 0.5) * size;

    // SDF for rounded rect
    let q = abs(p) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// SDF outline ring rendering.
//
// data2.x == -2.0 (flag), data2.z == ring_width in physical pixels.
// The vertex quad is the element bounds + (offset + ring_width) on each side.
// The ring occupies the outermost `ring_width` pixels of the quad.
// Fragment color is taken from in.color (outline color with alpha).
fn fs_outline_ring(in: VertexOutput) -> vec4<f32> {
    let ring_width_px = in.data2.z;

    // Estimate outer quad size from UV derivatives (physical pixels per UV unit)
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let outer_size = vec2<f32>(
        1.0 / max(abs(dx.x), 0.0001),
        1.0 / max(abs(dy.y), 0.0001),
    );

    // Convert UV to pixel coordinates from the quad center.
    // This avoids the UV-space mismatch when computing inner SDF:
    // UV=0 is the outer quad corner, but inner_size < outer_size,
    // so (uv-0.5)*inner_size gives wrong inner-quad SDF values.
    let p = (in.uv - 0.5) * outer_size;

    // Outer SDF: pixel coords vs outer half-extents
    let outer_half = outer_size * 0.5;
    // Per-corner radius based on quadrant (in.data = [tl, tr, br, bl])
    var r_o: f32;
    if p.x < 0.0 {
        if p.y < 0.0 { r_o = in.data.x; } else { r_o = in.data.w; }
    } else {
        if p.y < 0.0 { r_o = in.data.y; } else { r_o = in.data.z; }
    }
    let q_o = abs(p) - outer_half + r_o;
    let d_outer = min(max(q_o.x, q_o.y), 0.0) + length(max(q_o, vec2<f32>(0.0))) - r_o;

    // Inner SDF: pixel coords vs inner half-extents (ring_width inset from outer)
    let inner_half = max(outer_half - ring_width_px, vec2<f32>(0.0));
    // Inner corner radius: same as outer (approximately correct for small ring widths)
    var r_i: f32;
    if p.x < 0.0 {
        if p.y < 0.0 { r_i = in.data.x; } else { r_i = in.data.w; }
    } else {
        if p.y < 0.0 { r_i = in.data.y; } else { r_i = in.data.z; }
    }
    r_i = max(r_i - ring_width_px, 0.0);
    let q_i = abs(p) - inner_half + r_i;
    let d_inner = min(max(q_i.x, q_i.y), 0.0) + length(max(q_i, vec2<f32>(0.0))) - r_i;

    let aa = fwidth(d_outer) * 0.75;

    // Inside outer boundary (the full quad area)
    let in_outer = smoothstep(aa, -aa, d_outer);
    // Outside inner boundary (ring area, not the hole inside)
    let out_inner = smoothstep(-aa, aa, d_inner);

    let ring_alpha = in_outer * out_inner;
    if ring_alpha < 0.001 {
        discard;
    }
    return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * ring_alpha), in.logical_pos);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let max_radius = max(max(in.data.x, in.data.y), max(in.data.z, in.data.w));
    let border_width = in.data2.x;

    // Per-side border, no-radius mode: data2.x = -(50+α) ∈ [-51, -50].
    // Точный rect_size в data.xy, outer_alpha=1 (без SDF-AA halo) —
    // pixel-aligned 1 px бордер симметричен со всех сторон.
    if border_width < -10.0 {
        return fs_per_side_border_sharp(in);
    }

    // Outline ring mode: data2.x == -3.0
    if border_width < -2.5 {
        return fs_outline_ring(in);
    }

    // Per-side border mode (rounded): data2.x ∈ [-2, -1]
    if border_width < -0.5 {
        return fs_per_side_border(in, max_radius);
    }

    // Simple rect without rounding or border
    if max_radius < 0.5 && border_width < 0.5 {
        return apply_rounded_clip(in.color, in.logical_pos);
    }

    // Use explicit rect size from data2.zw if available, else estimate from derivatives
    // Border alpha is packed into data2.w: floor(height) * 256 + round(alpha * 255)
    var rect_size: vec2<f32>;
    var border_alpha: f32 = 1.0;
    if in.data2.z > 0.5 {
        let raw_w = in.data2.w;
        let h = floor(raw_w / 256.0);
        border_alpha = (raw_w - h * 256.0) / 255.0;
        rect_size = vec2<f32>(in.data2.z, h);
    } else {
        let dx = dpdx(in.uv);
        let dy = dpdy(in.uv);
        rect_size = vec2<f32>(1.0 / max(abs(dx.x), 0.0001), 1.0 / max(abs(dy.y), 0.0001));
    }

    // Sharp border path: no rounding, use UV-based edge detection
    if max_radius < 0.5 && border_width >= 0.5 {
        let packed = in.data2.y;
        let ri = floor(packed / 65536.0);
        let gi = floor((packed - ri * 65536.0) / 256.0);
        let bi = packed - ri * 65536.0 - gi * 256.0;
        let border_color = vec3<f32>(ri, gi, bi) / 255.0;

        // Distance from each edge in pixels.
        // edge_r/edge_b считаются через `rect_size - edge_l/edge_t` —
        // алгебраически эквивалентно `(1 - uv) * rect_size`, но без потери
        // точности при `1 - uv` (для больших rect uv ≈ 1.0 имеет mantissa
        // в области, где младшие биты теряются, а последующее умножение
        // усугубляет ошибку → правый/нижний бордер получался на ~1 ULP
        // тоньше левого/верхнего).
        let edge_l = in.uv.x * rect_size.x;
        let edge_t = in.uv.y * rect_size.y;
        let edge_r = rect_size.x - edge_l;
        let edge_b = rect_size.y - edge_t;
        let edge_dist = min(min(edge_l, edge_r), min(edge_t, edge_b));

        let border_mask = 1.0 - smoothstep(border_width - 0.5, border_width + 0.5, edge_dist);
        let rgb = mix(in.color.rgb, border_color, border_mask);
        let alpha = mix(in.color.a, border_alpha, border_mask);

        if alpha < 0.001 {
            discard;
        }
        return apply_rounded_clip(vec4<f32>(rgb, alpha), in.logical_pos);
    }

    let d = rounded_rect_sdf(in.uv, rect_size, in.data);

    // Anti-aliased edge
    let aa = fwidth(d) * 0.75;
    let outer_alpha = 1.0 - smoothstep(-aa, aa, d);

    // No border — just fill with rounded corners
    if border_width < 0.5 {
        return apply_rounded_clip(vec4<f32>(in.color.rgb, in.color.a * outer_alpha), in.logical_pos);
    }

    // Unpack border color from packed float: r*65536 + g*256 + b (each 0-255)
    let packed = in.data2.y;
    let ri = floor(packed / 65536.0);
    let gi = floor((packed - ri * 65536.0) / 256.0);
    let bi = packed - ri * 65536.0 - gi * 256.0;
    let border_color = vec3<f32>(ri, gi, bi) / 255.0;

    // border_mask: 1 in border zone (d > -border_width), 0 in fill zone
    let border_mask = smoothstep(-border_width - 0.5, -border_width + 0.5, d);

    // Mix fill and border based on mask, apply outer AA
    let rgb = mix(in.color.rgb, border_color, border_mask);
    let alpha = mix(in.color.a, border_alpha, border_mask) * outer_alpha;

    if alpha < 0.001 {
        discard;
    }

    return apply_rounded_clip(vec4<f32>(rgb, alpha), in.logical_pos);
}

// Per-side border rendering.
// data2.x = -(1.0 + border_alpha) (flag + alpha)
// data2.y = packed RGB border color
// data2.z = left_width * 256 + top_width (physical pixels)
// data2.w = right_width * 256 + bottom_width (physical pixels)
fn fs_per_side_border(in: VertexOutput, max_radius: f32) -> vec4<f32> {
    // Estimate rect size from UV derivatives
    let dx = dpdx(in.uv);
    let dy = dpdy(in.uv);
    let rect_size = vec2<f32>(1.0 / max(abs(dx.x), 0.0001), 1.0 / max(abs(dy.y), 0.0001));

    // Unpack per-side widths
    let lw = floor(in.data2.z / 256.0);
    let tw = in.data2.z - lw * 256.0;
    let rw = floor(in.data2.w / 256.0);
    let bw = in.data2.w - rw * 256.0;

    // Unpack border color
    let packed = in.data2.y;
    let ri = floor(packed / 65536.0);
    let gi = floor((packed - ri * 65536.0) / 256.0);
    let bi = packed - ri * 65536.0 - gi * 256.0;
    let border_color = vec3<f32>(ri, gi, bi) / 255.0;

    // SDF for rounded rect outer shape
    let d = rounded_rect_sdf(in.uv, rect_size, in.data);
    let aa = fwidth(d) * 0.75;
    let outer_alpha = 1.0 - smoothstep(-aa, aa, d);

    // Distance from each edge in pixels
    let edge_l = in.uv.x * rect_size.x;
    let edge_r = (1.0 - in.uv.x) * rect_size.x;
    let edge_t = in.uv.y * rect_size.y;
    let edge_b = (1.0 - in.uv.y) * rect_size.y;

    // Per-side border masks (1.0 = in border zone, 0.0 = in fill zone)
    var in_left = 0.0;
    var in_top = 0.0;
    var in_right = 0.0;
    var in_bottom = 0.0;

    if lw > 0.5 {
        in_left = 1.0 - smoothstep(lw - 0.5, lw + 0.5, edge_l);
    }
    if tw > 0.5 {
        in_top = 1.0 - smoothstep(tw - 0.5, tw + 0.5, edge_t);
    }
    if rw > 0.5 {
        in_right = 1.0 - smoothstep(rw - 0.5, rw + 0.5, edge_r);
    }
    if bw > 0.5 {
        in_bottom = 1.0 - smoothstep(bw - 0.5, bw + 0.5, edge_b);
    }

    // Combined border mask: maximum of all per-side masks
    let border_mask = max(max(in_left, in_top), max(in_right, in_bottom));

    // Border alpha encoded in data2.x as -(1.0 + alpha)
    let border_alpha = -in.data2.x - 1.0;

    // Mix fill color with border color based on mask
    let rgb = mix(in.color.rgb, border_color, border_mask);
    let alpha = mix(in.color.a, border_alpha, border_mask) * outer_alpha;

    if alpha < 0.001 {
        discard;
    }

    return apply_rounded_clip(vec4<f32>(rgb, alpha), in.logical_pos);
}

// Per-side border, no-radius mode (pixel-aligned, symmetric).
//
// Отличия от общего `fs_per_side_border`:
//   1. `rect_size` берётся напрямую из `data.xy` (физ. пиксели, переданные
//      CPU-side) — без derivative-аппроксимации `dpdx/dpdy`, неточной
//      на helper invocations за пределами quad.
//   2. `outer_alpha = 1.0` — без SDF-AA внешнего контура. На pixel-aligned
//      rect SDF-fade делал бордер визуально толще на доминирующей оси.
//   3. `edge_r = rect_size - edge_l` (вместо `(1 - uv) * rect_size`) —
//      алгебраически эквивалентно, но без потери точности при `1 - uv`.
//
// Совместно с half-pixel offset в `vs_main` (vertex corners совпадают с
// pixel corners) даёт точный 1 px бордер, симметричный со всех сторон.
//
// Кодировка: `data2.x = -(50 + α)`, `data.xy = rect_size`.
fn fs_per_side_border_sharp(in: VertexOutput) -> vec4<f32> {
    let rect_size = vec2<f32>(in.data.x, in.data.y);

    // Unpack per-side widths
    let lw = floor(in.data2.z / 256.0);
    let tw = in.data2.z - lw * 256.0;
    let rw = floor(in.data2.w / 256.0);
    let bw = in.data2.w - rw * 256.0;

    // Unpack border color
    let packed = in.data2.y;
    let ri = floor(packed / 65536.0);
    let gi = floor((packed - ri * 65536.0) / 256.0);
    let bi = packed - ri * 65536.0 - gi * 256.0;
    let border_color = vec3<f32>(ri, gi, bi) / 255.0;

    // Симметричные расстояния от каждой стороны.
    let edge_l = in.uv.x * rect_size.x;
    let edge_t = in.uv.y * rect_size.y;
    let edge_r = rect_size.x - edge_l;
    let edge_b = rect_size.y - edge_t;

    // Per-side AA-маски с симметричным 1 px fade. smoothstep(w-0.5, w+0.5, e)
    // даёт половинное покрытие на pixel-границе бордера — идентично для
    // всех 4 сторон.
    var in_left = 0.0;
    var in_top = 0.0;
    var in_right = 0.0;
    var in_bottom = 0.0;
    if lw > 0.5 { in_left = 1.0 - smoothstep(lw - 0.5, lw + 0.5, edge_l); }
    if tw > 0.5 { in_top = 1.0 - smoothstep(tw - 0.5, tw + 0.5, edge_t); }
    if rw > 0.5 { in_right = 1.0 - smoothstep(rw - 0.5, rw + 0.5, edge_r); }
    if bw > 0.5 { in_bottom = 1.0 - smoothstep(bw - 0.5, bw + 0.5, edge_b); }

    let border_mask = max(max(in_left, in_top), max(in_right, in_bottom));
    let border_alpha = -in.data2.x - 50.0;

    let rgb = mix(in.color.rgb, border_color, border_mask);
    let alpha = mix(in.color.a, border_alpha, border_mask);

    if alpha < 0.001 {
        discard;
    }

    return apply_rounded_clip(vec4<f32>(rgb, alpha), in.logical_pos);
}
