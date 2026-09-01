//! Операции редактирования модели документа.
//!
//! Все мутации проходят через функции этого модуля — это единая точка,
//! на которую следующим этапом ложится командный undo/redo. Смещения —
//! байтовые, в конкатенации ранов блока; стиль набора наследуется от
//! рана слева от каретки.

use super::model::*;
use super::state::{BlockOrder, CaretPos, DocSelection};

// ─── Операции над InlineText ────────────────────────────────────────────────

/// Вставка плоского текста; стиль — от рана слева от точки вставки.
pub fn text_insert(text: &mut InlineText, offset: usize, s: &str) {
    if s.is_empty() {
        return;
    }
    if text.0.is_empty() {
        text.0.push(InlineRun { text: s.to_string(), style: InlineStyle::default() });
        return;
    }
    let mut acc = 0usize;
    for run in text.0.iter_mut() {
        let len = run.text.len();
        if acc + len >= offset {
            let local = offset - acc;
            run.text.insert_str(local.min(len), s);
            return;
        }
        acc += len;
    }
    // Смещение за концом — дописываем в хвост.
    if let Some(last) = text.0.last_mut() {
        last.text.push_str(s);
    }
}

/// Удаление байтового диапазона.
pub fn text_delete(text: &mut InlineText, start: usize, end: usize) {
    if end <= start {
        return;
    }
    let runs = std::mem::take(&mut text.0);
    let mut acc = 0usize;
    for mut run in runs {
        let len = run.text.len();
        let r_start = acc;
        let r_end = acc + len;
        acc = r_end;
        let cut_start = start.max(r_start);
        let cut_end = end.min(r_end);
        if cut_start < cut_end {
            let local_s = cut_start - r_start;
            let local_e = cut_end - r_start;
            run.text.replace_range(local_s..local_e, "");
        }
        if !run.text.is_empty() {
            text.0.push(run);
        }
    }
    text.normalize();
}

/// Разрезает текст на две части по смещению.
pub fn text_split(text: &InlineText, offset: usize) -> (InlineText, InlineText) {
    let mut left = InlineText::default();
    let mut right = InlineText::default();
    let mut acc = 0usize;
    for run in &text.0 {
        let len = run.text.len();
        let r_start = acc;
        let r_end = acc + len;
        acc = r_end;
        if r_end <= offset {
            left.0.push(run.clone());
        } else if r_start >= offset {
            right.0.push(run.clone());
        } else {
            let local = offset - r_start;
            left.0.push(InlineRun {
                text: run.text[..local].to_string(),
                style: run.style.clone(),
            });
            right.0.push(InlineRun {
                text: run.text[local..].to_string(),
                style: run.style.clone(),
            });
        }
    }
    left.normalize();
    right.normalize();
    (left, right)
}

pub fn text_append(dst: &mut InlineText, src: &InlineText) {
    dst.0.extend(src.0.iter().cloned());
    dst.normalize();
}

/// Ближайшая граница символа слева от `offset - 1`.
pub fn prev_char_boundary(s: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut i = offset - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Границы слова вокруг смещения (для выделения двойным кликом).
pub fn word_bounds(s: &str, offset: usize) -> (usize, usize) {
    if s.is_empty() {
        return (0, 0);
    }
    let at = offset.min(s.len());
    let at = if s.is_char_boundary(at) { at } else { prev_char_boundary(s, at) };
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let ch = s[at..].chars().next().or_else(|| s[..at].chars().next_back());
    let Some(ch) = ch else { return (at, at) };
    if !is_word(ch) {
        let end = next_char_boundary(s, at);
        return (at, end);
    }
    let mut start = at;
    for (i, c) in s[..at].char_indices().rev() {
        if is_word(c) {
            start = i;
        } else {
            break;
        }
    }
    let mut end = s.len();
    for (i, c) in s[at..].char_indices() {
        if !is_word(c) {
            end = at + i;
            break;
        }
    }
    (start, end)
}

pub fn next_char_boundary(s: &str, offset: usize) -> usize {
    if offset >= s.len() {
        return s.len();
    }
    let mut i = offset + 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

// ─── Доступ к блокам ────────────────────────────────────────────────────────

pub fn find_block<'a>(blocks: &'a [DocBlock], id: BlockId) -> Option<&'a DocBlock> {
    for b in blocks {
        if b.id == id {
            return Some(b);
        }
        if let Some(children) = b.kind.children() {
            if let Some(found) = find_block(children, id) {
                return Some(found);
            }
        }
    }
    None
}

pub fn find_block_mut<'a>(blocks: &'a mut [DocBlock], id: BlockId) -> Option<&'a mut DocBlock> {
    for b in blocks.iter_mut() {
        if b.id == id {
            return Some(b);
        }
        if let Some(children) = b.kind.children_mut() {
            if let Some(found) = find_block_mut(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Вызывает `f` со списком сиблингов и индексом блока `id` в нём.
pub fn with_siblings<R>(
    blocks: &mut Vec<DocBlock>,
    id: BlockId,
    f: &mut dyn FnMut(&mut Vec<DocBlock>, usize) -> R,
) -> Option<R> {
    if let Some(idx) = blocks.iter().position(|b| b.id == id) {
        return Some(f(blocks, idx));
    }
    for b in blocks.iter_mut() {
        if let Some(children) = b.kind.children_mut() {
            if let Some(r) = with_siblings(children, id, f) {
                return Some(r);
            }
        }
    }
    None
}

/// Цепочка предков блока (от корня к родителю).
pub fn ancestors(blocks: &[DocBlock], id: BlockId) -> Vec<BlockId> {
    fn walk(blocks: &[DocBlock], id: BlockId, path: &mut Vec<BlockId>) -> bool {
        for b in blocks {
            if b.id == id {
                return true;
            }
            if let Some(children) = b.kind.children() {
                path.push(b.id);
                if walk(children, id, path) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
    let mut path = Vec::new();
    walk(blocks, id, &mut path);
    path
}

pub fn block_text_len(model: &DocModel, id: BlockId) -> usize {
    find_block(&model.blocks, id)
        .and_then(|b| b.kind.text())
        .map(|t| t.len_bytes())
        .unwrap_or(0)
}

/// Последовательная перенумерация Numbered-блоков в списке сиблингов.
fn renumber(siblings: &mut [DocBlock]) {
    let mut current: Option<u64> = None;
    for b in siblings.iter_mut() {
        if let BlockKind::Numbered { number, .. } = &mut b.kind {
            let n = current.map(|c| c + 1).unwrap_or(*number);
            *number = n;
            current = Some(n);
        } else {
            current = None;
        }
    }
}

// ─── Операции редактирования ────────────────────────────────────────────────

pub fn insert_text(model: &mut DocModel, caret: CaretPos, s: &str) -> CaretPos {
    if let Some(text) = find_block_mut(&mut model.blocks, caret.block).and_then(|b| b.kind.text_mut())
    {
        text_insert(text, caret.offset, s);
        text.normalize();
        CaretPos { block: caret.block, offset: caret.offset + s.len() }
    } else {
        caret
    }
}

/// Удаление выделения (возможно, через границы блоков).
pub fn delete_selection(model: &mut DocModel, order: &BlockOrder, sel: DocSelection) -> CaretPos {
    let (start, end) = sel.ordered(order);
    if start.block == end.block {
        if let Some(text) =
            find_block_mut(&mut model.blocks, start.block).and_then(|b| b.kind.text_mut())
        {
            text_delete(text, start.offset, end.offset);
        }
        return start;
    }

    let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else {
        return start;
    };
    let end_ancestors = ancestors(&model.blocks, end.block);

    // Средние блоки: предки конца только очищаются, остальные — удаляются
    // целиком вместе с поддеревом (их потомки тоже «средние»).
    for i in si + 1..ei {
        let id = order.ids[i];
        if end_ancestors.contains(&id) {
            if let Some(text) = find_block_mut(&mut model.blocks, id).and_then(|b| b.kind.text_mut())
            {
                text.0.clear();
            }
        } else if find_block(&model.blocks, id).is_some() {
            with_siblings(&mut model.blocks, id, &mut |sibs, idx| {
                sibs.remove(idx);
                renumber(sibs);
            });
        }
    }

    // Крайние блоки: хвост первого и голова последнего.
    if let Some(text) =
        find_block_mut(&mut model.blocks, start.block).and_then(|b| b.kind.text_mut())
    {
        let len = text.len_bytes();
        text_delete(text, start.offset, len);
    }
    if let Some(text) = find_block_mut(&mut model.blocks, end.block).and_then(|b| b.kind.text_mut())
    {
        text_delete(text, 0, end.offset);
    }

    // Склейка краёв: только когда конец — простой блок без детей.
    let end_simple = find_block(&model.blocks, end.block)
        .map(|b| b.kind.children().map(|c| c.is_empty()).unwrap_or(true))
        .unwrap_or(false);
    if end_simple {
        let tail = find_block(&model.blocks, end.block)
            .and_then(|b| b.kind.text())
            .cloned()
            .unwrap_or_default();
        if let Some(text) =
            find_block_mut(&mut model.blocks, start.block).and_then(|b| b.kind.text_mut())
        {
            text_append(text, &tail);
            with_siblings(&mut model.blocks, end.block, &mut |sibs, idx| {
                sibs.remove(idx);
                renumber(sibs);
            });
        }
    }
    start
}

/// Enter: разрез блока по каретке.
pub fn split_block(model: &mut DocModel, caret: CaretPos) -> CaretPos {
    let new_id = model.alloc_id();
    let result = with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
        let block = &mut sibs[idx];
        let Some(text) = block.kind.text() else { return caret };
        let (left, right) = text_split(text, caret.offset);

        let new_kind = match &mut block.kind {
            BlockKind::Paragraph(t) => {
                *t = left;
                BlockKind::Paragraph(right)
            }
            BlockKind::Heading { text, .. } => {
                *text = left;
                BlockKind::Paragraph(right)
            }
            BlockKind::Bullet { text, children } => {
                *text = left;
                BlockKind::Bullet { text: right, children: std::mem::take(children) }
            }
            BlockKind::Numbered { number, text, children } => {
                *text = left;
                let n = *number + 1;
                BlockKind::Numbered { number: n, text: right, children: std::mem::take(children) }
            }
            BlockKind::Todo { text, children, .. } => {
                *text = left;
                BlockKind::Todo { checked: false, text: right, children: std::mem::take(children) }
            }
            BlockKind::Toggle { summary, children, .. } => {
                // Enter в шапке toggle — новый параграф первым ребёнком.
                *summary = left;
                children.insert(0, DocBlock::new(new_id, BlockKind::Paragraph(right)));
                return CaretPos { block: new_id, offset: 0 };
            }
            BlockKind::Callout { title, children, .. } => {
                *title = left;
                children.insert(0, DocBlock::new(new_id, BlockKind::Paragraph(right)));
                return CaretPos { block: new_id, offset: 0 };
            }
            _ => return caret,
        };
        sibs.insert(idx + 1, DocBlock::new(new_id, new_kind));
        renumber(sibs);
        CaretPos { block: new_id, offset: 0 }
    });
    result.unwrap_or(caret)
}

/// Backspace в нулевой позиции: конверсия «сложного» блока в параграф либо
/// склейка с предыдущим текстовым сиблингом.
pub fn backspace_at_start(model: &mut DocModel, order: &BlockOrder, caret: CaretPos) -> CaretPos {
    // 1. Конверсия в параграф (Notion-поведение): списки, заголовки, toggle.
    let converted = with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
        let block = &mut sibs[idx];
        let (text, children) = match &mut block.kind {
            BlockKind::Heading { text, .. } => (std::mem::take(text), Vec::new()),
            BlockKind::Bullet { text, children }
            | BlockKind::Numbered { text, children, .. }
            | BlockKind::Todo { text, children, .. } => {
                (std::mem::take(text), std::mem::take(children))
            }
            BlockKind::Toggle { summary, children, .. } => {
                (std::mem::take(summary), std::mem::take(children))
            }
            _ => return false,
        };
        block.kind = BlockKind::Paragraph(text);
        // Дети поднимаются на уровень блока сразу после него.
        for (i, child) in children.into_iter().enumerate() {
            sibs.insert(idx + 1 + i, child);
        }
        renumber(sibs);
        true
    });
    if converted == Some(true) {
        return caret;
    }

    // 2. Непосредственный сосед-сиблинг слева может быть нетекстовым
    // (разделитель, медиа...) — Backspace удаляет его.
    let removed_nontext = with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
        if idx == 0 {
            return false;
        }
        if sibs[idx - 1].kind.text().is_none() {
            sibs.remove(idx - 1);
            renumber(sibs);
            true
        } else {
            false
        }
    });
    if removed_nontext == Some(true) {
        return caret;
    }

    // 3. Склейка с предыдущим сиблингом.
    let parent = ancestors(&model.blocks, caret.block).last().copied();
    let Some(prev_id) = order.prev(caret.block) else { return caret };
    let prev_parent = ancestors(&model.blocks, prev_id).last().copied();

    // Предыдущий в порядке обхода может быть на другом уровне — склеиваем
    // только сиблингов; иначе каретка просто уходит в конец предыдущего.
    if parent != prev_parent {
        return CaretPos { block: prev_id, offset: block_text_len(model, prev_id) };
    }

    let prev_len = block_text_len(model, prev_id);
    let tail = find_block(&model.blocks, caret.block)
        .and_then(|b| b.kind.text())
        .cloned()
        .unwrap_or_default();
    let cur_children = find_block(&model.blocks, caret.block)
        .and_then(|b| b.kind.children())
        .map(|c| c.to_vec())
        .unwrap_or_default();
    if let Some(prev_text) =
        find_block_mut(&mut model.blocks, prev_id).and_then(|b| b.kind.text_mut())
    {
        text_append(prev_text, &tail);
    } else {
        return caret;
    }
    // Дети склеиваемого блока переезжают к предыдущему (или поднимаются).
    if !cur_children.is_empty() {
        if let Some(prev_children) =
            find_block_mut(&mut model.blocks, prev_id).and_then(|b| b.kind.children_mut())
        {
            prev_children.extend(cur_children);
        } else {
            with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
                for (i, child) in cur_children.iter().cloned().enumerate() {
                    sibs.insert(idx + 1 + i, child);
                }
            });
        }
    }
    with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
        sibs.remove(idx);
        renumber(sibs);
    });
    CaretPos { block: prev_id, offset: prev_len }
}

/// Delete в конце блока: склейка следующего сиблинга в текущий.
pub fn delete_at_end(model: &mut DocModel, caret: CaretPos) -> CaretPos {
    with_siblings(&mut model.blocks, caret.block, &mut |sibs, idx| {
        if idx + 1 >= sibs.len() {
            return;
        }
        let next = &sibs[idx + 1];
        if next.kind.text().is_none() {
            sibs.remove(idx + 1);
            renumber(sibs);
            return;
        }
        let no_children = next.kind.children().map(|c| c.is_empty()).unwrap_or(true);
        if !no_children {
            return;
        }
        let tail = next.kind.text().cloned().unwrap_or_default();
        if let Some(text) = sibs[idx].kind.text_mut() {
            text_append(text, &tail);
            sibs.remove(idx + 1);
            renumber(sibs);
        }
    });
    caret
}

pub fn toggle_todo(model: &mut DocModel, id: BlockId) {
    if let Some(BlockKind::Todo { checked, .. }) =
        find_block_mut(&mut model.blocks, id).map(|b| &mut b.kind)
    {
        *checked = !*checked;
    }
}

pub fn toggle_collapse(model: &mut DocModel, id: BlockId) {
    if let Some(BlockKind::Toggle { collapsed, .. }) =
        find_block_mut(&mut model.blocks, id).map(|b| &mut b.kind)
    {
        *collapsed = !*collapsed;
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse_document;
    use super::super::serialize::serialize_document;
    use super::*;

    fn model(md: &str) -> (DocModel, BlockOrder) {
        let m = parse_document(md);
        let order = BlockOrder::of(&m);
        (m, order)
    }

    fn caret_at(order: &BlockOrder, row: usize, offset: usize) -> CaretPos {
        CaretPos { block: order.ids[row], offset }
    }

    #[test]
    fn insert_inherits_left_style() {
        let (mut m, order) = model("до **жирный** после\n");
        // Вставка сразу после «жирный» — в конец жирного рана.
        let bold_end = "до ".len() + "жирный".len();
        insert_text(&mut m, caret_at(&order, 0, bold_end), "!");
        let text = find_block(&m.blocks, order.ids[0]).unwrap().kind.text().unwrap();
        let bold_run = text.0.iter().find(|r| r.style.bold).unwrap();
        assert_eq!(bold_run.text, "жирный!");
    }

    #[test]
    fn split_paragraph() {
        let (mut m, order) = model("раздва\n");
        let caret = split_block(&mut m, caret_at(&order, 0, "раз".len()));
        assert_eq!(caret.offset, 0);
        assert_eq!(serialize_document(&m), "раз\n\nдва\n");
    }

    #[test]
    fn split_heading_tail_is_paragraph() {
        let (mut m, order) = model("# Заголовок\n");
        split_block(&mut m, caret_at(&order, 0, "Заго".len()));
        assert_eq!(serialize_document(&m), "# Заго\n\nловок\n");
    }

    #[test]
    fn split_list_item_moves_children_and_renumbers() {
        let (mut m, order) = model("1. один\n2. два\n");
        split_block(&mut m, caret_at(&order, 0, "од".len()));
        assert_eq!(serialize_document(&m), "1. од\n2. ин\n3. два\n");
    }

    #[test]
    fn backspace_converts_list_to_paragraph() {
        let (mut m, order) = model("- пункт\n  - ребёнок\n");
        let caret = backspace_at_start(&mut m, &order, caret_at(&order, 0, 0));
        assert_eq!(caret.offset, 0);
        // Пункт стал параграфом, ребёнок поднялся на его уровень.
        assert_eq!(serialize_document(&m), "пункт\n\n- ребёнок\n");
    }

    #[test]
    fn backspace_merges_paragraphs() {
        let (mut m, order) = model("раз\n\nдва\n");
        let caret = backspace_at_start(&mut m, &order, caret_at(&order, 1, 0));
        assert_eq!(caret, CaretPos { block: order.ids[0], offset: "раз".len() });
        assert_eq!(serialize_document(&m), "раздва\n");
    }

    #[test]
    fn backspace_removes_nontext_neighbor() {
        let (mut m, order) = model("раз\n\n---\n\nдва\n");
        backspace_at_start(&mut m, &order, caret_at(&order, 1, 0));
        assert_eq!(serialize_document(&m), "раз\n\nдва\n");
    }

    #[test]
    fn delete_selection_single_block() {
        let (mut m, order) = model("абвгд\n");
        let sel = DocSelection {
            anchor: caret_at(&order, 0, "аб".len()),
            head: caret_at(&order, 0, "абвг".len()),
        };
        let caret = delete_selection(&mut m, &order, sel);
        assert_eq!(caret.offset, "аб".len());
        assert_eq!(serialize_document(&m), "абд\n");
    }

    #[test]
    fn delete_selection_across_blocks() {
        let (mut m, order) = model("первый\n\nсредний\n\nтретий\n");
        let sel = DocSelection {
            anchor: caret_at(&order, 0, "пер".len()),
            head: caret_at(&order, 2, "тре".len()),
        };
        let caret = delete_selection(&mut m, &order, sel);
        assert_eq!(caret.offset, "пер".len());
        assert_eq!(serialize_document(&m), "пертий\n");
    }

    #[test]
    fn delete_forward_merges() {
        let (mut m, order) = model("раз\n\nдва\n");
        delete_at_end(&mut m, caret_at(&order, 0, "раз".len()));
        assert_eq!(serialize_document(&m), "раздва\n");
    }

    #[test]
    fn toggles() {
        let (mut m, order) = model("- [ ] задача\n");
        toggle_todo(&mut m, order.ids[0]);
        assert_eq!(serialize_document(&m), "- [x] задача\n");
    }

    #[test]
    fn selection_ordering() {
        let (m, order) = model("раз\n\nдва\n");
        let a = caret_at(&order, 1, 0);
        let b = caret_at(&order, 0, 2);
        let sel = DocSelection { anchor: a, head: b };
        let (s, e) = sel.ordered(&order);
        assert_eq!(s, b);
        assert_eq!(e, a);
        let _ = m;
    }
}
