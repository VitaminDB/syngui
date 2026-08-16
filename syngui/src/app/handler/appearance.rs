//! Мост между системным оформлением и реактивным сигналом приложения.
//!
//! Слежение живёт в отдельном потоке ([`crate::appearance`]), поэтому его
//! колбэк возвращается в главный поток через `run_on_main_thread` — только там
//! допустимо трогать сигналы и перерисовку.

use super::AppHandler;
use crate::appearance::{read_system_appearance, watch_system_appearance, ColorScheme};
use crate::window::WindowState;

impl AppHandler {
    pub(super) fn start_appearance_watcher(&mut self) {
        let Some(signal) = self.config.system_appearance else {
            return;
        };
        if self.appearance_watcher.is_some() {
            return;
        }

        let mut initial = read_system_appearance();
        // На Windows/macOS схему знает только winit — портала там нет.
        if let Some(scheme) = self.window_color_scheme() {
            initial.color_scheme = scheme;
        }
        signal.set(initial);

        self.appearance_watcher = Some(watch_system_appearance(move |appearance| {
            crate::async_runtime::run_on_main_thread(move || {
                if signal.get_untracked() != appearance {
                    signal.set(appearance);
                }
            });
        }));
    }

    /// winit-тема окна. На Linux всегда `None` (X11 не отдаёт тему вовсе, а на
    /// Wayland значение отражает лишь CSD-настройку самого приложения) — там
    /// схему приносит портал.
    #[cfg(not(target_os = "linux"))]
    fn window_color_scheme(&self) -> Option<ColorScheme> {
        let theme = self.window.as_ref()?.winit_window().theme()?;
        Some(match theme {
            winit::window::Theme::Dark => ColorScheme::Dark,
            winit::window::Theme::Light => ColorScheme::Light,
        })
    }

    #[cfg(target_os = "linux")]
    fn window_color_scheme(&self) -> Option<ColorScheme> {
        None
    }

    /// Держит системный фон окна в согласии с сигналом приложения: настройка
    /// «размывать фон» меняется в рантайме, а окно переживает пересоздание.
    pub(super) fn start_backdrop_effect(&mut self) {
        let Some(signal) = self.config.backdrop else {
            return;
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        crate::signal::create_effect(move || {
            let config = signal.get();
            if !crate::window::set_backdrop(&window, config) && config.blur {
                log::debug!("[syngui] compositor backdrop unavailable");
            }
        });
    }

    /// Обновляет сигнал состояния окна и, если форма эффекта зависит от
    /// размера, переустанавливает его. Вызывается из `sync_window_flags`, то
    /// есть на каждый resize, maximize и смену фокуса.
    pub(in crate::app) fn sync_window_state(&mut self, state: WindowState) {
        if let Some(signal) = self.config.window_state {
            if signal.get_untracked() != state {
                signal.set(state);
            }
        }
        self.refresh_backdrop();
    }

    /// Регион эффекта задан в координатах поверхности, поэтому после resize его
    /// нужно пересчитать. Повторные применения с теми же параметрами
    /// отбрасываются — resize приходит пачками.
    pub(in crate::app) fn refresh_backdrop(&mut self) {
        let Some(signal) = self.config.backdrop else {
            return;
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let config = signal.get_untracked();
        let size = window.size();
        if self.last_backdrop == Some((config, size)) {
            return;
        }
        self.last_backdrop = Some((config, size));
        crate::window::set_backdrop(&window, config);
    }

    /// `WindowEvent::ThemeChanged` — приходит на Windows/macOS/web.
    pub(in crate::app) fn handle_theme_changed(&mut self, theme: winit::window::Theme) {
        let Some(signal) = self.config.system_appearance else {
            return;
        };
        let scheme = match theme {
            winit::window::Theme::Dark => ColorScheme::Dark,
            winit::window::Theme::Light => ColorScheme::Light,
        };
        let mut appearance = signal.get_untracked();
        if appearance.color_scheme == scheme {
            return;
        }
        appearance.color_scheme = scheme;
        signal.set(appearance);
    }
}
