pub mod a11y;
pub mod animation;
#[cfg(feature = "winit")]
pub mod app;
pub mod appearance;
#[cfg(feature = "tokio")]
pub mod async_hook;
pub mod async_runtime;
#[cfg(feature = "audio")]
pub mod audio;
pub mod clipboard;
pub mod context_provider;
pub mod core;
pub mod debug;
pub mod devtools;
pub mod effects;
pub mod gpu;
#[cfg(feature = "i18n")]
pub mod i18n;
#[cfg(feature = "ffmpeg")]
pub mod video;
#[cfg(not(feature = "i18n"))]
pub(crate) mod i18n {
    pub(crate) fn builtin(_key: &str, fallback: &str) -> String {
        fallback.to_string()
    }
    pub(crate) fn builtin_args(
        _key: &str,
        fallback: &str,
        args: &[(&str, &dyn std::fmt::Display)],
    ) -> String {
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
pub mod scale;
pub mod signal;
pub mod text;
pub mod viewport;
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
    pub use crate::a11y::{AccessibilityInfo, FocusManager, NodeProperties, NodeState, Role};
    pub use crate::animation::{Animation, Easing};
    #[cfg(feature = "winit")]
    pub use crate::app::{App, AppBuilder, GpuBackend, GpuPowerPreference};
    pub use crate::appearance::{ColorScheme, SystemAppearance};
    pub use crate::core::*;
    pub use crate::effects::Effect;
    pub use crate::gpu::Renderer;
    pub use crate::input::{Event, EventResult, Key, Modifiers, MouseButton};
    pub use crate::layout::{Constraints, Layout};
    pub use crate::mss::{
        parse_stylesheet_str, ComputedStyle, ElementState, KeyframeStep, KeyframesDefinition,
        StyleContext, StyleEngine,
    };
    pub use crate::render::{Batch, Border, ClipRect, DisplayList, DrawCommand, RenderOp, Vertex};
    pub use crate::widget::{
        BuildContext, DirtyFlags, Element, ElementId, ElementTree, EventContext, RenderHandle,
        UpdateContext, Widget, WidgetExt,
    };
    #[cfg(feature = "winit")]
    pub use crate::window::{Window, WindowBuilder, WindowEvent};

    pub use crate::widget::{Center, Elide, Text};

    pub use crate::core::canvas::{CanvasContext, LineCap, LineJoin, Paint};

    pub use crate::signal::{
        create_effect, create_effect_with_cleanup, create_memo, dispose_effect, use_effect,
        use_effect_with_cleanup, use_signal, EffectId, Memo, RwSignal,
    };

    pub use crate::scale::{set_ui_scale, ui_scale, MAX_UI_SCALE, MIN_UI_SCALE};
    pub use crate::viewport::{viewport_below, viewport_size};

    #[cfg(feature = "tokio")]
    pub use crate::async_hook::use_async;
    pub use crate::async_runtime::run_on_main_thread;
    #[cfg(feature = "tokio")]
    pub use crate::async_runtime::spawn;

    pub use crate::context_provider::{provide_context, try_use_context, use_context};

    #[cfg(feature = "i18n")]
    pub use crate::i18n::{tr, tr_args, trn, trn_args, try_tr, Lang};
    #[cfg(feature = "i18n")]
    pub use crate::{tr, trn};

    pub use crate::widgets::{
        set_dialog_labels, AlertDialog, Autocomplete, Avatar, Badge, BadgeSize, Button, Calendar,
        Canvas, Card, Carousel, Checkbox, Chip, CircularProgress, ColorPicker, ColorValue, Column,
        Combobox, ConfirmDialog, ContextMenu, CrossAxisAlignment, Date, DatePicker, DecoratedBox,
        Dialog, DialogAction, Divider, DividerDirection, Draggable, DropArea, Dropdown,
        DropdownItem, Flex, FlexDirection, FloatingWindow, Grid, Icon, Image, ImageFit, ListItem,
        ListView, MainAxisAlignment, MenuItem, Multiselect, NotificationCtx, NotificationHost,
        NotificationItem, NotificationSeverity, Padding, Pagination, PopupMenu, Portal,
        ProgressBar, Property, PropertyGrid, PropertyValue, RadioButton, RadioGroup, Reactive,
        RichText, Router, RouterView, Row, ScrollDirection, ScrollView, SegmentedButton,
        SelectionMode, Slider, Snackbar, SnackbarPosition, SpinBox, SplitDirection, SplitView,
        Stack, StackFit, Tab, TabBar, TabPosition, TabState, TableColumn, TableView, TextField,
        TextSpan, Time, TimePicker, Toggle, ToolButton, Toolbar, Tooltip, TooltipPosition,
        TopAppBar, TreeNode, TreeView,
    };
}
