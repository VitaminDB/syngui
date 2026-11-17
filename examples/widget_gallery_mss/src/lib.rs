//! Widget Gallery — MSS Theme
//!
//! Галерея всех виджетов SYNGUI: заголовок окна (CSD), боковая панель
//! разделов и страница. Три оформления (`theme_data::DESIGNS`) — у каждого
//! своя оболочка и свой вид виджетов, светлая и тёмная палитра, переключение
//! на лету.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use std::sync::Arc;
use syngui::core::sync::Mutex;
use syngui::prelude::*;
use syngui::widgets::*;
use syngui::overlay::{ControlsSide, SystemWindowControls, WindowDragRegion, WindowResizeRegion};
use syngui::window::WindowState;

mod sections;
mod styles;
pub mod theme_data;

/// Global gallery context — available via `use_context::<GalleryCtx>()`.
#[derive(Clone)]
pub struct GalleryCtx {
    pub is_dark: RwSignal<bool>,
    /// Оформление (`theme_data::Design::id`).
    pub design: RwSignal<String>,
    pub theme_mss: RwSignal<String>,
    pub sidebar_state: RwSignal<usize>,
    pub current_route: RwSignal<String>,
    pub router: Arc<Mutex<Router>>,
    pub window: RwSignal<WindowState>,
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

    /// Оформление и светлая/тёмная палитра. Цвета перетекают благодаря
    /// `with_theme_transition`; смена оформления пересобирает оболочку.
    pub fn apply(&self, design: &str, dark: bool) {
        let d = theme_data::find_design(design);
        self.theme_mss.set(d.full_mss(dark));
        self.is_dark.set(dark);
        if self.design.get_untracked() != d.id {
            self.design.set(d.id.to_string());
        }
    }

    pub fn toggle_dark(&self) {
        self.apply(&self.design.get_untracked(), !self.is_dark.get_untracked());
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
    Section { key: "lottie", icon: "\u{E71C}", group: "motion" },
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

/// Шрифты оформлений (OFL, `assets/fonts`): Aurora — Outfit, Brutal —
/// Space Grotesk и JetBrains Mono для подписей.
fn register_fonts() {
    syngui::text::register_font("Outfit", &include_bytes!("../assets/fonts/Outfit.ttf")[..]);
    syngui::text::register_font("Space Grotesk", &include_bytes!("../assets/fonts/SpaceGrotesk.ttf")[..]);
    syngui::text::register_font("JetBrains Mono", &include_bytes!("../assets/fonts/JetBrainsMono.ttf")[..]);
}

fn route_keys() -> Vec<String> {
    SECTIONS.iter().map(|s| s.key.to_string()).collect()
}

/// Начальное оформление: `GALLERY_DESIGN`, иначе Studio; тёмное — по системе
/// или `GALLERY_DARK=1|0`.
fn initial_design() -> (String, bool) {
    let design = std::env::var("GALLERY_DESIGN").unwrap_or_default();
    let design = theme_data::find_design(&design).id.to_string();
    let dark = match std::env::var("GALLERY_DARK").ok().as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => syngui::appearance::read_system_appearance().is_dark(),
    };
    (design, dark)
}

/// Снимок без окна: `widget_gallery_mss --screenshot СНИМОК.png [--size ШxВ]
/// [--scale N] [--page РАЗДЕЛ] [--design ОФОРМЛЕНИЕ] [--dark|--light] [--lang ЯЗЫК]`.
#[cfg(not(target_arch = "wasm32"))]
struct ShotArgs {
    out: String,
    size: (u32, u32),
    scale: f64,
    page: Option<String>,
    design: String,
    dark: bool,
    lang: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_shot_args() -> Option<ShotArgs> {
    let (design, dark) = initial_design();
    let mut a = ShotArgs { out: String::new(), size: (1440, 900), scale: 1.0, page: None, design, dark, lang: None };
    let mut args = std::env::args().skip(1);
    let mut shot = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("widget_gallery_mss [--screenshot СНИМОК.png [--size ШxВ] [--scale N] [--page РАЗДЕЛ] [--design ОФОРМЛЕНИЕ] [--dark|--light] [--lang ЯЗЫК]]");
                println!("разделы: {}", SECTIONS.iter().map(|s| s.key).collect::<Vec<_>>().join(" "));
                println!("оформления: {}", theme_data::DESIGNS.iter().map(|d| d.id).collect::<Vec<_>>().join(" "));
                std::process::exit(0);
            }
            "--screenshot" => {
                shot = true;
                a.out = args.next().unwrap_or_else(|| "gallery.png".into());
            }
            "--size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')) {
                    a.size = (w.parse().unwrap_or(a.size.0), h.parse().unwrap_or(a.size.1));
                }
            }
            "--scale" => a.scale = args.next().and_then(|s| s.parse().ok()).unwrap_or(a.scale),
            "--page" => a.page = args.next(),
            "--design" | "--theme" => a.design = theme_data::find_design(&args.next().unwrap_or_default()).id.to_string(),
            "--dark" => a.dark = true,
            "--light" => a.dark = false,
            "--lang" => a.lang = args.next(),
            _ => {}
        }
    }
    shot.then_some(a)
}

#[cfg(not(target_arch = "wasm32"))]
fn run_shot(a: ShotArgs) {
    let mss = theme_data::find_design(&a.design).full_mss(a.dark);
    let (page, design, dark, mss2) = (a.page.clone(), a.design.clone(), a.dark, mss.clone());
    let r = syngui::shot::render_png(
        move || {
            let ctx = make_ctx(use_signal(mss2), page.as_deref(), &design, dark);
            provide_context(ctx);
            Box::new(build_gallery())
        },
        &mss,
        a.size,
        a.scale,
        &a.out,
    );
    match r {
        Ok((w, h)) => println!("{}: {w}x{h}", a.out),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn init_i18n(lang: Option<&str>) {
    syngui::i18n::register_catalogs(&[
        include_str!("../i18n/en.lang"),
        include_str!("../i18n/ru.lang"),
    ]);
    match lang {
        Some(l) => syngui::i18n::set_language(l),
        None => syngui::i18n::set_language(syngui::i18n::system_language()),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run_app() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
    register_fonts();

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(a) = parse_shot_args() {
        init_i18n(a.lang.as_deref());
        run_shot(a);
        return;
    }

    let (design, dark) = initial_design();
    let initial_mss = theme_data::find_design(&design).full_mss(dark);
    let theme_mss = use_signal(initial_mss.clone());
    let window = use_signal(WindowState::default());
    init_i18n(None);

    App::new()
        .title("SYNGUI Widget Gallery")
        .app_id("widget_gallery_mss")
        .size(1280, 900)
        .min_size(1024, 600)
        .maximized(true)
        .frameless()
        .transparent(true)
        .background(syngui::core::Color::from_srgb(0, 0, 0, 0.0))
        .vsync(false)
        .gpu_backend(GpuBackend::Auto)
        .gpu_power(GpuPowerPreference::LowPower)
        .with_font_url("fonts/DejaVuSans.ttf")
        .with_emoji_font_url("fonts/NotoColorEmoji.ttf")
        .with_icon_font(syngui::text::icon_fonts::material::FONT_DATA)
        .with_styles_str(&initial_mss)
        .with_dynamic_theme(theme_mss)
        .with_theme_transition(360)
        .with_window_state(window)
        .with_debug_overlay(false)
        .run(move |_ctx| {
            let ctx = make_ctx(theme_mss, None, &design, dark);
            provide_context(GalleryCtx { window, ..ctx });
            Box::new(DecoratedBox::new().class("grow").child(move || {
                syngui::i18n::subscribe();
                build_gallery()
            }))
        });
}

fn make_ctx(theme_mss: RwSignal<String>, page: Option<&str>, design: &str, dark: bool) -> GalleryCtx {
    // Начальный раздел: `--page` снимка или GALLERY_ROUTE=motion (проверка вёрстки).
    let initial = page
        .map(str::to_string)
        .or_else(|| std::env::var("GALLERY_ROUTE").ok())
        .filter(|r| SECTIONS.iter().any(|s| s.key == r))
        .unwrap_or_else(|| "mss-properties".to_string());
    let sidebar_state = use_signal(SECTIONS.iter().position(|s| s.key == initial).unwrap_or(0));
    let current_route = use_signal(initial.clone());
    let router = Arc::new(Mutex::new(Router::new(route_keys(), &initial)));

    GalleryCtx {
        is_dark: use_signal(dark),
        design: use_signal(design.to_string()),
        theme_mss,
        sidebar_state,
        current_route,
        router,
        window: use_signal(WindowState::default()),
    }
}

fn section_name(key: &str) -> String {
    tr!(&format!("gallery.section.{key}"))
}

type W = Box<dyn Widget>;

/// Боковая панель: группы разделов, значки Material, активный пункт.
///
/// Вид пунктов — дело оформления: в Studio активная вкладка перетекает в
/// карточку (`flow-edge`), в Aurora это светящаяся пилюля, в Brutal — номер
/// раздела моноширинным шрифтом и плашка с жёсткой тенью.
fn build_sidebar(design: &str) -> impl Widget {
    let current = use_context::<GalleryCtx>().current_route;
    let numbered = design == "brutal";
    DecoratedBox::new().class("gallery-sidebar").child(
        Page::new().vertical().scrollbar_policy(ScrollbarPolicy::Auto).child(
            Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("nav-list").child(move || {
                let cur = current.get();
                let mut col = Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch);
                let mut n = 0;
                for group in GROUPS {
                    col = col.child(Text::new(tr!(&format!("gallery.group.{group}"))).class("nav-group"));
                    for s in SECTIONS.iter().filter(|s| s.group == *group) {
                        n += 1;
                        let active = cur == s.key;
                        let key = s.key;
                        let mut row = Row::new().gap(10.0).cross_axis_alignment(CrossAxisAlignment::Center);
                        row = if numbered {
                            row.child(Text::new(format!("{n:02}")).class("nav-num"))
                        } else {
                            row.child(Icon::new(s.icon).class("nav-icon"))
                        };
                        row = row.child(Text::new(section_name(key)).max_lines(1).elide(Elide::End).class("nav-text"));
                        col = col.child(
                            GestureDetector::new()
                                .child(DecoratedBox::new().child(row).class(if active { "nav-item nav-item-active" } else { "nav-item" }))
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
        ).class("nav-page"),
    )
}

/// Знак галереи: квадрат с градиентом (Studio, Aurora) или чёрная плашка (Brutal).
fn brand() -> impl Widget {
    Row::new()
        .gap(10.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(DecoratedBox::new().class("brand-mark").child(Icon::new("\u{E1BD}").class("brand-icon")))
        .child(
            Column::new()
                .gap(0.0)
                .child(Text::new("SYNGUI").class("brand-title"))
                .child(Text::new(tr!("gallery.brand.sub")).class("brand-sub")),
        )
}

/// Заголовок окна (CSD): знак, перетаскиваемая середина с названием
/// раздела, переключатели оформления, тёмной темы и языка, кнопки окна.
fn titlebar() -> impl Widget {
    let ctx = use_context::<GalleryCtx>();
    let (route, design, is_dark, window) = (ctx.current_route, ctx.design, ctx.is_dark, ctx.window);
    let title = Reactive::new(move || -> Vec<W> {
        let key = route.get();
        let group = SECTIONS.iter().find(|s| s.key == key).map(|s| s.group).unwrap_or("basics");
        vec![Box::new(
            Row::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Text::new(tr!(&format!("gallery.group.{group}"))).class("tb-crumb"))
                .child(Text::new("/").class("tb-crumb-sep"))
                .child(Text::new(section_name(&key)).max_lines(1).elide(Elide::End).class("tb-title")),
        )]
    });
    let designs = Reactive::new(move || -> Vec<W> {
        let cur = design.get();
        let idx = theme_data::DESIGNS.iter().position(|d| d.id == cur).unwrap_or(0);
        let names: Vec<&str> = theme_data::DESIGNS.iter().map(|d| d.name).collect();
        vec![Box::new(
            SegmentedButton::new(names)
                .selected(idx)
                .on_change(|i| {
                    let ctx = use_context::<GalleryCtx>();
                    if let Some(d) = theme_data::DESIGNS.get(i) {
                        ctx.apply(d.id, ctx.is_dark.get_untracked());
                    }
                })
                .class("tb-designs"),
        )]
    });
    let dark_btn = Reactive::new(move || -> Vec<W> {
        let dark = is_dark.get();
        vec![Box::new(
            ToolButton::new(if dark { "\u{E518}" } else { "\u{E51C}" })
                .tooltip(if dark { tr!("gallery.light") } else { tr!("gallery.dark") })
                .on_click(|| use_context::<GalleryCtx>().toggle_dark())
                .class("tb-btn"),
        )]
    });
    let lang = {
        let current = syngui::i18n::language();
        let mut dd = Dropdown::new();
        for l in syngui::i18n::languages() {
            dd = dd.item(DropdownItem::new(l.tag.tag(), l.name));
        }
        dd.selected(current.tag()).on_change(move |tag: &str| syngui::i18n::set_language(tag)).class("tb-lang")
    };
    let controls = Reactive::new(move || -> Vec<W> {
        let w = window.get();
        vec![Box::new(SystemWindowControls::new(ControlsSide::Right).maximized(w.maximized).active(w.focused))]
    });
    DecoratedBox::new().class("titlebar").child(
        Row::new()
            .gap(0.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(DecoratedBox::new().class("tb-left").child(brand()))
            .child(DecoratedBox::new().class("grow tb-drag-wrap").child(WindowDragRegion::new().child(DecoratedBox::new().class("tb-drag").child(title))))
            .child(
                DecoratedBox::new().class("tb-tools").child(
                    Row::new()
                        .gap(8.0)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .child(designs)
                        .child(lang)
                        .child(dark_btn),
                ),
            )
            .child(DecoratedBox::new().class("window-controls").child(controls)),
    )
}

/// Фон Aurora: три размытых пятна радиальных градиентов медленно плывут
/// (keyframes), над ними — стеклянные панели с размытием подложки.
fn aurora_backdrop() -> impl Widget {
    Stack::new()
        .fit(StackFit::Expand)
        .child(DecoratedBox::new().class("aurora-base"))
        .child(DecoratedBox::new().class("aurora-blob aurora-blob-1"))
        .child(DecoratedBox::new().class("aurora-blob aurora-blob-2"))
        .child(DecoratedBox::new().class("aurora-blob aurora-blob-3"))
        .child(DecoratedBox::new().class("aurora-grain"))
}

/// Бегущая строка Brutal внизу окна: одинаковые куски фиксированной ширины
/// едут влево ровно на ширину куска и повторяются без шва.
fn ticker() -> impl Widget {
    let text = tr!("gallery.ticker");
    let mut track = Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Center).class("ticker-track");
    for _ in 0..6 {
        track = track.child(DecoratedBox::new().class("ticker-chunk").child(Text::new(text.clone()).max_lines(1).class("ticker-text")));
    }
    DecoratedBox::new().class("ticker").clip(true).child(track)
}

fn content_card() -> impl Widget {
    DecoratedBox::new().class("grow content-frame").child(DecoratedBox::new().class("content-card").child(build_content()))
}

/// Оболочка оформления.
fn shell(design: &str) -> W {
    match design {
        // Плавающие стеклянные панели над сиянием.
        "aurora" => Box::new(
            Stack::new().fit(StackFit::Expand).child(aurora_backdrop()).child(
                Column::new()
                    .gap(12.0)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .class("shell")
                    .child(titlebar())
                    .child(
                        Row::new()
                            .gap(12.0)
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .class("grow")
                            .child(build_sidebar(design))
                            .child(content_card()),
                    ),
            ),
        ),
        // Заголовок, панель и страница — блоки в толстых рамках.
        "brutal" => Box::new(
            Column::new()
                .gap(0.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .class("shell")
                .child(titlebar())
                .child(Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("grow").child(build_sidebar(design)).child(content_card()))
                .child(ticker()),
        ),
        // Studio: заголовок и панель — одна тёмная рамка, внутри карточка.
        _ => Box::new(
            Column::new()
                .gap(0.0)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .class("shell")
                .child(titlebar())
                .child(
                    DecoratedBox::new()
                        .class("grow gallery-frame")
                        .child(Row::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch).child(build_sidebar(design)).child(content_card())),
                ),
        ),
    }
}

/// Окно галереи: оболочка текущего оформления в скруглённой рамке окна
/// (без скругления развёрнутым) с изменением размера за края.
fn build_gallery() -> impl Widget {
    let design = use_context::<GalleryCtx>().design;
    Reactive::new(move || -> Vec<W> {
        let d = design.get();
        let frame = DecoratedBox::new().class(format!("window-frame design-{d}")).clip(true).child(shell(&d));
        vec![Box::new(WindowResizeRegion::new().over_content(true).inset(5.0).child(frame))]
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
    DecoratedBox::new().class("grow").child(move || {
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
        "lottie" => Box::new(page_wrap(sections::lottie::build_lottie_section())),
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
