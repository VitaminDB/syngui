pub mod types;
pub mod tree;
pub mod focus;
pub mod platform;

pub use types::*;
pub use tree::A11yTree;
pub use focus::FocusManager;
pub use platform::{PlatformAdapter, LoggingAdapter, NullAdapter};

#[cfg(feature = "accessibility")]
pub mod accesskit_adapter;
#[cfg(feature = "accessibility")]
pub use accesskit_adapter::AccessKitAdapter;
