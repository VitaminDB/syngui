//! Отладочный дамп Lottie: рендерит несколько кадров анимации в одну
//! PNG-полосу (кадры слева направо) для визуальной проверки.
//!
//! `cargo run -p syngui --example lottie_dump --features image -- file.json out.png [size] [frames] [repeat]`
//!
//! `repeat` > 1 — каждый кадр рисуется столько раз (замер и профилирование рендера).

use std::time::Instant;

use syngui::lottie::{Composition, Renderer};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: lottie_dump <in.json> <out.png> [size=240] [frames=6]");
        std::process::exit(2);
    }
    let size: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(240);
    let frames: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(6);
    let repeat: u32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
    let bytes = std::fs::read(&args[1]).expect("read");
    let t0 = Instant::now();
    let mut comp = Composition::from_bytes(&bytes).expect("parse");
    eprintln!(
        "{}: {}x{} fr={} ip={} op={} layers={} precomps={} images={} parse={:?} unsupported_layers={}",
        comp.name,
        comp.width,
        comp.height,
        comp.frame_rate,
        comp.in_point,
        comp.out_point,
        comp.layers.len(),
        comp.precomps.len(),
        comp.images.len(),
        t0.elapsed(),
        comp.unsupported_layers
    );
    let base_dir = std::path::Path::new(&args[1]).parent().map(|p| p.to_path_buf());
    comp.load_images(|asset| {
        let bytes = match &asset.embedded {
            Some(b) => b.clone(),
            None => {
                let p = base_dir.clone().unwrap_or_default().join(&asset.path);
                std::fs::read(&p).ok()?
            }
        };
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(syngui::lottie::ImageRgba {
            width: w,
            height: h,
            data: img.into_raw().into(),
        })
    });
    let aspect = comp.height / comp.width;
    let (fw, fh) = if aspect <= 1.0 { (size, (size as f32 * aspect).round().max(1.0) as u32) } else { ((size as f32 / aspect).round().max(1.0) as u32, size) };
    let mut r = Renderer::new();
    let mut strip = image::RgbaImage::from_pixel(fw * frames as u32, fh, image::Rgba([235, 235, 235, 255]));
    let mut total = std::time::Duration::ZERO;
    for i in 0..frames {
        let frame = comp.in_point + (comp.frame_count() - 1.0).max(0.0) * i as f32 / (frames.max(2) - 1) as f32;
        let t = Instant::now();
        let mut rgba = r.render_rgba8(&comp, frame, fw, fh);
        for _ in 1..repeat {
            rgba = r.render_rgba8(&comp, frame, fw, fh);
        }
        let dt = t.elapsed() / repeat;
        total += dt;
        eprintln!("  frame {frame:.1}: {dt:?}");
        for y in 0..fh {
            for x in 0..fw {
                let i4 = ((y * fw + x) * 4) as usize;
                let a = rgba[i4 + 3] as f32 / 255.0;
                let dst = strip.get_pixel_mut(x + fw * i as u32, y);
                for k in 0..3 {
                    dst[k] = (rgba[i4 + k] as f32 * a + dst[k] as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
    }
    eprintln!("  avg {:?}/frame", total / frames as u32);
    strip.save(&args[2]).expect("save png");
}
