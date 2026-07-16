//! Effects Showcase — 3-panel layout with sub-sidebar for effect categories.

mod shadows;
mod glow;
mod blur_glass;
mod filters;
mod color_effects;
mod outline;
mod overlay_effects;
mod distortion;
mod opacity;
mod chains;
mod transitions;
mod keyframes_fx;

use syngui::prelude::*;
use syngui::widgets::*;
use std::sync::Arc;
use syngui::core::sync::Mutex;


const ROUTE_KEYS: [&str; 12] = [
    "shadows",
    "glow",
    "blur-glass",
    "filters",
    "color-effects",
    "outline",
    "overlay-effects",
    "distortion",
    "opacity",
    "chains",
    "transitions",
    "keyframes",
];

const ROUTE_NAMES: [&str; 12] = [
    "Shadows",
    "Glow & Bloom",
    "Blur & Glass",
    "Filters",
    "Color Effects",
    "Outline & Stroke",
    "Overlay Effects",
    "Distortion",
    "Opacity",
    "Filter Chains",
    "Transitions",
    "Keyframe Animations",
];

const ROUTE_ICONS: [&str; 12] = [
    "🌑", "✨", "🔮", "🎨", "🎭", "⭕", "🌫", "🌀", "👁", "🔗", "🔄", "🎬",
];

pub fn build_effects_showcase() -> impl Widget {
    let router = Arc::new(Mutex::new(Router::new(
        ROUTE_KEYS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "shadows",
    )));

    let items: Vec<ListItem> = ROUTE_NAMES.iter().zip(ROUTE_ICONS.iter())
        .map(|(name, icon)| ListItem::new(*name).icon(*icon))
        .collect();

    Row::new()
        .gap(0.0)
        .child(
            Sidebar::new()
                .class("effects-sidebar")
                .child(
                    DecoratedBox::new().class("grow").child(
                        ListView::new(items)
                            .selection_mode(SelectionMode::Single)
                            .selected(vec![0])
                            .on_select({
                                let r = router.clone();
                                move |idx| {
                                    if let Some(key) = ROUTE_KEYS.get(idx) {
                                        r.lock().unwrap().navigate(*key);
                                    }
                                }
                            })
                    )
                ),
        )
        .child(
            DecoratedBox::new().class("grow").child(
                RouterView::new(router)
                    .route("shadows", || Box::new(page(shadows::build())))
                    .route("glow", || Box::new(page(glow::build())))
                    .route("blur-glass", || Box::new(page(blur_glass::build())))
                    .route("filters", || Box::new(page(filters::build())))
                    .route("color-effects", || Box::new(page(color_effects::build())))
                    .route("outline", || Box::new(page(outline::build())))
                    .route("overlay-effects", || Box::new(page(overlay_effects::build())))
                    .route("distortion", || Box::new(page(distortion::build())))
                    .route("opacity", || Box::new(page(opacity::build())))
                    .route("chains", || Box::new(page(chains::build())))
                    .route("transitions", || Box::new(page(transitions::build())))
                    .route("keyframes", || Box::new(page(keyframes_fx::build()))),
            ),
        )
}

fn page(child: impl Widget + 'static) -> impl Widget {
    Page::new()
        .vertical()
        .scrollbar_policy(ScrollbarPolicy::Auto)
        .child(child)
        .style("padding", 24.0_f32)
        .class("content")
}

/// Helper: demo box with gradient background + filter/effect class
#[allow(dead_code)]
pub(crate) fn fx_subject(gradient_class: &str, filter_class: &str) -> impl Widget {
    use syngui::mgui;
    DecoratedBox::new()
        .class("fx-demo-box")
        .class(gradient_class)
        .class(filter_class)
        .child(mgui! {
            Column::new().gap(4.0) => [
                Text::new("Aa Bb").bold().class("fx-demo-text").style("font-size", 18.0_f32),
                Row::new().gap(6.0) => [
                    Text::new("1 2 3").class("fx-demo-text").style("font-size", 12.0_f32),
                    Icon::new("★").class("fx-demo-text"),
                ],
            ]
        })
}

/// Helper: filter demo card with label + MSS code
pub(crate) fn filter_card(gradient_class: &str, filter_class: &str, label_text: &str, mss_code: &str) -> impl Widget {
    use syngui::mgui;
    mgui! {
        Column::new().gap(6.0) => [
            fx_subject(gradient_class, filter_class),
            Text::new(label_text).bold().class("fx-label"),
            Text::new(mss_code).class("fx-code"),
        ]
    }
}

/// Helper: shadow/outline card with surface background
pub(crate) fn shadow_card(extra_class: &str, label_text: &str, mss_code: &str) -> impl Widget {
    use syngui::mgui;
    mgui! {
        Column::new().gap(6.0) => [
            DecoratedBox::new()
                .class("fx-shadow-subject")
                .class(extra_class)
                .child(
                    Text::new(label_text).bold().class("text-primary").style("font-size", 13.0_f32)
                ),
            Text::new(label_text).bold().class("fx-label"),
            Text::new(mss_code).class("fx-code"),
        ]
    }
}

/// Helper: showcase card with gradient bg and effect
#[allow(dead_code)]
pub(crate) fn showcase_card(bg_class: &str, fx_class: &str, title: &str, code: &str, desc: &str) -> impl Widget {
    use syngui::mgui;
    mgui! {
        Column::new().gap(8.0) => [
            DecoratedBox::new()
                .class("fx-showcase-box")
                .class(bg_class)
                .class(fx_class)
                .child(
                    Column::new().gap(4.0)
                        .child(Text::new(title).bold().class("fx-demo-text").style("font-size", 14.0_f32))
                        .child(Icon::new("★").class("fx-demo-text"))
                ),
            Text::new(title).bold().class("fx-label"),
            Text::new(code).class("fx-code"),
            Text::new(desc).class("label"),
        ]
    }
}

/// Helper: surface card with effect (for shadows, outlines, glow)
pub(crate) fn surface_card(fx_class: &str, title: &str, code: &str, desc: &str) -> impl Widget {
    use syngui::mgui;
    mgui! {
        Column::new().gap(8.0) => [
            DecoratedBox::new()
                .class("fx-showcase-card")
                .class(fx_class)
                .child(
                    Column::new().gap(4.0)
                        .child(Text::new(title).bold().class("text-primary").style("font-size", 14.0_f32))
                        .child(Text::new(desc).class("label").style("font-size", 11.0_f32))
                ),
            Text::new(code).class("fx-code"),
        ]
    }
}
