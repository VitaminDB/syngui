//! Widget Gallery — MSS Theme
//!
//! Галерея всех виджетов SYNGUI с Sidebar+Content layout.
//! Модульная система тем: компонентные MSS + динамические :root переменные.
//! 10 тем (5 light + 5 dark) с переключением на лету.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use std::sync::Arc;
use syngui::core::sync::Mutex;
use syngui::prelude::*;
use syngui::widgets::*;

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

    /// Светлая ↔ тёмная: парная тема с тем же номером в своём списке
    /// (clean_modern ↔ catppuccin, ocean ↔ nord …). Цвета перетекают
    /// благодаря `with_theme_transition`.
    pub fn toggle_dark(&self) {
        let themes = theme_data::builtin_themes();
        let cur = self.current_theme_id.get_untracked();
        let dark = self.is_dark.get_untracked();
        let same: Vec<&theme_data::GalleryTheme> = themes.iter().filter(|t| t.is_dark == dark).collect();
        let other: Vec<&theme_data::GalleryTheme> = themes.iter().filter(|t| t.is_dark != dark).collect();
        let idx = same.iter().position(|t| t.id == cur).unwrap_or(0);
        if let Some(t) = other.get(idx).or_else(|| other.first()) {
            self.apply_theme(t);
        }
    }
}

/// Раздел галереи: ключ маршрута, значок Material и группа в боковой панели.
struct Section {
    key: &'static str,
    icon: &'static str,
    group: &'static str,
}

const SECTIONS: &[Section] = &[
    Section { key: "mss-properties", icon: "\u{E429}", group: "basics" },
    Section { key: "buttons", icon: "\u{E913}", group: "basics" },
    Section { key: "input", icon: "\u{E262}", group: "basics" },
    Section { key: "selection", icon: "\u{E834}", group: "basics" },
    Section { key: "visual", icon: "\u{E3F4}", group: "basics" },
    Section { key: "containers", icon: "\u{E8AA}", group: "basics" },
    Section { key: "navigation", icon: "\u{E87A}", group: "basics" },
    Section { key: "scroll", icon: "\u{E8D5}", group: "basics" },
    Section { key: "motion", icon: "\u{E176}", group: "motion" },
    Section { key: "animation", icon: "\u{E9C1}", group: "motion" },
    Section { key: "layout-animation", icon: "\u{E9E4}", group: "motion" },
    Section { key: "dialogs", icon: "\u{E0B7}", group: "overlays" },
    Section { key: "menus", icon: "\u{E5D2}", group: "overlays" },
    Section { key: "dragdrop", icon: "\u{E945}", group: "overlays" },
    Section { key: "feedback", icon: "\u{E87F}", group: "overlays" },
    Section { key: "canvas", icon: "\u{E3AE}", group: "content" },
    Section { key: "data", icon: "\u{E265}", group: "content" },
    Section { key: "markdown", icon: "\u{EF42}", group: "content" },
    Section { key: "charts", icon: "\u{E26B}", group: "content" },
    Section { key: "gradients", icon: "\u{E3E9}", group: "content" },
    Section { key: "effects-showcase", icon: "\u{E65F}", group: "effects" },
    Section { key: "three-d", icon: "\u{E9FE}", group: "effects" },
    Section { key: "effects", icon: "\u{EA0B}", group: "effects" },
    Section { key: "map", icon: "\u{E55B}", group: "plugins" },
    Section { key: "ffmpeg-video", icon: "\u{E02C}", group: "plugins" },
    Section { key: "terminal", icon: "\u{EB8E}", group: "plugins" },
    Section { key: "border-test", icon: "\u{E228}", group: "debug" },
];

const GROUPS: &[&str] = &["basics", "motion", "overlays", "content", "effects", "plugins", "debug"];

fn build_initial_mss() -> String {
    let theme = theme_data::default_light();
    format!("{}\n{}", theme.to_mss(), styles::component_styles())
}

fn route_keys() -> Vec<String> {
    SECTIONS.iter().map(|s| s.key.to_string()).collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run_app() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let initial_mss = build_initial_mss();
    let theme_mss = use_signal(initial_mss.clone());
    syngui::i18n::register_catalogs(&[
        include_str!("../i18n/en.lang"),
        include_str!("../i18n/ru.lang"),
    ]);
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
        .with_theme_transition(360)
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
    let router = Arc::new(Mutex::new(Router::new(route_keys(), "mss-properties")));

    GalleryCtx {
        is_dark,
        theme_mss,
        current_theme_id,
        sidebar_state,
        current_route,
        router,
    }
}

fn section_name(key: &str) -> String {
    tr!(&format!("gallery.section.{key}"))
}

/// Боковая панель: группы разделов, значки Material, активный пункт.
fn build_sidebar() -> impl Widget {
    let current = use_context::<GalleryCtx>().current_route;
    Sidebar::new().class("gallery-sidebar").child(
        Page::new().vertical().scrollbar_policy(ScrollbarPolicy::Auto).child(
            Column::new().gap(0.0).class("nav-list").child(move || {
                let cur = current.get();
                let mut col = Column::new().gap(2.0);
                for group in GROUPS {
                    col = col.child(Text::new(tr!(&format!("gallery.group.{group}"))).class("nav-group"));
                    for s in SECTIONS.iter().filter(|s| s.group == *group) {
                        let active = cur == s.key;
                        let key = s.key;
                        col = col.child(
                            GestureDetector::new()
                                .child(
                                    DecoratedBox::new()
                                        .child(
                                            Row::new()
                                                .gap(10.0)
                                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                                .child(Icon::new(s.icon).class("nav-icon"))
                                                .child(Text::new(section_name(key)).class("nav-text")),
                                        )
                                        .class(if active { "nav-item nav-item-active" } else { "nav-item" }),
                                )
                                .cursor(syngui::input::CursorIcon::Pointer)
                                .on_click(move || {
                                    let ctx = use_context::<GalleryCtx>();
                                    let idx = SECTIONS.iter().position(|s| s.key == key).unwrap_or(0);
                                    ctx.navigate_with_sidebar(key, idx);
                                }),
                        );
                    }
                }
                col
            }),
        ),
    )
}

fn build_gallery() -> impl Widget {
    Column::new().gap(0.0).child(build_header()).child(
        DecoratedBox::new().class("grow").child(
            Row::new()
                .gap(0.0)
                .child(build_sidebar())
                .child(DecoratedBox::new().class("grow").child(build_content())),
        ),
    )
}

fn build_header() -> impl Widget {
    let ctx = use_context::<GalleryCtx>();
    let is_dark = ctx.is_dark;

    TopAppBar::new(tr!("gallery.title"))
        .action(Badge::new(concat!("v", env!("CARGO_PKG_VERSION"))).medium().class("header-badge"))
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
        .action(move || {
            let themes = theme_data::builtin_themes();
            let cur = use_context::<GalleryCtx>().current_theme_id.get();
            let mut dd = Dropdown::new().placeholder(tr!("gallery.theme.placeholder"));
            for t in &themes {
                dd = dd.item(DropdownItem::simple(t.name));
            }
            let name = themes.iter().find(|t| t.id == cur).map(|t| t.name).unwrap_or(themes[0].name);
            dd.selected(name)
                .on_change(move |name: &str| {
                    if let Some(theme) = theme_data::find_theme_by_name(name) {
                        let ctx = use_context::<GalleryCtx>();
                        ctx.apply_theme(&theme);
                    }
                })
                .style("width", 200.0_f32)
        })
        .action(move || {
            let dark = is_dark.get();
            ToolButton::new(if dark { "\u{E518}" } else { "\u{E51C}" })
                .tooltip(if dark { tr!("gallery.light") } else { tr!("gallery.dark") })
                .on_click(|| use_context::<GalleryCtx>().toggle_dark())
                .class("header-toggle")
        })
}

fn page_wrap(child: impl Widget + 'static) -> impl Widget {
    Page::new()
        .vertical()
        .scrollbar_policy(ScrollbarPolicy::Auto)
        .child(child)
        .class("content")
}

/// Страница раздела: при смене маршрута старая уезжает вверх и тает,
/// новая въезжает снизу (AnimatedSwitcher по номеру раздела).
fn build_content() -> impl Widget {
    let current = use_context::<GalleryCtx>().current_route;
    DecoratedBox::new().class("grow content-switch").child(move || {
        let key = current.get();
        let idx = SECTIONS.iter().position(|s| s.key == key).unwrap_or(0) as u64;
        AnimatedSwitcher::new(idx, move || section_page(&key))
            .slide(0.0, 18.0)
            .duration_ms(240)
            .exit_duration_ms(140)
            .animate_size(false)
            .class("grow")
    })
}

fn section_page(key: &str) -> Box<dyn Widget> {
    match key {
        "mss-properties" => Box::new(page_wrap(sections::mss_properties::build_mss_properties_section())),
        "buttons" => Box::new(page_wrap(sections::buttons::build_buttons_section())),
        "input" => Box::new(page_wrap(sections::input::build_input_section())),
        "selection" => Box::new(page_wrap(sections::selection::build_selection_section())),
        "visual" => Box::new(page_wrap(sections::visual::build_visual_section())),
        "containers" => Box::new(page_wrap(sections::containers::build_containers_section())),
        "navigation" => Box::new(page_wrap(sections::navigation::build_navigation_section())),
        "scroll" => Box::new(page_wrap(sections::scroll::build_scroll_section())),
        "motion" => Box::new(page_wrap(sections::motion::build_motion_section())),
        "animation" => Box::new(page_wrap(sections::animation::build_animation_section())),
        "layout-animation" => Box::new(page_wrap(sections::layout_animation::build_layout_animation_section())),
        "dialogs" => Box::new(page_wrap(sections::dialogs::build_dialogs_section())),
        "menus" => Box::new(page_wrap(sections::menus::build_menus_section())),
        "dragdrop" => Box::new(page_wrap(sections::dragdrop::build_dragdrop_section())),
        "canvas" => Box::new(page_wrap(sections::canvas::build_canvas_section())),
        "data" => Box::new(page_wrap(sections::data::build_data_section())),
        "feedback" => Box::new(page_wrap(sections::feedback::build_feedback_section())),
        "markdown" => Box::new(page_wrap(sections::markdown::build_markdown_section())),
        "map" => {
            #[cfg(feature = "map")]
            {
                Box::new(page_wrap(sections::map::build_map_section()))
            }
            #[cfg(not(feature = "map"))]
            {
                Box::new(page_wrap(Text::new("Map widget requires 'map' feature")))
            }
        }
        "effects" => Box::new(page_wrap(sections::effects::build_effects_section())),
        "effects-showcase" => Box::new(sections::effects_showcase::build_effects_showcase()),
        "three-d" => Box::new(page_wrap(sections::three_d::build_three_d_section())),
        "gradients" => Box::new(page_wrap(sections::gradients::build_gradients_section())),
        "charts" => Box::new(sections::charts::build_charts_section()),
        "ffmpeg-video" => {
            #[cfg(feature = "ffmpeg")]
            {
                Box::new(page_wrap(sections::ffmpeg_video::build_ffmpeg_video_section()))
            }
            #[cfg(not(feature = "ffmpeg"))]
            {
                Box::new(page_wrap(Text::new("FFmpeg Video plugin требует feature 'ffmpeg' (включает libffmpeg).")))
            }
        }
        "terminal" => {
            #[cfg(feature = "terminal")]
            {
                Box::new(page_wrap(sections::terminal::build_terminal_section()))
            }
            #[cfg(not(feature = "terminal"))]
            {
                Box::new(page_wrap(Text::new("Terminal widget требует feature 'terminal' (portable-pty + vte).")))
            }
        }
        "border-test" => Box::new(page_wrap(sections::border_test::build_border_test_section())),
        _ => Box::new(page_wrap(Text::new("…"))),
    }
}

#[allow(dead_code)]
fn build_router_content() -> impl Widget {
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
        .route("three-d", || {
            Box::new(page_wrap(sections::three_d::build_three_d_section()))
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
