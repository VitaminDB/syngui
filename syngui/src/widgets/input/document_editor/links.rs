//! Точки инъекции хоста: ссылки и медиа.
//!
//! Редактор не знает про vault, CAS-блобы и плееры приложения — он
//! описывает интерфейсы, а хост подставляет реализации:
//! - [`DocLinkProvider`] — кандидаты автокомплита `[[`, существование
//!   страниц (окраска битых ссылок), открытие ссылок по Ctrl+клику;
//! - [`DocMediaResolver`] — превращение url медиа-блока (`blob:<sha>`,
//!   относительный путь) в локальный файл, постер видео и PCM-бины
//!   волновой формы аудио.
//!
//! Дефолтные реализации — заглушки: виджет работает и без хоста.

use std::path::PathBuf;

use super::model::{LinkTarget, MediaKind};

/// Кандидат автокомплита wiki-ссылки.
#[derive(Clone, Debug)]
pub struct LinkCandidate {
    /// Цель ссылки (`[[target]]`).
    pub target: String,
    /// Подпись в списке (обычно совпадает с target).
    pub label: String,
}

pub trait DocLinkProvider: Send + Sync {
    /// Кандидаты для `[[prefix` (пустой префикс — «все страницы»).
    fn complete(&self, prefix: &str) -> Vec<LinkCandidate> {
        let _ = prefix;
        Vec::new()
    }

    /// Существует ли цель wiki-ссылки (битые подсвечиваются другим цветом).
    fn link_exists(&self, target: &str) -> bool {
        let _ = target;
        true
    }

    /// Открытие ссылки (Ctrl+клик): wiki — страница хоста, url — браузер.
    fn open_link(&self, target: &LinkTarget) {
        let _ = target;
    }
}

/// Разрешённый медиа-ресурс.
#[derive(Clone, Debug)]
pub struct ResolvedMedia {
    /// Локальный файл для показа/воспроизведения.
    pub path: PathBuf,
    pub kind: MediaKind,
}

pub trait DocMediaResolver: Send + Sync {
    /// Локальный файл по url блока (`blob:<sha>.<ext>`, относительный путь).
    fn resolve(&self, url: &str) -> Option<ResolvedMedia> {
        let _ = url;
        None
    }

    /// Постер видео (кадр-превью), если есть.
    fn poster(&self, url: &str) -> Option<PathBuf> {
        let _ = url;
        None
    }

    /// PCM-бины волновой формы аудио (амплитуды 0..1, `bins` штук).
    fn pcm_bins(&self, url: &str, bins: usize) -> Option<Vec<f32>> {
        let _ = (url, bins);
        None
    }
}

/// Контекст построения врезки `![[…]]` — защита от рекурсии.
#[derive(Clone, Debug, Default)]
pub struct EmbedCtx {
    /// Глубина вложенности (0 — верхний документ).
    pub depth: usize,
    /// Цепочка целей от корня — для детекта циклов.
    pub chain: Vec<String>,
}

/// Фабрика живых врезок: хост отдаёт виджет содержимого цели
/// (read-only страница, редактируемая база, мини-канвас). `None` —
/// рисуется карточка-плейсхолдер.
pub trait EmbedFactory: Send + Sync {
    fn build(&self, target: &str, ctx: &EmbedCtx) -> Option<Box<dyn crate::widget::Widget>>;
}

/// Заглушки по умолчанию.
pub struct NoLinks;
impl DocLinkProvider for NoLinks {}

pub struct NoMedia;
impl DocMediaResolver for NoMedia {}
