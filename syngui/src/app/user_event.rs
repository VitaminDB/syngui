#[derive(Debug, Clone)]
pub enum SynGuiUserEvent {
    TrayShow,
    TrayHide,
    TrayToggle,
    TrayExit,
    MenuItem(String),
    Activate,
    /// Разбудить цикл и выполнить очередь `run_on_main_thread` — работает и
    /// когда окно не рендерится (свёрнутое/фоновое Android-приложение).
    MainThreadWake,
    #[cfg(feature = "wayland-dnd")]
    WaylandDnd(WaylandDndEvent),
}

#[cfg(feature = "wayland-dnd")]
#[derive(Debug, Clone)]
pub enum WaylandDndEvent {
    Enter { x: f32, y: f32 },
    Motion { x: f32, y: f32 },
    Leave,
    Drop {
        x: f32,
        y: f32,
        paths: Vec<std::path::PathBuf>,
    },
}
