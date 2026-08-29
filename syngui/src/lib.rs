pub mod a11y;
pub mod animation;
pub mod appearance;
pub mod clipboard;
#[cfg(feature = "winit")]
pub mod app;
pub mod async_runtime;
#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "ffmpeg")]
pub mod video;
#[cfg(feature = "tokio")]
pub mod async_hook;
pub mod context_provider;
pub mod core;
pub mod debug;
pub mod devtools;
pub mod effects;
pub mod gpu;
#[cfg(feature = "i18n")]
pub mod i18n;
#[cfg(not(feature = "i18n"))]
pub(crate) mod i18n {
    pub(crate) fn builtin(_key: &str, fallback: &str) -> String {
        fallback.to_string()
    }
    pub(crate) fn builtin_args(_key: &str, fallback: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
        let mut out = fallback.to_string();
        for (name, value) in args {
            out = out.replace(&format!("{{{name}}}"), &value.to_string());
        }
        out
    }
}
pub mod input;
pub mod layout;
pub mod mss;
pub mod perf;
pub mod render;
pub mod signal;
pub mod text;
pub mod widget;
pub mod widgets;
#[cfg(feature = "winit")]
pub mod window;

pub mod external_url;
pub use external_url::open_url;

// Кроссплатформенный Instant (на native = std::time, на wasm — performance.now).
// Реэкспорт чтобы потребители использовали тот же тип, что и виджеты карты
// (например MapMarker::fade_in_at/fade_out_at).
pub use web_time;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(feature = "winit")]
pub use app::*;
pub use core::*;
pub use effects::*;
pub use input::*;
pub use layout::*;
pub use mss::*;
pub use render::*;
pub use widget::*;
pub use widgets::*;
#[cfg(feature = "winit")]
pub use window::*;

pub mod prelude {
    pub use crate::a11y::{AccessibilityInfo, Role, NodeState, NodeProperties, FocusManager};
    pub use crate::animation::{Animation, Easing};
    pub use crate::appearance::{ColorScheme, SystemAppearance};
    #[cfg(feature = "winit")]
    pub use crate::app::{App, AppBuilder, GpuBackend, GpuPowerPreference};
    pub use crate::core::*;
    pub use crate::effects::Effect;
    pub use crate::gpu::Renderer;
    pub use crate::input::{Event, EventResult, Key, MouseButton, Modifiers};
    pub use crate::layout::{Constraints, Layout};
    pub use crate::mss::{StyleContext, StyleEngine, ComputedStyle, ElementState, parse_stylesheet_str, KeyframesDefinition, KeyframeStep};
    pub use crate::render::{DisplayList, ClipRect, Vertex, Batch, DrawCommand, Border, RenderOp};
    pub use crate::widget::{
        BuildContext, DirtyFlags, Element, ElementId, ElementTree, RenderHandle, 
        UpdateContext, EventContext, Widget, WidgetExt,
    };
    #[cfg(feature = "winit")]
    pub use crate::window::{Window, WindowBuilder, WindowEvent};
    
    pub use crate::widget::{Text, Center, Elide};
    
    pub use crate::core::canvas::{CanvasContext, Paint, LineCap, LineJoin};

    pub use crate::signal::{use_signal, create_memo, create_effect, create_effect_with_cleanup, dispose_effect, use_effect, use_effect_with_cleanup, RwSignal, Memo, EffectId};

    pub use crate::async_runtime::run_on_main_thread;
    #[cfg(feature = "tokio")]
    pub use crate::async_runtime::spawn;
    #[cfg(feature = "tokio")]
    pub use crate::async_hook::use_async;

    pub use crate::context_provider::{provide_context, use_context, try_use_context};

    #[cfg(feature = "i18n")]
    pub use crate::i18n::{tr, tr_args, trn, trn_args, try_tr, Lang};
    #[cfg(feature = "i18n")]
    pub use crate::{tr, trn};

    pub use crate::widgets::{
        Button, SegmentedButton, ToolButton,
        TextField, Checkbox, RadioButton, RadioGroup, Toggle, Slider, SpinBox,
        Multiselect, Autocomplete, DatePicker, Date, TimePicker, Time,
        ColorPicker, ColorValue,
        Row, Column, Flex, Grid, Stack, Padding, Carousel, SplitView, SplitDirection, DecoratedBox,
        MainAxisAlignment, CrossAxisAlignment, FlexDirection, StackFit, Reactive,
        ScrollView, ScrollDirection,
        Tab, TabBar, TabPosition, TabState, Toolbar, Router, RouterView,
        Pagination, TopAppBar,
        Avatar, Badge, BadgeSize, Canvas, Card, Chip, CircularProgress,
        Divider, DividerDirection, Icon, Image, ImageFit, ProgressBar,
        Calendar, RichText, TextSpan,
        ListView, ListItem, SelectionMode, TableView, TableColumn, TreeView, TreeNode,
        PropertyGrid, Property, PropertyValue,
        Dropdown, DropdownItem, Combobox,
        Tooltip, TooltipPosition, Snackbar, SnackbarPosition,
        NotificationCtx, NotificationHost, NotificationItem, NotificationSeverity,
        Dialog, AlertDialog, ConfirmDialog, DialogAction, set_dialog_labels,
        FloatingWindow,
        PopupMenu, MenuItem,
        ContextMenu,
        Draggable, DropArea,
        Portal,
    };
}
