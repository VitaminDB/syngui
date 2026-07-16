// Post-process shader — applies color filter effects to a rendered texture.
// Supports: grayscale, sepia, invert, brightness, contrast, HSB adjustment,
// pixelate, edge detection, chromatic aberration, scanlines/CRT,
// displacement/wave, vignette, noise/grain.

struct PostProcessUniforms {
    resolution: vec2<f32>,
    // Effect type: 0=grayscale, 1=sepia, 2=invert, 3=hsb_adjust,
    // 4=brightness, 5=contrast, 6=pixelate, 7=edge_detect,
    // 8=chromatic_aberration, 9=scanlines, 10=displacement, 11=vignette,
    // 12=noise
    effect_type: f32,
    intensity: f32,
    // Extra params: meaning depends on effect_type
    // grayscale: unused
    // sepia: unused
    // invert: unused
    // hsb: [hue_shift, saturation_mult, brightness_mult, 0]
    // pixelate: [block_size, 0, 0, 0]
    // chromatic: [offset_px, 0, 0, 0]
    // scanlines: [density, 0, 0, 0]
    // displacement: [amplitude, frequency, 0, 0]
    // vignette: [radius, softness, 0, 0]
    // noise: [seed/time, 0, 0, 0]
    params: vec4<f32>,
    time: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    // Extra params for effects needing >4 floats (gradient-map, duotone, etc.)
    params2: vec4<f32>,
    // Element bounds in UV space: [x, y, width, height] (0–1)
    bounds: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> pp: PostProcessUniforms;

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
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

// ── Helpers ──

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let v = max(max(c.r, c.g), c.b);
    let m = min(min(c.r, c.g), c.b);
    let d = v - m;
    var h = 0.0;
    var s = 0.0;
    if v > 0.0 { s = d / v; }
    if d > 0.001 {
        if v == c.r {
            h = (c.g - c.b) / d;
            if h < 0.0 { h += 6.0; }
        } else if v == c.g {
            h = 2.0 + (c.b - c.r) / d;
        } else {
            h = 4.0 + (c.r - c.g) / d;
        }
        h /= 6.0;
    }
    return vec3<f32>(h, s, v);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let h = fract(c.x) * 6.0;
    let s = c.y;
    let v = c.z;
    let f = h - floor(h);
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let hi = i32(floor(h)) % 6;
    if hi == 0 { return vec3<f32>(v, t, p); }
    if hi == 1 { return vec3<f32>(q, v, p); }
    if hi == 2 { return vec3<f32>(p, v, t); }
    if hi == 3 { return vec3<f32>(p, q, v); }
    if hi == 4 { return vec3<f32>(t, p, v); }
    return vec3<f32>(v, p, q);
}

/// Convert screen UV to element-local UV (0–1 within element bounds)
fn to_local_uv(uv: vec2<f32>) -> vec2<f32> {
    let b = pp.bounds; // x, y, w, h in UV space
    return (uv - b.xy) / max(b.zw, vec2<f32>(0.001));
}

/// Get element center in screen UV space
fn element_center() -> vec2<f32> {
    return pp.bounds.xy + pp.bounds.zw * 0.5;
}

/// Check if UV is within element bounds
fn in_bounds(uv: vec2<f32>) -> bool {
    let b_min = pp.bounds.xy;
    let b_max = pp.bounds.xy + pp.bounds.zw;
    return uv.x >= b_min.x && uv.x <= b_max.x && uv.y >= b_min.y && uv.y <= b_max.y;
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let effect = i32(pp.effect_type);
    let amount = pp.intensity;

    // ── 0: Grayscale ──
    if effect == 0 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        return vec4<f32>(mix(c.rgb, vec3<f32>(lum), amount), c.a);
    }

    // ── 1: Sepia ──
    if effect == 1 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let sepia = vec3<f32>(lum * 1.2, lum * 1.0, lum * 0.8);
        return vec4<f32>(mix(c.rgb, sepia, amount), c.a);
    }

    // ── 2: Invert ──
    if effect == 2 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        return vec4<f32>(mix(c.rgb, 1.0 - c.rgb, amount), c.a);
    }

    // ── 3: HSB Adjust ──
    if effect == 3 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        var hsv = rgb_to_hsv(c.rgb);
        hsv.x = fract(hsv.x + pp.params.x); // hue shift
        hsv.y = clamp(hsv.y * pp.params.y, 0.0, 1.0); // saturation multiply
        hsv.z = clamp(hsv.z * pp.params.z, 0.0, 1.0); // brightness multiply
        return vec4<f32>(hsv_to_rgb(hsv), c.a);
    }

    // ── 4: Brightness ──
    if effect == 4 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        return vec4<f32>(clamp(c.rgb * amount, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }

    // ── 5: Contrast ──
    if effect == 5 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let adjusted = (c.rgb - 0.5) * amount + 0.5;
        return vec4<f32>(clamp(adjusted, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }

    // ── 6: Pixelate ──
    if effect == 6 {
        let block_size = max(pp.params.x, 1.0);
        let pixel_uv = floor(in.uv * pp.resolution / block_size) * block_size / pp.resolution;
        return textureSample(input_texture, input_sampler, pixel_uv);
    }

    // ── 7: Edge Detection (Sobel) ──
    if effect == 7 {
        let texel = 1.0 / pp.resolution;
        let tl = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x, -texel.y)).rgb, vec3<f32>(0.333));
        let t  = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(0.0, -texel.y)).rgb, vec3<f32>(0.333));
        let tr = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>( texel.x, -texel.y)).rgb, vec3<f32>(0.333));
        let l  = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x, 0.0)).rgb, vec3<f32>(0.333));
        let r  = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>( texel.x, 0.0)).rgb, vec3<f32>(0.333));
        let bl = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x,  texel.y)).rgb, vec3<f32>(0.333));
        let b  = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(0.0,  texel.y)).rgb, vec3<f32>(0.333));
        let br = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>( texel.x,  texel.y)).rgb, vec3<f32>(0.333));
        let gx = -tl - 2.0*l - bl + tr + 2.0*r + br;
        let gy = -tl - 2.0*t - tr + bl + 2.0*b + br;
        let edge = sqrt(gx*gx + gy*gy);
        let c = textureSample(input_texture, input_sampler, in.uv);
        let edge_color = vec3<f32>(edge * amount);
        return vec4<f32>(mix(c.rgb, edge_color, amount), c.a);
    }

    // ── 8: Chromatic Aberration ──
    if effect == 8 {
        let offset = pp.params.x / pp.resolution.x;
        let r_val = textureSample(input_texture, input_sampler, in.uv + vec2<f32>(offset, 0.0)).r;
        let g_val = textureSample(input_texture, input_sampler, in.uv).g;
        let b_val = textureSample(input_texture, input_sampler, in.uv - vec2<f32>(offset, 0.0)).b;
        let a_val = textureSample(input_texture, input_sampler, in.uv).a;
        return vec4<f32>(r_val, g_val, b_val, a_val);
    }

    // ── 9: Scanlines / CRT ──
    if effect == 9 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let density = pp.params.x;
        let scanline = sin(in.uv.y * pp.resolution.y * 3.14159 / density) * 0.5 + 0.5;
        let scan_effect = mix(1.0, scanline, amount);
        // Barrel distortion
        let centered = in.uv - 0.5;
        let r2 = dot(centered, centered);
        let vignette_crt = 1.0 - r2 * 2.0 * amount;
        return vec4<f32>(c.rgb * scan_effect * vignette_crt, c.a);
    }

    // ── 10: Displacement / Wave ──
    if effect == 10 {
        let amp = pp.params.x / pp.resolution.x;
        let freq = pp.params.y;
        let offset_x = sin(in.uv.y * freq + pp.time) * amp;
        let offset_y = cos(in.uv.x * freq + pp.time) * amp * 0.5;
        let displaced_uv = in.uv + vec2<f32>(offset_x, offset_y);
        return textureSample(input_texture, input_sampler, (displaced_uv));
    }

    // ── 11: Vignette ──
    if effect == 11 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let local = to_local_uv(in.uv);
        let centered = local - 0.5;
        let dist = length(centered) * 2.0;
        let radius = pp.params.x;
        let softness = pp.params.y;
        let vignette_val = smoothstep(radius, radius - softness, dist);
        return vec4<f32>(c.rgb * mix(1.0, vignette_val, amount), c.a);
    }

    // ── 12: Noise / Grain ──
    if effect == 12 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let noise_val = hash21(in.uv * pp.resolution + vec2<f32>(pp.time * 100.0)) * 2.0 - 1.0;
        let noisy = c.rgb + vec3<f32>(noise_val * amount);
        return vec4<f32>(clamp(noisy, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }

    // ── 13: Glitch ──
    if effect == 13 {
        let block_size = max(pp.params.x, 4.0);
        let block_y = floor(in.uv.y * pp.resolution.y / block_size);
        let time_seed = floor(pp.time * 8.0);
        let rand_block = hash21(vec2<f32>(block_y, time_seed));
        var uv = in.uv;
        if rand_block > (1.0 - amount * 0.3) {
            uv.x += (hash21(vec2<f32>(block_y + 100.0, time_seed)) - 0.5) * amount * 0.15;
        }
        let shift = amount * 5.0 / pp.resolution.x;
        let r_val = textureSample(input_texture, input_sampler, uv + vec2<f32>(shift, 0.0)).r;
        let g_val = textureSample(input_texture, input_sampler, uv).g;
        let b_val = textureSample(input_texture, input_sampler, uv - vec2<f32>(shift, 0.0)).b;
        let a_val = textureSample(input_texture, input_sampler, uv).a;
        let scan = step(0.99 - amount * 0.1, hash21(vec2<f32>(in.uv.y * pp.resolution.y, time_seed)));
        return vec4<f32>(r_val + scan * 0.1, g_val, b_val - scan * 0.1, a_val);
    }

    // ── 14: Dissolve / Dither ──
    if effect == 14 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let noise_val = hash21(in.uv * pp.resolution * 0.5 + vec2<f32>(pp.time * 10.0));
        let alpha = select(0.0, c.a, noise_val > amount);
        return vec4<f32>(c.rgb, alpha);
    }

    // ── 15: Swirl ──
    if effect == 15 {
        let center = element_center();
        let max_angle = pp.params.z;
        let radius_uv = pp.params.w * max(pp.bounds.z, pp.bounds.w);
        let swirl_radius = max(radius_uv, 0.001);
        let diff = in.uv - center;
        let dist = length(diff);
        let angle = atan2(diff.y, diff.x);
        let factor = max(1.0 - dist / swirl_radius, 0.0);
        let new_angle = angle + max_angle * factor * factor;
        let new_uv = center + vec2<f32>(cos(new_angle), sin(new_angle)) * dist;
        return textureSample(input_texture, input_sampler, (new_uv));
    }

    // ── 16: Bulge / Pinch ──
    if effect == 16 {
        let center = element_center();
        let strength = pp.params.z;
        let radius_uv = pp.params.w * max(pp.bounds.z, pp.bounds.w);
        let bulge_radius = max(radius_uv, 0.001);
        let diff = in.uv - center;
        let dist = length(diff);
        let norm_dist = dist / bulge_radius;
        if norm_dist < 1.0 {
            let factor = 1.0 - norm_dist;
            let displacement = strength * factor * factor;
            let new_dist = dist * (1.0 - displacement);
            let dir = select(vec2<f32>(0.0), normalize(diff), dist > 0.0001);
            let new_uv = center + dir * new_dist;
            return textureSample(input_texture, input_sampler, (new_uv));
        }
        return textureSample(input_texture, input_sampler, in.uv);
    }

    // ── 17: Gradient Map ──
    if effect == 17 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let dark = vec3<f32>(pp.params.x, pp.params.y, pp.params.z);
        let light = vec3<f32>(pp.params2.x, pp.params2.y, pp.params2.z);
        return vec4<f32>(mix(dark, light, lum), c.a);
    }

    // ── 18: Duotone ──
    if effect == 18 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        let shadow_c = vec3<f32>(pp.params.x, pp.params.y, pp.params.z);
        let highlight_c = vec3<f32>(pp.params2.x, pp.params2.y, pp.params2.z);
        let duotone = mix(shadow_c, highlight_c, lum);
        return vec4<f32>(mix(c.rgb, duotone, amount), c.a);
    }

    // ── 19: Silhouette ──
    if effect == 19 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let fill = vec3<f32>(pp.params.x, pp.params.y, pp.params.z);
        let fill_a = pp.params.w;
        if c.a > 0.01 {
            return vec4<f32>(fill, fill_a * c.a);
        }
        return vec4<f32>(0.0);
    }

    // ── 20: Heat Haze / Turbulence ──
    if effect == 20 {
        let amp = pp.params.x / pp.resolution.x;
        let speed = pp.params.y;
        let t = pp.time * speed;
        let n1x = sin(in.uv.y * 12.0 + t * 3.0) * amp;
        let n1y = cos(in.uv.x * 10.0 + t * 2.5) * amp * 0.7;
        let n2x = sin(in.uv.y * 24.0 + t * 5.0) * amp * 0.4;
        let n2y = cos(in.uv.x * 20.0 + t * 4.0) * amp * 0.3;
        let n3x = sin(in.uv.y * 48.0 + t * 7.0) * amp * 0.15;
        let displaced_uv = in.uv + vec2<f32>(n1x + n2x + n3x, n1y + n2y);
        return textureSample(input_texture, input_sampler, (displaced_uv));
    }

    // ── 21: Radial / Zoom Blur ──
    if effect == 21 {
        let center = element_center();
        let dir = in.uv - center;
        let samples = 12;
        var color = vec4<f32>(0.0);
        for (var i = 0; i < samples; i++) {
            let t = f32(i) / f32(samples - 1);
            let offset = dir * t * amount * 0.1;
            color += textureSample(input_texture, input_sampler, in.uv - offset);
        }
        return color / f32(samples);
    }

    // ── 22: Color Grading (Lift/Gamma/Gain) ──
    if effect == 22 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let lift = pp.params.x;
        let gamma = max(pp.params.y, 0.01);
        let gain = pp.params.z;
        var rgb = c.rgb;
        // Lift: add to shadows
        rgb = rgb + vec3<f32>(lift);
        // Gamma: adjust midtones (power curve)
        rgb = pow(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / gamma));
        // Gain: multiply highlights
        rgb = rgb * vec3<f32>(gain);
        return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    }

    // ── 23: Hologram / X-Ray ──
    if effect == 23 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let tint = vec3<f32>(pp.params.x, pp.params.y, pp.params.z);
        // Edge detection (Sobel)
        let texel = 1.0 / pp.resolution;
        let tl = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x, -texel.y)).rgb, vec3<f32>(0.333));
        let t_v = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(0.0, -texel.y)).rgb, vec3<f32>(0.333));
        let tr = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(texel.x, -texel.y)).rgb, vec3<f32>(0.333));
        let l_v = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x, 0.0)).rgb, vec3<f32>(0.333));
        let r_v = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(texel.x, 0.0)).rgb, vec3<f32>(0.333));
        let bl = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(-texel.x, texel.y)).rgb, vec3<f32>(0.333));
        let b_v = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(0.0, texel.y)).rgb, vec3<f32>(0.333));
        let br = dot(textureSample(input_texture, input_sampler, in.uv + vec2<f32>(texel.x, texel.y)).rgb, vec3<f32>(0.333));
        let gx = -tl - 2.0 * l_v - bl + tr + 2.0 * r_v + br;
        let gy = -tl - 2.0 * t_v - tr + bl + 2.0 * b_v + br;
        let edge = sqrt(gx * gx + gy * gy);
        // Scanlines
        let scan = sin(in.uv.y * pp.resolution.y * 3.14159 / 2.0) * 0.5 + 0.5;
        // Compose hologram: edge glow + tint + reduced alpha + scanlines
        let glow_str = edge * 2.0;
        let base = c.rgb * 0.2 + tint * glow_str;
        let final_color = base * mix(1.0, scan, 0.3);
        return vec4<f32>(final_color * amount, c.a * 0.7 * amount);
    }

    // ── 24: Refraction (procedural) ──
    if effect == 24 {
        let distortion = pp.params.x;
        let ior = pp.params.y;
        let local = to_local_uv(in.uv);
        let center = local - 0.5;
        let dist = length(center);
        // Fresnel-like intensity at edges
        let fresnel = pow(dist * 2.0, 2.0);
        // Refraction displacement based on IOR
        let refract_amount = distortion * fresnel * (ior - 1.0);
        let dir = select(vec2<f32>(0.0), normalize(center), dist > 0.001);
        let displaced_uv = (in.uv + dir * refract_amount * 0.1);
        // Slight chromatic split for realism
        let split = refract_amount * 0.005;
        let r_val = textureSample(input_texture, input_sampler, (displaced_uv + dir * split)).r;
        let g_val = textureSample(input_texture, input_sampler, displaced_uv).g;
        let b_val = textureSample(input_texture, input_sampler, (displaced_uv - dir * split)).b;
        let a_val = textureSample(input_texture, input_sampler, displaced_uv).a;
        return vec4<f32>(r_val, g_val, b_val, a_val);
    }

    // ── 25: Lens Flare (simplified) ──
    if effect == 25 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let threshold = pp.params.x;
        let lum = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
        // Extract bright areas
        let bright = max(lum - threshold, 0.0) / max(1.0 - threshold, 0.001);
        // Generate radial streaks from center
        let center = element_center();
        let dir = in.uv - center;
        let dist = length(dir);
        // Ghost images at mirrored positions
        let ghost1_uv = (center - dir * 0.5);
        let ghost2_uv = (center - dir * 1.0);
        let g1 = textureSample(input_texture, input_sampler, ghost1_uv);
        let g2 = textureSample(input_texture, input_sampler, ghost2_uv);
        let g1_bright = max(dot(g1.rgb, vec3<f32>(0.299, 0.587, 0.114)) - threshold, 0.0);
        let g2_bright = max(dot(g2.rgb, vec3<f32>(0.299, 0.587, 0.114)) - threshold, 0.0);
        // Compose flare
        let flare_color = vec3<f32>(1.0, 0.9, 0.7);
        let flare = flare_color * (bright * 0.5 + g1_bright * 0.3 + g2_bright * 0.2);
        // Radial falloff
        let falloff = 1.0 - smoothstep(0.0, 1.0, dist);
        return vec4<f32>(c.rgb + flare * amount * falloff, c.a);
    }

    // ── 26: Mask Reveal ──
    if effect == 26 {
        let c = textureSample(input_texture, input_sampler, in.uv);
        let progress = amount;
        let direction = pp.params.x;
        // Use element-local UV for gradient
        let local = to_local_uv(in.uv);
        let dir_vec = vec2<f32>(cos(direction), sin(direction));
        let gradient = dot(local - 0.5, dir_vec) + 0.5;
        // Add noise for organic edge
        let noise_val = hash21(in.uv * pp.resolution * 0.3) * 0.15;
        let edge = gradient + noise_val;
        // Soft reveal edge
        let reveal = smoothstep(progress - 0.05, progress + 0.05, edge);
        return vec4<f32>(c.rgb, c.a * reveal);
    }

    // Fallback: passthrough
    return textureSample(input_texture, input_sampler, in.uv);
}
