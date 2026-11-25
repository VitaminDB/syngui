pub mod font_atlas;
pub mod line_break;
pub mod raster;
pub mod registry;
pub mod script;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod font_discovery;

#[cfg(target_os = "android")]
pub mod font_discovery_android;
/// На Android — тот же интерфейс поиска шрифтов (`list_families`,
/// `discover_styled`, `discover_fallback_font`…) по `/system/fonts`.
#[cfg(target_os = "android")]
pub use font_discovery_android as font_discovery;

#[cfg(any(feature = "material-icons", feature = "font-awesome"))]
pub mod icon_fonts;

pub use font_atlas::{FontAtlas, FontAtlasStats, FontWeight};
pub use line_break::{break_class, breaks_before, BreakClass};
pub use raster::{rasterize_text, RasterizedText, TextRasterOpts};
pub use registry::{family_covers, font_family_name, register_font, register_font_file, register_font_file_auto};
pub use script::{script_of, Script};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub use font_discovery::list_monospace_families;
