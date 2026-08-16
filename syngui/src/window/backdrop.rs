//! Системный фон за прозрачным окном: размытие и «стекло» KWin.
//!
//! Композитор умеет размывать то, что находится **позади** окна, но делает это
//! только по явной просьбе приложения. На Wayland просьба выражается
//! протоколами `org_kde_kwin_blur` / `org_kde_kwin_contrast`, на X11 —
//! свойством `_KDE_NET_WM_BLUR_BEHIND_REGION`.
//!
//! Эффект виден лишь там, где окно полупрозрачно, поэтому осмыслен только в
//! паре с [`AppBuilder::transparent(true)`](crate::app::AppBuilder::transparent)
//! и полупрозрачным фоном в палитре темы.

use crate::window::Window;

/// Параметры «стекла» (KWin contrast): значения вокруг 1.0 — без изменений.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropContrast {
    pub contrast: f64,
    pub intensity: f64,
    pub saturation: f64,
}

impl Default for BackdropContrast {
    fn default() -> Self {
        // Значения, на которых KWin рисует Plasma-панели.
        Self { contrast: 1.0, intensity: 1.0, saturation: 1.15 }
    }
}

/// Форма области, к которой применяется эффект.
///
/// У окна с собственным титлбаром поверхность обычно больше видимой части:
/// вокруг «шелла» остаётся прозрачная рамка под тень и resize-захват. Размывать
/// её нельзя — иначе вокруг окна повисает мутный прямоугольник.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BackdropRegion {
    /// Вся поверхность целиком.
    #[default]
    Surface,
    /// Прямоугольник со скруглением внутри поверхности: отступ от её краёв и
    /// радиус углов, оба в логических пикселях.
    RoundedRect { inset: f32, radius: f32 },
}

/// Что просим у композитора.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BackdropConfig {
    pub blur: bool,
    /// `None` — контраст не трогаем.
    pub contrast: Option<BackdropContrast>,
    pub region: BackdropRegion,
}

impl BackdropConfig {
    pub fn blur() -> Self {
        Self { blur: true, contrast: None, region: BackdropRegion::Surface }
    }

    pub fn frosted() -> Self {
        Self {
            blur: true,
            contrast: Some(BackdropContrast::default()),
            region: BackdropRegion::Surface,
        }
    }

    pub fn disabled() -> Self {
        Self::default()
    }

    /// Ограничивает эффект формой «шелла» — прямоугольником со скруглением.
    pub fn with_shell(mut self, inset: f32, radius: f32) -> Self {
        self.region = BackdropRegion::RoundedRect { inset, radius };
        self
    }
}

/// Применяет (или снимает) системный фон для окна.
///
/// Возвращает `true`, если композитор просьбу принял. `false` — окружение
/// эффект не поддерживает: приложение остаётся просто прозрачным.
pub fn set_backdrop(window: &Window, config: BackdropConfig) -> bool {
    #[cfg(all(target_os = "linux", feature = "system-blur"))]
    {
        platform::set_backdrop(window, config)
    }
    #[cfg(not(all(target_os = "linux", feature = "system-blur")))]
    {
        let _ = (window, config);
        false
    }
}

#[cfg(all(target_os = "linux", feature = "system-blur"))]
mod platform {
    use super::{BackdropConfig, Window};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub(super) fn set_backdrop(window: &Window, config: BackdropConfig) -> bool {
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        match handle.as_raw() {
            RawWindowHandle::Wayland(_) => wayland::set_backdrop(window, config),
            RawWindowHandle::Xlib(h) => x11::set_blur(h.window as u32, config.blur),
            RawWindowHandle::Xcb(h) => x11::set_blur(h.window.get(), config.blur),
            _ => false,
        }
    }

    /// Wayland: объекты эффекта должны жить, пока эффект нужен, — держим их
    /// вместе с соединением в одном месте.
    ///
    /// Поддерживаются два протокола: стандартный `ext-background-effect-v1`
    /// (KWin 6.7+) и старый `org_kde_kwin_blur`/`contrast` — какой из них
    /// рекламирует композитор, тот и используется.
    mod wayland {
        use std::sync::{Mutex, OnceLock};

        use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
        use wayland_backend::sys::client::{Backend, ObjectId};
        use wayland_client::globals::{registry_queue_init, GlobalListContents};
        use wayland_client::protocol::wl_compositor::WlCompositor;
        use wayland_client::protocol::wl_region::WlRegion;
        use wayland_client::protocol::wl_registry::WlRegistry;
        use wayland_client::protocol::wl_surface::WlSurface;
        use wayland_client::{delegate_noop, Connection, EventQueue, Proxy, QueueHandle};
        use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
        use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;
        use wayland_protocols_plasma::contrast::client::org_kde_kwin_contrast::OrgKdeKwinContrast;
        use wayland_protocols_plasma::contrast::client::org_kde_kwin_contrast_manager::OrgKdeKwinContrastManager;

        use crate::window::ext_background_effect::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
        use crate::window::ext_background_effect::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

        use super::super::{BackdropConfig, Window};

        struct State;

        impl wayland_client::Dispatch<WlRegistry, GlobalListContents> for State {
            fn event(
                _: &mut Self,
                _: &WlRegistry,
                _: <WlRegistry as Proxy>::Event,
                _: &GlobalListContents,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }

        delegate_noop!(State: ignore OrgKdeKwinBlurManager);
        delegate_noop!(State: ignore OrgKdeKwinBlur);
        delegate_noop!(State: ignore OrgKdeKwinContrastManager);
        delegate_noop!(State: ignore OrgKdeKwinContrast);
        delegate_noop!(State: ignore ExtBackgroundEffectManagerV1);
        delegate_noop!(State: ignore ExtBackgroundEffectSurfaceV1);
        delegate_noop!(State: ignore WlCompositor);
        delegate_noop!(State: ignore WlRegion);

        /// Запас для варианта «вся поверхность»: композитор обрезает регион по
        /// размеру surface, поэтому точное значение не нужно.
        const WHOLE_SURFACE: i32 = 1 << 20;

        struct Applied {
            connection: Connection,
            /// Очередь держим живой — на неё ссылается `queue_handle`.
            _queue: EventQueue<State>,
            queue_handle: QueueHandle<State>,
            surface: WlSurface,
            compositor: Option<WlCompositor>,
            ext_manager: Option<ExtBackgroundEffectManagerV1>,
            ext_effect: Option<ExtBackgroundEffectSurfaceV1>,
            kwin_blur_manager: Option<OrgKdeKwinBlurManager>,
            kwin_blur: Option<OrgKdeKwinBlur>,
            kwin_contrast_manager: Option<OrgKdeKwinContrastManager>,
            kwin_contrast: Option<OrgKdeKwinContrast>,
        }

        // SAFETY: все поля — wayland-объекты, у которых внутренняя
        // синхронизация обеспечивается libwayland; сюда попадают только из
        // главного потока под мьютексом.
        unsafe impl Send for Applied {}

        static CURRENT: OnceLock<Mutex<Option<Applied>>> = OnceLock::new();

        pub(super) fn set_backdrop(window: &Window, config: BackdropConfig) -> bool {
            let slot = CURRENT.get_or_init(|| Mutex::new(None));
            let Ok(mut guard) = slot.lock() else {
                return false;
            };

            if guard.is_none() {
                match connect(window) {
                    Some(applied) => *guard = Some(applied),
                    None => return false,
                }
            }
            let Some(applied) = guard.as_mut() else {
                return false;
            };

            // Логический размер: регионы Wayland живут в surface-local
            // координатах, а `Window::size()` отдаёт физические пиксели.
            let scale = window.scale_factor().max(0.1);
            let (width_px, height_px) = window.size();
            let size = (width_px as f64 / scale, height_px as f64 / scale);

            let ok = if applied.ext_manager.is_some() {
                apply_ext(applied, config, size)
            } else {
                apply_kwin(applied, config, size)
            };

            let _ = applied.connection.flush();
            ok
        }

        /// `ext-background-effect-v1`: пустой регион = эффекта нет, поэтому
        /// включение — это установка региона во всю поверхность.
        fn apply_ext(applied: &mut Applied, config: BackdropConfig, size: (f64, f64)) -> bool {
            if applied.ext_effect.is_none() {
                let Some(manager) = applied.ext_manager.as_ref() else {
                    return false;
                };
                applied.ext_effect = Some(manager.get_background_effect(
                    &applied.surface,
                    &applied.queue_handle,
                    (),
                ));
            }
            let Some(effect) = applied.ext_effect.as_ref() else {
                return false;
            };

            if !config.blur {
                effect.set_blur_region(None);
                return true;
            }

            let Some(compositor) = applied.compositor.as_ref() else {
                return false;
            };
            let region = compositor.create_region(&applied.queue_handle, ());
            fill_region(&region, config.region, size);
            effect.set_blur_region(Some(&region));
            // set_blur_region копирует регион — объект больше не нужен.
            region.destroy();
            true
        }

        /// Старый путь KWin: отдельные объекты blur и contrast, null-регион у
        /// них означает «вся поверхность».
        fn apply_kwin(applied: &mut Applied, config: BackdropConfig, size: (f64, f64)) -> bool {
            let mut ok = false;
            // У протокола KWin null-регион означает «вся поверхность», поэтому
            // явный объект нужен только для формы шелла.
            let shaped = |applied: &Applied| -> Option<WlRegion> {
                if matches!(config.region, super::super::BackdropRegion::Surface) {
                    return None;
                }
                let compositor = applied.compositor.as_ref()?;
                let region = compositor.create_region(&applied.queue_handle, ());
                fill_region(&region, config.region, size);
                Some(region)
            };

            match (config.blur, applied.kwin_blur.take()) {
                (true, existing) => {
                    let blur = existing.or_else(|| {
                        applied.kwin_blur_manager.as_ref().map(|m| {
                            m.create(&applied.surface, &applied.queue_handle, ())
                        })
                    });
                    if let Some(blur) = blur {
                        let region = shaped(applied);
                        blur.set_region(region.as_ref());
                        blur.commit();
                        if let Some(region) = region {
                            region.destroy();
                        }
                        applied.kwin_blur = Some(blur);
                        ok = true;
                    }
                }
                (false, Some(blur)) => {
                    blur.release();
                    if let Some(m) = applied.kwin_blur_manager.as_ref() {
                        m.unset(&applied.surface);
                    }
                }
                (false, None) => {}
            }

            match (config.contrast, applied.kwin_contrast.take()) {
                (Some(params), existing) => {
                    let contrast = existing.or_else(|| {
                        applied.kwin_contrast_manager.as_ref().map(|m| {
                            m.create(&applied.surface, &applied.queue_handle, ())
                        })
                    });
                    if let Some(contrast) = contrast {
                        let region = shaped(applied);
                        contrast.set_region(region.as_ref());
                        contrast.set_contrast(params.contrast);
                        contrast.set_intensity(params.intensity);
                        contrast.set_saturation(params.saturation);
                        contrast.commit();
                        if let Some(region) = region {
                            region.destroy();
                        }
                        applied.kwin_contrast = Some(contrast);
                        ok = true;
                    }
                }
                (None, Some(contrast)) => {
                    contrast.release();
                    if let Some(m) = applied.kwin_contrast_manager.as_ref() {
                        m.unset(&applied.surface);
                    }
                }
                (None, None) => {}
            }

            ok
        }

        /// Заполняет `wl_region` формой из конфига. Скругление собирается
        /// горизонтальными полосами — прямоугольники это всё, что умеет
        /// `wl_region`, а строка высотой в пиксель даёт край не хуже
        /// антиалиасинга композитора.
        fn fill_region(
            region: &WlRegion,
            shape: super::super::BackdropRegion,
            size: (f64, f64),
        ) {
            use super::super::BackdropRegion;

            let (inset, radius) = match shape {
                BackdropRegion::Surface => {
                    region.add(0, 0, WHOLE_SURFACE, WHOLE_SURFACE);
                    return;
                }
                BackdropRegion::RoundedRect { inset, radius } => (inset.max(0.0), radius.max(0.0)),
            };

            let width = size.0 - 2.0 * inset as f64;
            let height = size.1 - 2.0 * inset as f64;
            if width <= 1.0 || height <= 1.0 {
                // Окно ещё не получило размер — размываем всё, иначе эффекта не
                // будет вовсе.
                region.add(0, 0, WHOLE_SURFACE, WHOLE_SURFACE);
                return;
            }

            let x = inset as f64;
            let y = inset as f64;
            let r = (radius as f64).min(width / 2.0).min(height / 2.0);
            if r < 1.0 {
                region.add(x as i32, y as i32, width as i32, height as i32);
                return;
            }

            region.add(x as i32, (y + r) as i32, width as i32, (height - 2.0 * r) as i32);
            let steps = r.ceil() as i32;
            for i in 0..steps {
                let dy = i as f64 + 0.5;
                let dx = r - (r * r - (r - dy) * (r - dy)).max(0.0).sqrt();
                let seg_x = (x + dx).round() as i32;
                let seg_w = (width - 2.0 * dx).round() as i32;
                if seg_w <= 0 {
                    continue;
                }
                region.add(seg_x, (y + i as f64) as i32, seg_w, 1);
                region.add(seg_x, (y + height - 1.0 - i as f64) as i32, seg_w, 1);
            }
        }

        fn connect(window: &Window) -> Option<Applied> {
            let display = match window.display_handle().ok()?.as_raw() {
                RawDisplayHandle::Wayland(d) => d.display,
                _ => return None,
            };
            let surface_ptr = match window.window_handle().ok()?.as_raw() {
                RawWindowHandle::Wayland(w) => w.surface,
                _ => return None,
            };

            // SAFETY: указатели получены из живого winit-окна; Backend не
            // становится владельцем display'а, а лишь оборачивает его — тот же
            // приём, что и в `app::wayland_dnd`.
            let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
            let connection = Connection::from_backend(backend);

            let (globals, queue) = registry_queue_init::<State>(&connection).ok()?;
            let queue_handle = queue.handle();

            let ext_manager = globals
                .bind::<ExtBackgroundEffectManagerV1, _, _>(&queue_handle, 1..=1, ())
                .ok();
            let compositor = globals.bind::<WlCompositor, _, _>(&queue_handle, 1..=6, ()).ok();
            let kwin_blur_manager = globals
                .bind::<OrgKdeKwinBlurManager, _, _>(&queue_handle, 1..=1, ())
                .ok();
            let kwin_contrast_manager = globals
                .bind::<OrgKdeKwinContrastManager, _, _>(&queue_handle, 1..=2, ())
                .ok();

            if ext_manager.is_none() && kwin_blur_manager.is_none() {
                log::debug!("[syngui] compositor advertises no background-effect protocol");
                return None;
            }
            if ext_manager.is_some() && compositor.is_none() {
                log::debug!("[syngui] wl_compositor unavailable, cannot build blur region");
                return None;
            }

            // SAFETY: wl_surface принадлежит тому же wl_display, поверх которого
            // построен backend, поэтому id валиден в этом соединении.
            let id = unsafe {
                ObjectId::from_ptr(WlSurface::interface(), surface_ptr.as_ptr().cast())
            }
            .ok()?;
            let surface = WlSurface::from_id(&connection, id).ok()?;

            Some(Applied {
                connection,
                _queue: queue,
                queue_handle,
                surface,
                compositor,
                ext_manager,
                ext_effect: None,
                kwin_blur_manager,
                kwin_blur: None,
                kwin_contrast_manager,
                kwin_contrast: None,
            })
        }
    }

    /// X11: KWin читает регион размытия из свойства окна. Пустой регион —
    /// «размывать всё окно».
    mod x11 {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
        use x11rb::wrapper::ConnectionExt as _;

        pub(super) fn set_blur(window: u32, enabled: bool) -> bool {
            let Ok((conn, _)) = x11rb::connect(None) else {
                return false;
            };
            let Ok(cookie) = conn.intern_atom(false, b"_KDE_NET_WM_BLUR_BEHIND_REGION") else {
                return false;
            };
            let Ok(atom) = cookie.reply().map(|r| r.atom) else {
                return false;
            };
            let result = if enabled {
                conn.change_property32(PropMode::REPLACE, window, atom, AtomEnum::CARDINAL, &[])
                    .map(|_| ())
            } else {
                conn.delete_property(window, atom).map(|_| ())
            };
            if result.is_err() {
                return false;
            }
            conn.flush().is_ok()
        }
    }
}
