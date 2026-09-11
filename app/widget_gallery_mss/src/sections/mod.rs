pub mod animation;
pub mod border_test;
pub mod buttons;
pub mod canvas;
pub mod charts;
pub mod containers;
pub mod data;
pub mod dialogs;
pub mod dragdrop;
pub mod effects;
pub mod effects_showcase;
pub mod feedback;
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_video;
pub mod gradients;
pub mod input;
pub mod layout_animation;
#[cfg(feature = "map")]
pub mod map;
pub mod markdown;
pub mod menus;
pub mod mss_properties;
pub mod navigation;
pub mod scroll;
pub mod selection;
#[cfg(feature = "terminal")]
pub mod terminal;
pub mod visual;

use syngui::prelude::*;

pub(crate) fn section_card(child: impl Widget + 'static) -> impl Widget {
    DecoratedBox::new().child(child).class("section-card")
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
