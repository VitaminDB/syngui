//! Значок перетаскивания Wayland (`wl_data_device.start_drag` с icon): рисует
//! его композитор у курсора над любыми окнами и слоями. Растеризуется на CPU
//! в ARGB8888 (премультиплицированный): карточка с документом (стопка — для
//! нескольких файлов), над принимающей целью — крупнее, с акцентной рамкой и
//! значком «+».

/// Логический размер холста значка.
pub(crate) const SIZE: (i32, i32) = (84, 92);
/// Смещение значка от курсора (курсор — у левого верхнего угла карточки).
pub(crate) const OFFSET: (i32, i32) = (-14, -14);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Look {
    pub accepted: bool,
    pub multi: bool,
}

#[derive(Clone, Copy)]
struct Rgba(f32, f32, f32, f32);

const ACCENT: Rgba = Rgba(0.18, 0.83, 0.75, 1.0);
const BADGE: Rgba = Rgba(0.13, 0.77, 0.37, 1.0);

/// Пиксели значка: ширина, высота (в пикселях буфера) и байты ARGB8888.
pub(crate) fn render(look: Look, scale: i32) -> (i32, i32, Vec<u8>) {
    let s = scale.max(1) as f32;
    let (w, h) = (SIZE.0 * scale.max(1), SIZE.1 * scale.max(1));
    let mut c = Canvas {
        w,
        h,
        px: vec![0.0; (w * h * 4) as usize],
    };
    // Карточка: принятая — крупнее (как «приподнялась» над целью).
    let k = if look.accepted { 1.08 } else { 1.0 };
    let (cw, ch) = (52.0 * k, 60.0 * k);
    let (cx, cy) = (8.0, 8.0);
    if look.multi {
        c.rrect(
            (cx + 7.0) * s,
            (cy - 3.0) * s,
            cw * s,
            ch * s,
            10.0 * s,
            Rgba(0.16, 0.19, 0.24, 0.75),
            0.0,
        );
        c.rrect(
            (cx + 7.0) * s,
            (cy - 3.0) * s,
            cw * s,
            ch * s,
            10.0 * s,
            Rgba(1.0, 1.0, 1.0, 0.10),
            -1.0 * s,
        );
    }
    // Тень.
    c.rrect(
        (cx + 1.0) * s,
        (cy + 4.0) * s,
        cw * s,
        ch * s,
        12.0 * s,
        Rgba(0.0, 0.0, 0.0, 0.35),
        0.0,
    )
    .soft(7.0 * s);
    // Рамка и тело.
    let border = if look.accepted {
        ACCENT
    } else {
        Rgba(1.0, 1.0, 1.0, 0.22)
    };
    let bw = if look.accepted { 2.0 } else { 1.0 };
    c.rrect(cx * s, cy * s, cw * s, ch * s, 11.0 * s, border, 0.0);
    c.rrect(
        (cx + bw) * s,
        (cy + bw) * s,
        (cw - 2.0 * bw) * s,
        (ch - 2.0 * bw) * s,
        (11.0 - bw) * s,
        Rgba(0.12, 0.15, 0.20, 0.96),
        0.0,
    );
    // Документ.
    let (dw, dh) = (24.0 * k, 30.0 * k);
    let (dx, dy) = (cx + (cw - dw) / 2.0, cy + (ch - dh) / 2.0);
    c.rrect(
        dx * s,
        dy * s,
        dw * s,
        dh * s,
        3.5 * s,
        Rgba(0.93, 0.95, 0.97, 1.0),
        0.0,
    );
    let fold = 7.0 * k;
    c.rrect(
        (dx + dw - fold) * s,
        dy * s,
        fold * s,
        fold * s,
        1.5 * s,
        Rgba(0.70, 0.74, 0.80, 1.0),
        0.0,
    );
    for i in 0..3 {
        let ly = dy + dh * (0.42 + 0.17 * i as f32);
        let lw = if i == 2 { dw * 0.45 } else { dw * 0.66 };
        c.rrect(
            (dx + dw * 0.17) * s,
            ly * s,
            lw * s,
            2.2 * s,
            1.1 * s,
            Rgba(0.55, 0.60, 0.67, 1.0),
            0.0,
        );
    }
    if look.accepted {
        let r = 11.0;
        let (bx, by) = (cx + cw - r - 1.0, cy + ch - r - 1.0);
        c.rrect(
            (bx - r - 2.0) * s,
            (by - r - 2.0) * s,
            (2.0 * r + 4.0) * s,
            (2.0 * r + 4.0) * s,
            (r + 2.0) * s,
            Rgba(0.12, 0.15, 0.20, 1.0),
            0.0,
        );
        c.rrect(
            (bx - r) * s,
            (by - r) * s,
            2.0 * r * s,
            2.0 * r * s,
            r * s,
            BADGE,
            0.0,
        );
        c.rrect(
            (bx - 6.0) * s,
            (by - 1.25) * s,
            12.0 * s,
            2.5 * s,
            1.25 * s,
            Rgba(1.0, 1.0, 1.0, 1.0),
            0.0,
        );
        c.rrect(
            (bx - 1.25) * s,
            (by - 6.0) * s,
            2.5 * s,
            12.0 * s,
            1.25 * s,
            Rgba(1.0, 1.0, 1.0, 1.0),
            0.0,
        );
    }
    (w, h, c.into_argb())
}

struct Canvas {
    w: i32,
    h: i32,
    /// Премультиплицированные RGBA, f32.
    px: Vec<f32>,
}

/// Отложенная заливка скруглённого прямоугольника (`.soft()` — с размытием края).
struct Fill<'a> {
    c: &'a mut Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: Rgba,
    inset: f32,
    soft: f32,
}

impl Canvas {
    fn rrect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        color: Rgba,
        inset: f32,
    ) -> Fill<'_> {
        Fill {
            c: self,
            x,
            y,
            w,
            h,
            r,
            color,
            inset,
            soft: 0.0,
        }
    }

    fn into_argb(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len());
        for p in self.px.chunks_exact(4) {
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            // wl_shm ARGB8888 в памяти little-endian: B, G, R, A.
            out.extend_from_slice(&[b(p[2]), b(p[1]), b(p[0]), b(p[3])]);
        }
        out
    }
}

impl Fill<'_> {
    fn soft(mut self, blur: f32) -> Self {
        self.soft = blur;
        self
    }
}

impl Drop for Fill<'_> {
    fn drop(&mut self) {
        let Fill {
            x,
            y,
            w,
            h,
            r,
            color,
            inset,
            soft,
            ..
        } = *self;
        let (hw, hh) = (w / 2.0, h / 2.0);
        let (mx, my) = (x + hw, y + hh);
        let r = r.min(hw).min(hh).max(0.0);
        let pad = soft + 1.0;
        let x0 = ((x - pad).floor() as i32).max(0);
        let y0 = ((y - pad).floor() as i32).max(0);
        let x1 = ((x + w + pad).ceil() as i32).min(self.c.w);
        let y1 = ((y + h + pad).ceil() as i32).min(self.c.h);
        for py in y0..y1 {
            for px in x0..x1 {
                // SDF скруглённого прямоугольника в центре пикселя.
                let qx = (px as f32 + 0.5 - mx).abs() - (hw - r);
                let qy = (py as f32 + 0.5 - my).abs() - (hh - r);
                let d = (qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r) + inset.max(0.0);
                let cov = if soft > 0.0 {
                    let t = (1.0 - (d + soft) / (2.0 * soft)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                } else {
                    (0.5 - d).clamp(0.0, 1.0)
                };
                // Отрицательный inset — только обводка на 1 px внутри.
                let cov = if inset < 0.0 {
                    cov * (1.0 - (0.5 - (d - inset)).clamp(0.0, 1.0))
                } else {
                    cov
                };
                if cov <= 0.0 {
                    continue;
                }
                let a = color.3 * cov;
                let i = ((py * self.c.w + px) * 4) as usize;
                let p = &mut self.c.px[i..i + 4];
                p[0] = color.0 * a + p[0] * (1.0 - a);
                p[1] = color.1 * a + p[1] * (1.0 - a);
                p[2] = color.2 * a + p[2] * (1.0 - a);
                p[3] = a + p[3] * (1.0 - a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_both_looks() {
        for accepted in [false, true] {
            let (w, h, px) = render(
                Look {
                    accepted,
                    multi: true,
                },
                2,
            );
            assert_eq!((w, h), (SIZE.0 * 2, SIZE.1 * 2));
            assert_eq!(px.len(), (w * h * 4) as usize);
            // Центр карточки непрозрачен, угол холста — прозрачен.
            let at = |x: i32, y: i32| px[((y * w + x) * 4 + 3) as usize];
            assert!(at(w / 3, h / 2) > 200);
            assert_eq!(at(w - 1, 0), 0);
        }
    }
}
