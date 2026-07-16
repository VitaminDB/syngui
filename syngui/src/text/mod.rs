pub mod font_atlas;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod font_discovery;

#[cfg(target_os = "android")]
pub mod font_discovery_android;

#[cfg(any(feature = "material-icons", feature = "font-awesome"))]
pub mod icon_fonts;

pub use font_atlas::{FontAtlas, FontAtlasStats};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub use font_discovery::list_monospace_families;
