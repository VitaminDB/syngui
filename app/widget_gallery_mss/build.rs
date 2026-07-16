fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winres::WindowsResource::new();
        // When cross-compiling from Linux, use mingw windres
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_env == "gnu" {
            res.set_windres_path("x86_64-w64-mingw32-windres");
        }
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "SYNGUI Widget Gallery");
        res.set("FileDescription", "SYNGUI Widget Gallery — UI framework demo");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("CompanyName", "Alexeyev Vitaly");
        res.set("LegalCopyright", "Copyright \u{00A9} 2025-2026 Alexeyev Vitaly");
        res.compile().expect("Failed to compile Windows resources");
    }
}
