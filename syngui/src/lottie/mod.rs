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
//! null/precomp/image, parenting, маски и track matte, time remap,
//! градиенты, trim paths, repeater, пунктир и скругления. Текстовые слои и
//! выражения не поддерживаются.

pub mod anim;
pub mod geom;
pub mod json;
pub mod model;
pub mod raster;
pub mod render;

pub use geom::Mat;
pub use model::{Composition, ImageAsset, LottieError, Marker};
pub use raster::{ImageRgba, Pixmap};
pub use render::Renderer;
