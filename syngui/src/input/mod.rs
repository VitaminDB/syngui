pub mod double_click;
pub mod edit_diff;
pub mod events;
pub mod function_keys;
pub mod haptics;
pub mod keyboard;
pub mod mouse;
pub mod touch;
pub mod velocity;

pub use double_click::resolve_double_click_interval;
pub use edit_diff::{edit_diff, EditDiff};
pub use events::*;
pub use haptics::{haptic, set_haptic_handler, Haptic};
pub use function_keys::{captured_function_keys, set_captured_function_keys, FunctionKeys};
pub use keyboard::*;
pub use mouse::*;
pub use touch::{is_synthesized_mouse, last_press, set_last_press, set_touch_config, touch_config, TouchConfig, TouchTracker};
pub use velocity::VelocityTracker;
