//! Свободная раскладка документа: координаты блоков, привязка, фон-сетка.
//!
//! В потоковом режиме блоки идут колонкой (Notion). В свободном каждый
//! верхнеуровневый блок получает координаты `x`/`y` и ширину `w` — их
//! держит [`super::model::Attrs`] блока, поэтому дублирование, перенос и
//! удаление блока таскают геометрию за собой без параллельных структур.
//!
//! На диск геометрия уходит **одним хвостовым блоком** документа:
//!
//! ```text
//! ~~~doc-layout
//! 0 40 120 520
//! 2 40 300 360
//! ~~~
//! ```
//!
//! Строка — `индекс x y w` по порядку верхнеуровневых блоков. Такой формат
//! не требует места под атрибуты в синтаксисе каждого вида блока (у
//! параграфа, таблицы или разделителя его нет) и не может быть спутан с
//! текстом пользователя: `parse_document` снимает блок и раскладывает
//! координаты обратно по атрибутам.

use super::model::{Attrs, DocBlock};

/// Инфо-строка служебного fenced-блока с геометрией.
pub const LAYOUT_FENCE: &str = "doc-layout";

/// Ключи геометрии в атрибутах блока (в markdown инлайном не пишутся).
pub const ATTR_X: &str = "x";
pub const ATTR_Y: &str = "y";
pub const ATTR_W: &str = "w";

/// Ключ геометрии, который сериализуется отдельным блоком, а не инлайном.
pub fn is_geom_key(key: &str) -> bool {
    matches!(key, ATTR_X | ATTR_Y | ATTR_W)
}

/// Фон холста в свободной раскладке.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocGrid {
    #[default]
    None,
    Dots,
    Lines,
    Cross,
}

impl DocGrid {
    pub fn name(self) -> &'static str {
        match self {
            DocGrid::None => "none",
            DocGrid::Dots => "dots",
            DocGrid::Lines => "lines",
            DocGrid::Cross => "cross",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "dots" => DocGrid::Dots,
            "lines" => DocGrid::Lines,
            "cross" => DocGrid::Cross,
            _ => DocGrid::None,
        }
    }

    pub const ALL: [DocGrid; 4] = [DocGrid::None, DocGrid::Dots, DocGrid::Lines, DocGrid::Cross];
}

/// Раскладка страницы: поток либо свободное размещение блоков.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocLayout {
    /// Свободная раскладка (иначе — колонка потока).
    pub free: bool,
    pub grid: DocGrid,
    pub grid_step: f32,
    /// Привязка перетаскивания к шагу `snap_step`.
    pub snap: bool,
    pub snap_step: f32,
    /// Ширина нового блока в свободной раскладке.
    pub block_width: f32,
}

impl Default for DocLayout {
    fn default() -> Self {
        Self {
            free: false,
            grid: DocGrid::None,
            grid_step: 20.0,
            snap: true,
            snap_step: 5.0,
            block_width: 520.0,
        }
    }
}

impl DocLayout {
    /// Значение, притянутое к шагу привязки.
    pub fn snapped(&self, v: f32) -> f32 {
        let step = self.snap_step;
        if !self.snap || step < 0.5 {
            return v;
        }
        (v / step).round() * step
    }

    pub fn grid_step_px(&self) -> f32 {
        self.grid_step.max(2.0)
    }
}

/// Координаты блока в свободной раскладке.
pub fn pos_of(attrs: &Attrs) -> Option<(f32, f32)> {
    let x = attrs.get(ATTR_X)?.parse::<f32>().ok()?;
    let y = attrs.get(ATTR_Y)?.parse::<f32>().ok()?;
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// Ширина блока в свободной раскладке.
pub fn width_of(attrs: &Attrs) -> Option<f32> {
    let w = attrs.get(ATTR_W)?.parse::<f32>().ok()?;
    (w.is_finite() && w > 1.0).then_some(w)
}

pub fn set_pos(attrs: &mut Attrs, x: f32, y: f32) {
    attrs.set(ATTR_X, fmt(x));
    attrs.set(ATTR_Y, fmt(y));
}

pub fn set_width(attrs: &mut Attrs, w: f32) {
    attrs.set(ATTR_W, fmt(w));
}

/// Убрать геометрию (возврат в поток не трогает её — это делает хост
/// явной командой «сбросить раскладку»).
pub fn clear(attrs: &mut Attrs) {
    attrs.remove(ATTR_X);
    attrs.remove(ATTR_Y);
    attrs.remove(ATTR_W);
}

fn fmt(v: f32) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < f32::EPSILON {
        format!("{}", r.round() as i64)
    } else {
        format!("{r}")
    }
}

/// Виды блоков, у которых в markdown есть своё место под `{атрибуты}`.
/// У остальных (параграф, список, код, таблица, разделитель) его нет —
/// их атрибуты уходят в служебный блок хвостом документа.
pub fn has_inline_attrs(kind: &super::model::BlockKind) -> bool {
    use super::model::BlockKind;
    matches!(
        kind,
        BlockKind::Heading { .. }
            | BlockKind::Callout { .. }
            | BlockKind::Toggle { .. }
            | BlockKind::Media { .. }
            | BlockKind::Embed { .. }
    )
}

/// Атрибуты блока, которые пишутся служебным блоком: геометрия всегда, а
/// у блоков без места под инлайн-атрибуты — и всё остальное.
fn sidecar_attrs(block: &DocBlock) -> Attrs {
    let inline_ok = has_inline_attrs(&block.kind);
    let mut out = Attrs::default();
    for (key, value) in block.attrs.0.iter() {
        if is_geom_key(key) || !inline_ok {
            out.set(key.clone(), value.clone());
        }
    }
    out
}

/// Служебный блок с геометрией и свойствами для хвоста markdown; пусто —
/// если писать нечего.
pub fn serialize_geometry(blocks: &[DocBlock]) -> Option<String> {
    let mut lines = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let attrs = sidecar_attrs(b);
        if attrs.is_empty() {
            continue;
        }
        lines.push(format!("{i} {}", super::attrs::serialize_attrs(&attrs)));
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Разложить служебный блок обратно по атрибутам блоков.
///
/// Понимает и старый позиционный формат `индекс x y w` — им записаны
/// страницы до появления свойств блока.
pub fn apply_geometry(blocks: &mut [DocBlock], body: &str) {
    for line in body.lines() {
        let line = line.trim();
        let Some((head, rest)) = line.split_once(char::is_whitespace) else { continue };
        let Ok(i) = head.parse::<usize>() else { continue };
        let Some(block) = blocks.get_mut(i) else { continue };
        let rest = rest.trim();
        if let Some(attrs) = super::attrs::parse_attr_block(rest) {
            for (k, v) in attrs.0 {
                block.attrs.set(k, v);
            }
            continue;
        }
        // Старый формат: `x y w`.
        let mut it = rest.split_whitespace();
        let (Some(x), Some(y)) = (it.next(), it.next()) else { continue };
        let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) else { continue };
        set_pos(&mut block.attrs, x, y);
        if let Some(Ok(w)) = it.next().map(str::parse::<f32>) {
            if w > 1.0 {
                set_width(&mut block.attrs, w);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::input::document_editor::model::{BlockKind, InlineText};

    fn block(id: u64) -> DocBlock {
        DocBlock::new(
            crate::widgets::input::document_editor::model::BlockId(id),
            BlockKind::Paragraph(InlineText::plain("x")),
        )
    }

    #[test]
    fn geometry_roundtrip() {
        let mut blocks = vec![block(1), block(2), block(3)];
        set_pos(&mut blocks[0].attrs, 40.0, 120.0);
        set_width(&mut blocks[0].attrs, 520.0);
        set_pos(&mut blocks[2].attrs, 40.5, 300.0);
        blocks[2].attrs.set("color", "#ff8800");
        let body = serialize_geometry(&blocks).unwrap();

        let mut back = vec![block(1), block(2), block(3)];
        apply_geometry(&mut back, &body);
        assert_eq!(pos_of(&back[0].attrs), Some((40.0, 120.0)));
        assert_eq!(width_of(&back[0].attrs), Some(520.0));
        assert_eq!(pos_of(&back[1].attrs), None);
        assert_eq!(pos_of(&back[2].attrs), Some((40.5, 300.0)));
        assert_eq!(back[2].attrs.get("color"), Some("#ff8800"));
    }

    #[test]
    fn old_positional_format_still_reads() {
        let mut back = vec![block(1), block(2)];
        apply_geometry(&mut back, "1 40 120 520");
        assert_eq!(pos_of(&back[1].attrs), Some((40.0, 120.0)));
        assert_eq!(width_of(&back[1].attrs), Some(520.0));
    }

    #[test]
    fn snapping() {
        let l = DocLayout { snap: true, snap_step: 5.0, ..DocLayout::default() };
        assert_eq!(l.snapped(13.0), 15.0);
        assert_eq!(l.snapped(-2.0), -0.0);
        let off = DocLayout { snap: false, ..l };
        assert_eq!(off.snapped(13.0), 13.0);
    }
}
