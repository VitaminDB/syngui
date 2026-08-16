//! Оформление рамки окна, как его задаёт рабочий стол: набор и порядок кнопок,
//! их размеры и — если тема это позволяет — их точный внешний вид.
//!
//! Приложению с собственным титлбаром (CSD) этого хватает, чтобы выглядеть
//! системно: раскладка берётся из настроек DE, а на KDE с темой Aurorae кнопки
//! рисуются прямо из SVG темы, один в один с декорациями остальных окон.
//!
//! Источники:
//! * KDE — `kwinrc`: `[org.kde.kdecoration3]` (Plasma 6) или `[org.kde.kdecoration2]`,
//!   ключи `ButtonsOnLeft`/`ButtonsOnRight`, `library`, `theme`; метрики и
//!   резервная раскладка — из `<тема>rc` самой Aurorae-темы.
//! * GNOME — `gsettings org.gnome.desktop.wm.preferences button-layout`.

use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use super::desktop::{config_dir, Ini};

/// Кнопка титлбара. Набор совпадает с KDecoration — лишние для приложения
/// варианты (тени, вкладки) в раскладку не попадают.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowButton {
    Menu,
    ApplicationMenu,
    OnAllDesktops,
    Minimize,
    Maximize,
    Close,
    ContextHelp,
    KeepAbove,
    KeepBelow,
    Shade,
    Spacer,
}

impl WindowButton {
    /// Буква из `ButtonsOnLeft`/`ButtonsOnRight` KWin.
    fn from_kde_char(c: char) -> Option<Self> {
        Some(match c {
            'M' => Self::Menu,
            'N' => Self::ApplicationMenu,
            'S' => Self::OnAllDesktops,
            'I' => Self::Minimize,
            'A' => Self::Maximize,
            'X' => Self::Close,
            'H' => Self::ContextHelp,
            'F' => Self::KeepAbove,
            'B' => Self::KeepBelow,
            'L' => Self::Shade,
            '_' => Self::Spacer,
            _ => return None,
        })
    }

    /// Имя кнопки в `button-layout` GNOME.
    fn from_gnome_name(name: &str) -> Option<Self> {
        Some(match name.trim() {
            "menu" => Self::Menu,
            "appmenu" => Self::ApplicationMenu,
            "minimize" => Self::Minimize,
            "maximize" => Self::Maximize,
            "close" => Self::Close,
            "spacer" => Self::Spacer,
            _ => return None,
        })
    }

    /// Базовое имя SVG в теме Aurorae (без расширения).
    fn aurorae_stem(self) -> Option<&'static str> {
        Some(match self {
            Self::Menu => "menu",
            Self::ApplicationMenu => "appmenu",
            Self::OnAllDesktops => "alldesktops",
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Close => "close",
            Self::ContextHelp => "help",
            Self::KeepAbove => "keepabove",
            Self::KeepBelow => "keepbelow",
            Self::Shade => "shade",
            Self::Spacer => return None,
        })
    }
}

/// Состояние кнопки — в Aurorae каждое лежит отдельным элементом SVG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// Обычное состояние в активном окне.
    Normal,
    Hover,
    Pressed,
    /// Окно неактивно.
    Inactive,
    /// Действие недоступно (например «развернуть» у нерастягиваемого окна).
    Disabled,
}

impl ButtonState {
    /// id элемента внутри SVG темы Aurorae.
    fn aurorae_element(self) -> &'static str {
        match self {
            Self::Normal => "active-center",
            Self::Hover => "hover-center",
            Self::Pressed => "pressed-center",
            Self::Inactive => "inactive-center",
            Self::Disabled => "deactivated-center",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Порядок кнопок по сторонам титлбара.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecorationLayout {
    pub left: Vec<WindowButton>,
    pub right: Vec<WindowButton>,
}

impl DecorationLayout {
    /// Раскладка по умолчанию, если система молчит: кнопки справа, как в Breeze.
    pub fn fallback() -> Self {
        Self {
            left: Vec::new(),
            right: vec![WindowButton::Minimize, WindowButton::Maximize, WindowButton::Close],
        }
    }

    fn parse_kde(spec: &str) -> Vec<WindowButton> {
        spec.chars().filter_map(WindowButton::from_kde_char).collect()
    }
}

/// Метрики титлбара в логических пикселях.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecorationMetrics {
    pub button_size: f32,
    pub button_spacing: f32,
    /// Отступы от края окна до блока кнопок.
    pub edge_left: f32,
    pub edge_right: f32,
    pub title_alignment: TitleAlignment,
}

impl Default for DecorationMetrics {
    fn default() -> Self {
        Self {
            button_size: 18.0,
            button_spacing: 8.0,
            edge_left: 12.0,
            edge_right: 12.0,
            title_alignment: TitleAlignment::Left,
        }
    }
}

/// Откуда берётся внешний вид кнопок.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecorationStyle {
    /// KDE Aurorae: SVG-тема на диске, кнопки можно нарисовать точно как в системе.
    Aurorae(AuroraeTheme),
    /// Декорации рисуются кодом самого DE (Breeze, Adwaita, Windows, macOS) —
    /// приложение отрисует кнопки своим встроенным стилем.
    Native,
}

/// Каталог темы Aurorae со SVG-кнопками.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuroraeTheme {
    pub name: String,
    pub dir: PathBuf,
}

impl AuroraeTheme {
    /// Путь к SVG кнопки. `None` — тема этой кнопки не содержит.
    pub fn button_svg(&self, button: WindowButton) -> Option<PathBuf> {
        let stem = button.aurorae_stem()?;
        let path = self.dir.join(format!("{stem}.svg"));
        path.is_file().then_some(path)
    }

    /// Отдельный файл для «восстановить» — Aurorae хранит его как `restore.svg`,
    /// а при отсутствии переиспользует `maximize.svg`.
    pub fn restore_svg(&self) -> Option<PathBuf> {
        let path = self.dir.join("restore.svg");
        if path.is_file() {
            Some(path)
        } else {
            self.button_svg(WindowButton::Maximize)
        }
    }
}

/// Всё, что приложение знает о системной рамке.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemDecorations {
    pub layout: DecorationLayout,
    pub metrics: DecorationMetrics,
    pub style: DecorationStyle,
}

impl Default for SystemDecorations {
    fn default() -> Self {
        Self {
            layout: DecorationLayout::fallback(),
            metrics: DecorationMetrics::default(),
            style: DecorationStyle::Native,
        }
    }
}

/// Читает оформление рамки у рабочего стола. На платформах и в окружениях, где
/// такой настройки нет, возвращает [`SystemDecorations::default`].
pub fn read_system_decorations() -> SystemDecorations {
    read_platform().unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn read_platform() -> Option<SystemDecorations> {
    kde().or_else(gnome)
}

#[cfg(not(target_os = "linux"))]
fn read_platform() -> Option<SystemDecorations> {
    None
}

// ─── KDE ────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn kde() -> Option<SystemDecorations> {
    let text = std::fs::read_to_string(config_dir()?.join("kwinrc")).ok()?;
    let kwinrc = Ini::parse(&text);

    // Plasma 6 переехал на kdecoration3, но при обновлении часто остаётся
    // старая секция — читаем ту, что заполнена.
    let section = ["org.kde.kdecoration3", "org.kde.kdecoration2"]
        .into_iter()
        .find(|s| kwinrc.get(s, "library").is_some() || kwinrc.get(s, "ButtonsOnRight").is_some())?;

    let theme = aurorae_theme(kwinrc.get(section, "library"), kwinrc.get(section, "theme"));
    let theme_rc = theme.as_ref().and_then(read_theme_rc);
    let theme_ini = theme_rc.as_deref().map(Ini::parse);

    // kwinrc главнее темы; пустая строка — «кнопок нет», а не «читай тему»,
    // поэтому смотрим именно на наличие ключа.
    let left = kwinrc
        .get(section, "ButtonsOnLeft")
        .or_else(|| theme_ini.as_ref().and_then(|i| i.get("General", "LeftButtons")))
        .map(DecorationLayout::parse_kde)
        .unwrap_or_default();
    let right = kwinrc
        .get(section, "ButtonsOnRight")
        .or_else(|| theme_ini.as_ref().and_then(|i| i.get("General", "RightButtons")))
        .map(DecorationLayout::parse_kde)
        .unwrap_or_else(|| DecorationLayout::fallback().right);

    let mut metrics = DecorationMetrics::default();
    if let Some(ini) = theme_ini.as_ref() {
        if let Some(v) = ini.get("Layout", "ButtonWidth").and_then(parse_f32) {
            metrics.button_size = v;
        }
        if let Some(v) = ini.get("Layout", "ButtonHeight").and_then(parse_f32) {
            metrics.button_size = metrics.button_size.max(v);
        }
        if let Some(v) = ini.get("Layout", "ButtonSpacing").and_then(parse_f32) {
            metrics.button_spacing = v;
        }
        if let Some(v) = ini.get("Layout", "TitleEdgeLeft").and_then(parse_f32) {
            metrics.edge_left = v;
        }
        if let Some(v) = ini.get("Layout", "TitleEdgeRight").and_then(parse_f32) {
            metrics.edge_right = v;
        }
        metrics.title_alignment = match ini.get("General", "TitleAlignment") {
            Some(v) if v.eq_ignore_ascii_case("Center") => TitleAlignment::Center,
            Some(v) if v.eq_ignore_ascii_case("Right") => TitleAlignment::Right,
            _ => TitleAlignment::Left,
        };
    }

    Some(SystemDecorations {
        layout: DecorationLayout { left, right },
        metrics,
        style: theme.map_or(DecorationStyle::Native, DecorationStyle::Aurorae),
    })
}

/// `library=org.kde.kwin.aurorae*` + `theme=__aurorae__svg__<имя>` → каталог темы.
#[cfg(target_os = "linux")]
fn aurorae_theme(library: Option<&str>, theme: Option<&str>) -> Option<AuroraeTheme> {
    if !library?.contains("aurorae") {
        return None;
    }
    let name = theme?.rsplit("__").next()?.to_string();
    if name.is_empty() {
        return None;
    }
    let dir = data_dirs()
        .into_iter()
        .map(|d| d.join("aurorae/themes").join(&name))
        .find(|d| d.is_dir())?;
    Some(AuroraeTheme { name, dir })
}

/// `<каталог темы>/<имя темы>rc` — метрики и цвета заголовка.
#[cfg(target_os = "linux")]
fn read_theme_rc(theme: &AuroraeTheme) -> Option<String> {
    let named = theme.dir.join(format!("{}rc", theme.name));
    if let Ok(text) = std::fs::read_to_string(&named) {
        return Some(text);
    }
    // Часть тем называет файл по каталогу иначе — берём первый *rc.
    let entry = std::fs::read_dir(&theme.dir).ok()?.flatten().find(|e| {
        e.path()
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("rc"))
    })?;
    std::fs::read_to_string(entry.path()).ok()
}

#[cfg(target_os = "linux")]
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(home));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }
    let extra = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(extra.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs
}

#[cfg(target_os = "linux")]
fn parse_f32(v: &str) -> Option<f32> {
    v.trim().parse::<f32>().ok().filter(|v| *v > 0.0)
}

// ─── GNOME ──────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn gnome() -> Option<SystemDecorations> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let spec = raw.trim().trim_matches('\'');
    let (left, right) = spec.split_once(':')?;
    let parse = |s: &str| -> Vec<WindowButton> {
        s.split(',').filter_map(WindowButton::from_gnome_name).collect()
    };
    Some(SystemDecorations {
        layout: DecorationLayout { left: parse(left), right: parse(right) },
        metrics: DecorationMetrics::default(),
        style: DecorationStyle::Native,
    })
}

// ─── Растеризация кнопок Aurorae ────────────────────────────────────────

/// Готовая к загрузке в GPU кнопка: премультиплицированная альфа снята,
/// формат совпадает с [`crate::widgets::Image::from_rgba`].
#[cfg(feature = "svg")]
#[derive(Debug, Clone)]
pub struct RasterizedButton {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Рисует одно состояние кнопки из SVG темы Aurorae в квадрат `size_px`.
///
/// Внутри SVG состояния разнесены по холсту отдельными группами
/// (`active-center`, `hover-center`, …) — рендерим ровно нужную.
#[cfg(feature = "svg")]
pub fn rasterize_aurorae_button(
    svg_path: &Path,
    state: ButtonState,
    size_px: u32,
) -> Option<RasterizedButton> {
    use resvg::{tiny_skia, usvg};

    let size_px = size_px.clamp(1, 512);
    let data = std::fs::read(svg_path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;

    let node = tree
        .node_by_id(state.aurorae_element())
        // Не во всех темах есть все состояния — откатываемся на обычное.
        .or_else(|| tree.node_by_id(ButtonState::Normal.aurorae_element()))?;
    let bbox = node.abs_layer_bounding_box()?;
    if bbox.width() <= 0.0 || bbox.height() <= 0.0 {
        return None;
    }

    // Рендерим всё дерево, сдвинув нужное состояние в начало координат:
    // `render_node` теряет трансформы родительских групп, а состояния в теме
    // разнесены по холсту именно ими. Соседние состояния при этом остаются за
    // пределами pixmap.
    let scale = size_px as f32 / bbox.width().max(bbox.height());
    let mut pixmap = tiny_skia::Pixmap::new(size_px, size_px)?;
    let transform = tiny_skia::Transform::from_scale(scale, scale)
        .pre_translate(-bbox.x(), -bbox.y());
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut rgba = pixmap.data().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a > 0 && a < 255 {
            let inv = 255.0 / a as f32;
            px[0] = ((px[0] as f32 * inv).round() as u32).min(255) as u8;
            px[1] = ((px[1] as f32 * inv).round() as u32).min(255) as u8;
            px[2] = ((px[2] as f32 * inv).round() as u32).min(255) as u8;
        }
    }
    Some(RasterizedButton { width: size_px, height: size_px, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kde_letters_map_to_buttons() {
        assert_eq!(
            DecorationLayout::parse_kde("IAX"),
            vec![WindowButton::Minimize, WindowButton::Maximize, WindowButton::Close]
        );
        // Неизвестные буквы просто игнорируются.
        assert_eq!(DecorationLayout::parse_kde("XQ"), vec![WindowButton::Close]);
        assert!(DecorationLayout::parse_kde("").is_empty());
    }

    #[test]
    fn gnome_names_map_to_buttons() {
        assert_eq!(WindowButton::from_gnome_name("appmenu"), Some(WindowButton::ApplicationMenu));
        assert_eq!(WindowButton::from_gnome_name("close"), Some(WindowButton::Close));
        assert_eq!(WindowButton::from_gnome_name("tab"), None);
    }
}
