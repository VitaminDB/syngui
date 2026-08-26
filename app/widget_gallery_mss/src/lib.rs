//! Widget Gallery — MSS Theme
//!
//! Галерея всех виджетов SYNGUI с Sidebar+Content layout.
//! Модульная система тем: компонентные MSS + динамические :root переменные.
//! 10 тем (5 light + 5 dark) с переключением на лету.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use syngui::core::sync::Mutex;
use syngui::prelude::*;
use syngui::widgets::*;
use std::sync::Arc;

mod sections;
mod styles;
pub mod theme_data;

/// Global gallery context — available via `use_context::<GalleryCtx>()`.
#[derive(Clone)]
pub struct GalleryCtx {
    pub is_dark: RwSignal<bool>,
    pub theme_mss: RwSignal<String>,
    pub current_theme_id: RwSignal<String>,
    pub sidebar_state: RwSignal<usize>,
    pub current_route: RwSignal<String>,
    pub router: Arc<Mutex<Router>>,
}

impl GalleryCtx {
    pub fn navigate(&self, route: &str) {
        if let Ok(mut r) = self.router.lock() {
            r.navigate(route);
        }
        self.current_route.set(route.to_string());
    }

    pub fn navigate_with_sidebar(&self, route: &str, idx: usize) {
        self.navigate(route);
        self.sidebar_state.set(idx);
    }

    pub fn apply_theme(&self, theme: &theme_data::GalleryTheme) {
        self.is_dark.set(theme.is_dark);
        self.current_theme_id.set(theme.id.to_string());
        let full_mss = format!("{}\n{}", theme.to_mss(), styles::component_styles());
        self.theme_mss.set(full_mss);
    }
}

fn build_initial_mss() -> String {
    let theme = theme_data::default_light();
    format!("{}\n{}", theme.to_mss(), styles::component_styles())
}

const ROUTE_KEYS: [&str; 25] = [
    "mss-properties",
    "buttons",
    "input",
    "selection",
    "visual",
    "containers",
    "navigation",
    "scroll",
    "animation",
    "layout-animation",
    "dialogs",
    "menus",
    "dragdrop",
    "canvas",
    "data",
    "feedback",
    "markdown",
    "map",
    "effects",
    "effects-showcase",
    "gradients",
    "charts",
    "ffmpeg-video",
    "terminal",
    "border-test",
];

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run_app() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let initial_mss = build_initial_mss();
    let theme_mss = use_signal(initial_mss.clone());
    syngui::i18n::register_catalogs(&[include_str!("../i18n/en.lang"), include_str!("../i18n/ru.lang")]);
    syngui::i18n::set_language(syngui::i18n::system_language());

    App::new()
        .title("SYNGUI Widget Gallery")
        .size(1280, 900)
        .min_size(1024, 600)
        .maximized(true)
        .vsync(false)
        .gpu_backend(GpuBackend::Auto)
        .gpu_power(GpuPowerPreference::LowPower)
        .with_font_url("fonts/DejaVuSans.ttf")
        .with_emoji_font_url("fonts/NotoColorEmoji.ttf")
        .with_icon_font(syngui::text::icon_fonts::material::FONT_DATA)
        .with_styles_str(&initial_mss)
        .with_dynamic_theme(theme_mss)
        .with_debug_overlay(false)
        .run(move |_ctx| {
            provide_context(make_ctx(theme_mss));
            Box::new(DecoratedBox::new().class("grow").child(move || {
                syngui::i18n::subscribe();
                build_gallery()
            }))
        });
}

fn make_ctx(theme_mss: RwSignal<String>) -> GalleryCtx {
    let is_dark = use_signal(false);
    let current_theme_id = use_signal("clean_modern".to_string());
    let sidebar_state = use_signal(0usize);
    let current_route = use_signal("mss-properties".to_string());
    let router = Arc::new(Mutex::new(Router::new(
        ROUTE_KEYS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "mss-properties",
    )));

    GalleryCtx {
        is_dark,
        theme_mss,
        current_theme_id,
        sidebar_state,
        current_route,
        router,
    }
}

const SECTION_ICONS: [&str; 25] = [
    "🎛", "🔘", "⌨", "☑", "🎨", "📦", "🧭", "📜", "✨", "📐", "💬", "📋", "🔄",
    "🖌", "📊", "💡", "📄", "🗺", "⚡", "🌟", "🎨", "📈", "🎬", "🖥", "⬜",
];

fn section_items() -> Vec<ListItem> {
    ROUTE_KEYS
        .iter()
        .zip(SECTION_ICONS)
        .map(|(key, icon)| ListItem::new(tr!(&format!("gallery.section.{key}"))).icon(icon))
        .collect()
}

fn build_gallery() -> impl Widget {
    let sidebar_state = use_context::<GalleryCtx>().sidebar_state;

    Column::new().gap(0.0).child(build_header()).child(
        DecoratedBox::new().class("grow").child(
            Row::new()
                .gap(0.0)
                .child(
                    Sidebar::new().class("gallery-sidebar").child(
                        DecoratedBox::new().class("grow").child(
                            ListView::new(section_items())
                            .selection_mode(SelectionMode::Single)
                            .selected(vec![sidebar_state.get()])
                            .on_select(move |idx| {
                                if let Some(key) = ROUTE_KEYS.get(idx) {
                                    let ctx = use_context::<GalleryCtx>();
                                    ctx.navigate_with_sidebar(key, idx);
                                }
                            }),
                        ),
                    ),
                )
                .child(DecoratedBox::new().class("grow").child(build_content())),
        ),
    )
}

fn build_header() -> impl Widget {
    let _ctx = use_context::<GalleryCtx>();

    TopAppBar::new(tr!("gallery.title"))
        .action(Badge::new("v0.1").medium().class("header-badge"))
        .action(Text::new(tr!("gallery.language")).class("header-subtitle"))
        .action({
            let current = syngui::i18n::language();
            let mut dd = Dropdown::new();
            for lang in syngui::i18n::languages() {
                dd = dd.item(DropdownItem::new(lang.tag.tag(), lang.name));
            }
            dd.selected(current.tag())
                .on_change(move |tag: &str| syngui::i18n::set_language(tag))
                .style("width", 150.0_f32)
        })
        .action(Text::new(tr!("gallery.theme")).class("header-subtitle"))
        .action({
            let themes = theme_data::builtin_themes();
            let mut dd = Dropdown::new().placeholder(tr!("gallery.theme.placeholder"));
            for t in &themes {
                dd = dd.item(DropdownItem::simple(t.name));
            }
            dd.selected(themes[0].name)
                .on_change(move |name: &str| {
                    if let Some(theme) = theme_data::find_theme_by_name(name) {
                        let ctx = use_context::<GalleryCtx>();
                        ctx.apply_theme(&theme);
                    }
                })
                .style("width", 200.0_f32)
        })
}

fn page_wrap(child: impl Widget + 'static) -> impl Widget {
    Page::new()
        .vertical()
        .scrollbar_policy(ScrollbarPolicy::Auto)
        .child(child)
        .class("content")
}

fn build_content() -> impl Widget {
    let ctx = use_context::<GalleryCtx>();
    RouterView::new(ctx.router.clone())
        .route("mss-properties", || {
            Box::new(page_wrap(
                sections::mss_properties::build_mss_properties_section(),
            ))
        })
        .route("buttons", || {
            Box::new(page_wrap(sections::buttons::build_buttons_section()))
        })
        .route("input", || {
            Box::new(page_wrap(sections::input::build_input_section()))
        })
        .route("selection", || {
            Box::new(page_wrap(sections::selection::build_selection_section()))
        })
        .route("visual", || {
            Box::new(page_wrap(sections::visual::build_visual_section()))
        })
        .route("containers", || {
            Box::new(page_wrap(sections::containers::build_containers_section()))
        })
        .route("navigation", || {
            Box::new(page_wrap(sections::navigation::build_navigation_section()))
        })
        .route("scroll", || {
            Box::new(page_wrap(sections::scroll::build_scroll_section()))
        })
        .route("animation", || {
            Box::new(page_wrap(sections::animation::build_animation_section()))
        })
        .route("layout-animation", || {
            Box::new(page_wrap(
                sections::layout_animation::build_layout_animation_section(),
            ))
        })
        .route("dialogs", || {
            Box::new(page_wrap(sections::dialogs::build_dialogs_section()))
        })
        .route("menus", || {
            Box::new(page_wrap(sections::menus::build_menus_section()))
        })
        .route("dragdrop", || {
            Box::new(page_wrap(sections::dragdrop::build_dragdrop_section()))
        })
        .route("canvas", || {
            Box::new(page_wrap(sections::canvas::build_canvas_section()))
        })
        .route("data", || {
            Box::new(page_wrap(sections::data::build_data_section()))
        })
        .route("feedback", || {
            Box::new(page_wrap(sections::feedback::build_feedback_section()))
        })
        .route("markdown", || {
            Box::new(page_wrap(sections::markdown::build_markdown_section()))
        })
        .route("map", || {
            #[cfg(feature = "map")]
            {
                Box::new(page_wrap(sections::map::build_map_section()))
            }
            #[cfg(not(feature = "map"))]
            {
                Box::new(page_wrap(Text::new("Map widget requires 'map' feature")))
            }
        })
        .route("effects", || {
            Box::new(page_wrap(sections::effects::build_effects_section()))
        })
        .route("effects-showcase", || {
            Box::new(sections::effects_showcase::build_effects_showcase())
        })
        .route("gradients", || {
            Box::new(page_wrap(sections::gradients::build_gradients_section()))
        })
        .route("charts", || {
            Box::new(sections::charts::build_charts_section())
        })
        .route("ffmpeg-video", || {
            #[cfg(feature = "ffmpeg")]
            {
                Box::new(page_wrap(
                    sections::ffmpeg_video::build_ffmpeg_video_section(),
                ))
            }
            #[cfg(not(feature = "ffmpeg"))]
            {
                Box::new(page_wrap(Text::new(
                    "FFmpeg Video plugin требует feature 'ffmpeg' (включает libffmpeg).",
                )))
            }
        })
        .route("terminal", || {
            #[cfg(feature = "terminal")]
            {
                Box::new(page_wrap(sections::terminal::build_terminal_section()))
            }
            #[cfg(not(feature = "terminal"))]
            {
                Box::new(page_wrap(Text::new(
                    "Terminal widget требует feature 'terminal' (portable-pty + vte).",
                )))
            }
        })
        .route("border-test", || {
            Box::new(page_wrap(sections::border_test::build_border_test_section()))
        })
}
