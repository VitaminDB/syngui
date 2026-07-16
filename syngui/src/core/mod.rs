//! based on [`euclid`] with compile-time unit safety, color representation ([`Color`]),

pub mod color;
pub mod gradient;
pub mod shadow;
pub mod types;
pub mod geometry;
pub mod math;
pub mod error;
pub mod canvas;
pub mod sync;

pub use color::*;
pub use gradient::*;
pub use shadow::*;
pub use types::*;
pub use geometry::*;
pub use math::*;
pub use error::*;
