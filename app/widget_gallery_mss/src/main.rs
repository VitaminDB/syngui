use widget_gallery_mss::run_app;

fn main() {
    // RUST_LOG override'ит уровень; дефолт INFO, чтобы видеть hwaccel /
    // ffmpeg-логи из syngui без дополнительной настройки.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    run_app();
}
