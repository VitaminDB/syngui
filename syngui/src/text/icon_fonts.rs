#[cfg(feature = "material-icons")]
pub mod material {
    pub const FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/MaterialIcons-Regular.ttf");

    pub const FAMILY_NAME: &str = "Material Icons";
}

#[cfg(feature = "font-awesome")]
pub mod awesome {
    pub const SOLID_FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/FontAwesome6-Solid.otf");

    pub const REGULAR_FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/FontAwesome6-Regular.otf");

    pub const BRANDS_FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/FontAwesome6-Brands.otf");

    pub const SOLID_FAMILY_NAME: &str = "Font Awesome 6 Free Solid";
    pub const REGULAR_FAMILY_NAME: &str = "Font Awesome 6 Free";
    pub const BRANDS_FAMILY_NAME: &str = "Font Awesome 6 Brands";
}
