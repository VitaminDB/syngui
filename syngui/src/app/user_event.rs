#[derive(Debug, Clone)]
pub enum MguiUserEvent {
    TrayShow,
    TrayHide,
    TrayToggle,
    TrayExit,
    MenuItem(String),
    Activate,
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
