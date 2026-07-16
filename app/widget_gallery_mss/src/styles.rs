/// Assembles component MSS styles from modular files.
/// Theme variables (:root block) are provided separately by theme_data.

pub fn component_styles() -> &'static str {
    concat!(
        // Components
        include_str!("../styles/components/layout_helpers.mss"),
        "\n",
        include_str!("../styles/components/layout.mss"),
        "\n",
        include_str!("../styles/components/sections.mss"),
        "\n",
        include_str!("../styles/components/header.mss"),
        "\n",
        include_str!("../styles/components/dimensions.mss"),
        "\n",
        include_str!("../styles/components/widgets.mss"),
        "\n",
        include_str!("../styles/components/markdown.mss"),
        "\n",
        include_str!("../styles/components/decorative.mss"),
        "\n",
        include_str!("../styles/components/keyframes.mss"),
        "\n",
        include_str!("../styles/components/stepper.mss"),
        "\n",
        include_str!("../styles/components/icon_color.mss"),
        "\n",
        // Pages
        include_str!("../styles/pages/mss_properties.mss"),
        "\n",
        include_str!("../styles/pages/layout_animation.mss"),
        "\n",
        include_str!("../styles/pages/gradients.mss"),
        "\n",
        include_str!("../styles/pages/charts.mss"),
        "\n",
        include_str!("../styles/pages/visual_effects.mss"),
        "\n",
        include_str!("../styles/pages/ffmpeg_video.mss"),
    )
}
