use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use crate::widget::{Element, ElementId, ElementTree, Widget};

mod clipboard_filter;
mod config;
mod element;
mod grid;
mod input;
mod mouse;
mod palette;
mod parser;
mod pty;
mod selection;
mod session;

pub use config::TerminalConfig;
pub use element::TerminalElement;
pub use mouse::{MouseEncoding, MouseMode};
pub use selection::SelectionMode;
pub use session::TerminalSession;

/// Перехват клавиш приложением: `true` — клавиша его (горячая клавиша окна),
/// в PTY она не уходит. Видит `KeyDown`; символ, который клавиша набирает,
/// приходит отдельным `CharInput` раньше неё, поэтому горячие клавиши — с Ctrl
/// (при Ctrl окно символов не шлёт).
pub type TerminalKeyHook =
    Arc<dyn Fn(crate::input::Key, crate::input::Modifiers) -> bool + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCommand {
    Copy,
    Paste,
    Clear,
    /// Выделить весь текст: прокрутку и экран.
    SelectAll,
}

pub struct Terminal {
    pub(crate) config: TerminalConfig,
    pub(crate) classes: Vec<String>,
    pub(crate) session: Option<TerminalSession>,
    pub(crate) command_signal: Option<crate::signal::RwSignal<Option<TerminalCommand>>>,
    pub(crate) autofocus: bool,
    pub(crate) key_hook: Option<TerminalKeyHook>,
}

impl Terminal {
    pub fn new() -> Self {
        Self {
            config: TerminalConfig::default(),
            classes: Vec::new(),
            session: None,
            command_signal: None,
            autofocus: false,
            key_hook: None,
        }
    }

    pub fn command_signal(
        mut self,
        signal: crate::signal::RwSignal<Option<TerminalCommand>>,
    ) -> Self {
        self.command_signal = Some(signal);
        self
    }

    pub fn autofocus(mut self, on: bool) -> Self {
        self.autofocus = on;
        self
    }

    pub fn attach(mut self, session: TerminalSession) -> Self {
        self.session = Some(session);
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, class.into());
        self
    }

    pub fn command(mut self, cmd: impl Into<String>) -> Self {
        self.config.command = cmd.into();
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.config.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.config.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.env.push((key.into(), value.into()));
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size.max(6.0);
        self.config.font_size_explicit = true;
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.config.font_family = family.into();
        self.config.font_family_explicit = true;
        self
    }

    pub fn line_height(mut self, ratio: f32) -> Self {
        self.config.line_height = ratio.max(1.0);
        self
    }

    /// Свои 16 цветов ANSI (0–7 обычные, 8–15 яркие) вместо Tango. Цвета
    /// текста, фона и курсора по умолчанию — из MSS (`color`,
    /// `background-color`, `accent-color`).
    pub fn palette(mut self, colors: [crate::core::Color; 16]) -> Self {
        self.config.palette = Some(colors);
        self
    }

    /// Сколько строк истории держать (по умолчанию 5000).
    pub fn scrollback(mut self, lines: usize) -> Self {
        self.config.scrollback = lines;
        self
    }

    /// Горячие клавиши приложения поверх терминала: функция видит каждое
    /// нажатие раньше PTY и, вернув `true`, забирает его (см. [`TerminalKeyHook`]).
    pub fn on_key(
        mut self,
        f: impl Fn(crate::input::Key, crate::input::Modifiers) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.key_hook = Some(Arc::new(f));
        self
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Terminal {
    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = TerminalElement::new(self.config.clone(), self.session.clone());
        if !self.classes.is_empty() {
            elem.set_classes(self.classes.clone());
        }
        elem.command_signal = self.command_signal;
        elem.autofocus = self.autofocus;
        elem.key_hook = self.key_hook.clone();
        if self.autofocus {
            let take = self
                .session
                .as_ref()
                .map_or(true, |s| s.try_consume_autofocus());
            if take {
                elem.focus_request_pending = true;
            } else if let Some(s) = self.session.as_ref() {
                s.with_state(|st| st.focused = false);
            }
        }
        Box::new(elem)
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}
