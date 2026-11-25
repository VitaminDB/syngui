//! Lottie (bodymovin JSON) — разбор и отрисовка собственными силами, без
//! сторонних библиотек: JSON-парсер, модель, интерполяция ключевых кадров,
//! растеризатор с точным покрытием и композитинг слоёв.
//!
//! ```ignore
//! let comp = syngui::lottie::Composition::from_str(json)?;
//! let mut r = syngui::lottie::Renderer::new();
//! let rgba = r.render_rgba8(&comp, frame, 256, 256); // прямая альфа, sRGB
//! ```
//!
//! Виджет — [`crate::widgets::Lottie`]. Поддерживаются слои shape/solid/
//! null/precomp/image/text (встроенные глифы, аниматоры текста, текст по
//! контуру), parenting, маски (с расширением) и track matte, time remap,
//! выражения After Effects (свой интерпретатор, [`expr`]),
//! 3D-слои и камера, градиенты, trim paths, merge paths, repeater, пунктир,
//! модификаторы контуров и эффекты Fill/Tint/DropShadow/GaussianBlur, а
//! также контейнер dotLottie (.lottie, свой ZIP+inflate). Текст без
//! встроенных глифов — системными шрифтами (`sysfont`).

pub mod anim;
pub mod expr;
pub mod geom;
pub mod inflate;
pub mod json;
pub mod model;
pub mod raster;
pub mod render;
/// Глифы системных шрифтов для текста без встроенных контуров —
/// через текстовую подсистему фреймворка (font-kit + swash).
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod sysfont;
pub mod zip;

pub use geom::Mat;
pub use model::{
    Composition, DotLottieAnimation, DotLottieManifest, ImageAsset, LottieError, Marker,
};
pub use raster::{ImageRgba, Pixmap};
pub use render::Renderer;

/// Декодировать растровые ассеты композиции (PNG/JPEG через `image`).
#[cfg(all(feature = "image", not(target_arch = "wasm32")))]
pub fn load_images(comp: &mut Composition, base: Option<&std::path::Path>) {
    comp.load_images(|asset| {
        let bytes = match &asset.embedded {
            Some(b) => b.clone(),
            None => {
                std::fs::read(base.unwrap_or(std::path::Path::new(".")).join(&asset.path)).ok()?
            }
        };
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(ImageRgba {
            width: w,
            height: h,
            data: img.into_raw().into(),
        })
    });
}

#[cfg(not(all(feature = "image", not(target_arch = "wasm32"))))]
pub fn load_images(comp: &mut Composition, _base: Option<&std::path::Path>) {
    if comp.has_images() {
        log::warn!("Lottie: растровые слои пропущены — нужна feature `image`");
    }
}

/// Глифы системных шрифтов для текста без встроенных `chars`.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub fn load_system_glyphs(comp: &mut Composition) {
    sysfont::load_missing_glyphs(comp);
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub fn load_system_glyphs(_comp: &mut Composition) {}

/// Анимация из файла (JSON или dotLottie) — готовая к отрисовке: растровые
/// ассеты декодированы (рядом с файлом или из архива), текст без встроенных
/// глифов — системными шрифтами.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<Composition, String> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut c = Composition::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    load_images(&mut c, path.parent());
    load_system_glyphs(&mut c);
    Ok(c)
}
