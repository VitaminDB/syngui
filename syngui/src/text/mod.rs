pub mod font_atlas;
pub mod line_break;
pub mod script;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod font_discovery;

#[cfg(target_os = "android")]
pub mod font_discovery_android;

#[cfg(any(feature = "material-icons", feature = "font-awesome"))]
pub mod icon_fonts;

pub use font_atlas::{FontAtlas, FontAtlasStats};
pub use line_break::{break_class, breaks_before, BreakClass};
pub use script::{script_of, Script};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub use font_discovery::list_monospace_families;
