#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCloseAction {
    HideToTray,
    Exit,
}

impl Default for TrayCloseAction {
    fn default() -> Self {
        Self::HideToTray
    }
}

#[derive(Debug, Clone)]
pub enum TrayMenuItem {
    Show(String),
    Hide(String),
    Exit(String),
    Separator,
    Custom { id: String, label: String },
}

#[derive(Debug, Clone)]
pub struct TrayConfig {
    pub(crate) icon_png: Option<Vec<u8>>,
    pub(crate) tooltip: Option<String>,
    pub(crate) items: Vec<TrayMenuItem>,
    pub(crate) close_action: TrayCloseAction,
    pub(crate) activate_on_left_click: bool,
}

impl TrayConfig {
    pub fn new() -> Self {
        Self {
            icon_png: None,
            tooltip: None,
            items: Vec::new(),
            close_action: TrayCloseAction::HideToTray,
            activate_on_left_click: true,
        }
    }

    pub fn icon_png(mut self, png: Vec<u8>) -> Self {
        self.icon_png = Some(png);
        self
    }

    pub fn tooltip(mut self, s: impl Into<String>) -> Self {
        self.tooltip = Some(s.into());
        self
    }

    pub fn menu_show(mut self, label: impl Into<String>) -> Self {
        self.items.push(TrayMenuItem::Show(label.into()));
        self
    }

    pub fn menu_hide(mut self, label: impl Into<String>) -> Self {
        self.items.push(TrayMenuItem::Hide(label.into()));
        self
    }

    pub fn menu_separator(mut self) -> Self {
        self.items.push(TrayMenuItem::Separator);
        self
    }

    pub fn menu_exit(mut self, label: impl Into<String>) -> Self {
        self.items.push(TrayMenuItem::Exit(label.into()));
        self
    }

    pub fn menu_custom(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.items.push(TrayMenuItem::Custom {
            id: id.into(),
            label: label.into(),
        });
        self
    }

    pub fn close_action(mut self, a: TrayCloseAction) -> Self {
        self.close_action = a;
        self
    }

    pub fn activate_on_left_click(mut self, on: bool) -> Self {
        self.activate_on_left_click = on;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn close_action_value(&self) -> TrayCloseAction {
        self.close_action
    }
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(
    feature = "tray",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub(crate) use platform::TrayManager;

#[cfg(all(
    feature = "tray",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
mod platform {
    use super::*;
    use crate::app::user_event::SynGuiUserEvent;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
    };
    use winit::event_loop::EventLoopProxy;

    const ID_SHOW: &str = "__syngui_tray_show";
    const ID_HIDE: &str = "__syngui_tray_hide";
    const ID_EXIT: &str = "__syngui_tray_exit";

    pub(crate) struct TrayManager {
        #[cfg(target_os = "linux")]
        thread: Option<std::thread::JoinHandle<()>>,

        #[cfg(not(target_os = "linux"))]
        _tray_icon: tray_icon::TrayIcon,
    }

    impl TrayManager {
        pub fn start(
            cfg: TrayConfig,
            proxy: EventLoopProxy<SynGuiUserEvent>,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            install_event_handlers(&cfg, proxy.clone());

            #[cfg(target_os = "linux")]
            {
                Self::start_linux(cfg)
            }

            #[cfg(not(target_os = "linux"))]
            {
                use std::panic::{catch_unwind, AssertUnwindSafe};
                let tray_icon = match catch_unwind(AssertUnwindSafe(|| build_tray_icon(&cfg))) {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => return Err(e),
                    Err(panic) => return Err(format!("tray init panicked: {}", panic_message(&panic)).into()),
                };
                Ok(Self {
                    _tray_icon: tray_icon,
                })
            }
        }

        #[cfg(target_os = "linux")]
        fn start_linux(cfg: TrayConfig) -> Result<Self, Box<dyn std::error::Error>> {
            use std::panic::{catch_unwind, AssertUnwindSafe};
            use std::sync::mpsc;

            let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

            let thread = std::thread::Builder::new()
                .name("syngui-tray".into())
                .spawn(move || {
                    let init = catch_unwind(AssertUnwindSafe(|| -> Result<_, String> {
                        gtk::init().map_err(|e| format!("gtk::init failed: {e}"))?;
                        build_tray_icon(&cfg).map_err(|e| format!("tray build failed: {e}"))
                    }));
                    let tray_icon = match init {
                        Ok(Ok(t)) => t,
                        Ok(Err(msg)) => {
                            let _ = init_tx.send(Err(msg));
                            return;
                        }
                        Err(panic) => {
                            let msg = panic_message(&panic);
                            let _ = init_tx.send(Err(format!("tray init panicked: {msg}")));
                            return;
                        }
                    };
                    let _ = init_tx.send(Ok(()));
                    gtk::main();
                    drop(tray_icon);
                })?;

            match init_rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(Ok(())) => Ok(Self {
                    thread: Some(thread),
                }),
                Ok(Err(msg)) => Err(msg.into()),
                Err(_) => Err("tray thread did not initialise within timeout".into()),
            }
        }
    }

    impl Drop for TrayManager {
        fn drop(&mut self) {
            MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
            TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);

            #[cfg(target_os = "linux")]
            {
                use gtk::glib;
                glib::idle_add(|| {
                    gtk::main_quit();
                    glib::ControlFlow::Break
                });
                if let Some(handle) = self.thread.take() {
                    let _ = handle.join();
                }
            }
        }
    }

    fn install_event_handlers(cfg: &TrayConfig, proxy: EventLoopProxy<SynGuiUserEvent>) {
        {
            let proxy = proxy.clone();
            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                let id = ev.id.0;
                let user_ev = match id.as_str() {
                    ID_SHOW => SynGuiUserEvent::TrayShow,
                    ID_HIDE => SynGuiUserEvent::TrayHide,
                    ID_EXIT => SynGuiUserEvent::TrayExit,
                    _ => SynGuiUserEvent::MenuItem(id),
                };
                let _ = proxy.send_event(user_ev);
            }));
        }
        if cfg.activate_on_left_click {
            let proxy = proxy.clone();
            TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    let _ = proxy.send_event(SynGuiUserEvent::TrayToggle);
                }
            }));
        } else {
            TrayIconEvent::set_event_handler(Some(|_: TrayIconEvent| {}));
        }
    }

    fn build_tray_icon(cfg: &TrayConfig) -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
        let icon = match cfg.icon_png.as_deref() {
            Some(bytes) => Some(decode_png_to_icon(bytes)?),
            None => None,
        };

        let menu = Menu::new();
        for item in &cfg.items {
            match item {
                TrayMenuItem::Show(label) => {
                    menu.append(&MenuItem::with_id(MenuId::new(ID_SHOW), label, true, None))?;
                }
                TrayMenuItem::Hide(label) => {
                    menu.append(&MenuItem::with_id(MenuId::new(ID_HIDE), label, true, None))?;
                }
                TrayMenuItem::Exit(label) => {
                    menu.append(&MenuItem::with_id(MenuId::new(ID_EXIT), label, true, None))?;
                }
                TrayMenuItem::Separator => {
                    menu.append(&PredefinedMenuItem::separator())?;
                }
                TrayMenuItem::Custom { id, label } => {
                    menu.append(&MenuItem::with_id(MenuId::new(id), label, true, None))?;
                }
            }
        }

        let mut builder = TrayIconBuilder::new();
        if let Some(icon) = icon {
            builder = builder.with_icon(icon);
        }
        if let Some(tooltip) = cfg.tooltip.as_deref() {
            builder = builder.with_tooltip(tooltip);
        }
        if !cfg.items.is_empty() {
            builder = builder.with_menu(Box::new(menu));
        }
        Ok(builder.build()?)
    }

    fn decode_png_to_icon(bytes: &[u8]) -> Result<Icon, Box<dyn std::error::Error>> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (w, h) = img.dimensions();
        let icon = Icon::from_rgba(img.into_raw(), w, h)?;
        Ok(icon)
    }

    fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
        if let Some(s) = panic.downcast_ref::<&'static str>() {
            (*s).to_string()
        } else if let Some(s) = panic.downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".to_string()
        }
    }
}
