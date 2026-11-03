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
//! null/precomp/image/text (встроенные глифы), parenting, маски и track
//! matte, time remap, градиенты, trim paths, merge paths, repeater, пунктир,
//! модификаторы контуров и эффекты Fill/Tint/DropShadow/GaussianBlur, а
//! также контейнер dotLottie (.lottie, свой ZIP+inflate). Выражения и
//! текст системными шрифтами не поддерживаются.

pub mod anim;
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
pub use model::{Composition, ImageAsset, LottieError, Marker};
pub use raster::{ImageRgba, Pixmap};
pub use render::Renderer;
