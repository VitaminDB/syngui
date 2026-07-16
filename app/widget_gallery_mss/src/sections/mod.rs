pub mod animation;
pub mod border_test;
pub mod buttons;
pub mod canvas;
pub mod containers;
pub mod data;
pub mod dialogs;
pub mod dragdrop;
pub mod feedback;
pub mod input;
pub mod menus;
pub mod navigation;
pub mod scroll;
pub mod selection;
pub mod visual;
pub mod markdown;
#[cfg(feature = "map")]
pub mod map;
pub mod layout_animation;
pub mod mss_properties;
pub mod effects;
pub mod gradients;
pub mod charts;
pub mod effects_showcase;
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_video;
#[cfg(feature = "terminal")]
pub mod terminal;

use syngui::prelude::*;

pub(crate) fn section_card(child: impl Widget + 'static) -> impl Widget {
    DecoratedBox::new()
        .child(child)
        .class("section-card")
}

pub(crate) fn section_title(title: &str) -> impl Widget {
    Column::new()
        .gap(4.0)
        .child(Text::new(title).class("section-title"))
        .child(Divider::horizontal())
}

pub(crate) fn label(text: &str) -> impl Widget {
    Text::new(text).class("label")
}
