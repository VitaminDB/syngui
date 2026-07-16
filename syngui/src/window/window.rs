use raw_window_handle::{HasWindowHandle, HasDisplayHandle};

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

#[derive(Clone, Debug)]
pub enum WindowEvent {
    Resized { width: u32, height: u32 },
    CloseRequested,
    FocusGained,
    FocusLost,
    ScaleFactorChanged { scale_factor: f64 },
}

pub struct WindowBuilder {
    title: String,
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
    resizable: bool,
    maximized: bool,
    decorations: bool,
    transparent: bool,
    fullscreen: bool,
}

impl WindowBuilder {
    pub fn new() -> Self {
        Self {
            title: "SYNGUI".to_string(),
            width: 1280,
            height: 720,
            min_width: 400,
            min_height: 300,
            resizable: true,
            maximized: false,
            decorations: true,
            transparent: false,
            fullscreen: false,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn with_min_size(mut self, width: u32, height: u32) -> Self {
        self.min_width = width;
        self.min_height = height;
        self
    }

    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn with_fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Default for WindowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Window {
    inner: winit::window::Window,
}

impl Window {
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        builder: WindowBuilder,
    ) -> Self {
        let attributes = winit::window::Window::default_attributes()
            .with_title(builder.title);

        #[cfg(not(target_os = "android"))]
        let attributes = attributes
            .with_inner_size(winit::dpi::LogicalSize::new(builder.width, builder.height))
            .with_min_inner_size(winit::dpi::LogicalSize::new(builder.min_width, builder.min_height))
            .with_resizable(builder.resizable)
            .with_maximized(builder.maximized)
            .with_decorations(builder.decorations)
            .with_transparent(builder.transparent);

        let inner = event_loop.create_window(attributes).expect("Failed to create window");

        inner.set_ime_allowed(true);

        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        if builder.fullscreen {
            inner.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        #[cfg(target_arch = "wasm32")]
        {
            let canvas = inner.canvas().expect("Failed to get canvas");
            canvas.set_width(builder.width);
            canvas.set_height(builder.height);
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .map(|body| body.append_child(&canvas).unwrap());
        }

        Self { inner }
    }

    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    pub fn size(&self) -> (u32, u32) {
        let size = self.inner.inner_size();
        (size.width, size.height)
    }

    pub fn scale_factor(&self) -> f64 {
        self.inner.scale_factor()
    }

    pub fn set_cursor_icon(&self, icon: winit::window::CursorIcon) {
        self.inner.set_cursor(winit::window::Cursor::Icon(icon));
    }

    pub fn winit_window(&self) -> &winit::window::Window {
        &self.inner
    }

    pub fn set_visible(&self, visible: bool) {
        self.inner.set_visible(visible);
    }

    pub fn is_visible(&self) -> Option<bool> {
        self.inner.is_visible()
    }

    pub fn focus(&self) {
        self.inner.focus_window();
    }

    pub fn set_window_icon(&self, icon: Option<winit::window::Icon>) {
        self.inner.set_window_icon(icon);
    }

    pub fn set_window_icon_from_png(
        &self,
        #[allow(unused_variables)] bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(feature = "image-support")]
        {
            let img = image::load_from_memory(bytes)?.to_rgba8();
            let (w, h) = img.dimensions();
            let icon = winit::window::Icon::from_rgba(img.into_raw(), w, h)?;
            self.inner.set_window_icon(Some(icon));
            Ok(())
        }
        #[cfg(not(feature = "image-support"))]
        {
            Err("image-support feature is required for set_window_icon_from_png".into())
        }
    }
}

impl crate::signal::RedrawNotifier for Window {
    fn request_redraw(&self) {
        self.inner.request_redraw();
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.inner.window_handle()
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        self.inner.display_handle()
    }
}
