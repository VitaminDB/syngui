//! Корневой виджет DocumentEditor.
//!
//! Контейнер владеет моделью (через Arc — общий с [`DocumentEditorHandle`]),
//! строит по-блочные дочерние элементы и централизует ввод: фокус, каретка,
//! выделение, клавиатура и IME живут здесь, а не в дочерних строках
//! (паттерн «пассивные строки», см. план notes-режима). Геометрию строк
//! публикуют TextRow-элементы в общий [`GeomMap`]; по ней контейнер делает
//! хит-тест мыши, рисует каретку (post_build_display_list — поверх детей)
//! и выделение (build_display_list — под детьми).

use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::{ComputedStyle, TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::signal::{use_signal, RwSignal};
use crate::widget::context::{EventContext, TextMeasure, UpdateContext};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, Widget,
};

use super::build::{block_widget, BuildEnv};
use super::chrome::Chrome;
use super::edit;
use super::links::{DocLinkProvider, DocMediaResolver, EmbedCtx, EmbedFactory, LinkCandidate};
use super::history::{EditClass, UndoStack};
use super::model::{BlockKind, DocBlock, DocModel, InlineStyle, InlineText};
use super::shortcuts::{block_shortcut, try_inline_shortcut, BlockShortcut};
use super::slash::{default_items, filter_items, SlashAction, SlashItem, SlashState};
use super::parse::parse_document;
use super::serialize::serialize_document;
use super::state::{
    gutter_action, new_block_rect_map, new_code_geom_map, new_geom_map, new_table_geom_map,
    BlockOrder, BlockRectMap, CaretPos, CodeCaret, CodeGeom, CodeGeomMap, DocSelection, GeomMap,
    GutterAction, RowGeom, TableCaret, TableGeom, TableGeomMap,
};
use super::free::{self, DocGrid, DocLayout};
use super::props::{self, BlockOutline, TableOp};
use super::shape;
use super::style::DocStyle;

/// Операция хоста над документом «у каретки» — кладётся в очередь ручки
/// ([`DocumentEditorHandle::queue_op`]) и применяется самим элементом на
/// ближайшем `update` (после бампа `model_epoch` хостом): так правка
/// проходит через историю undo и ставит каретку, чего внешний код без
/// доступа к состоянию элемента сделать не может.
#[derive(Clone, Debug)]
pub enum DocOp {
    /// Вставить markdown-фрагмент после блока каретки (без каретки — в
    /// конец). Пустой параграф под кареткой заменяется фрагментом.
    InsertMarkdown(String),
    /// Вставить новый пустой блок заданного типа после блока каретки
    /// (пустой параграф под кареткой используется на месте) — та же
    /// логика, что у пункта slash-меню.
    InsertBlock(SlashAction),
    /// Сменить тип блока каретки (как пункт slash-меню).
    TurnInto(SlashAction),
    /// Продублировать блок каретки сразу под ним.
    Duplicate,
    /// Удалить блок каретки (последний блок документа — очистить).
    Delete,
    /// Сдвинуть блок каретки на одну позицию среди соседей.
    Move { down: bool },
    /// Сделать блок текущим (клик в дереве блоков хоста).
    Select(super::model::BlockId),
    /// Свойство блока: `None` — вернуть к теме (см. [`super::props`]).
    SetAttr { block: super::model::BlockId, key: String, value: Option<String> },
    /// Строки и колонки таблицы.
    Table { block: super::model::BlockId, op: TableOp },
    /// Удалить блок по id (хост забрал его — например, в карточку доски).
    DeleteBlock(super::model::BlockId),
    /// Вставить markdown в точку: на холст свободной раскладки — ровно в
    /// неё, в потоке — у блока под точкой (дроп извне).
    InsertMarkdownAt { at: Point, md: String },
    /// Отменить / повторить последнюю правку (кнопки хоста; история живёт
    /// в ручке страницы и переживает пересоздание элемента).
    Undo,
    Redo,
    /// Скопировать в буфер обмена выделенные блоки (без выделения —
    /// текущий блок) как markdown.
    Copy,
    /// Вырезать: копия в буфер + удаление одним шагом истории.
    Cut,
    /// Вставить из буфера: после выделенных блоков, без выделения — у
    /// каретки (многострочный текст — блоками markdown).
    Paste,
    /// Выделить блоки верхнего уровня (пустой список — снять выделение).
    SelectBlocks(Vec<super::model::BlockId>),
}

/// Снимок свойств блока для панели хоста.
#[derive(Clone, Debug)]
pub struct BlockProps {
    pub id: super::model::BlockId,
    /// Машинный тип: `paragraph`, `heading`, `table`, …
    pub kind: &'static str,
    /// Уровень заголовка (иначе 0).
    pub level: u8,
    /// Язык код-блока.
    pub language: Option<String>,
    /// (строк с шапкой, колонок) — для таблицы.
    pub table: Option<(usize, usize)>,
    /// Вид векторного примитива.
    pub shape: Option<super::model::ShapeKind>,
    /// Цель врезки `![[…]]` (страница либо объект хоста `kind:id`).
    pub embed: Option<String>,
    pub attrs: super::model::Attrs,
}

/// Ручка редактора для хоста: доступ к модели (сериализация для автосейва)
/// и сигнал ревизии, растущий на каждую правку.
#[derive(Clone)]
pub struct DocumentEditorHandle {
    model: Arc<Mutex<DocModel>>,
    revision: RwSignal<u64>,
    /// Очередь операций хоста, применяемая элементом (см. [`DocOp`]).
    ops: Arc<Mutex<Vec<DocOp>>>,
    /// Текущий блок (каретка либо выбор в дереве блоков хоста). Элемент
    /// обновляет сигнал сам — панель свойств подписывается на него.
    selected: RwSignal<Option<super::model::BlockId>>,
    /// Отпечаток markdown, из которого модель уже загружена. Живёт в
    /// ручке, а не в элементе: элемент пересоздаётся при каждом
    /// размонтировании поддерева (переключение вкладки/маршрута), и без
    /// этой отметки он перепарсил бы исходник хоста поверх правок.
    source_fp: Arc<Mutex<Option<u64>>>,
    /// История undo/redo — тоже в ручке: иначе смена плитки рейла или
    /// страницы теряла бы (а при общем элементе — путала бы) историю.
    history: Arc<Mutex<UndoStack>>,
    /// (есть что отменить, есть что повторить) — кнопки хоста.
    history_state: RwSignal<(bool, bool)>,
    /// Выделенные блоки страницы (верхний уровень, порядок документа) —
    /// подсветка строк в дереве блоков хоста.
    block_selection: RwSignal<Vec<super::model::BlockId>>,
}

impl DocumentEditorHandle {
    pub fn new() -> Self {
        Self {
            model: Arc::new(Mutex::new(DocModel::new())),
            revision: use_signal(0),
            ops: Arc::new(Mutex::new(Vec::new())),
            selected: use_signal(None),
            source_fp: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(UndoStack::new())),
            history_state: use_signal((false, false)),
            block_selection: use_signal(Vec::new()),
        }
    }

    /// Сигнал `(можно отменить, можно повторить)`: `.get()` в Reactive —
    /// подписка для кнопок истории хоста.
    pub fn history_state(&self) -> RwSignal<(bool, bool)> {
        self.history_state
    }

    /// Сигнал выделенных блоков (id верхнего уровня в порядке документа).
    pub fn block_selection(&self) -> RwSignal<Vec<super::model::BlockId>> {
        self.block_selection
    }

    /// Сигнал текущего блока: `.get()` в Reactive — подписка на выбор.
    pub fn selected(&self) -> RwSignal<Option<super::model::BlockId>> {
        self.selected
    }

    /// Дерево блоков страницы (в том числе пустых — их в документе не
    /// видно, а настроить надо).
    pub fn outline(&self) -> Vec<BlockOutline> {
        props::outline_of(&lock(&self.model).blocks)
    }

    /// Снимок блока для панели свойств: тип, уровень, атрибуты.
    pub fn block_props(&self, id: super::model::BlockId) -> Option<BlockProps> {
        let model = lock(&self.model);
        let b = edit::find_block(&model.blocks, id)?;
        Some(BlockProps {
            id,
            kind: props::kind_name(&b.kind),
            level: match &b.kind {
                BlockKind::Heading { level, .. } => *level,
                _ => 0,
            },
            language: match &b.kind {
                BlockKind::CodeBlock { language, .. } => language.clone(),
                _ => None,
            },
            table: match &b.kind {
                BlockKind::Table { headers, rows, .. } => Some((rows.len() + 1, headers.len())),
                _ => None,
            },
            shape: match &b.kind {
                BlockKind::Shape { shape } => Some(*shape),
                _ => None,
            },
            embed: match &b.kind {
                BlockKind::Embed { target } => Some(target.clone()),
                _ => None,
            },
            attrs: b.attrs.clone(),
        })
    }

    /// Markdown одного верхнеуровневого блока (без геометрии раскладки) —
    /// для переноса блока в карточку доски.
    pub fn block_markdown(&self, id: super::model::BlockId) -> Option<String> {
        let model = lock(&self.model);
        let b = model.blocks.iter().find(|b| b.id == id)?;
        Some(super::serialize::block_markdown(b))
    }

    /// Отпечаток исходника, из которого загружена модель ручки.
    fn loaded_fp(&self) -> Option<u64> {
        *self.source_fp.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_loaded_fp(&self, fp: u64) {
        *self.source_fp.lock().unwrap_or_else(|e| e.into_inner()) = Some(fp);
    }

    /// Забыть отметку загрузки: следующий `markdown(..)` с тем же текстом
    /// снова заменит модель (перезагрузка страницы с диска).
    pub fn reset_source(&self) {
        *self.source_fp.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Поставить операцию в очередь элемента. Применяется на ближайшем
    /// `update` виджета — хост после вызова бампает `.model_epoch(..)`
    /// (пересоздание виджета Reactive'ом); без этого очередь ждёт
    /// следующего пересоздания.
    pub fn queue_op(&self, op: DocOp) {
        self.ops.lock().unwrap_or_else(|e| e.into_inner()).push(op);
    }

    /// Есть ли блок с текстом в документе (для доступности пунктов меню).
    pub fn is_empty(&self) -> bool {
        lock(&self.model).blocks.is_empty()
    }

    /// Текущий документ в markdown.
    pub fn serialize(&self) -> String {
        serialize_document(&lock(&self.model))
    }

    /// Сигнал ревизии: `.get()` в эффекте — подписка на правки.
    pub fn revision(&self) -> RwSignal<u64> {
        self.revision
    }

    /// Дописывает markdown-фрагмент в конец документа (палитра вставки
    /// хоста). Id блоков фрагмента переназначаются из счётчика модели;
    /// ревизия бампается, перестройку хост запускает через model_epoch.
    pub fn append_markdown(&self, md: &str) {
        let fragment = parse_document(md);
        let mut model = lock(&self.model);
        let mut blocks = fragment.blocks;
        remap_ids(&mut blocks, &mut model);
        model.blocks.extend(blocks);
        drop(model);
        self.revision.set(self.revision.get_untracked() + 1);
    }

    /// Замена `pending:<token>` на реальный url после ingest'а хоста
    /// (drop файла → фоновая загрузка в хранилище → patch). Бампает
    /// ревизию (автосейв запишет свежий url); перестройку виджетов хост
    /// триггерит инкрементом `.model_epoch(...)` своего сигнала.
    pub fn patch_media(&self, token: &str, url: &str) -> bool {
        let pending = format!("pending:{token}");
        let mut model = lock(&self.model);
        fn walk(blocks: &mut [super::model::DocBlock], pending: &str, url: &str) -> bool {
            for b in blocks.iter_mut() {
                if let super::model::BlockKind::Media { media, url: u, .. } = &mut b.kind {
                    if u == pending {
                        *u = url.to_string();
                        *media = super::model::MediaKind::detect(url, &b.attrs);
                        return true;
                    }
                }
                if let Some(children) = b.kind.children_mut() {
                    if walk(children, pending, url) {
                        return true;
                    }
                }
            }
            false
        }
        let found = walk(&mut model.blocks, &pending, url);
        drop(model);
        if found {
            self.revision.set(self.revision.get_untracked() + 1);
        }
        found
    }
}

impl Default for DocumentEditorHandle {
    fn default() -> Self {
        Self::new()
    }
}

fn lock(model: &Arc<Mutex<DocModel>>) -> MutexGuard<'_, DocModel> {
    model.lock().unwrap_or_else(|e| e.into_inner())
}

/// Переназначение runtime-id блоков фрагмента из счётчика модели
/// (фрагмент распарсен отдельно — его id пересекаются с документом).
fn remap_ids(blocks: &mut Vec<DocBlock>, model: &mut DocModel) {
    for b in blocks.iter_mut() {
        b.id = model.alloc_id();
        if let Some(children) = b.kind.children_mut() {
            remap_ids(children, model);
        }
    }
}

pub struct DocumentEditor {
    source: String,
    read_only: bool,
    classes: Vec<String>,
    handle: Option<DocumentEditorHandle>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
    slash_items: Vec<SlashItem>,
    on_slash_custom: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    links: Option<Arc<dyn DocLinkProvider>>,
    media: Option<Arc<dyn DocMediaResolver>>,
    embeds: Option<Arc<dyn EmbedFactory>>,
    embed_ctx: EmbedCtx,
    model_epoch: u64,
    on_drop_file: Option<Arc<dyn Fn(std::path::PathBuf, String) + Send + Sync>>,
    on_context_menu: Option<Arc<dyn Fn(Point) + Send + Sync>>,
    layout: DocLayout,
    fill_height: bool,
    /// Получить фокус сразу после создания (правка карточки на месте).
    autofocus: bool,
    /// Фокус ушёл (клик где угодно ещё) — хост закрывает правку.
    on_focus_lost: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Блок отпущен после перетаскивания за ⋮⋮ в точке: хост может забрать
    /// его себе (в карточку доски) — тогда возвращает `true`, и блок
    /// удаляется из документа.
    on_block_drop: Option<Arc<dyn Fn(Point, super::model::BlockId) -> bool + Send + Sync>>,
    /// Дроп не-файла (карточка доски и т.п.) на документ: хост решает, что
    /// вставить; `true` — принято.
    on_drop_data: Option<Arc<dyn Fn(Point, &crate::input::DragData) -> bool + Send + Sync>>,
    /// Тип drag-данных для переноса блока за ⋮⋮: с ним перенос становится
    /// ещё и drag'ом дерева (payload — id блока, без призрака), и его
    /// принимают DropArea хоста — в частности доски внутри самой страницы.
    block_drag_type: Option<String>,
    /// Без обвязки блоков: ни ручки ⋮⋮, ни подсветки/рамки блока, ни зон
    /// растяжения — для маленьких встроенных редакторов (карточка доски).
    plain: bool,
    /// Подсказка в пустом абзаце / пустом заголовке.
    placeholder: Option<String>,
    heading_placeholder: Option<String>,
}

impl DocumentEditor {
    pub fn new() -> Self {
        Self {
            source: String::new(),
            read_only: false,
            classes: Vec::new(),
            handle: None,
            on_change: None,
            slash_items: default_items(),
            on_slash_custom: None,
            links: None,
            media: None,
            embeds: None,
            embed_ctx: EmbedCtx::default(),
            model_epoch: 0,
            on_drop_file: None,
            autofocus: false,
            on_focus_lost: None,
            on_block_drop: None,
            on_drop_data: None,
            block_drag_type: None,
            plain: false,
            placeholder: None,
            heading_placeholder: None,
            on_context_menu: None,
            layout: DocLayout::default(),
            fill_height: false,
        }
    }

    /// Раскладка страницы: поток либо свободное размещение блоков с
    /// привязкой и фон-сеткой (см. [`DocLayout`]).
    pub fn layout(mut self, layout: DocLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Держать высоту не меньше видимой области: клик и правый клик ниже
    /// последнего блока попадают в редактор, а не в пустоту скроллера.
    /// Фокус (и каретка в первый блок) сразу после создания.
    pub fn autofocus(mut self, on: bool) -> Self {
        self.autofocus = on;
        self
    }

    /// Редактор потерял фокус: клик по другому вводу или по пустому месту.
    pub fn on_focus_lost(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_focus_lost = Some(Arc::new(f));
        self
    }

    /// Блок отпущен после переноса за ⋮⋮ в точке `Point`: верните `true`,
    /// если забрали его (он удалится из документа), иначе блок ляжет как
    /// обычно.
    pub fn on_block_drop(
        mut self,
        f: impl Fn(Point, super::model::BlockId) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.on_block_drop = Some(Arc::new(f));
        self
    }

    /// Дроп данных перетаскивания (не файла) на документ.
    pub fn on_drop_data(
        mut self,
        f: impl Fn(Point, &crate::input::DragData) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.on_drop_data = Some(Arc::new(f));
        self
    }

    /// Перенос блока за ⋮⋮ объявляется drag'ом дерева этого типа (payload —
    /// id блока, `DragData::without_ghost`): его принимают DropArea хоста,
    /// в том числе внутри самой страницы. Отпускание тогда приходит как
    /// `DragEnd`, а блок, который DropArea забрал, хост удаляет через
    /// `DocOp::DeleteBlock`.
    pub fn block_drag_type(mut self, drag_type: impl Into<String>) -> Self {
        self.block_drag_type = Some(drag_type.into());
        self
    }

    /// Без обвязки блоков (ручка ⋮⋮, подсветка и рамка блока, зоны
    /// растяжения): маленький встроенный редактор — карточка доски.
    pub fn plain(mut self, plain: bool) -> Self {
        self.plain = plain;
        self
    }

    /// Подсказка в каждом пустом абзаце (приглушённым цветом).
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Подсказка в пустом заголовке.
    pub fn heading_placeholder(mut self, text: impl Into<String>) -> Self {
        self.heading_placeholder = Some(text.into());
        self
    }

    pub fn fill_height(mut self, fill: bool) -> Self {
        self.fill_height = fill;
        self
    }

    /// Фабрика живых врезок `![[…]]`.
    pub fn embeds(mut self, factory: Arc<dyn EmbedFactory>) -> Self {
        self.embeds = Some(factory);
        self
    }

    /// Контекст вложенности (host передаёт depth+1 во вложенные редакторы).
    pub fn embed_ctx(mut self, ctx: EmbedCtx) -> Self {
        self.embed_ctx = ctx;
        self
    }

    /// Эпоха модели: хост инкрементирует после внешних мутаций через
    /// handle (patch_media) — смена значения перестраивает блоки без
    /// репарса markdown.
    pub fn model_epoch(mut self, epoch: u64) -> Self {
        self.model_epoch = epoch;
        self
    }

    /// Дроп файла в документ: редактор вставил pending-блок и отдаёт
    /// (путь, токен) — хост загружает файл и зовёт handle.patch_media.
    pub fn on_drop_file(
        mut self,
        f: impl Fn(std::path::PathBuf, String) + Send + Sync + 'static,
    ) -> Self {
        self.on_drop_file = Some(Arc::new(f));
        self
    }

    /// Правый клик внутри документа: редактор ставит каретку в точку клика
    /// и отдаёт абсолютную позицию — хост показывает своё контекстное меню
    /// и действует через [`DocumentEditorHandle::queue_op`]. Без колбэка
    /// правый клик не перехватывается (сработает внешний `ContextMenu`).
    pub fn on_context_menu(mut self, f: impl Fn(Point) + Send + Sync + 'static) -> Self {
        self.on_context_menu = Some(Arc::new(f));
        self
    }

    /// Провайдер ссылок хоста: автокомплит `[[`, битые ссылки, открытие.
    pub fn links(mut self, provider: Arc<dyn DocLinkProvider>) -> Self {
        self.links = Some(provider);
        self
    }

    /// Резолвер медиа хоста (`blob:` → локальный файл, постер, волна).
    pub fn media(mut self, resolver: Arc<dyn DocMediaResolver>) -> Self {
        self.media = Some(resolver);
        self
    }

    /// Полная замена каталога slash-меню (локализация, свои пункты).
    pub fn slash_items(mut self, items: Vec<SlashItem>) -> Self {
        self.slash_items = items;
        self
    }

    /// Обработчик кастомных пунктов slash-меню (`SlashAction::Custom`).
    pub fn on_slash_custom(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_slash_custom = Some(Arc::new(f));
        self
    }

    /// Markdown-исходник документа. Смена строки (по fingerprint) заменяет
    /// модель целиком — путь для set_markdown/перезагрузки с диска.
    pub fn markdown(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn handle(mut self, handle: &DocumentEditorHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Лёгкий колбэк на каждую правку (без сериализации).
    pub fn on_change(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for DocumentEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn fingerprint(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Widget for DocumentEditor {
    fn create_element(&self) -> Box<dyn Element> {
        let fp = fingerprint(&self.source);
        let model = match &self.handle {
            Some(h) => {
                // Ручка уже держит модель, загруженную из этого же
                // исходника (пересоздание элемента после размонтирования)
                // — правки в ней свежее исходника хоста, не трогаем.
                if h.loaded_fp() != Some(fp) {
                    *lock(&h.model) = parse_document(&self.source);
                    h.set_loaded_fp(fp);
                }
                h.model.clone()
            }
            None => {
                let model = Arc::new(Mutex::new(DocModel::new()));
                *lock(&model) = parse_document(&self.source);
                model
            }
        };
        Box::new(DocumentEditorElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            classes: self.classes.clone(),
            model,
            style: Arc::new(DocStyle::default()),
            source_fp: Some(fp),
            read_only: self.read_only,
            rebuild: true,
            geom: new_geom_map(),
            blocks: new_block_rect_map(),
            tables: new_table_geom_map(),
            codes: new_code_geom_map(),
            code_caret: None,
            selected: self.handle.as_ref().map(|h| h.selected),
            table_caret: None,
            tm: None,
            selection: None,
            mouse_selecting: false,
            goal_x: None,
            blink_ms: 0.0,
            caret_on: true,
            preedit: None,
            revision: self.handle.as_ref().map(|h| h.revision),
            on_change: self.on_change.clone(),
            history: self.handle.as_ref().map(|h| h.history.clone()).unwrap_or_default(),
            slash: None,
            slash_items: self.slash_items.clone(),
            on_slash_custom: self.on_slash_custom.clone(),
            ui_rects: Mutex::new(UiRects::default()),
            hover_block: None,
            drag: None,
            links: self.links.clone(),
            media: self.media.clone(),
            embeds: self.embeds.clone(),
            embed_ctx: self.embed_ctx.clone(),
            model_epoch: self.model_epoch,
            on_drop_file: self.on_drop_file.clone(),
            autofocus: self.autofocus,
            focus_request_pending: self.autofocus,
            focused: self.autofocus,
            on_focus_lost: self.on_focus_lost.clone(),
            on_block_drop: self.on_block_drop.clone(),
            on_drop_data: self.on_drop_data.clone(),
            block_drag_type: self.block_drag_type.clone(),
            plain: self.plain,
            placeholder: self.placeholder.clone(),
            heading_placeholder: self.heading_placeholder.clone(),
            on_context_menu: self.on_context_menu.clone(),
            ops: self.handle.as_ref().map(|h| h.ops.clone()).unwrap_or_default(),
            wiki: None,
            layout: self.layout,
            fill_height: self.fill_height,
            menu_pos: None,
            free_drag: None,
            object_sel: None,
            history_state: self.handle.as_ref().map(|h| h.history_state),
            block_sel: Vec::new(),
            block_anchor: None,
            block_sel_sig: self.handle.as_ref().map(|h| h.block_selection),
            marquee: None,
        })
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

pub struct DocumentEditorElement {
    autofocus: bool,
    focus_request_pending: bool,
    on_focus_lost: Option<Arc<dyn Fn() + Send + Sync>>,
    on_block_drop: Option<Arc<dyn Fn(Point, super::model::BlockId) -> bool + Send + Sync>>,
    on_drop_data: Option<Arc<dyn Fn(Point, &crate::input::DragData) -> bool + Send + Sync>>,
    block_drag_type: Option<String>,
    plain: bool,
    placeholder: Option<String>,
    heading_placeholder: Option<String>,
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    classes: Vec<String>,
    model: Arc<Mutex<DocModel>>,
    style: Arc<DocStyle>,
    /// Отпечаток исходника, из которого разобрана текущая модель.
    source_fp: Option<u64>,
    read_only: bool,
    rebuild: bool,
    geom: GeomMap,
    /// Прямоугольники верхнеуровневых блоков (публикуют обёртки Chrome).
    blocks: BlockRectMap,
    /// Геометрия таблиц (публикуют TableBlockElement'ы).
    tables: TableGeomMap,
    /// Геометрия код-блоков (публикуют CodeBlockElement'ы).
    codes: CodeGeomMap,
    /// Каретка внутри код-блока (отдельный режим, как у таблицы).
    code_caret: Option<CodeCaret>,
    /// Сигнал текущего блока (общий с ручкой).
    selected: Option<RwSignal<Option<super::model::BlockId>>>,
    /// Каретка внутри ячейки таблицы (отдельный режим от `selection`).
    table_caret: Option<TableCaret>,
    tm: Option<Arc<dyn TextMeasure>>,
    focused: bool,
    selection: Option<DocSelection>,
    mouse_selecting: bool,
    /// Целевой X при вертикальной навигации.
    goal_x: Option<f32>,
    blink_ms: f32,
    caret_on: bool,
    preedit: Option<String>,
    revision: Option<RwSignal<u64>>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
    history: Arc<Mutex<UndoStack>>,
    slash: Option<SlashState>,
    slash_items: Vec<SlashItem>,
    on_slash_custom: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// Прямоугольники всплывающих панелей, вычисленные при отрисовке
    /// (paint идёт по &self — интерьерная мутабельность, как у MarkdownView).
    ui_rects: Mutex<UiRects>,
    /// Блок под курсором — для показа ручки ⋮⋮.
    hover_block: Option<super::model::BlockId>,
    /// Перетаскивание блока за ручку.
    drag: Option<DragBlock>,
    links: Option<Arc<dyn DocLinkProvider>>,
    media: Option<Arc<dyn DocMediaResolver>>,
    embeds: Option<Arc<dyn EmbedFactory>>,
    embed_ctx: EmbedCtx,
    model_epoch: u64,
    on_drop_file: Option<Arc<dyn Fn(std::path::PathBuf, String) + Send + Sync>>,
    on_context_menu: Option<Arc<dyn Fn(Point) + Send + Sync>>,
    /// Очередь операций хоста (общая с ручкой).
    ops: Arc<Mutex<Vec<DocOp>>>,
    /// Автокомплит wiki-ссылки `[[`.
    wiki: Option<WikiState>,
    /// Раскладка страницы (поток / свободная + сетка и привязка).
    layout: DocLayout,
    /// Держать высоту не меньше видимой области: клик ниже последнего
    /// блока должен попадать в редактор, а не в пустоту скроллера.
    fill_height: bool,
    /// Точка последнего правого клика в координатах холста — место
    /// вставки блока из контекстного меню в свободной раскладке.
    menu_pos: Option<Point>,
    /// Перенос/растяжение блока по холсту свободной раскладки.
    free_drag: Option<FreeDrag>,
    /// Выбранный блок, у которого нет каретки (фигура): панель свойств,
    /// рамка габаритов и ручки размера работают и над ним.
    object_sel: Option<super::model::BlockId>,
    /// (можно отменить, можно повторить) для хоста.
    history_state: Option<RwSignal<(bool, bool)>>,
    /// Выделенные блоки верхнего уровня (порядок документа) — отдельный
    /// режим: каретки нет, Delete/Ctrl+C/X/V работают над блоками.
    block_sel: Vec<super::model::BlockId>,
    /// Якорь диапазона Shift+клик.
    block_anchor: Option<super::model::BlockId>,
    block_sel_sig: Option<RwSignal<Vec<super::model::BlockId>>>,
    /// Рамка выделения: нажатие в пустом месте и протяжка.
    marquee: Option<Marquee>,
}

/// Рамка выделения блоков. До порога движения — обычный клик (каретка
/// ставится на отпускании), после — прямоугольник, выделяющий все блоки,
/// которых он касается.
#[derive(Clone, Copy)]
struct Marquee {
    start: Point,
    current: Point,
    active: bool,
}

fn rect_from_points(a: Point, b: Point) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    Rect::new(Point::new(x0, y0), Size::new((a.x - b.x).abs(), (a.y - b.y).abs()))
}

fn rects_intersect(a: Rect, b: Rect) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && b.origin.x < a.origin.x + a.size.width
        && a.origin.y < b.origin.y + b.size.height
        && b.origin.y < a.origin.y + a.size.height
}

/// Контур прямоугольника четырьмя тонкими полосками.
fn stroke_rect(list: &mut DisplayList, r: Rect, color: crate::core::Color, w: f32) {
    let (x, y, wd, h) = (r.origin.x, r.origin.y, r.size.width, r.size.height);
    list.push_rect(Rect::new(Point::new(x, y), Size::new(wd, w)), color, [0.0; 4]);
    list.push_rect(Rect::new(Point::new(x, y + h - w), Size::new(wd, w)), color, [0.0; 4]);
    list.push_rect(Rect::new(Point::new(x, y), Size::new(w, h)), color, [0.0; 4]);
    list.push_rect(Rect::new(Point::new(x + wd - w, y), Size::new(w, h)), color, [0.0; 4]);
}

/// Состояние открытого автокомплита `[[`.
struct WikiState {
    block: super::model::BlockId,
    /// Смещение первой `[` в тексте блока.
    start: usize,
    query: String,
    selected: usize,
    /// Кандидаты последнего запроса (замораживаются на кадр отрисовки).
    candidates: Vec<LinkCandidate>,
}

/// Состояние перетаскивания блока.
struct DragBlock {
    block: super::model::BlockId,
    start: Point,
    current: Point,
    /// Порог в 4px пройден — рисуем ghost и индикатор.
    started: bool,
    /// Куда вставлять: (блок-цель, перед ним?).
    target: Option<(super::model::BlockId, bool)>,
    /// Drag дерева уже объявлен (`block_drag_type`).
    announced: bool,
}

/// Перетаскивание блока по холсту свободной раскладки.
struct FreeDrag {
    block: super::model::BlockId,
    /// Смещение курсора от левого верхнего угла блока.
    grab: Point,
    mode: FreeDragMode,
    /// Ширина блока на старте (для resize).
    start_width: f32,
    moved: bool,
    /// Drag дерева уже объявлен (`block_drag_type`).
    announced: bool,
}

/// Что тянут за блок в свободной раскладке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FreeDragMode {
    /// Перенос за ручку ⋮⋮.
    Move,
    /// Правая кромка — ширина.
    Width,
    /// Нижняя кромка — высота (фигура, медиа).
    Height,
    /// Правый нижний угол — ширина и высота сразу.
    Corner,
    /// Хваталка линейной фигуры: 0/1 — концы, 2/3 — направляющие кривой.
    Endpoint(usize),
}

impl FreeDragMode {
    fn cursor(self) -> CursorIcon {
        match self {
            FreeDragMode::Move => CursorIcon::Grabbing,
            FreeDragMode::Width => CursorIcon::ColResize,
            FreeDragMode::Height => CursorIcon::RowResize,
            FreeDragMode::Corner => CursorIcon::SeResize,
            FreeDragMode::Endpoint(_) => CursorIcon::Crosshair,
        }
    }
}

/// Хит-зоны панелей, нарисованных поверх документа.
#[derive(Default)]
struct UiRects {
    /// (прямоугольник меню, число видимых пунктов).
    slash: Option<(Rect, usize)>,
    /// (прямоугольник тулбара, ширина кнопки).
    toolbar: Option<(Rect, f32)>,
    /// (прямоугольник ручки ⋮⋮, блок).
    handle: Option<(Rect, super::model::BlockId)>,
    /// (правая кромка блока, блок) — растяжение в свободной раскладке.
    resize: Option<(Rect, super::model::BlockId)>,
    /// (нижняя кромка блока, блок) — высота фигуры и медиа.
    resize_v: Option<(Rect, super::model::BlockId)>,
    /// (правый нижний угол, блок) — ширина и высота сразу.
    corner: Option<(Rect, super::model::BlockId)>,
    /// (хваталки линейной фигуры — концы и направляющие, блок).
    ends: Option<(Vec<Rect>, super::model::BlockId)>,
    /// (прямоугольник wiki-меню, число пунктов).
    wiki: Option<(Rect, usize)>,
}

impl DocumentEditorElement {
    fn model(&self) -> MutexGuard<'_, DocModel> {
        lock(&self.model)
    }

    fn history(&self) -> MutexGuard<'_, UndoStack> {
        self.history.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Сигнал «можно отменить / повторить» для кнопок хоста.
    fn publish_history(&mut self) {
        let Some(sig) = self.history_state else { return };
        let now = {
            let h = self.history();
            (h.can_undo(), h.can_redo())
        };
        if sig.get_untracked() != now {
            sig.set(now);
        }
    }

    /// Снимок в историю перед правкой (группировка по классу и блоку).
    fn checkpoint(&mut self, class: EditClass) {
        let block = self.caret().map(|c| c.block);
        let model = lock(&self.model);
        let snapshot_sel = self.selection;
        // NB: заимствуем guard только на время клона.
        self.history().checkpoint(&model, snapshot_sel, class, block);
    }

    fn undo(&mut self) {
        let snap = {
            let model = lock(&self.model);
            self.history().undo(&model, self.selection)
        };
        if let Some(s) = snap {
            *lock(&self.model) = s.model;
            self.selection = s.selection;
            self.after_edit();
        }
    }

    fn redo(&mut self) {
        let snap = {
            let model = lock(&self.model);
            self.history().redo(&model, self.selection)
        };
        if let Some(s) = snap {
            *lock(&self.model) = s.model;
            self.selection = s.selection;
            self.after_edit();
        }
    }

    /// Tab / Shift+Tab с записью в историю (снапшот откатывается, если
    /// правка не применилась).
    fn tab_indent_checkpointed(&mut self, outdent: bool) -> bool {
        self.checkpoint(EditClass::Structure);
        let done = self.tab_indent(outdent);
        if !done {
            self.history().discard_last_checkpoint();
        }
        done
    }

    /// Tab / Shift+Tab: отступ пункта списка.
    fn tab_indent(&mut self, outdent: bool) -> bool {
        let Some(pos) = self.caret() else { return false };
        let mut model = self.model();
        let done = if outdent {
            edit::outdent_item(&mut model, pos.block)
        } else {
            edit::indent_item(&mut model, pos.block)
        };
        drop(model);
        if done {
            self.after_edit();
        }
        done
    }

    /// Конверсия параграфа по блочному шорткату (`# `, `- `, `1. `...).
    fn apply_block_shortcut(&mut self, sc: BlockShortcut, eaten: usize) {
        let Some(pos) = self.caret() else { return };
        let new_caret = {
            let mut model = self.model();
            let extra_id = model.alloc_id();
            edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                let own_id = sibs[idx].id;
                let Some(text_ref) = sibs[idx].kind.text_mut() else { return None };
                let mut text = std::mem::take(text_ref);
                edit::text_delete(&mut text, 0, eaten);
                let after = CaretPos { block: own_id, offset: pos.offset.saturating_sub(eaten) };
                match &sc {
                    BlockShortcut::Heading(n) => {
                        sibs[idx].kind = BlockKind::Heading { level: *n, text };
                        Some(after)
                    }
                    BlockShortcut::Bullet => {
                        sibs[idx].kind = BlockKind::Bullet { text, children: Vec::new() };
                        Some(after)
                    }
                    BlockShortcut::Numbered(n) => {
                        sibs[idx].kind =
                            BlockKind::Numbered { number: *n, text, children: Vec::new() };
                        Some(after)
                    }
                    BlockShortcut::Todo => {
                        sibs[idx].kind =
                            BlockKind::Todo { checked: false, text, children: Vec::new() };
                        Some(after)
                    }
                    BlockShortcut::Toggle => {
                        sibs[idx].kind =
                            BlockKind::Toggle { summary: text, children: Vec::new(), collapsed: false };
                        Some(after)
                    }
                    BlockShortcut::Quote => {
                        let inner = DocBlock::new(extra_id, BlockKind::Paragraph(text));
                        sibs[idx].kind = BlockKind::Quote(vec![inner]);
                        Some(CaretPos { block: extra_id, offset: after.offset })
                    }
                    BlockShortcut::CodeBlock => {
                        sibs[idx].kind =
                            BlockKind::CodeBlock { language: None, code: String::new() };
                        let p = DocBlock::new(extra_id, BlockKind::Paragraph(text));
                        sibs.insert(idx + 1, p);
                        Some(CaretPos { block: extra_id, offset: 0 })
                    }
                    BlockShortcut::Divider => {
                        sibs[idx].kind = BlockKind::Divider;
                        let p = DocBlock::new(extra_id, BlockKind::Paragraph(text));
                        sibs.insert(idx + 1, p);
                        Some(CaretPos { block: extra_id, offset: 0 })
                    }
                }
            })
        };
        if let Some(Some(caret)) = new_caret {
            self.selection = Some(DocSelection::caret(caret));
            self.after_edit();
        }
    }

    /// Проверка блочного шортката после ввода символа.
    fn maybe_block_shortcut(&mut self) -> bool {
        let Some(pos) = self.caret() else { return false };
        let found = {
            let model = self.model();
            let block = edit::find_block(&model.blocks, pos.block);
            let is_paragraph = matches!(block.map(|b| &b.kind), Some(BlockKind::Paragraph(_)));
            if !is_paragraph {
                None
            } else {
                let text = block.and_then(|b| b.kind.text()).map(|t| t.text()).unwrap_or_default();
                let head = &text[..pos.offset.min(text.len())];
                block_shortcut(head)
            }
        };
        if let Some((sc, eaten)) = found {
            self.apply_block_shortcut(sc, eaten);
            true
        } else {
            false
        }
    }

    /// Инлайн-шорткат после ввода замыкающего маркера.
    fn maybe_inline_shortcut(&mut self) {
        let Some(pos) = self.caret() else { return };
        let new_offset = {
            let mut model = self.model();
            edit::find_block_mut(&mut model.blocks, pos.block)
                .and_then(|b| b.kind.text_mut())
                .and_then(|t| try_inline_shortcut(t, pos.offset))
        };
        if let Some(offset) = new_offset {
            self.selection = Some(DocSelection::caret(CaretPos { block: pos.block, offset }));
            self.after_edit();
        }
    }

    /// Открытие slash-меню после ввода `/`.
    fn maybe_open_slash(&mut self) {
        let Some(pos) = self.caret() else { return };
        let ok = {
            let model = self.model();
            let block = edit::find_block(&model.blocks, pos.block);
            let is_paragraph = matches!(block.map(|b| &b.kind), Some(BlockKind::Paragraph(_)));
            let text = block.and_then(|b| b.kind.text()).map(|t| t.text()).unwrap_or_default();
            // `/` уже в тексте: до него либо пусто, либо пробел.
            let before = &text[..pos.offset.saturating_sub(1)];
            is_paragraph && (before.is_empty() || before.ends_with(' '))
        };
        if ok {
            self.slash = Some(SlashState {
                block: pos.block,
                start: pos.offset - 1,
                query: String::new(),
                selected: 0,
            });
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn close_slash(&mut self) {
        if self.slash.take().is_some() {
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    /// Применение пункта slash-меню.
    fn apply_slash(&mut self, action: SlashAction) {
        let Some(sl) = self.slash.take() else { return };
        self.checkpoint(EditClass::Structure);
        // Удаляем `/query` из текста.
        {
            let mut model = self.model();
            if let Some(t) =
                edit::find_block_mut(&mut model.blocks, sl.block).and_then(|b| b.kind.text_mut())
            {
                edit::text_delete(t, sl.start, sl.start + 1 + sl.query.len());
            }
        }
        self.selection = Some(DocSelection::caret(CaretPos { block: sl.block, offset: sl.start }));
        self.apply_action(action);
    }

    /// Применение действия slash-/контекстного меню к блоку каретки.
    /// Чекпойнт истории делает вызывающий.
    fn apply_action(&mut self, action: SlashAction) {
        match &action {
            SlashAction::Paragraph => self.convert_current(BlockKind::Paragraph),
            SlashAction::Heading(n) => self.convert_current(|text| BlockKind::Heading {
                level: *n,
                text,
            }),
            SlashAction::Bullet => {
                self.convert_current(|text| BlockKind::Bullet { text, children: Vec::new() })
            }
            SlashAction::Numbered => self.convert_current(|text| BlockKind::Numbered {
                number: 1,
                text,
                children: Vec::new(),
            }),
            SlashAction::Todo => self.convert_current(|text| BlockKind::Todo {
                checked: false,
                text,
                children: Vec::new(),
            }),
            SlashAction::Toggle => self.convert_current(|text| BlockKind::Toggle {
                summary: text,
                children: Vec::new(),
                collapsed: false,
            }),
            SlashAction::Quote => self.apply_block_shortcut(BlockShortcut::Quote, 0),
            SlashAction::Callout => self.convert_current(|text| BlockKind::Callout {
                kind: "note".to_string(),
                title: text,
                children: Vec::new(),
            }),
            SlashAction::CodeBlock => self.apply_block_shortcut(BlockShortcut::CodeBlock, 0),
            SlashAction::Divider => self.apply_block_shortcut(BlockShortcut::Divider, 0),
            SlashAction::Table => {
                let Some(pos) = self.caret() else { return };
                {
                    let mut model = self.model();
                    let id = model.alloc_id();
                    let empty = InlineText::default;
                    let table = BlockKind::Table {
                        headers: vec![empty(), empty()],
                        rows: vec![vec![empty(), empty()]],
                        aligns: vec![Default::default(), Default::default()],
                    };
                    edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                        sibs.insert(idx + 1, DocBlock::new(id, table.clone()));
                    });
                }
                self.after_edit();
            }
            SlashAction::Shape(shape_kind) => {
                let Some(pos) = self.caret().map(|c| c.block).or(self.object_sel) else { return };
                // Каретка уже в фигуре — меняем её вид, сохраняя оформление
                // (пункт «превратить в» для примитивов).
                let mut model = self.model();
                if let Some(b) = edit::find_block_mut(&mut model.blocks, pos) {
                    if matches!(b.kind, BlockKind::Shape { .. }) {
                        b.kind = BlockKind::Shape { shape: *shape_kind };
                        drop(model);
                        self.after_edit();
                        return;
                    }
                }
                let id = model.alloc_id();
                let mut block = DocBlock::new(id, BlockKind::Shape { shape: *shape_kind });
                if !shape_kind.is_line() {
                    free::set_height(&mut block.attrs, shape::DEFAULT_H);
                }
                edit::with_siblings(&mut model.blocks, pos, &mut |sibs, idx| {
                    sibs.insert(idx + 1, block.clone());
                });
                drop(model);
                self.object_sel = Some(id);
                self.after_edit();
            }
            SlashAction::Custom(id) => {
                if let Some(cb) = &self.on_slash_custom {
                    cb(id);
                }
                self.after_edit();
            }
        }
    }

    /// Очередь операций хоста ([`DocumentEditorHandle::queue_op`]).
    fn apply_pending_ops(&mut self, ctx: &mut UpdateContext) {
        let ops: Vec<DocOp> =
            std::mem::take(&mut *self.ops.lock().unwrap_or_else(|e| e.into_inner()));
        if ops.is_empty() || self.read_only {
            return;
        }
        for op in ops {
            self.apply_op(op);
        }
        ctx.mark_layout_dirty();
    }

    fn apply_op(&mut self, op: DocOp) {
        // В свободной раскладке вставка из контекстного меню идёт в точку
        // правого клика, а не «после блока каретки».
        let at = self.layout.free.then(|| self.menu_pos).flatten();
        self.menu_pos = None;
        match op {
            DocOp::InsertMarkdown(md) => {
                if let Some(at) = at {
                    self.insert_free_markdown(&md, at);
                } else {
                    self.insert_markdown_at_caret(&md);
                }
            }
            DocOp::TurnInto(action) => {
                // У фигуры каретки нет — она выбрана как объект; уводить
                // каретку в последний блок в этом случае нельзя.
                if self.caret().is_none() && self.object_sel.is_none() {
                    self.caret_to_last_block();
                }
                if self.caret().is_none() && self.object_sel.is_none() {
                    return;
                }
                self.checkpoint(EditClass::Structure);
                self.apply_action(action);
            }
            DocOp::InsertBlock(action) => {
                if let Some(at) = at {
                    self.insert_free_block(action, at);
                    return;
                }
                if self.caret().is_none() && self.object_sel.is_none() {
                    self.caret_to_last_block();
                }
                self.checkpoint(EditClass::Structure);
                let before = self.top_ids();
                let anchor = self.caret().map(|c| c.block).or(self.object_sel);
                let target = {
                    let mut model = self.model();
                    let reuse = anchor.and_then(|id| {
                        edit::find_block(&model.blocks, id).map(|b| {
                            matches!(&b.kind, BlockKind::Paragraph(t) if t.text().is_empty())
                        })
                    });
                    match (anchor, reuse) {
                        (Some(id), Some(true)) => Some(id),
                        (Some(id), _) => {
                            let new_id = model.alloc_id();
                            let par = DocBlock::new(new_id, BlockKind::Paragraph(InlineText::default()));
                            let mut slot = Some(par);
                            edit::with_siblings(&mut model.blocks, id, &mut |sibs, idx| {
                                if let Some(p) = slot.take() {
                                    sibs.insert(idx + 1, p);
                                }
                            });
                            Some(new_id)
                        }
                        (None, _) => {
                            let new_id = model.alloc_id();
                            model.blocks.push(DocBlock::new(
                                new_id,
                                BlockKind::Paragraph(InlineText::default()),
                            ));
                            Some(new_id)
                        }
                    }
                };
                if let Some(id) = target {
                    if let Some(anchor) = anchor {
                        self.pin_below(anchor, id);
                    }
                    self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }));
                    self.table_caret = None;
                    self.code_caret = None;
                    self.apply_action(action);
                    if self.layout.free {
                        self.settle_inserted(&before, None, anchor.or(Some(id)));
                    }
                }
            }
            DocOp::Duplicate => self.duplicate_current(),
            DocOp::Delete => self.delete_current(),
            DocOp::DeleteBlock(id) => self.delete_block(id),
            DocOp::InsertMarkdownAt { at: point, md } => self.insert_markdown_at_point(&md, point),
            DocOp::Move { down } => self.move_current(down),
            DocOp::Select(id) => self.select_block(id),
            // Кнопки истории хоста: клик по ним увёл фокус приложения с
            // редактора — просим обратно, чтобы восстановленная каретка
            // была видна и набор продолжился.
            DocOp::Undo => {
                self.undo();
                self.focused = true;
                self.focus_request_pending = true;
            }
            DocOp::Redo => {
                self.redo();
                self.focused = true;
                self.focus_request_pending = true;
            }
            DocOp::Copy => {
                self.copy_blocks();
            }
            DocOp::Cut => self.cut_blocks(),
            DocOp::Paste => {
                self.paste_blocks();
                self.focused = true;
                self.focus_request_pending = true;
            }
            DocOp::SelectBlocks(ids) => self.select_blocks(ids),
            DocOp::SetAttr { block, key, value } => {
                self.checkpoint(EditClass::Structure);
                let changed = {
                    let mut model = self.model();
                    match edit::find_block_mut(&mut model.blocks, block) {
                        Some(b) => {
                            props::set(&mut b.attrs, &key, value.as_deref());
                            true
                        }
                        None => false,
                    }
                };
                if changed {
                    self.after_edit();
                } else {
                    self.history().discard_last_checkpoint();
                }
            }
            DocOp::Table { block, op } => {
                self.checkpoint(EditClass::Structure);
                let at = self
                    .table_caret
                    .filter(|tc| tc.block == block)
                    .map(|tc| (tc.row, tc.col))
                    .unwrap_or((0, 0));
                let changed = {
                    let mut model = self.model();
                    edit::table_op(&mut model, block, op, at)
                };
                if changed {
                    self.after_edit();
                } else {
                    self.history().discard_last_checkpoint();
                }
            }
        }
    }

    /// Сделать блок текущим: каретка внутрь него в его режиме.
    fn select_block(&mut self, id: super::model::BlockId) {
        let exists = {
            let model = self.model();
            edit::find_block(&model.blocks, id).is_some()
        };
        if !exists {
            return;
        }
        self.caret_into(id);
        self.focused = true;
        // Выбор пришёл извне (панель блоков хоста): клик по панели снял
        // фокус приложения с редактора — просим его обратно, иначе редактор
        // «фокусирован» сам по себе и делит клавиатуру со следующим вводом.
        self.focus_request_pending = true;
        self.caret_on = true;
        self.blink_ms = 0.0;
        self.publish_selection();
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Блок, который считается текущим для панели свойств.
    fn current_block(&self) -> Option<super::model::BlockId> {
        self.code_caret
            .map(|c| c.block)
            .or_else(|| self.table_caret.map(|t| t.block))
            .or_else(|| self.caret().map(|c| c.block))
            .or(self.object_sel)
            .or_else(|| self.block_sel.first().copied())
    }

    /// Обновить сигнал текущего блока в ручке (панель свойств хоста).
    fn publish_selection(&mut self) {
        let Some(sig) = self.selected else { return };
        let now = self.current_block();
        if sig.get_untracked() != now {
            sig.set(now);
        }
    }

    /// Каретка в конец последнего блока (операции без каретки).
    fn caret_to_last_block(&mut self) {
        let target = {
            let model = self.model();
            model.blocks.last().map(|b| (b.id, edit::block_text_len(&model, b.id)))
        };
        if let Some((id, len)) = target {
            self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: len }));
        }
    }

    /// Вставка распарсенного фрагмента после блока каретки; пустой
    /// параграф под кареткой заменяется. Каретка — в первый текстовый
    /// блок фрагмента.
    fn top_ids(&self) -> Vec<super::model::BlockId> {
        self.model().blocks.iter().map(|b| b.id).collect()
    }

    fn is_empty_paragraph(&self, id: super::model::BlockId) -> bool {
        let model = self.model();
        model
            .blocks
            .iter()
            .find(|b| b.id == id)
            .map(|b| matches!(&b.kind, BlockKind::Paragraph(t) if t.text().is_empty()))
            .unwrap_or(false)
    }

    fn remove_top_block(&mut self, id: super::model::BlockId) {
        self.model().blocks.retain(|b| b.id != id);
    }

    /// Каретка внутрь только что вставленного блока — в его собственном
    /// режиме (таблица и код своих текстовых строк документа не имеют).
    fn caret_into(&mut self, id: super::model::BlockId) {
        let kind = {
            let model = self.model();
            model.blocks.iter().find(|b| b.id == id).map(|b| match &b.kind {
                BlockKind::Table { .. } => 1u8,
                BlockKind::CodeBlock { .. } => 2,
                k if k.text().is_some() => 3,
                _ => 0,
            })
        };
        self.selection = None;
        self.table_caret = None;
        self.code_caret = None;
        self.object_sel = None;
        self.drop_block_sel();
        match kind {
            Some(1) => self.table_caret = Some(TableCaret { block: id, row: 0, col: 0, offset: 0 }),
            Some(2) => self.code_caret = Some(CodeCaret { block: id, offset: 0 }),
            Some(3) => {
                self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }))
            }
            // Фигура и разделитель: каретке некуда встать — блок просто
            // становится текущим.
            _ => self.object_sel = Some(id),
        }
    }

    /// Разложить блоки, появившиеся от действия вставки: первый — в точку
    /// (или под якорь), остальные — лесенкой под ним.
    ///
    /// Действие могло не превратить блок-каркас, а **добавить свой рядом**
    /// (так делает таблица, и так же код добавляет параграф следом) — тогда
    /// координаты остались бы на пустом каркасе, а сама таблица уехала бы в
    /// колонку потока в углу холста.
    fn settle_inserted(
        &mut self,
        before: &[super::model::BlockId],
        at: Option<Point>,
        anchor: Option<super::model::BlockId>,
    ) {
        let mut fresh: Vec<super::model::BlockId> =
            self.top_ids().into_iter().filter(|id| !before.contains(id)).collect();
        // Пустые параграфы вокруг — служебные: свой каркас до действия и
        // «строка после» от шорткатов кода и разделителя. Если действие
        // дало настоящий блок, они только мусорят на холсте.
        if fresh.iter().any(|id| !self.is_empty_paragraph(*id)) {
            let junk: Vec<super::model::BlockId> =
                fresh.iter().copied().filter(|id| self.is_empty_paragraph(*id)).collect();
            for id in &junk {
                self.remove_top_block(*id);
            }
            fresh.retain(|id| !junk.contains(id));
        }
        let Some(&first) = fresh.first() else { return };
        match (at, anchor) {
            (Some(at), _) => self.place_free_block(first, at),
            (None, Some(anchor)) => self.pin_below(anchor, first),
            _ => {}
        }
        let mut prev = first;
        for id in fresh.iter().skip(1) {
            self.pin_below(prev, *id);
            prev = *id;
        }
        self.caret_into(first);
    }

    /// Новый блок в точке холста: верхним уровнем, с кареткой внутри.
    fn insert_free_block(&mut self, action: SlashAction, at: Point) {
        self.checkpoint(EditClass::Structure);
        let before = self.top_ids();
        let id = {
            let mut model = self.model();
            let id = model.alloc_id();
            model.blocks.push(DocBlock::new(id, BlockKind::Paragraph(InlineText::default())));
            id
        };
        self.place_free_block(id, at);
        self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }));
        self.table_caret = None;
        self.code_caret = None;
        self.apply_action(action);
        self.settle_inserted(&before, Some(at), None);
        self.after_edit();
    }

    /// Markdown-фрагмент в точке холста (врезки базы/канваса из меню).
    fn insert_free_markdown(&mut self, md: &str, at: Point) {
        let fragment = parse_document(md);
        if fragment.blocks.is_empty() {
            return;
        }
        self.checkpoint(EditClass::Structure);
        let first = {
            let mut model = self.model();
            let mut blocks = fragment.blocks;
            remap_ids(&mut blocks, &mut model);
            let first = blocks.first().map(|b| (b.id, b.kind.text().is_some()));
            model.blocks.extend(blocks);
            first
        };
        if let Some((id, has_text)) = first {
            self.place_free_block(id, at);
            if has_text {
                self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }));
            }
        }
        self.after_edit();
    }

    fn insert_markdown_at_caret(&mut self, md: &str) {
        let fragment = parse_document(md);
        if fragment.blocks.is_empty() {
            return;
        }
        self.checkpoint(EditClass::Structure);
        // Якорь — текущий блок: под кареткой либо выбранный объект
        // (фигура, доска): у них каретки нет, но вставка «после» нужна.
        let anchor = self.target_block().map(|(b, _)| b);
        let first = {
            let mut model = self.model();
            let mut blocks = fragment.blocks;
            remap_ids(&mut blocks, &mut model);
            let first = blocks.first().map(|b| (b.id, b.kind.text().is_some()));
            let mut slot = Some(blocks);
            let inserted = anchor.and_then(|id| {
                edit::with_siblings(&mut model.blocks, id, &mut |sibs, idx| {
                    let Some(blocks) = slot.take() else { return };
                    let empty_par = matches!(&sibs[idx].kind, BlockKind::Paragraph(t) if t.text().is_empty());
                    let at = if empty_par {
                        sibs.remove(idx);
                        idx
                    } else {
                        idx + 1
                    };
                    for (i, b) in blocks.into_iter().enumerate() {
                        sibs.insert(at + i, b);
                    }
                })
            });
            if inserted.is_none() {
                if let Some(blocks) = slot.take() {
                    model.blocks.extend(blocks);
                }
            }
            first
        };
        if let Some((id, has_text)) = first {
            if let Some(anchor) = anchor {
                self.pin_below(anchor, id);
            }
            if has_text {
                self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }));
            }
        }
        self.after_edit();
    }

    /// Блок, над которым работают операции меню: под кареткой либо
    /// выбранный объект (у фигуры каретки нет).
    fn target_block(&self) -> Option<(super::model::BlockId, usize)> {
        match self.caret() {
            Some(pos) => Some((pos.block, pos.offset)),
            None => self.current_block().map(|id| (id, 0)),
        }
    }

    fn duplicate_current(&mut self) {
        let Some(pos) = self.target_block().map(|(block, offset)| CaretPos { block, offset }) else {
            return;
        };
        self.checkpoint(EditClass::Structure);
        let new_id = {
            let mut model = self.model();
            let mut copy: Option<DocBlock> = None;
            edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                copy = Some(sibs[idx].clone());
            });
            let Some(dup) = copy else { return };
            let mut v = vec![dup];
            remap_ids(&mut v, &mut model);
            let new_id = v[0].id;
            let mut slot = v.pop();
            edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                if let Some(d) = slot.take() {
                    sibs.insert(idx + 1, d);
                }
            });
            new_id
        };
        if self.layout.free {
            // Иначе копия ложится ровно на оригинал и выглядит пропажей.
            let step = self.layout.grid_step_px();
            let mut model = self.model();
            if let Some(b) = model.blocks.iter_mut().find(|b| b.id == new_id) {
                if let Some((x, y)) = free::pos_of(&b.attrs) {
                    free::set_pos(&mut b.attrs, x + step, y + step);
                }
            }
        }
        let len = edit::block_text_len(&self.model(), new_id);
        self.selection =
            Some(DocSelection::caret(CaretPos { block: new_id, offset: pos.offset.min(len) }));
        self.after_edit();
    }

    /// Хост забирает блок себе (дроп в карточку доски): `true` — блок
    /// удалён из документа.
    fn host_took_block(&mut self, at: Point, block: super::model::BlockId) -> bool {
        let Some(cb) = self.on_block_drop.clone() else { return false };
        let Some(top) = self.top_level_of(block) else { return false };
        if !cb(at, top) {
            return false;
        }
        self.delete_block(top);
        true
    }

    /// Удалить блок по id; последний блок документа заменяется пустым
    /// параграфом. Каретка и выбор, стоявшие в нём, снимаются.
    fn delete_block(&mut self, id: super::model::BlockId) {
        self.checkpoint(EditClass::Structure);
        let removed = {
            let mut model = self.model();
            let removed = edit::with_siblings(&mut model.blocks, id, &mut |sibs, idx| {
                sibs.remove(idx);
            })
            .is_some();
            if removed && model.blocks.is_empty() {
                let nid = model.alloc_id();
                model.blocks.push(DocBlock::new(nid, BlockKind::Paragraph(InlineText::default())));
            }
            removed
        };
        if !removed {
            self.history().discard_last_checkpoint();
            return;
        }
        if self.selection.as_ref().map(|s| s.head.block == id || s.anchor.block == id).unwrap_or(false) {
            self.selection = None;
        }
        if self.table_caret.as_ref().map(|t| t.block == id).unwrap_or(false) {
            self.table_caret = None;
        }
        if self.code_caret.as_ref().map(|c| c.block == id).unwrap_or(false) {
            self.code_caret = None;
        }
        if self.object_sel == Some(id) {
            self.object_sel = None;
        }
        self.after_edit();
    }

    /// Вставка markdown в точку: на холсте — ровно туда, в потоке — у блока
    /// под точкой (без него — в конец).
    fn insert_markdown_at_point(&mut self, md: &str, at: Point) {
        if self.layout.free {
            self.insert_free_markdown(md, at);
            return;
        }
        match self.hit_caret(at) {
            Some(pos) => self.selection = Some(DocSelection::caret(pos)),
            None => self.caret_to_last_block(),
        }
        self.table_caret = None;
        self.code_caret = None;
        self.object_sel = None;
        self.insert_markdown_at_caret(md);
    }

    fn delete_current(&mut self) {
        let Some(pos) = self.target_block().map(|(block, offset)| CaretPos { block, offset }) else {
            return;
        };
        self.checkpoint(EditClass::Structure);
        let next = {
            let mut model = self.model();
            let neighbour = edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                sibs.remove(idx);
                if sibs.is_empty() {
                    None
                } else {
                    Some(sibs[idx.saturating_sub(1).min(sibs.len() - 1)].id)
                }
            })
            .flatten();
            if model.blocks.is_empty() {
                let id = model.alloc_id();
                model.blocks.push(DocBlock::new(id, BlockKind::Paragraph(InlineText::default())));
                Some(id)
            } else {
                neighbour
            }
        };
        self.selection = next.and_then(|id| {
            let model = self.model();
            let has_text = edit::find_block(&model.blocks, id).and_then(|b| b.kind.text()).is_some();
            let len = edit::block_text_len(&model, id);
            has_text.then(|| DocSelection::caret(CaretPos { block: id, offset: len }))
        });
        self.table_caret = None;
        self.after_edit();
    }

    fn move_current(&mut self, down: bool) {
        let Some(pos) = self.target_block().map(|(block, offset)| CaretPos { block, offset }) else {
            return;
        };
        self.checkpoint(EditClass::Structure);
        // В свободной раскладке перестановка соседей ничего не меняет
        // визуально — двигаем блок по холсту на шаг сетки.
        if self.layout.free {
            let step = self.layout.grid_step_px() * if down { 1.0 } else { -1.0 };
            let target = self.top_level_of(pos.block);
            let moved = target
                .map(|id| {
                    let mut model = self.model();
                    match model.blocks.iter_mut().find(|b| b.id == id) {
                        Some(b) => match free::pos_of(&b.attrs) {
                            Some((x, y)) => {
                                free::set_pos(&mut b.attrs, x, (y + step).max(0.0));
                                true
                            }
                            None => false,
                        },
                        None => false,
                    }
                })
                .unwrap_or(false);
            if moved {
                self.after_edit();
            } else {
                self.history().discard_last_checkpoint();
            }
            return;
        }
        let moved = {
            let mut model = self.model();
            edit::with_siblings(&mut model.blocks, pos.block, &mut |sibs, idx| {
                if down {
                    if idx + 1 < sibs.len() {
                        sibs.swap(idx, idx + 1);
                        true
                    } else {
                        false
                    }
                } else if idx > 0 {
                    sibs.swap(idx, idx - 1);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false)
        };
        if !moved {
            self.history().discard_last_checkpoint();
            return;
        }
        self.after_edit();
    }

    /// Смена типа текущего текстового блока с сохранением текста.
    fn convert_current(&mut self, make: impl Fn(InlineText) -> BlockKind) {
        let Some(pos) = self.caret() else { return };
        {
            let mut model = self.model();
            if let Some(block) = edit::find_block_mut(&mut model.blocks, pos.block) {
                if let Some(text_ref) = block.kind.text_mut() {
                    let text = std::mem::take(text_ref);
                    block.kind = make(text);
                }
            }
        }
        self.after_edit();
    }

    /// Переключение инлайн-стиля выделения (тулбар и Ctrl+B/I/E/Shift+S).
    fn toggle_inline(
        &mut self,
        pred: fn(&InlineStyle) -> bool,
        apply: fn(&mut InlineStyle, bool),
    ) {
        let Some(sel) = self.selection else { return };
        if sel.is_caret() {
            return;
        }
        self.checkpoint(EditClass::Structure);
        let mut model = self.model();
        let order = BlockOrder::of(&model);
        let (start, end) = sel.ordered(&order);
        let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else { return };

        let portion = |model: &DocModel, i: usize| -> (usize, usize) {
            let id = order.ids[i];
            let len = edit::block_text_len(model, id);
            let lo = if i == si { start.offset.min(len) } else { 0 };
            let hi = if i == ei { end.offset.min(len) } else { len };
            (lo, hi)
        };

        // Фаза 1: весь диапазон уже стилизован?
        let mut all = true;
        for i in si..=ei {
            let (lo, hi) = portion(&model, i);
            if hi <= lo {
                continue;
            }
            let styled = edit::find_block(&model.blocks, order.ids[i])
                .and_then(|b| b.kind.text())
                .map(|t| edit::range_has_style(t, lo, hi, &pred))
                .unwrap_or(true);
            if !styled {
                all = false;
                break;
            }
        }
        let target = !all;
        for i in si..=ei {
            let (lo, hi) = portion(&model, i);
            if hi <= lo {
                continue;
            }
            if let Some(t) = edit::find_block_mut(&mut model.blocks, order.ids[i])
                .and_then(|b| b.kind.text_mut())
            {
                edit::style_range(t, lo, hi, &|s| apply(s, target));
            }
        }
        drop(model);
        self.after_edit();
    }

    /// Фиксация правки: перестройка детей, ревизия, колбэк.
    fn after_edit(&mut self) {
        self.publish_selection();
        self.publish_history();
        self.rebuild = true;
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        self.caret_on = true;
        self.blink_ms = 0.0;
        if let Some(rev) = self.revision {
            rev.set(rev.get_untracked() + 1);
        }
        if let Some(cb) = &self.on_change {
            cb();
        }
    }

    // ─── Выделение блоков ──────────────────────────────────────────────────

    /// Верхнеуровневый блок под точкой — по опубликованным прямоугольникам
    /// (`BlockRectMap`), без допусков: для выделения нужен именно блок под
    /// курсором, а не ближайший (как у `row_at` для ручки ⋮⋮).
    fn top_block_at(&self, p: Point) -> Option<super::model::BlockId> {
        let map = self.blocks.lock().ok()?;
        map.iter().find(|(_, r)| r.contains(p)).map(|(id, _)| *id)
    }

    fn top_order(&self) -> Vec<super::model::BlockId> {
        self.model().blocks.iter().map(|b| b.id).collect()
    }

    fn publish_block_sel(&mut self) {
        let Some(sig) = self.block_sel_sig else { return };
        if sig.get_untracked() != self.block_sel {
            sig.set(self.block_sel.clone());
        }
    }

    /// Снять выделение блоков тихо (каретка встаёт — режим блоков кончился).
    fn drop_block_sel(&mut self) {
        if !self.block_sel.is_empty() {
            self.block_sel.clear();
            self.publish_block_sel();
        }
        self.block_anchor = None;
    }

    fn clear_block_sel(&mut self) {
        if !self.block_sel.is_empty() {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        self.drop_block_sel();
    }

    /// Выделить блоки (id верхнего уровня; порядок берётся из документа,
    /// чужие id отбрасываются). Каретка и выбор объекта снимаются —
    /// выделение блоков живёт отдельным режимом.
    fn select_blocks(&mut self, ids: Vec<super::model::BlockId>) {
        let sel: Vec<super::model::BlockId> =
            self.top_order().into_iter().filter(|id| ids.contains(id)).collect();
        if !sel.is_empty() {
            self.selection = None;
            self.table_caret = None;
            self.code_caret = None;
            self.object_sel = None;
            self.mouse_selecting = false;
            if self.block_anchor.map_or(true, |a| !sel.contains(&a)) {
                self.block_anchor = sel.first().copied();
            }
        } else {
            self.block_anchor = None;
        }
        self.block_sel = sel;
        self.publish_block_sel();
        self.publish_selection();
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Ctrl+клик: добавить/убрать блок; он же становится якорем диапазона.
    fn toggle_block_sel(&mut self, id: super::model::BlockId) {
        let mut ids = self.block_sel.clone();
        match ids.iter().position(|b| *b == id) {
            Some(i) => {
                ids.remove(i);
            }
            None => ids.push(id),
        }
        self.select_blocks(ids);
        self.block_anchor = Some(id);
    }

    /// Shift+клик: диапазон от якоря до блока в порядке документа.
    fn select_block_range(&mut self, to: super::model::BlockId) {
        let order = self.top_order();
        let from = self.block_anchor.or_else(|| self.block_sel.first().copied()).unwrap_or(to);
        let (Some(a), Some(b)) =
            (order.iter().position(|x| *x == from), order.iter().position(|x| *x == to))
        else {
            self.select_blocks(vec![to]);
            return;
        };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.select_blocks(order[lo..=hi].to_vec());
        self.block_anchor = Some(from);
    }

    /// Блоки, которых касается прямоугольник рамки.
    fn blocks_in_rect(&self, r: Rect) -> Vec<super::model::BlockId> {
        let Ok(map) = self.blocks.lock() else { return Vec::new() };
        map.iter().filter(|(_, b)| rects_intersect(r, **b)).map(|(id, _)| *id).collect()
    }

    /// Markdown блоков для буфера: выделенные, без выделения — текущий
    /// (верхнего уровня). Блоки разделены пустой строкой.
    fn selection_blocks_markdown(&self) -> Option<(Vec<super::model::BlockId>, String)> {
        let ids: Vec<super::model::BlockId> = if !self.block_sel.is_empty() {
            self.block_sel.clone()
        } else {
            self.target_block().and_then(|(b, _)| self.top_level_of(b)).into_iter().collect()
        };
        if ids.is_empty() {
            return None;
        }
        let model = self.model();
        let mut out = String::new();
        for id in &ids {
            if let Some(b) = model.blocks.iter().find(|b| b.id == *id) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&super::serialize::block_markdown(b));
            }
        }
        Some((ids, out))
    }

    fn copy_blocks(&mut self) -> bool {
        let Some((_, md)) = self.selection_blocks_markdown() else { return false };
        crate::clipboard::copy(&md);
        true
    }

    fn cut_blocks(&mut self) {
        let Some((ids, md)) = self.selection_blocks_markdown() else { return };
        crate::clipboard::copy(&md);
        self.delete_blocks(ids);
    }

    /// Удалить блоки верхнего уровня одним шагом истории; каретка — в
    /// блок над первым удалённым (или в первый оставшийся).
    fn delete_blocks(&mut self, ids: Vec<super::model::BlockId>) {
        if ids.is_empty() {
            return;
        }
        self.checkpoint(EditClass::Structure);
        let next = {
            let mut model = self.model();
            let first_idx = model.blocks.iter().position(|b| ids.contains(&b.id)).unwrap_or(0);
            model.blocks.retain(|b| !ids.contains(&b.id));
            if model.blocks.is_empty() {
                let id = model.alloc_id();
                model.blocks.push(DocBlock::new(id, BlockKind::Paragraph(InlineText::default())));
            }
            let idx = first_idx.saturating_sub(1).min(model.blocks.len() - 1);
            model.blocks[idx].id
        };
        self.block_sel.clear();
        self.block_anchor = None;
        self.publish_block_sel();
        self.caret_into(next);
        if let Some(sel) = self.selection {
            let len = edit::block_text_len(&self.model(), sel.head.block);
            self.selection = Some(DocSelection::caret(CaretPos { block: sel.head.block, offset: len }));
        }
        self.after_edit();
    }

    /// Ctrl+V: однострочный текст — в строку у каретки; многострочный (или
    /// без каретки — над фигурой/доской) — блоками markdown, так что
    /// структура из другого редактора и наши копии блоков сохраняются.
    fn paste_text(&mut self, text: &str) {
        let text = text.replace('\r', "");
        let multi = text.trim_end_matches('\n').contains('\n');
        if multi || self.caret().is_none() {
            self.insert_markdown_at_caret(&text);
        } else {
            self.checkpoint(EditClass::Structure);
            self.paste(&text);
        }
    }

    /// Вставка из буфера: после выделенных блоков, иначе как Ctrl+V.
    fn paste_blocks(&mut self) {
        let Some(text) = crate::clipboard::paste() else { return };
        if text.trim().is_empty() {
            return;
        }
        if let Some(last) = self.block_sel.last().copied() {
            self.block_sel.clear();
            self.block_anchor = None;
            self.publish_block_sel();
            self.selection = None;
            self.table_caret = None;
            self.code_caret = None;
            self.object_sel = Some(last);
            self.insert_markdown_at_caret(&text);
            return;
        }
        self.paste_text(&text);
    }

    // ─── Геометрия ──────────────────────────────────────────────────────────

    /// X-координата смещения внутри строки (относительно origin строки).
    fn x_of_offset(&self, row: &RowGeom, line_idx: usize, offset: usize) -> f32 {
        let Some(line) = row.lines.get(line_idx) else { return row.gutter };
        let Some(tm) = self.tm.as_deref() else { return row.gutter };
        for seg in &line.segs {
            if offset <= seg.abs_start {
                return seg.x;
            }
            if offset <= seg.abs_end() {
                let local = &seg.text[..offset - seg.abs_start];
                return seg.x
                    + tm.measure_text_width_styled(
                        local,
                        seg.font_size,
                        local.chars().count(),
                        seg.bold,
                        None,
                    );
            }
        }
        line.segs.last().map(|s| s.x + s.width).unwrap_or(row.gutter)
    }

    /// Прямоугольник каретки в абсолютных координатах.
    fn caret_rect(&self, pos: CaretPos) -> Option<Rect> {
        let map = self.geom.lock().ok()?;
        let row = map.get(&pos.block)?;
        let line_idx = row.line_of_offset(pos.offset);
        let x = self.x_of_offset(row, line_idx, pos.offset);
        let y = row.lines.get(line_idx).map(|l| l.y).unwrap_or(0.0);
        Some(Rect::new(
            Point::new(row.origin.x + x, row.origin.y + y + 1.0),
            Size::new(2.0, (row.line_h - 2.0).max(4.0)),
        ))
    }

    /// Хит-тест позиции мыши в позицию каретки.
    fn hit_caret(&self, p: Point) -> Option<CaretPos> {
        let map = self.geom.lock().ok()?;
        let tm = self.tm.as_deref()?;
        // Ближайшая по вертикали строка среди всех блоков.
        let mut best: Option<(f32, super::model::BlockId, usize)> = None;
        for (id, row) in map.iter() {
            if row.lines.is_empty() {
                let dy = dist(p.y, row.origin.y, row.origin.y + row.line_h);
                if best.map(|(d, _, _)| dy < d).unwrap_or(true) {
                    best = Some((dy, *id, 0));
                }
                continue;
            }
            for (li, line) in row.lines.iter().enumerate() {
                let top = row.origin.y + line.y;
                let dy = dist(p.y, top, top + row.line_h);
                if best.map(|(d, _, _)| dy < d).unwrap_or(true) {
                    best = Some((dy, *id, li));
                }
            }
        }
        let (_, block, line_idx) = best?;
        let row = map.get(&block)?;
        let Some(line) = row.lines.get(line_idx) else {
            return Some(CaretPos { block, offset: 0 });
        };
        let x = p.x - row.origin.x;
        // Сегмент под курсором (или ближайший).
        let mut offset = line.segs.first().map(|s| s.abs_start).unwrap_or(0);
        for seg in &line.segs {
            if x < seg.x {
                break;
            }
            if x <= seg.x + seg.width {
                let local_x = x - seg.x;
                let ci = tm.hit_test_char_styled(&seg.text, seg.font_size, local_x, None);
                let byte = seg
                    .text
                    .char_indices()
                    .nth(ci)
                    .map(|(b, _)| b)
                    .unwrap_or(seg.text.len());
                return Some(CaretPos { block, offset: seg.abs_start + byte });
            }
            offset = seg.abs_end();
        }
        Some(CaretPos { block, offset })
    }

    /// Клик по гаттеру строки: чекбокс / шеврон toggle.
    fn gutter_hit(&self, p: Point) -> Option<(super::model::BlockId, GutterAction)> {
        let map = self.geom.lock().ok()?;
        let model = self.model();
        for (id, row) in map.iter() {
            if row.gutter <= 0.0 {
                continue;
            }
            let hit = p.x >= row.origin.x
                && p.x < row.origin.x + row.gutter
                && p.y >= row.origin.y
                && p.y < row.origin.y + row.line_h;
            if hit {
                let action =
                    edit::find_block(&model.blocks, *id).and_then(|b| gutter_action(&b.kind));
                if let Some(a) = action {
                    return Some((*id, a));
                }
            }
        }
        None
    }

    // ─── Навигация ──────────────────────────────────────────────────────────

    fn caret(&self) -> Option<CaretPos> {
        self.selection.map(|s| s.head)
    }

    fn set_caret(&mut self, pos: CaretPos, extend: bool) {
        self.object_sel = None;
        self.drop_block_sel();
        self.selection = Some(match (self.selection, extend) {
            (Some(sel), true) => DocSelection { anchor: sel.anchor, head: pos },
            _ => DocSelection::caret(pos),
        });
        self.caret_on = true;
        self.blink_ms = 0.0;
        self.publish_selection();
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn move_horizontal(&mut self, dir: i32, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let model = self.model();
        let order = BlockOrder::of(&model);
        let text = edit::find_block(&model.blocks, pos.block)
            .and_then(|b| b.kind.text())
            .map(|t| t.text())
            .unwrap_or_default();
        let new = if dir < 0 {
            if pos.offset > 0 {
                CaretPos { block: pos.block, offset: edit::prev_char_boundary(&text, pos.offset) }
            } else if let Some(prev) = order.prev(pos.block) {
                CaretPos { block: prev, offset: edit::block_text_len(&model, prev) }
            } else {
                pos
            }
        } else if pos.offset < text.len() {
            CaretPos { block: pos.block, offset: edit::next_char_boundary(&text, pos.offset) }
        } else if let Some(next) = order.next(pos.block) {
            CaretPos { block: next, offset: 0 }
        } else {
            pos
        };
        drop(model);
        self.goal_x = None;
        self.set_caret(new, extend);
    }

    fn move_vertical(&mut self, dir: i32, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let Some(rect) = self.caret_rect(pos) else { return };
        let goal_x = self.goal_x.unwrap_or(rect.origin.x);
        self.goal_x = Some(goal_x);

        let target = {
            let map = match self.geom.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let model = self.model();
            let order = BlockOrder::of(&model);
            let row = map.get(&pos.block);
            let line_idx = row.map(|r| r.line_of_offset(pos.offset)).unwrap_or(0);
            let lines_n = row.map(|r| r.lines.len().max(1)).unwrap_or(1);

            if dir < 0 && line_idx > 0 {
                Some((pos.block, line_idx - 1))
            } else if dir > 0 && line_idx + 1 < lines_n {
                Some((pos.block, line_idx + 1))
            } else {
                let neighbor =
                    if dir < 0 { order.prev(pos.block) } else { order.next(pos.block) };
                neighbor.map(|nb| {
                    let n_lines = map.get(&nb).map(|r| r.lines.len().max(1)).unwrap_or(1);
                    (nb, if dir < 0 { n_lines - 1 } else { 0 })
                })
            }
        };
        let Some((block, line_idx)) = target else { return };
        // Смещение в целевой строке по goal_x.
        let new = {
            let map = match self.geom.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let Some(row) = map.get(&block) else { return };
            let y = row.origin.y + row.lines.get(line_idx).map(|l| l.y).unwrap_or(0.0)
                + row.line_h / 2.0;
            Point::new(goal_x, y)
        };
        if let Some(new_pos) = self.hit_caret(new) {
            self.set_caret(new_pos, extend);
        }
    }

    fn move_line_edge(&mut self, end: bool, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let map = match self.geom.lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        let Some(row) = map.get(&pos.block) else { return };
        let li = row.line_of_offset(pos.offset);
        let Some(line) = row.lines.get(li) else { return };
        let offset = if end {
            line.segs.last().map(|s| s.abs_end()).unwrap_or(0)
        } else {
            line.segs.first().map(|s| s.abs_start).unwrap_or(0)
        };
        drop(map);
        self.goal_x = None;
        self.set_caret(CaretPos { block: pos.block, offset }, extend);
    }

    fn select_all(&mut self) {
        let model = self.model();
        let order = BlockOrder::of(&model);
        let (Some(first), Some(last)) = (order.ids.first(), order.ids.last()) else { return };
        let sel = DocSelection {
            anchor: CaretPos { block: *first, offset: 0 },
            head: CaretPos { block: *last, offset: edit::block_text_len(&model, *last) },
        };
        drop(model);
        self.selection = Some(sel);
        self.table_caret = None;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Плоский текст выделения (для клипборда).
    fn selection_text(&self) -> String {
        let Some(sel) = self.selection else { return String::new() };
        if sel.is_caret() {
            return String::new();
        }
        let model = self.model();
        let order = BlockOrder::of(&model);
        let (start, end) = sel.ordered(&order);
        let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else {
            return String::new();
        };
        let mut out = String::new();
        for i in si..=ei {
            let id = order.ids[i];
            let text = edit::find_block(&model.blocks, id)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let lo = if i == si { start.offset.min(text.len()) } else { 0 };
            let hi = if i == ei { end.offset.min(text.len()) } else { text.len() };
            if i > si {
                out.push('\n');
            }
            out.push_str(&text[lo..hi]);
        }
        out
    }

    // ─── Правки ─────────────────────────────────────────────────────────────

    fn delete_selection_if_any(&mut self) -> bool {
        let Some(sel) = self.selection else { return false };
        if sel.is_caret() {
            return false;
        }
        let mut model = self.model();
        let order = BlockOrder::of(&model);
        let caret = edit::delete_selection(&mut model, &order, sel);
        drop(model);
        self.selection = Some(DocSelection::caret(caret));
        self.after_edit();
        true
    }

    fn insert_str(&mut self, s: &str) {
        self.delete_selection_if_any();
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = edit::insert_text(&mut model, pos, s);
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn paste(&mut self, text: &str) {
        let text = text.replace('\r', "");
        let mut lines = text.split('\n');
        if let Some(first) = lines.next() {
            self.insert_str(first);
        }
        for line in lines {
            self.enter(false);
            if !line.is_empty() {
                self.insert_str(line);
            }
        }
    }

    fn backspace(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = if pos.offset == 0 {
            let order = BlockOrder::of(&model);
            edit::backspace_at_start(&mut model, &order, pos)
        } else {
            let text = edit::find_block(&model.blocks, pos.block)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let start = edit::prev_char_boundary(&text, pos.offset);
            if let Some(t) =
                edit::find_block_mut(&mut model.blocks, pos.block).and_then(|b| b.kind.text_mut())
            {
                edit::text_delete(t, start, pos.offset);
            }
            CaretPos { block: pos.block, offset: start }
        };
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn delete_forward(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let len = edit::block_text_len(&model, pos.block);
        let new = if pos.offset >= len {
            edit::delete_at_end(&mut model, pos)
        } else {
            let text = edit::find_block(&model.blocks, pos.block)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let end = edit::next_char_boundary(&text, pos.offset);
            if let Some(t) =
                edit::find_block_mut(&mut model.blocks, pos.block).and_then(|b| b.kind.text_mut())
            {
                edit::text_delete(t, pos.offset, end);
            }
            pos
        };
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn enter(&mut self, shift: bool) {
        if shift {
            self.insert_str("\n");
            return;
        }
        self.delete_selection_if_any();
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = edit::split_block(&mut model, pos);
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.pin_below(pos.block, new.block);
        self.after_edit();
    }

    // ─── Отрисовка выделения ────────────────────────────────────────────────

    fn draw_selection(&self, list: &mut DisplayList) {
        let Some(sel) = self.selection else { return };
        if sel.is_caret() {
            return;
        }
        let model = self.model();
        let order = BlockOrder::of(&model);
        drop(model);
        let (start, end) = sel.ordered(&order);
        let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else { return };
        let Ok(map) = self.geom.lock() else { return };

        for i in si..=ei {
            let id = order.ids[i];
            let Some(row) = map.get(&id) else { continue };
            let block_start = if i == si { start.offset } else { 0 };
            let block_end = if i == ei {
                end.offset
            } else {
                row.lines.last().and_then(|l| l.segs.last()).map(|s| s.abs_end()).unwrap_or(0)
            };
            for (li, line) in row.lines.iter().enumerate() {
                let (Some(first), Some(last)) = (line.segs.first(), line.segs.last()) else {
                    continue;
                };
                let l_start = first.abs_start;
                let l_end = last.abs_end();
                let lo = block_start.max(l_start);
                let hi = block_end.min(l_end);
                if hi <= lo && !(block_start < l_start && block_end > l_end) {
                    continue;
                }
                let x1 = self.x_of_offset(row, li, lo);
                let x2 = self.x_of_offset(row, li, hi);
                if x2 <= x1 {
                    continue;
                }
                let rect = Rect::new(
                    Point::new(row.origin.x + x1, row.origin.y + line.y),
                    Size::new(x2 - x1, row.line_h),
                );
                list.push_rect(rect, self.style.selection_color, [2.0; 4]);
            }
        }
    }
}

/// Оценка высоты блока, у которого ещё нет опубликованной геометрии
/// (медиа, врезка, разделитель): нужна только для первого размещения в
/// свободной раскладке и для запаса высоты холста.
fn estimate_height(block: &DocBlock, style: &DocStyle) -> f32 {
    match &block.kind {
        BlockKind::Shape { shape } if shape.is_line() => {
            shape::line_box(&block.attrs, *shape).height
        }
        BlockKind::Shape { .. } => shape::height_of(&block.attrs),
        BlockKind::Media { .. } => {
            free::height_of(&block.attrs).unwrap_or(220.0)
        }
        BlockKind::Divider => 17.0,
        BlockKind::Embed { .. } => free::height_of(&block.attrs).unwrap_or(200.0),
        BlockKind::Table { rows, .. } => (rows.len() as f32 + 1.0) * 30.0,
        _ => style.line_h(style.text_size) * 2.0,
    }
}

// ─── Свободная раскладка ────────────────────────────────────────────────────

impl DocumentEditorElement {
    /// Новый блок, родившийся рядом с закреплённым (Enter, вставка), встаёт
    /// **под ним**. Иначе он уходил в колонку потока — то есть улетал в угол
    /// холста, за километр от места, где его создавали.
    fn pin_below(&mut self, anchor: super::model::BlockId, block: super::model::BlockId) {
        if !self.layout.free {
            return;
        }
        let (Some(top_anchor), Some(top_block)) =
            (self.top_level_of(anchor), self.top_level_of(block))
        else {
            return;
        };
        if top_anchor == top_block {
            return;
        }
        let anchor_geom = {
            let model = self.model();
            let b = model.blocks.iter().find(|b| b.id == top_anchor);
            b.and_then(|b| free::pos_of(&b.attrs).map(|(x, y)| (x, y, free::width_of(&b.attrs))))
        };
        let Some((x, y, width)) = anchor_geom else { return };
        let height = self
            .block_rect(top_anchor)
            .map(|r| r.size.height)
            .unwrap_or_else(|| self.style.line_h(self.style.text_size));
        let below = self.layout.snapped(y + height + self.style.block_spacing);
        let width = width.unwrap_or(self.layout.block_width);
        let mut model = self.model();
        let Some(b) = model.blocks.iter_mut().find(|b| b.id == top_block) else { return };
        if free::pos_of(&b.attrs).is_none() {
            free::set_pos(&mut b.attrs, x, below);
            free::set_width(&mut b.attrs, width);
        }
    }

    /// Поставить блок в точку холста (вставка из контекстного меню).
    fn place_free_block(&mut self, id: super::model::BlockId, at: Point) {
        let width = self.layout.block_width;
        let mut model = self.model();
        let Some(b) = model.blocks.iter_mut().find(|b| b.id == id) else { return };
        free::set_pos(&mut b.attrs, at.x.max(0.0), at.y.max(0.0));
        if free::width_of(&b.attrs).is_none() {
            // Фигура шире колонки текста смотрелась бы нелепо: у неё свои
            // умолчания, а у линии ширина — это длина отрезка.
            let w = match &b.kind {
                BlockKind::Shape { shape } if shape.is_line() => {
                    shape::line_box(&b.attrs, *shape).width
                }
                BlockKind::Shape { .. } => shape::DEFAULT_W,
                _ => width,
            };
            free::set_width(&mut b.attrs, w);
        }
    }

    /// Верхнеуровневый блок, содержащий данный (сам блок либо его предок).
    fn top_level_of(&self, id: super::model::BlockId) -> Option<super::model::BlockId> {
        let model = self.model();
        fn has(block: &DocBlock, id: super::model::BlockId) -> bool {
            if block.id == id {
                return true;
            }
            block.kind.children().map(|cs| cs.iter().any(|c| has(c, id))).unwrap_or(false)
        }
        model.blocks.iter().find(|b| has(b, id)).map(|b| b.id)
    }

    /// Геометрия блока на холсте: из атрибутов, а для ещё не закреплённого
    /// блока — из его текущего места в потоке (перенос не должен прыгать).
    fn free_geom(&self, block: super::model::BlockId) -> Option<(f32, f32, f32)> {
        let pinned = {
            let model = self.model();
            let b = model.blocks.iter().find(|b| b.id == block)?;
            free::pos_of(&b.attrs).map(|(x, y)| (x, y, free::width_of(&b.attrs)))
        };
        let rect = self.block_rect(block);
        match pinned {
            Some((x, y, w)) => Some((
                x,
                y,
                w.or_else(|| rect.map(|r| r.size.width)).unwrap_or(self.layout.block_width),
            )),
            None => {
                let r = rect?;
                Some((
                    r.origin.x - self.bounds.origin.x,
                    r.origin.y - self.bounds.origin.y,
                    r.size.width,
                ))
            }
        }
    }

    /// Закрепить блок там, где он сейчас (первый перенос из потока).
    fn pin_block(&mut self, block: super::model::BlockId) -> Option<(f32, f32, f32)> {
        let geom = self.free_geom(block)?;
        let mut model = self.model();
        let b = model.blocks.iter_mut().find(|b| b.id == block)?;
        free::set_pos(&mut b.attrs, geom.0, geom.1);
        if free::width_of(&b.attrs).is_none() {
            free::set_width(&mut b.attrs, geom.2);
        }
        Some(geom)
    }

    /// Вид фигуры блока (если блок — примитив).
    fn shape_of(&self, block: super::model::BlockId) -> Option<super::model::ShapeKind> {
        let model = self.model();
        match model.blocks.iter().find(|b| b.id == block)?.kind {
            BlockKind::Shape { shape } => Some(shape),
            _ => None,
        }
    }

    /// Врезка-объект со своей высотой (доска, диаграмма).
    fn is_sized_embed(&self, block: super::model::BlockId) -> bool {
        let model = self.model();
        match model.blocks.iter().find(|b| b.id == block).map(|b| &b.kind) {
            Some(BlockKind::Embed { target }) => {
                self.embeds.as_ref().is_some_and(|f| f.has_own_height(target))
            }
            _ => false,
        }
    }

    /// Блок, у которого высота задаётся явно (фигура и медиа): у текста она
    /// считается по контенту, тянуть её за кромку нечего.
    fn has_free_height(&self, block: super::model::BlockId) -> bool {
        let model = self.model();
        match model.blocks.iter().find(|b| b.id == block).map(|b| &b.kind) {
            Some(BlockKind::Shape { shape }) => !shape.is_line(),
            // У видео и аудио высоту задаёт сам плеер, у файла — карточка.
            Some(BlockKind::Media { media, .. }) => {
                matches!(media, super::model::MediaKind::Image)
            }
            // Врезка со своей высотой (доска, диаграмма) — решает хост.
            Some(BlockKind::Embed { target }) => {
                self.embeds.as_ref().is_some_and(|f| f.has_own_height(target))
            }
            _ => false,
        }
    }

    /// Зона растяжения высоты блока (нижняя кромка).
    fn resize_v_rect(&self, block: super::model::BlockId) -> Option<Rect> {
        if !self.layout.free || !self.has_free_height(block) {
            return None;
        }
        let rect = self.block_rect(block)?;
        Some(Rect::new(
            Point::new(rect.origin.x, rect.origin.y + rect.size.height - 3.0),
            Size::new(rect.size.width.max(24.0), 8.0),
        ))
    }

    /// Правый нижний угол: ширина и высота сразу.
    fn resize_corner_rect(&self, block: super::model::BlockId) -> Option<Rect> {
        if !self.layout.free || !self.has_free_height(block) {
            return None;
        }
        let rect = self.block_rect(block)?;
        Some(Rect::new(
            Point::new(
                rect.origin.x + rect.size.width - 5.0,
                rect.origin.y + rect.size.height - 5.0,
            ),
            Size::new(12.0, 12.0),
        ))
    }

    /// Хваталки линейной фигуры: два конца, а у кривой — ещё две
    /// направляющие точки Безье (порядок как в [`shape::line_handles`]).
    fn endpoint_rects(&self, block: super::model::BlockId) -> Option<Vec<Rect>> {
        if !self.layout.free {
            return None;
        }
        let kind = self.shape_of(block).filter(|k| k.is_line())?;
        let rect = self.block_rect(block)?;
        let attrs = {
            let model = self.model();
            model.blocks.iter().find(|b| b.id == block)?.attrs.clone()
        };
        let r = 7.0;
        Some(
            shape::local_handles(&attrs, kind)
                .into_iter()
                .map(|p| {
                    Rect::new(
                        Point::new(rect.origin.x + p.0 - r, rect.origin.y + p.1 - r),
                        Size::new(r * 2.0, r * 2.0),
                    )
                })
                .collect(),
        )
    }

    /// Фигура под точкой: верхний по порядку блок-примитив, накрывающий её.
    /// Текстовый блок поверх фигуры перехватывает клик — как и должно быть
    /// при наложении.
    fn shape_at(&self, p: Point) -> Option<super::model::BlockId> {
        let rects = self.blocks.lock().ok()?.clone();
        let model = self.model();
        let mut hit: Option<(super::model::BlockId, bool)> = None;
        for b in model.blocks.iter() {
            let Some(rect) = rects.get(&b.id) else { continue };
            if !rect.contains(p) {
                continue;
            }
            // У линии зона клика — сам отрезок, а не его bbox: иначе
            // длинная диагональ перекрывала бы полхолста.
            if let BlockKind::Shape { shape } = &b.kind {
                if shape.is_line() {
                    let local = (p.x - rect.origin.x, p.y - rect.origin.y);
                    let path = shape::line_polyline(&b.attrs, *shape);
                    let near = path
                        .windows(2)
                        .any(|w| dist_to_segment(local, w[0], w[1]) <= 8.0);
                    if !near {
                        continue;
                    }
                }
                hit = Some((b.id, true));
            } else if let BlockKind::Embed { target } = &b.kind {
                // Врезка со своей высотой (доска, диаграмма): клик по её
                // пустому месту делает блок текущим — панель свойств и
                // хваталки размера; клики по её виджетам до сюда не доходят.
                let own = self.embeds.as_ref().is_some_and(|f| f.has_own_height(target));
                hit = Some((b.id, own));
            } else {
                hit = Some((b.id, false));
            }
        }
        hit.filter(|(_, is_object)| *is_object).map(|(id, _)| id)
    }

    /// Сделать блок текущим без каретки (клик по фигуре или объекту).
    /// `focus` — оставить редактору фокус (Delete и стрелки над фигурой);
    /// над чужой врезкой (доска, диаграмма) фокус снимается: приложение его
    /// туда не даёт, а два «фокусированных» ввода делят клавиатуру.
    fn select_object(&mut self, id: super::model::BlockId, focus: bool) {
        self.drop_block_sel();
        self.selection = None;
        self.table_caret = None;
        self.code_caret = None;
        self.object_sel = Some(id);
        self.focused = focus;
        if !focus {
            self.mouse_selecting = false;
        }
        self.publish_selection();
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Хваталка размера под курсором (порядок: концы → угол → кромки).
    fn size_handle_at(&self, p: Point) -> Option<(super::model::BlockId, FreeDragMode)> {
        let ui = self.ui_rects.lock().ok()?;
        if let Some((rects, block)) = &ui.ends {
            for (i, r) in rects.iter().enumerate() {
                if r.contains(p) {
                    return Some((*block, FreeDragMode::Endpoint(i)));
                }
            }
        }
        for (zone, mode) in [
            (ui.corner, FreeDragMode::Corner),
            (ui.resize, FreeDragMode::Width),
            (ui.resize_v, FreeDragMode::Height),
        ] {
            if let Some((rect, block)) = zone {
                if rect.contains(p) {
                    return Some((block, mode));
                }
            }
        }
        None
    }

    /// Зона растяжения ширины блока (правая кромка) в свободной раскладке.
    fn resize_rect(&self, block: super::model::BlockId) -> Option<Rect> {
        if !self.layout.free || self.shape_of(block).is_some_and(|k| k.is_line()) {
            return None;
        }
        let (x, _, w) = self.free_geom(block)?;
        let rect = self.block_rect(block)?;
        let left = self.bounds.origin.x + x + w - 3.0;
        Some(Rect::new(
            Point::new(left, rect.origin.y),
            Size::new(8.0, rect.size.height.max(HANDLE_H)),
        ))
    }

    /// Начать перенос (или растяжение) блока по холсту.
    fn start_free_drag(&mut self, block: super::model::BlockId, at: Point, mode: FreeDragMode) -> bool {
        if !self.layout.free {
            return false;
        }
        let Some(top) = self.top_level_of(block) else { return false };
        self.checkpoint(EditClass::Structure);
        // Блок мог ещё стоять в потоке — закрепляем его ровно там, где он
        // сейчас нарисован, иначе перенос начинался бы со скачка.
        let Some((x, y, width)) = self.pin_block(top) else {
            self.history().discard_last_checkpoint();
            return false;
        };
        self.free_drag = Some(FreeDrag {
            block: top,
            grab: Point::new(at.x - (self.bounds.origin.x + x), at.y - (self.bounds.origin.y + y)),
            mode,
            start_width: width,
            moved: false,
            announced: false,
        });
        true
    }

    /// Объявить перенос блока drag'ом дерева (если хост задал тип): payload
    /// — id блока, призрака нет — блок и так виден (едет живьём на холсте,
    /// в потоке — свой ghost).
    fn announce_block_drag(&self, block: super::model::BlockId, at: Point, ctx: &mut EventContext) -> bool {
        let Some(t) = self.block_drag_type.clone() else { return false };
        ctx.cursor_position = at;
        ctx.start_drag(crate::input::DragData::new(t, block.0.to_string(), self.id.0).without_ghost());
        true
    }

    /// Любая врезка под точкой (объект со своей высотой или вложенная
    /// страница): её виджеты редактору чужие — фокус по клику там не
    /// берётся (`text_input_hit`), каретка не ставится.
    fn embed_at(&self, p: Point) -> Option<super::model::BlockId> {
        let map = self.blocks.lock().ok()?;
        let model = self.model();
        model
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Embed { .. }))
            .find(|b| map.get(&b.id).is_some_and(|r| r.contains(p)))
            .map(|b| b.id)
    }

    /// Снять фокус редактора самому: клик пришёлся на чужую врезку, и
    /// приложение фокус ему не отдало (`text_input_hit`) — `FocusLost`
    /// сюда не придёт, а прежняя каретка иначе осталась бы живой.
    fn drop_focus(&mut self) {
        if self.focused {
            self.focused = false;
            self.mouse_selecting = false;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    /// Блок-объект со своей высотой (доска, диаграмма) под точкой: над ним
    /// свой индикатор вставки не рисуем — дроп туда принимает сам объект.
    fn sized_embed_at(&self, p: Point) -> Option<super::model::BlockId> {
        let map = self.blocks.lock().ok()?;
        map.iter()
            .find(|(id, rect)| rect.contains(p) && self.is_sized_embed(**id))
            .map(|(id, _)| *id)
    }

    /// Тянем блок: координаты (или ширина) пишутся сразу в модель, чтобы
    /// раскладка шла обычным перестроением; ревизия бампается один раз в
    /// конце (иначе автосейв дёргался бы на каждый кадр).
    fn update_free_drag(&mut self, at: Point) {
        let Some(drag) = &self.free_drag else { return };
        let (block, grab, mode, start_width) = (drag.block, drag.grab, drag.mode, drag.start_width);
        let origin = self.bounds.origin;
        let layout = self.layout;
        let changed = {
            let mut model = self.model();
            let Some(b) = model.blocks.iter_mut().find(|b| b.id == block) else { return };
            let (bx, by) = free::pos_of(&b.attrs).unwrap_or((0.0, 0.0));
            match mode {
                FreeDragMode::Move => {
                    let x = layout.snapped(at.x - grab.x - origin.x).max(0.0);
                    let y = layout.snapped(at.y - grab.y - origin.y).max(0.0);
                    let same = (bx - x).abs() < 0.5 && (by - y).abs() < 0.5;
                    if !same {
                        free::set_pos(&mut b.attrs, x, y);
                    }
                    !same
                }
                FreeDragMode::Width | FreeDragMode::Height | FreeDragMode::Corner => {
                    let mut changed = false;
                    if mode != FreeDragMode::Height {
                        let w = layout.snapped(at.x - origin.x - bx).clamp(40.0, 4000.0);
                        if (w - free::width_of(&b.attrs).unwrap_or(start_width)).abs() >= 0.5 {
                            free::set_width(&mut b.attrs, w);
                            changed = true;
                        }
                    }
                    if mode != FreeDragMode::Width {
                        let h = layout.snapped(at.y - origin.y - by).clamp(20.0, 4000.0);
                        if (h - free::height_of(&b.attrs).unwrap_or(-1.0)).abs() >= 0.5 {
                            free::set_height(&mut b.attrs, h);
                            changed = true;
                        }
                    }
                    changed
                }
                // Конец отрезка тянется по холсту; модель держится в
                // каноничном виде — минимум концов равен нулю, а рамка
                // блока сдвигается под них.
                // Хваталка тянется по холсту; модель держится в каноничном
                // виде — минимум точек равен нулю, а рамка блока сдвигается
                // под них. У кривой вместе с концами так же ездят её
                // направляющие: иначе после переноса конца дуга «съезжала».
                FreeDragMode::Endpoint(index) => {
                    let Some(kind) = (match &b.kind {
                        BlockKind::Shape { shape } => Some(*shape),
                        _ => None,
                    }) else {
                        return;
                    };
                    let pad = shape::LINE_PAD;
                    let pts = shape::line_handles(&b.attrs, kind);
                    let (minx, miny) = pts.iter().fold((f32::MAX, f32::MAX), |acc, p| {
                        (acc.0.min(p.0), acc.1.min(p.1))
                    });
                    let mut abs: Vec<(f32, f32)> = pts
                        .iter()
                        .map(|p| (bx + p.0 - minx + pad, by + p.1 - miny + pad))
                        .collect();
                    let Some(slot) = abs.get_mut(index) else { return };
                    *slot = (
                        layout.snapped(at.x - origin.x).max(0.0),
                        layout.snapped(at.y - origin.y).max(0.0),
                    );
                    let (nx, ny) = abs.iter().fold((f32::MAX, f32::MAX), |acc, p| {
                        (acc.0.min(p.0), acc.1.min(p.1))
                    });
                    let pos = ((nx - pad).max(0.0), (ny - pad).max(0.0));
                    let local: Vec<(f32, f32)> =
                        abs.iter().map(|p| (p.0 - nx, p.1 - ny)).collect();
                    let same = local == pts
                        && (bx - pos.0).abs() < 0.5
                        && (by - pos.1).abs() < 0.5;
                    if !same {
                        free::set_pos(&mut b.attrs, pos.0, pos.1);
                        shape::set_endpoints(&mut b.attrs, local[0], local[1]);
                        if kind.is_curve() {
                            shape::set_controls(&mut b.attrs, local[2], local[3]);
                        }
                        let bbox = shape::line_box(&b.attrs, kind);
                        free::set_width(&mut b.attrs, bbox.width);
                        free::set_height(&mut b.attrs, bbox.height);
                    }
                    !same
                }
            }
        };
        if changed {
            if let Some(drag) = &mut self.free_drag {
                drag.moved = true;
            }
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    /// Габариты блока: лёгкая заливка под курсором и рамка у текущего.
    /// Пустой блок иначе не виден вовсе — ни где он, ни какого размера.
    fn draw_block_bounds(&self, list: &mut DisplayList) {
        if self.read_only || self.plain {
            return;
        }
        let pad = 3.0;
        let inflate = |r: Rect| {
            Rect::new(
                Point::new(r.origin.x - pad, r.origin.y - pad),
                Size::new(r.size.width + pad * 2.0, r.size.height + pad * 2.0),
            )
        };
        let current = self.current_block().and_then(|id| self.top_level_of(id));
        // Доска и диаграмма — самостоятельные объекты со своим оформлением:
        // ни подсветки под курсором, ни рамки выбора вокруг них.
        let plain = |id: super::model::BlockId| self.is_sized_embed(id);
        if let Some(block) = self.hover_block.filter(|b| Some(*b) != current && !plain(*b)) {
            if let Some(rect) = self.block_rect(block) {
                list.push_rect(inflate(rect), self.style.block_hover_color, [4.0; 4]);
            }
        }
        if let Some(rect) = current.filter(|id| !plain(*id)).and_then(|id| self.block_rect(id)) {
            for edge in edges(inflate(rect)) {
                list.push_rect(edge, self.style.block_selected_color, [0.0; 4]);
            }
        }
    }

    /// Фон холста: точки, линии или кресты с шагом сетки.
    fn draw_grid(&self, list: &mut DisplayList, clip: Rect) {
        if !self.layout.free || self.layout.grid == DocGrid::None {
            return;
        }
        let area = intersect(self.bounds, clip);
        if area.size.width <= 0.0 || area.size.height <= 0.0 {
            return;
        }
        let step = self.layout.grid_step_px();
        let color = self.style.grid_color;
        let nx = (area.size.width / step).ceil() as i32 + 1;
        let ny = (area.size.height / step).ceil() as i32 + 1;
        if nx <= 0 || ny <= 0 || nx > 600 || ny > 600 {
            return;
        }
        let ox = self.bounds.origin.x;
        let oy = self.bounds.origin.y;
        let first_x = ((area.origin.x - ox) / step).floor() * step + ox;
        let first_y = ((area.origin.y - oy) / step).floor() * step + oy;
        match self.layout.grid {
            DocGrid::Lines => {
                for i in 0..nx {
                    let x = first_x + i as f32 * step;
                    list.push_rect(
                        Rect::new(Point::new(x, area.origin.y), Size::new(1.0, area.size.height)),
                        color,
                        [0.0; 4],
                    );
                }
                for j in 0..ny {
                    let y = first_y + j as f32 * step;
                    list.push_rect(
                        Rect::new(Point::new(area.origin.x, y), Size::new(area.size.width, 1.0)),
                        color,
                        [0.0; 4],
                    );
                }
            }
            DocGrid::Dots => {
                for j in 0..ny {
                    for i in 0..nx {
                        let x = first_x + i as f32 * step;
                        let y = first_y + j as f32 * step;
                        list.push_rect(
                            Rect::new(Point::new(x - 0.75, y - 0.75), Size::new(1.5, 1.5)),
                            color,
                            [0.75; 4],
                        );
                    }
                }
            }
            DocGrid::Cross => {
                for j in 0..ny {
                    for i in 0..nx {
                        let x = first_x + i as f32 * step;
                        let y = first_y + j as f32 * step;
                        list.push_rect(
                            Rect::new(Point::new(x - 3.0, y - 0.5), Size::new(6.0, 1.0)),
                            color,
                            [0.0; 4],
                        );
                        list.push_rect(
                            Rect::new(Point::new(x - 0.5, y - 3.0), Size::new(1.0, 6.0)),
                            color,
                            [0.0; 4],
                        );
                    }
                }
            }
            DocGrid::None => {}
        }
    }
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.origin.x.max(b.origin.x);
    let y0 = a.origin.y.max(b.origin.y);
    let x1 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    Rect::new(Point::new(x0, y0), Size::new((x1 - x0).max(0.0), (y1 - y0).max(0.0)))
}

impl DocumentEditorElement {
    /// Прямоугольник блока целиком (обёртка публикует его при раскладке).
    fn block_rect(&self, block: super::model::BlockId) -> Option<Rect> {
        self.blocks.lock().ok()?.get(&block).copied()
    }

    /// Прямоугольник ручки ⋮⋮ для блока (слева от контента).
    fn handle_rect(&self, block: super::model::BlockId) -> Option<Rect> {
        let rect = self.block_rect(block)?;
        // По первой строке блока, а не по его центру: у таблицы и кода
        // высота в десятки строк, ручка посередине выглядит потерянной.
        let line_h = self
            .geom
            .lock()
            .ok()
            .and_then(|m| m.get(&block).map(|r| r.line_h))
            .unwrap_or(HANDLE_H);
        let x = (rect.origin.x - HANDLE_W - 6.0).max(self.bounds.origin.x + 2.0);
        let y = rect.origin.y + (line_h.min(rect.size.height) - HANDLE_H) / 2.0;
        Some(Rect::new(Point::new(x, y), Size::new(HANDLE_W, HANDLE_H)))
    }

    /// Блок под курсором: тот, чей прямоугольник накрывает точку по
    /// вертикали (полоса слева под ручку тоже считается его зоной).
    fn row_at(&self, p: Point) -> Option<super::model::BlockId> {
        let map = self.blocks.lock().ok()?;
        let mut best: Option<(f32, super::model::BlockId)> = None;
        for (id, rect) in map.iter() {
            let left = rect.origin.x - HANDLE_W - 10.0;
            let right = rect.origin.x + rect.size.width + 4.0;
            if p.x < left || p.x > right {
                continue;
            }
            let dy = dist(p.y, rect.origin.y, rect.origin.y + rect.size.height);
            if best.map(|(d, _)| dy < d).unwrap_or(true) {
                best = Some((dy, *id));
            }
        }
        best.filter(|(d, _)| *d < 6.0).map(|(_, id)| id)
    }

    /// Слот вставки при перетаскивании: до/после ближайшего блока по Y.
    fn drop_target(&self, p: Point) -> Option<(super::model::BlockId, bool)> {
        let map = self.blocks.lock().ok()?;
        let mut best: Option<(f32, super::model::BlockId, bool)> = None;
        for (id, rect) in map.iter() {
            let mid = rect.origin.y + rect.size.height / 2.0;
            let before = p.y < mid;
            let d = (p.y - mid).abs();
            if best.map(|(bd, _, _)| d < bd).unwrap_or(true) {
                best = Some((d, *id, before));
            }
        }
        best.map(|(_, id, before)| (id, before))
    }

    /// Y-координата индикатора вставки.
    fn drop_indicator_y(&self, target: (super::model::BlockId, bool)) -> Option<(f32, f32, f32)> {
        let rect = self.block_rect(target.0)?;
        let y = if target.1 {
            rect.origin.y - self.style.block_spacing / 2.0
        } else {
            rect.origin.y + rect.size.height + self.style.block_spacing / 2.0
        };
        Some((rect.origin.x, y, rect.size.width.max(120.0)))
    }

    /// Ручка ⋮⋮, ghost и индикатор вставки.
    fn draw_drag_ui(&self, list: &mut DisplayList) {
        if self.plain {
            return;
        }
        let s = &self.style;
        // Выделенные блоки — заливка с контуром; рамка протяжки — тоньше.
        if !self.block_sel.is_empty() {
            if let Ok(map) = self.blocks.lock() {
                for id in &self.block_sel {
                    if let Some(r) = map.get(id) {
                        let rr = Rect::new(
                            Point::new(r.origin.x - 4.0, r.origin.y - 2.0),
                            Size::new(r.size.width + 8.0, r.size.height + 4.0),
                        );
                        list.push_rect(rr, s.selection_color, [6.0; 4]);
                        stroke_rect(list, rr, s.caret_color.with_alpha(0.7), 1.0);
                    }
                }
            }
        }
        if let Some(m) = &self.marquee {
            if m.active {
                let r = rect_from_points(m.start, m.current);
                list.push_rect(r, s.selection_color.with_alpha(0.12), [2.0; 4]);
                stroke_rect(list, r, s.caret_color.with_alpha(0.6), 1.0);
            }
        }
        // Ручка у блока под курсором (когда не тянем).
        if self.drag.is_none() {
            if let Some(block) = self.hover_block {
                if let Some(rect) = self.handle_rect(block) {
                    let mut c = crate::core::canvas::CanvasContext::new(
                        rect.origin,
                        rect.size,
                    );
                    c.set_color(s.muted_color.with_alpha(0.8));
                    // Сетка 2×3 точки, отцентрованная в рамке ручки:
                    // ширина 7+2r, высота 12+2r при r=1.6.
                    let x0 = (HANDLE_W - 7.0 - 3.2) / 2.0 + 1.6;
                    let y0 = (HANDLE_H - 12.0 - 3.2) / 2.0 + 1.6;
                    for row in 0..3 {
                        for col in 0..2 {
                            c.fill_circle(
                                x0 + col as f32 * 7.0,
                                y0 + row as f32 * 6.0,
                                1.6,
                            );
                        }
                    }
                    c.flush(list);
                    if let Ok(mut ui) = self.ui_rects.lock() {
                        ui.handle = Some((rect, block));
                    }
                }
                // Правая кромка блока — растяжение ширины (свободная
                // раскладка): узкая полоска в цвет ручки. У объекта со
                // своим оформлением (доска, диаграмма) зоны есть, а
                // полосок нет — они читались как рамка вокруг блока.
                let plain = self.is_sized_embed(block);
                if let Some(rect) = self.resize_rect(block) {
                    if !plain {
                        list.push_rect(
                            Rect::new(
                                Point::new(rect.origin.x + 2.0, rect.origin.y),
                                Size::new(2.0, rect.size.height),
                            ),
                            s.muted_color.with_alpha(0.5),
                            [1.0; 4],
                        );
                    }
                    if let Ok(mut ui) = self.ui_rects.lock() {
                        ui.resize = Some((rect, block));
                    }
                }
            }
            // Хваталки размера у блока с явной высотой и концы линии —
            // и под курсором, и у выбранного (фигуру часто настраивают
            // из панели свойств, курсор при этом далеко).
            // Порядок важен: хит-зоны пишутся в общий реестр, и последним
            // должен оказаться блок под курсором — иначе его хваталки
            // перекрывал бы выбранный блок где-то в другом углу холста.
            let focus = self.current_block().and_then(|id| self.top_level_of(id));
            for block in [focus, self.hover_block].into_iter().flatten() {
                let plain = self.is_sized_embed(block);
                if let Some(rect) = self.resize_v_rect(block) {
                    if !plain {
                        list.push_rect(
                            Rect::new(
                                Point::new(rect.origin.x, rect.origin.y + 2.0),
                                Size::new(rect.size.width, 2.0),
                            ),
                            s.muted_color.with_alpha(0.5),
                            [1.0; 4],
                        );
                    }
                    if let Ok(mut ui) = self.ui_rects.lock() {
                        ui.resize_v = Some((rect, block));
                    }
                }
                if let Some(rect) = self.resize_corner_rect(block) {
                    if !plain {
                        list.push_rect(
                            Rect::new(
                                Point::new(rect.origin.x + 1.0, rect.origin.y + 1.0),
                                Size::new(8.0, 8.0),
                            ),
                            s.shape_handle_color.with_alpha(0.9),
                            [2.0; 4],
                        );
                    }
                    if let Ok(mut ui) = self.ui_rects.lock() {
                        ui.corner = Some((rect, block));
                    }
                }
                if let Some(rects) = self.endpoint_rects(block) {
                    // У кривой хваталок четыре: два конца (сплошные кружки)
                    // и две направляющие Безье — те меньше и привязаны к
                    // своему концу тонкой линией, как в векторных редакторах.
                    let center = |r: &Rect| {
                        Point::new(
                            r.origin.x + r.size.width / 2.0,
                            r.origin.y + r.size.height / 2.0,
                        )
                    };
                    if rects.len() >= 4 {
                        let mut c = crate::core::canvas::CanvasContext::new(
                            self.bounds.origin,
                            self.bounds.size,
                        );
                        c.set_anti_alias(1.0);
                        c.set_color(s.shape_handle_color.with_alpha(0.5));
                        c.set_stroke_width(1.0);
                        let rel = |p: Point| {
                            (p.x - self.bounds.origin.x, p.y - self.bounds.origin.y)
                        };
                        for (end, ctrl) in [(0, 2), (1, 3)] {
                            let (a, b) = (rel(center(&rects[end])), rel(center(&rects[ctrl])));
                            c.draw_polyline(&[a, b]);
                        }
                        c.flush(list);
                    }
                    for (i, r) in rects.iter().enumerate() {
                        let control = i >= 2;
                        let radius = if control { 4.0 } else { 5.0 };
                        let mut c = crate::core::canvas::CanvasContext::new(r.origin, r.size);
                        c.set_anti_alias(1.0);
                        c.set_color(if control { s.shape_handle_color } else { s.menu_bg });
                        c.fill_circle(r.size.width / 2.0, r.size.height / 2.0, radius + 0.5);
                        if !control {
                            c.set_color(s.shape_handle_color);
                            c.set_stroke_width(2.0);
                            let pts = crate::core::canvas::tessellator::circle_points(
                                Point::new(r.size.width / 2.0, r.size.height / 2.0),
                                radius,
                                18,
                            );
                            let mut ring: Vec<(f32, f32)> =
                                pts.iter().map(|p| (p.x, p.y)).collect();
                            if let Some(&first) = ring.first() {
                                ring.push(first);
                            }
                            c.draw_polyline(&ring);
                        }
                        c.flush(list);
                    }
                    if let Ok(mut ui) = self.ui_rects.lock() {
                        ui.ends = Some((rects, block));
                    }
                }
            }
        }
        // Индикатор вставки и ghost при активном перетаскивании.
        if let Some(drag) = &self.drag {
            if drag.started {
                if let Some(target) = drag.target {
                    if let Some((x, y, w)) = self.drop_indicator_y(target) {
                        list.push_rect(
                            Rect::new(Point::new(x, y - 1.0), Size::new(w, 2.0)),
                            s.caret_color,
                            [1.0; 4],
                        );
                    }
                }
                // Ghost: первая строка текста блока рядом с курсором.
                let text = {
                    let model = self.model();
                    edit::find_block(&model.blocks, drag.block)
                        .and_then(|b| b.kind.text())
                        .map(|t| t.text())
                        .unwrap_or_default()
                };
                let label: String = text.chars().take(40).collect();
                let w = self
                    .tm
                    .as_deref()
                    .map(|tm| {
                        tm.measure_text_width(&label, s.text_size, label.chars().count())
                    })
                    .unwrap_or(120.0)
                    + 20.0;
                let ghost = Rect::new(
                    Point::new(drag.current.x + 10.0, drag.current.y + 8.0),
                    Size::new(w.max(60.0), s.line_h(s.text_size) + 6.0),
                );
                list.push_rect(ghost, s.menu_bg.with_alpha(0.92), [6.0; 4]);
                list.push_text_styled_singleline(
                    &label,
                    Rect::new(
                        Point::new(ghost.origin.x + 10.0, ghost.origin.y + 3.0),
                        Size::new(ghost.size.width - 20.0, s.text_size * 1.4),
                    ),
                    s.text_color,
                    s.text_size,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    400,
                    None,
                );
            }
        }
    }

    /// Дроп файла: вставка pending-блока Media рядом с точкой сброса.
    /// Возвращает токен для handle.patch_media.
    fn insert_pending_media(&mut self, at: Point, path: &std::path::Path) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TOKEN: AtomicU64 = AtomicU64::new(1);
        let token = TOKEN.fetch_add(1, Ordering::Relaxed).to_string();
        let url = format!("pending:{token}");
        let alt = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "файл".to_string());
        let media = super::model::MediaKind::detect(&path.display().to_string(), &super::model::Attrs::default());

        self.checkpoint(EditClass::Structure);
        let target = self.drop_target(at);
        {
            let mut model = self.model();
            let id = model.alloc_id();
            let block = super::model::DocBlock::new(
                id,
                super::model::BlockKind::Media { media, url, alt },
            );
            match target {
                Some((target_id, before)) => {
                    edit::with_siblings(&mut model.blocks, target_id, &mut |sibs, idx| {
                        let pos = if before { idx } else { idx + 1 };
                        sibs.insert(pos, block.clone());
                    });
                }
                None => model.blocks.push(block),
            }
        }
        self.after_edit();
        token
    }

    /// Ссылка сегмента под курсором (для Ctrl+клика).
    fn link_at(&self, p: Point) -> Option<super::model::LinkTarget> {
        let map = self.geom.lock().ok()?;
        for row in map.values() {
            for line in &row.lines {
                let y0 = row.origin.y + line.y;
                if p.y < y0 || p.y > y0 + row.line_h {
                    continue;
                }
                for seg in &line.segs {
                    let x0 = row.origin.x + seg.x;
                    if p.x >= x0 && p.x <= x0 + seg.width {
                        return seg.link.clone();
                    }
                }
            }
        }
        None
    }

    fn wiki_candidates(&self, query: &str) -> Vec<LinkCandidate> {
        let mut out = self
            .links
            .as_deref()
            .map(|l| l.complete(query))
            .unwrap_or_default();
        out.truncate(SLASH_MAX_ROWS);
        // Без совпадений, но с запросом — пункт «создать как есть».
        if out.is_empty() && !query.is_empty() {
            out.push(LinkCandidate { target: query.to_string(), label: query.to_string() });
        }
        out
    }

    /// Открытие автокомплита после ввода второй `[`.
    fn maybe_open_wiki(&mut self) {
        let Some(pos) = self.caret() else { return };
        let ok = {
            let model = self.model();
            let text = edit::find_block(&model.blocks, pos.block)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            pos.offset >= 2 && text[..pos.offset].ends_with("[[")
        };
        if ok {
            self.close_slash();
            self.wiki = Some(WikiState {
                block: pos.block,
                start: pos.offset - 2,
                query: String::new(),
                selected: 0,
                candidates: self.wiki_candidates(""),
            });
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn close_wiki(&mut self) {
        if self.wiki.take().is_some() {
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    /// Применение кандидата: `[[query` заменяется готовой wiki-ссылкой.
    fn apply_wiki(&mut self, idx: usize) {
        let Some(w) = self.wiki.take() else { return };
        let Some(c) = w.candidates.get(idx).cloned() else { return };
        self.checkpoint(EditClass::Structure);
        let end = w.start + 2 + w.query.len();
        let new_offset = {
            let mut model = self.model();
            edit::find_block_mut(&mut model.blocks, w.block)
                .and_then(|b| b.kind.text_mut())
                .map(|t| edit::replace_with_wiki_link(t, w.start, end, &c.target, &c.target))
        };
        if let Some(offset) = new_offset {
            self.selection =
                Some(DocSelection::caret(CaretPos { block: w.block, offset }));
        }
        self.after_edit();
    }

    /// Меню автокомплита `[[` под кареткой.
    fn draw_wiki_menu(&self, list: &mut DisplayList) {
        let Some(w) = &self.wiki else { return };
        let anchor = self
            .caret_rect(CaretPos { block: w.block, offset: w.start })
            .or_else(|| self.caret().and_then(|p| self.caret_rect(p)));
        let Some(anchor) = anchor else { return };
        let count = w.candidates.len().min(SLASH_MAX_ROWS);
        if count == 0 {
            return;
        }
        let s = &self.style;
        let rect = Rect::new(
            Point::new(anchor.origin.x, anchor.origin.y + anchor.size.height + 4.0),
            Size::new(SLASH_MENU_W, count as f32 * SLASH_ROW_H + 8.0),
        );
        list.push_rect(rect, s.menu_bg, [8.0; 4]);
        for edge in edges(rect) {
            list.push_rect(edge, s.menu_border, [0.0; 4]);
        }
        let selected = w.selected.min(count - 1);
        for (i, item) in w.candidates.iter().take(count).enumerate() {
            let row = Rect::new(
                Point::new(rect.origin.x + 4.0, rect.origin.y + 4.0 + i as f32 * SLASH_ROW_H),
                Size::new(rect.size.width - 8.0, SLASH_ROW_H),
            );
            if i == selected {
                list.push_rect(row, s.menu_sel_bg, [5.0; 4]);
            }
            list.push_text_styled_singleline(
                &item.label,
                Rect::new(
                    Point::new(
                        row.origin.x + 8.0,
                        row.origin.y + (SLASH_ROW_H - s.text_size * 1.3) / 2.0,
                    ),
                    Size::new(row.size.width - 16.0, s.text_size * 1.4),
                ),
                s.link_color,
                s.text_size,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
                None,
            );
        }
        if let Ok(mut ui) = self.ui_rects.lock() {
            let hit = Rect::new(
                Point::new(rect.origin.x, rect.origin.y + 4.0),
                Size::new(rect.size.width, count as f32 * SLASH_ROW_H),
            );
            ui.wiki = Some((hit, count));
        }
    }

    /// Slash-меню под кареткой.
    fn draw_slash_menu(&self, list: &mut DisplayList) {
        let Some(sl) = &self.slash else { return };
        let anchor = self
            .caret_rect(CaretPos { block: sl.block, offset: sl.start })
            .or_else(|| self.caret().and_then(|p| self.caret_rect(p)));
        let Some(anchor) = anchor else { return };
        let items = filter_items(&self.slash_items, &sl.query);
        let count = items.len().min(SLASH_MAX_ROWS);
        if count == 0 {
            return;
        }
        let s = &self.style;
        let rect = Rect::new(
            Point::new(anchor.origin.x, anchor.origin.y + anchor.size.height + 4.0),
            Size::new(SLASH_MENU_W, count as f32 * SLASH_ROW_H + 8.0),
        );
        list.push_rect(rect, s.menu_bg, [8.0; 4]);
        // Тонкая рамка четырьмя полосами.
        for edge in edges(rect) {
            list.push_rect(edge, s.menu_border, [0.0; 4]);
        }
        let selected = sl.selected.min(count - 1);
        for (i, item) in items.iter().take(count).enumerate() {
            let row = Rect::new(
                Point::new(rect.origin.x + 4.0, rect.origin.y + 4.0 + i as f32 * SLASH_ROW_H),
                Size::new(rect.size.width - 8.0, SLASH_ROW_H),
            );
            if i == selected {
                list.push_rect(row, s.menu_sel_bg, [5.0; 4]);
            }
            list.push_text_styled_singleline(
                &item.label,
                Rect::new(
                    Point::new(row.origin.x + 8.0, row.origin.y + (SLASH_ROW_H - s.text_size * 1.3) / 2.0),
                    Size::new(row.size.width - 16.0, s.text_size * 1.4),
                ),
                s.text_color,
                s.text_size,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
                None,
            );
        }
        if let Ok(mut ui) = self.ui_rects.lock() {
            // Хит-зона — только строки (без внутренних полей).
            let hit = Rect::new(
                Point::new(rect.origin.x, rect.origin.y + 4.0),
                Size::new(rect.size.width, count as f32 * SLASH_ROW_H),
            );
            ui.slash = Some((hit, count));
        }
    }

    /// Мини-тулбар инлайн-стилей над выделением.
    fn draw_toolbar(&self, list: &mut DisplayList) {
        let Some(sel) = self.selection else { return };
        if sel.is_caret() || self.mouse_selecting || self.slash.is_some() {
            return;
        }
        let model = self.model();
        let order = BlockOrder::of(&model);
        drop(model);
        let (start, _) = sel.ordered(&order);
        let Some(anchor) = self.caret_rect(start) else { return };
        let s = &self.style;
        let labels = ["B", "I", "S", "<>"];
        let w = TOOLBAR_BTN_W * labels.len() as f32;
        // Над выделением; у верхней кромки редактора места нет (тулбар
        // уходил под шапку панели) — переносим под строку выделения.
        let mut y = anchor.origin.y - TOOLBAR_H - 6.0;
        if y < self.bounds.origin.y + 2.0 {
            y = anchor.origin.y + anchor.size.height + 6.0;
        }
        let max_x = (self.bounds.origin.x + self.bounds.size.width - w - 4.0)
            .max(self.bounds.origin.x);
        let x = anchor.origin.x.min(max_x);
        let rect = Rect::new(Point::new(x, y), Size::new(w, TOOLBAR_H));
        list.push_rect(rect, s.menu_bg, [6.0; 4]);
        for edge in edges(rect) {
            list.push_rect(edge, s.menu_border, [0.0; 4]);
        }
        for (i, label) in labels.iter().enumerate() {
            let cell = Rect::new(
                Point::new(rect.origin.x + i as f32 * TOOLBAR_BTN_W, rect.origin.y),
                Size::new(TOOLBAR_BTN_W, TOOLBAR_H),
            );
            list.push_text_styled_singleline(
                label,
                Rect::new(
                    Point::new(cell.origin.x, cell.origin.y + (TOOLBAR_H - s.text_size * 1.3) / 2.0),
                    Size::new(cell.size.width, s.text_size * 1.4),
                ),
                s.text_color,
                s.text_size,
                TextAlign::CENTER,
                TextDecoration::None,
                if i == 0 { 700 } else { 500 },
                None,
            );
        }
        if let Ok(mut ui) = self.ui_rects.lock() {
            ui.toolbar = Some((rect, TOOLBAR_BTN_W));
        }
    }
}

// ─── Редактирование ячеек таблицы ───────────────────────────────────────────
//
// Таблица — лист без своей каретки: геометрию публикует TableBlockElement,
// а контейнер по ней хит-тестит ячейки, держит [`TableCaret`] (отдельный
// режим от `selection`), правит плоский текст ячейки в модели и рисует
// рамку/каретку поверх.
impl DocumentEditorElement {
    fn table_geom_of(&self, block: super::model::BlockId) -> Option<TableGeom> {
        self.tables.lock().ok()?.get(&block).cloned()
    }

    /// Хит-тест точки в ячейку таблицы.
    // ─── Каретка внутри код-блока ───────────────────────────────────────────

    fn code_geom_of(&self, block: super::model::BlockId) -> Option<CodeGeom> {
        self.codes.lock().ok()?.get(&block).cloned()
    }

    /// Текст код-блока.
    fn code_text(&self, block: super::model::BlockId) -> Option<String> {
        let model = self.model();
        let b = edit::find_block(&model.blocks, block)?;
        let BlockKind::CodeBlock { code, .. } = &b.kind else { return None };
        Some(code.clone())
    }

    fn set_code(&mut self, block: super::model::BlockId, new_code: String) {
        {
            let mut model = self.model();
            let Some(b) = edit::find_block_mut(&mut model.blocks, block) else { return };
            let BlockKind::CodeBlock { code, .. } = &mut b.kind else { return };
            *code = new_code;
        }
        self.after_edit();
    }

    /// Смещение в тексте кода по экранной точке.
    fn code_offset_at(&self, g: &CodeGeom, code: &str, p: Point) -> usize {
        if g.lines.is_empty() {
            return 0;
        }
        let rel_y = p.y - g.origin.y - g.pad;
        let line = ((rel_y / g.line_h).floor().max(0.0) as usize).min(g.lines.len() - 1);
        let (start, end) = g.lines[line];
        let text = &code[start.min(code.len())..end.min(code.len())];
        let local_x = (p.x - g.origin.x - g.pad).max(0.0);
        match self.tm.as_deref() {
            Some(tm) => {
                let ci = tm.hit_test_char(text, g.font_size, local_x);
                start + text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
            }
            None => end,
        }
    }

    fn code_hit(&self, p: Point) -> Option<CodeCaret> {
        let (block, g) = {
            let map = self.codes.lock().ok()?;
            map.iter()
                .find(|(_, g)| {
                    let h = g.pad * 2.0 + g.lines.len() as f32 * g.line_h;
                    p.y >= g.origin.y && p.y <= g.origin.y + h
                })
                .map(|(id, g)| (*id, g.clone()))?
        };
        let code = self.code_text(block)?;
        Some(CodeCaret { block, offset: self.code_offset_at(&g, &code, p) })
    }

    fn code_insert(&mut self, s: &str) {
        let Some(cc) = self.code_caret else { return };
        let Some(mut code) = self.code_text(cc.block) else { return };
        let at = floor_char_boundary(&code, cc.offset);
        code.insert_str(at, s);
        self.code_caret = Some(CodeCaret { offset: at + s.len(), ..cc });
        self.set_code(cc.block, code);
    }

    /// Клавиши в режиме каретки кода. `true` — событие поглощено.
    fn code_key(&mut self, key: &Key, shift: bool) -> bool {
        let Some(cc) = self.code_caret else { return false };
        let Some(code) = self.code_text(cc.block) else {
            self.code_caret = None;
            return false;
        };
        let at = floor_char_boundary(&code, cc.offset);
        let set_offset = |me: &mut Self, off: usize| {
            me.code_caret = Some(CodeCaret { offset: off, ..cc });
            me.caret_on = true;
            me.blink_ms = 0.0;
            me.mark_dirty(DirtyFlags::RENDER);
        };
        match key {
            Key::Escape => {
                self.code_caret = None;
                self.mark_dirty(DirtyFlags::RENDER);
                true
            }
            // В коде Enter — перевод строки, а не разрыв блока; выйти из
            // блока можно Escape или стрелками за его границы.
            Key::Enter if !shift => {
                self.code_insert("\n");
                true
            }
            Key::Tab => {
                self.code_insert("    ");
                true
            }
            Key::Backspace => {
                if at == 0 {
                    return true;
                }
                let prev = code[..at].chars().next_back().map(|c| at - c.len_utf8()).unwrap_or(0);
                let mut next = code.clone();
                next.replace_range(prev..at, "");
                set_offset(self, prev);
                self.set_code(cc.block, next);
                true
            }
            Key::Delete => {
                if at >= code.len() {
                    return true;
                }
                let end = code[at..].chars().next().map(|c| at + c.len_utf8()).unwrap_or(code.len());
                let mut next = code.clone();
                next.replace_range(at..end, "");
                self.set_code(cc.block, next);
                true
            }
            Key::Left => {
                let prev = code[..at].chars().next_back().map(|c| at - c.len_utf8()).unwrap_or(0);
                set_offset(self, prev);
                true
            }
            Key::Right => {
                let next =
                    code[at..].chars().next().map(|c| at + c.len_utf8()).unwrap_or(code.len());
                set_offset(self, next);
                true
            }
            Key::Home | Key::End | Key::Up | Key::Down => {
                let Some(g) = self.code_geom_of(cc.block) else { return true };
                let line = g.line_of(at);
                let (ls, le) = g.lines[line];
                match key {
                    Key::Home => set_offset(self, ls),
                    Key::End => set_offset(self, le),
                    Key::Up if line == 0 => {
                        // Выше кода — обычная каретка документа.
                        self.code_caret = None;
                        self.caret_to_neighbour(cc.block, false);
                    }
                    Key::Down if line + 1 >= g.lines.len() => {
                        self.code_caret = None;
                        self.caret_to_neighbour(cc.block, true);
                    }
                    _ => {
                        let target = if matches!(key, Key::Up) { line - 1 } else { line + 1 };
                        let (ts, te) = g.lines[target];
                        let col = at.saturating_sub(ls);
                        set_offset(self, (ts + col).min(te));
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Каретка в соседний текстовый блок (выход из кода стрелками).
    fn caret_to_neighbour(&mut self, block: super::model::BlockId, down: bool) {
        let target = {
            let model = self.model();
            let order = BlockOrder::of(&model);
            // Код в порядке блоков не участвует — берём ближайший текстовый
            // блок по документу.
            let ids: Vec<super::model::BlockId> = order.ids.clone();
            let doc_idx = model.blocks.iter().position(|b| b.id == block);
            drop(model);
            match doc_idx {
                Some(_) if down => ids.first().copied(),
                _ => ids.last().copied(),
            }
        };
        if let Some(id) = target {
            let len = edit::block_text_len(&self.model(), id);
            self.selection = Some(DocSelection::caret(CaretPos {
                block: id,
                offset: if down { 0 } else { len },
            }));
        }
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn draw_code_caret(&self, list: &mut DisplayList) {
        let Some(cc) = self.code_caret else { return };
        if !self.caret_on {
            return;
        }
        let Some(g) = self.code_geom_of(cc.block) else { return };
        let Some(code) = self.code_text(cc.block) else { return };
        let at = floor_char_boundary(&code, cc.offset);
        let line = g.line_of(at);
        let (start, end) = g.lines[line];
        let prefix = &code[start.min(code.len())..at.clamp(start, end.min(code.len()))];
        let x = match self.tm.as_deref() {
            Some(tm) => tm.measure_text_width(prefix, g.font_size, prefix.chars().count()),
            None => prefix.chars().count() as f32 * g.font_size * 0.6,
        };
        let rect = Rect::new(
            Point::new(g.origin.x + g.pad + x, g.origin.y + g.pad + line as f32 * g.line_h + 1.0),
            Size::new(2.0, (g.line_h - 2.0).max(4.0)),
        );
        list.push_rect(rect, self.style.caret_color, [1.0; 4]);
    }

    fn table_hit(&self, p: Point) -> Option<TableCaret> {
        let (block, row, col, cell_x) = {
            let map = self.tables.lock().ok()?;
            let mut found = None;
            for (id, g) in map.iter() {
                if g.col_widths.is_empty() || g.rows_n == 0 {
                    continue;
                }
                let w = g.total_width();
                let h = g.rows_n as f32 * g.row_h;
                if p.x < g.origin.x
                    || p.x > g.origin.x + w
                    || p.y < g.origin.y
                    || p.y > g.origin.y + h
                {
                    continue;
                }
                let row = (((p.y - g.origin.y) / g.row_h) as usize).min(g.rows_n - 1);
                let mut col = g.col_widths.len() - 1;
                let mut x = g.origin.x;
                for (i, cw) in g.col_widths.iter().enumerate() {
                    if p.x < x + cw {
                        col = i;
                        break;
                    }
                    x += cw;
                }
                found = Some((*id, row, col, g.col_x(col)));
                break;
            }
            found?
        };
        let text = self.table_cell_text(block, row, col)?;
        let offset = match self.tm.as_deref() {
            Some(tm) => {
                let local_x = p.x - cell_x - self.style.table_cell_padding_h;
                let ci =
                    tm.hit_test_char_styled(&text, self.style.text_size, local_x.max(0.0), None);
                text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len())
            }
            None => text.len(),
        };
        Some(TableCaret { block, row, col, offset })
    }

    /// Плоский текст ячейки (`row == 0` — шапка). Отсутствующая ячейка
    /// ragged-строки — пустая строка.
    fn table_cell_text(
        &self,
        block: super::model::BlockId,
        row: usize,
        col: usize,
    ) -> Option<String> {
        let model = self.model();
        let b = edit::find_block(&model.blocks, block)?;
        let BlockKind::Table { headers, rows, .. } = &b.kind else { return None };
        let cell = if row == 0 { headers.get(col) } else { rows.get(row.checked_sub(1)?)?.get(col) };
        Some(cell.map(|t| t.text()).unwrap_or_default())
    }

    /// Записывает плоский текст в ячейку; недостающие ячейки добивает пустыми.
    fn set_table_cell(&mut self, tc: TableCaret, new_text: String) {
        {
            let mut model = self.model();
            let Some(b) = edit::find_block_mut(&mut model.blocks, tc.block) else { return };
            let BlockKind::Table { headers, rows, .. } = &mut b.kind else { return };
            let slot = if tc.row == 0 {
                while headers.len() <= tc.col {
                    headers.push(InlineText::default());
                }
                headers.get_mut(tc.col)
            } else {
                let Some(data_row) = rows.get_mut(tc.row - 1) else { return };
                while data_row.len() <= tc.col {
                    data_row.push(InlineText::default());
                }
                data_row.get_mut(tc.col)
            };
            if let Some(cell) = slot {
                *cell = InlineText::plain(new_text);
            }
        }
        self.after_edit();
    }

    /// Вставка текста в позицию каретки таблицы.
    fn table_insert(&mut self, s: &str) {
        let Some(tc) = self.table_caret else { return };
        let Some(mut text) = self.table_cell_text(tc.block, tc.row, tc.col) else { return };
        let at = floor_char_boundary(&text, tc.offset);
        text.insert_str(at, s);
        self.table_caret = Some(TableCaret { offset: at + s.len(), ..tc });
        self.set_table_cell(tc, text);
    }

    /// Перевод каретки в другую ячейку (смещение в конец её текста — как
    /// при переходе Tab'ом; `offset` подрежется при отрисовке).
    fn table_goto(&mut self, tc: TableCaret, row: usize, col: usize, at_end: bool) {
        let offset = if at_end {
            self.table_cell_text(tc.block, row, col).map(|t| t.len()).unwrap_or(0)
        } else {
            0
        };
        self.table_caret = Some(TableCaret { block: tc.block, row, col, offset });
        self.caret_on = true;
        self.blink_ms = 0.0;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Клавиши в режиме каретки таблицы. `true` — событие поглощено.
    fn table_key(&mut self, key: &Key, shift: bool) -> bool {
        let Some(tc) = self.table_caret else { return false };
        let Some(text) = self.table_cell_text(tc.block, tc.row, tc.col) else {
            self.table_caret = None;
            return false;
        };
        let Some(g) = self.table_geom_of(tc.block) else {
            self.table_caret = None;
            return false;
        };
        let (cols, rows_n) = (g.col_widths.len().max(1), g.rows_n.max(1));
        let off = floor_char_boundary(&text, tc.offset);
        match key {
            Key::Escape => {
                self.table_caret = None;
                self.mark_dirty(DirtyFlags::RENDER);
                true
            }
            Key::Left => {
                if off > 0 {
                    let new = edit::prev_char_boundary(&text, off);
                    self.table_caret = Some(TableCaret { offset: new, ..tc });
                    self.caret_on = true;
                    self.mark_dirty(DirtyFlags::RENDER);
                } else if tc.col > 0 {
                    self.table_goto(tc, tc.row, tc.col - 1, true);
                } else if tc.row > 0 {
                    self.table_goto(tc, tc.row - 1, cols - 1, true);
                }
                true
            }
            Key::Right => {
                if off < text.len() {
                    let new = edit::next_char_boundary(&text, off);
                    self.table_caret = Some(TableCaret { offset: new, ..tc });
                    self.caret_on = true;
                    self.mark_dirty(DirtyFlags::RENDER);
                } else if tc.col + 1 < cols {
                    self.table_goto(tc, tc.row, tc.col + 1, false);
                } else if tc.row + 1 < rows_n {
                    self.table_goto(tc, tc.row + 1, 0, false);
                }
                true
            }
            Key::Up => {
                if tc.row > 0 {
                    self.table_goto(tc, tc.row - 1, tc.col, true);
                }
                true
            }
            Key::Down | Key::Enter => {
                if tc.row + 1 < rows_n {
                    self.table_goto(tc, tc.row + 1, tc.col, true);
                } else if matches!(key, Key::Enter) {
                    // Внизу таблицы Enter выходит из режима ячейки.
                    self.table_caret = None;
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                true
            }
            Key::Tab => {
                if shift {
                    if tc.col > 0 {
                        self.table_goto(tc, tc.row, tc.col - 1, true);
                    } else if tc.row > 0 {
                        self.table_goto(tc, tc.row - 1, cols - 1, true);
                    }
                } else if tc.col + 1 < cols {
                    self.table_goto(tc, tc.row, tc.col + 1, true);
                } else if tc.row + 1 < rows_n {
                    self.table_goto(tc, tc.row + 1, 0, true);
                }
                true
            }
            Key::Home => {
                self.table_caret = Some(TableCaret { offset: 0, ..tc });
                self.mark_dirty(DirtyFlags::RENDER);
                true
            }
            Key::End => {
                self.table_caret = Some(TableCaret { offset: text.len(), ..tc });
                self.mark_dirty(DirtyFlags::RENDER);
                true
            }
            Key::Backspace => {
                if off > 0 {
                    self.checkpoint(EditClass::Typing);
                    let start = edit::prev_char_boundary(&text, off);
                    let mut new_text = text;
                    new_text.replace_range(start..off, "");
                    self.table_caret = Some(TableCaret { offset: start, ..tc });
                    self.set_table_cell(tc, new_text);
                }
                true
            }
            Key::Delete => {
                if off < text.len() {
                    self.checkpoint(EditClass::Typing);
                    let end = edit::next_char_boundary(&text, off);
                    let mut new_text = text;
                    new_text.replace_range(off..end, "");
                    self.table_caret = Some(TableCaret { offset: off, ..tc });
                    self.set_table_cell(tc, new_text);
                }
                true
            }
            _ => false,
        }
    }

    /// Рамка редактируемой ячейки и каретка в ней.
    fn draw_table_caret(&self, list: &mut DisplayList) {
        let Some(tc) = self.table_caret else { return };
        let Some(g) = self.table_geom_of(tc.block) else { return };
        if tc.col >= g.col_widths.len() || tc.row >= g.rows_n {
            return;
        }
        let s = &self.style;
        let x0 = g.col_x(tc.col);
        let y0 = g.origin.y + tc.row as f32 * g.row_h;
        let w = g.col_widths[tc.col];
        let cell = Rect::new(Point::new(x0, y0), Size::new(w, g.row_h));
        for edge in edges(cell) {
            list.push_rect(edge, s.caret_color, [0.0; 4]);
        }
        if !self.caret_on {
            return;
        }
        let text = self.table_cell_text(tc.block, tc.row, tc.col).unwrap_or_default();
        let off = floor_char_boundary(&text, tc.offset);
        let prefix = &text[..off];
        let px = match self.tm.as_deref() {
            Some(tm) => tm.measure_text_width_styled(
                prefix,
                s.text_size,
                prefix.chars().count(),
                tc.row == 0,
                None,
            ),
            None => 0.0,
        };
        let cx = (x0 + s.table_cell_padding_h + px).min(x0 + w - 3.0);
        list.push_rect(
            Rect::new(
                Point::new(cx, y0 + s.table_cell_padding_v),
                Size::new(2.0, (g.row_h - s.table_cell_padding_v * 2.0).max(4.0)),
            ),
            s.caret_color,
            [1.0; 4],
        );
    }
}

/// Ближайшая граница символа не выше `at` (после внешних правок смещение
/// могло уйти внутрь многобайтового символа).
/// Расстояние от точки до отрезка — зона клика по линейной фигуре.
fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 0.001 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    let (cx, cy) = (a.0 + dx * t, a.1 + dy * t);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

fn floor_char_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

const SLASH_ROW_H: f32 = 26.0;
const SLASH_MENU_W: f32 = 230.0;
const SLASH_MAX_ROWS: usize = 8;
const TOOLBAR_BTN_W: f32 = 30.0;
const TOOLBAR_H: f32 = 26.0;
const HANDLE_W: f32 = 16.0;
const HANDLE_H: f32 = 18.0;

/// Четыре стороны прямоугольника толщиной 1px (рамка без заливки).
fn edges(r: Rect) -> [Rect; 4] {
    [
        Rect::new(r.origin, Size::new(r.size.width, 1.0)),
        Rect::new(
            Point::new(r.origin.x, r.origin.y + r.size.height - 1.0),
            Size::new(r.size.width, 1.0),
        ),
        Rect::new(r.origin, Size::new(1.0, r.size.height)),
        Rect::new(
            Point::new(r.origin.x + r.size.width - 1.0, r.origin.y),
            Size::new(1.0, r.size.height),
        ),
    ]
}

fn dist(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo - v
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

impl Element for DocumentEditorElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<DocumentEditor>() else { return };
        self.read_only = w.read_only;
        self.on_change = w.on_change.clone();
        let links_changed = self.links.is_some() != w.links.is_some();
        self.links = w.links.clone();
        self.media = w.media.clone();
        self.embeds = w.embeds.clone();
        self.embed_ctx = w.embed_ctx.clone();
        self.on_drop_file = w.on_drop_file.clone();
        self.on_focus_lost = w.on_focus_lost.clone();
        self.on_block_drop = w.on_block_drop.clone();
        self.on_drop_data = w.on_drop_data.clone();
        self.block_drag_type = w.block_drag_type.clone();
        self.plain = w.plain;
        if self.placeholder != w.placeholder || self.heading_placeholder != w.heading_placeholder {
            self.placeholder = w.placeholder.clone();
            self.heading_placeholder = w.heading_placeholder.clone();
            self.rebuild = true;
        }
        if w.autofocus && !self.autofocus {
            self.focus_request_pending = true;
            self.focused = true;
        }
        self.autofocus = w.autofocus;
        self.on_context_menu = w.on_context_menu.clone();
        self.fill_height = w.fill_height;
        if self.layout != w.layout {
            let was_free = self.layout.free;
            self.layout = w.layout;
            let _ = was_free;
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        if links_changed {
            self.rebuild = true;
        }
        if w.model_epoch != self.model_epoch {
            self.model_epoch = w.model_epoch;
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        if let Some(h) = &w.handle {
            if !Arc::ptr_eq(&self.model, &h.model) {
                self.model = h.model.clone();
                self.revision = Some(h.revision);
                self.ops = h.ops.clone();
                self.selected = Some(h.selected);
                self.history = h.history.clone();
                self.history_state = Some(h.history_state);
                self.block_sel_sig = Some(h.block_selection);
                self.block_sel.clear();
                self.block_anchor = None;
                self.object_sel = None;
                self.marquee = None;
                // Модель ручки уже загружена из своего исходника — берём
                // её отметку, иначе смена страницы перепарсила бы поверх
                // несохранённых правок.
                self.source_fp = h.loaded_fp();
                self.selection = None;
                self.table_caret = None;
                self.rebuild = true;
            }
        }
        let fp = fingerprint(&w.source);
        if Some(fp) != self.source_fp {
            self.source_fp = Some(fp);
            if let Some(h) = &w.handle {
                h.set_loaded_fp(fp);
            }
            *lock(&self.model) = parse_document(&w.source);
            self.selection = None;
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        if self.autofocus && self.focused && self.selection.is_none()
            && self.table_caret.is_none() && self.code_caret.is_none() && self.object_sel.is_none()
        {
            // Автофокус: каретка в начало первого блока, чтобы можно было
            // сразу печатать (правка карточки на месте).
            let first = self.model().blocks.first().map(|b| (b.id, b.kind.text().is_some()));
            if let Some((id, true)) = first {
                self.selection = Some(DocSelection::caret(CaretPos { block: id, offset: 0 }));
                self.caret_on = true;
                self.blink_ms = 0.0;
            }
        }
        self.apply_pending_ops(ctx);
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.tm = tree.text_measure.clone();
        // Приём дропа файлов (события Drop идут только по реестру целей).
        tree.register_drop_target(self.id);
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        // С детьми дерево зовёт layout с tight-размером (min == max) — его
        // и принимаем; без детей (пустой документ) — высота одной строки.
        let tight = constraints.min_height.is_finite()
            && (constraints.min_height - constraints.max_height).abs() < 0.5
            && constraints.min_height > 0.0;
        let height = if tight {
            constraints.min_height
        } else {
            self.style.doc_padding * 2.0 + self.style.line_h(self.style.text_size)
        };
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn min_max_dimensions(
        &self,
        _parent_width: f32,
        parent_height: f32,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        // Документ короче видимой области всё равно занимает её целиком:
        // иначе клик и правый клик ниже последнего блока уходят мимо
        // редактора (в пустоту скроллера) и каретку поставить некуда.
        let min_h = (self.fill_height && parent_height.is_finite() && parent_height > 0.0)
            .then_some(parent_height);
        (None, None, min_h, None)
    }

    fn layout_hint(&self) -> LayoutHint {
        let s = &self.style;
        if self.layout.free {
            // Свободная раскладка: блоки — Positioned-обёртки, размер
            // холста держит распорка (см. build_children).
            return LayoutHint::Stack { expand: false };
        }
        // Колонка как в Notion: листья сами ограничивают свою ширину
        // max_content_width (см. rows::clamp_width), а корень центрирует.
        LayoutHint::Column {
            gap: s.block_spacing,
            cross_align: CrossAxisAlignment::Center,
            main_align: MainAxisAlignment::Start,
            padding_left: s.doc_padding,
            padding_top: s.doc_padding,
            padding_right: s.doc_padding,
            padding_bottom: s.doc_padding,
            expand: false,
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        self.rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        let env = BuildEnv {
            style: self.style.clone(),
            geom: self.geom.clone(),
            tables: self.tables.clone(),
            codes: self.codes.clone(),
            links: self.links.clone(),
            media: self.media.clone(),
            embeds: self.embeds.clone(),
            embed_ctx: self.embed_ctx.clone(),
            placeholder: self.placeholder.clone(),
            heading_placeholder: self.heading_placeholder.clone(),
        };
        let model = self.model();
        // Каждый верхнеуровневый блок обёрнут Chrome'ом, который публикует
        // свой прямоугольник: у таблицы, кода, медиа и разделителя нет
        // текстовых строк, а ручка ⋮⋮ и цель дропа нужны и им.
        let wrap = |b: &DocBlock| -> Box<dyn Widget> {
            Box::new(
                Chrome::new()
                    .center(true)
                    .track(b.id, self.blocks.clone())
                    .child(block_widget(b, &env)),
            )
        };
        if !self.layout.free {
            return model.blocks.iter().map(wrap).collect();
        }
        // Свободная раскладка: блок с координатами стоит на холсте, блок
        // без них остаётся в колонке потока — страница, которую ещё не
        // трогали мышью, выглядит ровно как раньше.
        let pad = self.style.doc_padding;
        let rects = self.blocks.lock().ok().map(|m| m.clone()).unwrap_or_default();
        let origin = self.bounds.origin;
        let mut flow: Vec<Box<dyn Widget>> = Vec::new();
        let mut pinned: Vec<Box<dyn Widget>> = Vec::new();
        let mut extent = (pad, pad);
        for b in model.blocks.iter() {
            let Some((x, y)) = free::pos_of(&b.attrs) else {
                flow.push(wrap(b));
                continue;
            };
            let w = free::width_of(&b.attrs).unwrap_or(self.layout.block_width);
            let h = rects
                .get(&b.id)
                .map(|r| r.size.height)
                .unwrap_or_else(|| estimate_height(b, &self.style));
            extent.0 = extent.0.max(x + w + pad);
            extent.1 = extent.1.max(y + h + pad);
            let _ = origin;
            pinned.push(Box::new(
                Chrome::new()
                    .absolute(x, y)
                    .fixed_width(w)
                    .track(b.id, self.blocks.clone())
                    .child(block_widget(b, &env)),
            ));
        }
        let mut out: Vec<Box<dyn Widget>> = Vec::with_capacity(pinned.len() + 2);
        if !flow.is_empty() {
            // Колонка потока держит ширину видимой области: холст
            // прокручивается и по горизонтали, ширина у него бесконечная,
            // и без этого колонка ужалась бы к своим строкам и уехала в
            // левый край вместо центра.
            out.push(Box::new(
                Chrome::new()
                    .center(true)
                    .fill_width(true)
                    .gap(self.style.block_spacing)
                    .padding(pad, pad, pad, pad)
                    .children(flow),
            ));
        }
        out.extend(pinned);
        out.push(Box::new(Chrome::extent(extent.0, extent.1)));
        out
    }

    fn clear_rebuild(&mut self) {
        self.rebuild = false;
        // Чистим геометрию исчезнувших блоков.
        let mut alive = HashSet::new();
        self.model().for_each(&mut |b| {
            alive.insert(b.id);
        });
        if let Ok(mut map) = self.geom.lock() {
            map.retain(|id, _| alive.contains(id));
        }
        if let Ok(mut map) = self.tables.lock() {
            map.retain(|id, _| alive.contains(id));
        }
        if let Ok(mut map) = self.blocks.lock() {
            map.retain(|id, _| alive.contains(id));
        }
        if let Ok(mut map) = self.codes.lock() {
            map.retain(|id, _| alive.contains(id));
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, clip: Rect) {
        self.draw_grid(list, clip);
        self.draw_block_bounds(list);
        self.draw_selection(list);
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        // Сбрасываем хит-зоны панелей; отрисовка ниже заполнит актуальные.
        if let Ok(mut ui) = self.ui_rects.lock() {
            *ui = UiRects::default();
        }
        if self.read_only {
            return;
        }
        self.draw_drag_ui(list);
        if !self.focused {
            return;
        }
        self.draw_slash_menu(list);
        self.draw_wiki_menu(list);
        self.draw_toolbar(list);
        self.draw_table_caret(list);
        self.draw_code_caret(list);
        let Some(pos) = self.caret() else { return };
        let Some(rect) = self.caret_rect(pos) else { return };
        if self.caret_on {
            list.push_rect(rect, self.style.caret_color, [1.0; 4]);
        }
        // Preedit IME: текст с подчёркиванием у каретки.
        if let Some(pre) = &self.preedit {
            if !pre.is_empty() {
                let fs = self.style.text_size;
                let w = self
                    .tm
                    .as_deref()
                    .map(|tm| tm.measure_text_width(pre, fs, pre.chars().count()))
                    .unwrap_or(0.0);
                let text_rect = Rect::new(
                    Point::new(rect.origin.x + 3.0, rect.origin.y),
                    Size::new(w, 0.0),
                );
                list.push_rect(
                    Rect::new(
                        Point::new(rect.origin.x + 2.0, rect.origin.y - 1.0),
                        Size::new(w + 4.0, rect.size.height + 2.0),
                    ),
                    self.style.selection_color,
                    [2.0; 4],
                );
                list.push_text_aligned(
                    pre,
                    text_rect,
                    self.style.text_color,
                    fs,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    400,
                );
                list.push_rect(
                    Rect::new(
                        Point::new(rect.origin.x + 3.0, rect.origin.y + rect.size.height - 1.0),
                        Size::new(w, 1.0),
                    ),
                    self.style.caret_color,
                    [0.0; 4],
                );
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        // Метрика текста могла быть установлена в дерево после mount
        // (пересоздание рендера, headless-харнес) — подхватываем из контекста.
        if self.tm.is_none() {
            self.tm = ctx.text_measure.clone();
        }
        match event {
            Event::MouseDown { button: MouseButton::Left, position } => {
                // Точка правого клика актуальна только до следующего
                // действия: дальше вставка снова идёт «у каретки».
                self.menu_pos = None;
                if !self.bounds.contains(*position) {
                    if self.focused {
                        self.focused = false;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Ignored;
                }
                self.focused = true;
                ctx.set_focused_text(String::new());
                // Ctrl+клик — открытие ссылки через провайдера хоста.
                if ctx.modifiers.ctrl {
                    if let Some(link) = self.link_at(*position) {
                        if let Some(provider) = &self.links {
                            provider.open_link(&link);
                        }
                        return EventResult::Handled;
                    }
                }
                // Клик по панелям, нарисованным поверх (тулбар, slash-меню).
                if !self.read_only {
                    let (toolbar_hit, slash_hit) = {
                        let ui = self.ui_rects.lock().unwrap_or_else(|e| e.into_inner());
                        let t = ui.toolbar.and_then(|(rect, btn_w)| {
                            rect.contains(*position)
                                .then(|| ((position.x - rect.origin.x) / btn_w) as usize)
                        });
                        let sl = ui.slash.and_then(|(rect, count)| {
                            (rect.contains(*position) && count > 0).then(|| {
                                (((position.y - rect.origin.y) / SLASH_ROW_H) as usize)
                                    .min(count - 1)
                            })
                        });
                        (t, sl)
                    };
                    if let Some(btn) = toolbar_hit {
                        match btn {
                            0 => self.toggle_inline(|s| s.bold, |s, v| s.bold = v),
                            1 => self.toggle_inline(|s| s.italic, |s, v| s.italic = v),
                            2 => self.toggle_inline(|s| s.strike, |s, v| s.strike = v),
                            _ => self.toggle_inline(|s| s.code, |s, v| s.code = v),
                        }
                        return EventResult::Handled;
                    }
                    if let Some(idx) = slash_hit {
                        let action = self
                            .slash
                            .as_ref()
                            .and_then(|sl| {
                                filter_items(&self.slash_items, &sl.query)
                                    .get(idx)
                                    .map(|it| it.action.clone())
                            });
                        if let Some(action) = action {
                            self.apply_slash(action);
                        }
                        return EventResult::Handled;
                    }
                }
                // Захват ручки ⋮⋮ — перетаскивание блока (в свободной
                // раскладке — перенос по холсту), правая кромка — ширина.
                if !self.read_only {
                    let (handle_hit, resize_hit) = {
                        let ui = self.ui_rects.lock().unwrap_or_else(|e| e.into_inner());
                        (
                            ui.handle.and_then(|(rect, block)| {
                                rect.contains(*position).then_some(block)
                            }),
                            ui.resize.and_then(|(rect, block)| {
                                rect.contains(*position).then_some(block)
                            }),
                        )
                    };
                    let _ = resize_hit;
                    if let Some((block, mode)) = self.size_handle_at(*position) {
                        if self.start_free_drag(block, *position, mode) {
                            ctx.capture();
                            return EventResult::Handled;
                        }
                    }
                    if let Some(block) = handle_hit {
                        if self.layout.free {
                            if self.start_free_drag(block, *position, FreeDragMode::Move) {
                                ctx.capture();
                                return EventResult::Handled;
                            }
                        }
                        self.drag = Some(DragBlock {
                            block,
                            start: *position,
                            current: *position,
                            started: false,
                            target: None,
                            announced: false,
                        });
                        ctx.capture();
                        return EventResult::Handled;
                    }
                }
                self.close_slash();
                // Клик по гаттеру: чекбокс/шеврон.
                if !self.read_only {
                    if let Some((id, action)) = self.gutter_hit(*position) {
                        self.checkpoint(EditClass::Structure);
                        let mut model = self.model();
                        match action {
                            GutterAction::ToggleTodo => edit::toggle_todo(&mut model, id),
                            GutterAction::ToggleCollapse => edit::toggle_collapse(&mut model, id),
                        }
                        drop(model);
                        self.after_edit();
                        return EventResult::Handled;
                    }
                }
                // Выделение блоков: Ctrl+клик переключает блок, Shift+клик
                // при выделении тянет диапазон, нажатие в пустом месте
                // (поля, холст, ниже текста) начинает рамку — без протяжки
                // на отпускании ставится обычная каретка. Простой клик по
                // блоку снимает выделение.
                if !self.read_only && !self.plain {
                    let top = self.top_block_at(*position);
                    if ctx.modifiers.ctrl {
                        if let Some(id) = top {
                            self.toggle_block_sel(id);
                            return EventResult::Handled;
                        }
                    }
                    if ctx.modifiers.shift && !self.block_sel.is_empty() {
                        if let Some(id) = top {
                            self.select_block_range(id);
                            return EventResult::Handled;
                        }
                    }
                    if top.is_none() && self.shape_at(*position).is_none() {
                        self.marquee =
                            Some(Marquee { start: *position, current: *position, active: false });
                        ctx.capture();
                        return EventResult::Handled;
                    }
                    self.clear_block_sel();
                }
                // Клик по фигуре: каретке внутри неё места нет — она просто
                // становится текущим блоком (панель свойств, ручки размера).
                // Врезка-объект (доска, диаграмма) — так же, но её виджеты
                // чужие: приложение фокус редактору над ней не даёт
                // (`text_input_hit`), и редактор его не удерживает — иначе
                // каретка в заголовке над доской оставалась бы живой.
                if !self.read_only {
                    let foreign = self.embed_at(*position).is_some();
                    if let Some(id) = self.shape_at(*position) {
                        self.select_object(id, !foreign);
                        return EventResult::Handled;
                    }
                    if foreign {
                        self.drop_focus();
                        return EventResult::Handled;
                    }
                }
                // Клик в ячейку таблицы — режим каретки таблицы.
                if !self.read_only {
                    if let Some(tc) = self.table_hit(*position) {
                        self.table_caret = Some(tc);
                        self.code_caret = None;
                        self.selection = None;
                        self.goal_x = None;
                        self.caret_on = true;
                        self.blink_ms = 0.0;
                        self.publish_selection();
                        self.mark_dirty(DirtyFlags::RENDER);
                        return EventResult::Handled;
                    }
                    // Клик в код — режим каретки кода: у код-блока нет
                    // текстовых строк документа, каретка своя.
                    if let Some(cc) = self.code_hit(*position) {
                        self.code_caret = Some(cc);
                        self.table_caret = None;
                        self.selection = None;
                        self.goal_x = None;
                        self.caret_on = true;
                        self.blink_ms = 0.0;
                        self.publish_selection();
                        self.mark_dirty(DirtyFlags::RENDER);
                        return EventResult::Handled;
                    }
                }
                if let Some(pos) = self.hit_caret(*position) {
                    self.table_caret = None;
                    self.code_caret = None;
                    self.goal_x = None;
                    self.set_caret(pos, ctx.modifiers.shift);
                    if !ctx.modifiers.shift {
                        self.mouse_selecting = true;
                        ctx.capture();
                    }
                }
                EventResult::Handled
            }
            Event::MouseDown { button: MouseButton::Right, position } => {
                if !self.bounds.contains(*position) || self.read_only {
                    return EventResult::Ignored;
                }
                let Some(cb) = self.on_context_menu.clone() else {
                    return EventResult::Ignored;
                };
                // Внутри чужой врезки без своего объекта (вложенная
                // страница) меню документа не по адресу.
                let foreign = self.embed_at(*position).is_some();
                let object = self.shape_at(*position);
                if foreign && object.is_none() {
                    self.drop_focus();
                    return EventResult::Ignored;
                }
                // Как левый клик: фокус и каретка в точку клика, панели
                // закрыть. Клик внутри непустого выделения его сохраняет —
                // меню действует над выделенным диапазоном.
                self.focused = !foreign;
                ctx.set_focused_text(String::new());
                self.close_slash();
                self.wiki = None;
                let keep_blocks =
                    self.top_block_at(*position).is_some_and(|t| self.block_sel.contains(&t));
                if keep_blocks {
                    // Меню над выделенными блоками действует над всеми ними —
                    // выделение не трогаем.
                } else if let Some(id) = object {
                    // Меню над фигурой или объектом: он и становится текущим
                    // блоком — каретка в соседний текст не прыгает, «Удалить
                    // блок» удаляет именно его.
                    self.select_object(id, !foreign);
                } else if let Some(pos) = self.hit_caret(*position) {
                    let keep = self
                        .selection
                        .filter(|s| !s.is_caret())
                        .map(|s| {
                            let model = self.model();
                            let order = BlockOrder::of(&model);
                            let (a, b) = s.ordered(&order);
                            order.cmp(a, pos).is_le() && order.cmp(pos, b).is_le()
                        })
                        .unwrap_or(false);
                    if !keep {
                        self.table_caret = None;
                        self.code_caret = None;
                        self.goal_x = None;
                        self.set_caret(pos, false);
                    }
                } else if self.caret().is_none() {
                    self.caret_to_last_block();
                }
                // Точка вставки для свободной раскладки — в координатах
                // холста, уже с привязкой к сетке.
                self.menu_pos = self.layout.free.then(|| {
                    Point::new(
                        self.layout.snapped(position.x - self.bounds.origin.x),
                        self.layout.snapped(position.y - self.bounds.origin.y),
                    )
                });
                self.mark_dirty(DirtyFlags::RENDER);
                cb(*position);
                EventResult::Handled
            }
            Event::MouseMove(position) => {
                if let Some(mut m) = self.marquee {
                    m.current = *position;
                    if !m.active
                        && (m.current.x - m.start.x).abs() + (m.current.y - m.start.y).abs() > 4.0
                    {
                        m.active = true;
                    }
                    self.marquee = Some(m);
                    if m.active {
                        let ids = self.blocks_in_rect(rect_from_points(m.start, m.current));
                        self.select_blocks(ids);
                    }
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if let Some(mode) = self.free_drag.as_ref().map(|d| d.mode) {
                    ctx.set_cursor(mode.cursor());
                    self.update_free_drag(*position);
                    let announce = self
                        .free_drag
                        .as_ref()
                        .map(|d| d.mode == FreeDragMode::Move && d.moved && !d.announced)
                        .unwrap_or(false);
                    if announce {
                        let block = self.free_drag.as_ref().map(|d| d.block).unwrap();
                        let done = self.announce_block_drag(block, *position, ctx);
                        if let Some(d) = &mut self.free_drag {
                            d.announced = done;
                        }
                    }
                    return EventResult::Handled;
                }
                if let Some(drag) = &mut self.drag {
                    drag.current = *position;
                    if !drag.started
                        && (drag.current.x - drag.start.x).abs()
                            + (drag.current.y - drag.start.y).abs()
                            > 4.0
                    {
                        drag.started = true;
                    }
                    if drag.started {
                        let src = drag.block;
                        let announce = !drag.announced;
                        let over_object = self.sized_embed_at(*position).is_some_and(|id| id != src);
                        let target = if over_object {
                            None
                        } else {
                            self.drop_target(*position).filter(|(t, _)| *t != src)
                        };
                        if announce && self.announce_block_drag(src, *position, ctx) {
                            if let Some(drag) = &mut self.drag {
                                drag.announced = true;
                            }
                        }
                        if let Some(drag) = &mut self.drag {
                            drag.target = target;
                        }
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                }
                // Блок под курсором: ручка ⋮⋮ и подсветка габаритов
                // показываются по наведению, не дожидаясь клика.
                if !self.read_only && self.bounds.contains(*position) {
                    let hovered = self.row_at(*position);
                    if hovered != self.hover_block {
                        self.hover_block = hovered;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    // Курсор над хваталками блока: ручка ⋮⋮ тащит блок,
                    // правая кромка (свободная раскладка) тянет ширину.
                    if let Some((_, mode)) = self.size_handle_at(*position) {
                        ctx.set_cursor(mode.cursor());
                    } else if let Some(block) = self.hover_block {
                        if self.handle_rect(block).is_some_and(|r| r.contains(*position)) {
                            ctx.set_cursor(CursorIcon::Grab);
                        } else if self.shape_at(*position).is_some() {
                            ctx.set_cursor(CursorIcon::Pointer);
                        }
                    }
                }
                if self.mouse_selecting {
                    if let Some(pos) = self.hit_caret(*position) {
                        if let Some(sel) = &mut self.selection {
                            if sel.head != pos {
                                sel.head = pos;
                                self.mark_dirty(DirtyFlags::RENDER);
                            }
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, position } => {
                if let Some(m) = self.marquee.take() {
                    if !m.active {
                        // Клик без протяжки — каретка в ближайшую строку,
                        // как и раньше.
                        self.clear_block_sel();
                        if let Some(pos) = self.hit_caret(m.start) {
                            self.table_caret = None;
                            self.code_caret = None;
                            self.goal_x = None;
                            self.set_caret(pos, false);
                        }
                    }
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if let Some(drag) = self.free_drag.take() {
                    // Перенос за ⋮⋮ мог закончиться над карточкой доски —
                    // тогда блок забирает хост.
                    if drag.mode == FreeDragMode::Move
                        && drag.moved
                        && self.host_took_block(*position, drag.block)
                    {
                        return EventResult::Handled;
                    }
                    // Даже без движения блок мог только что закрепиться —
                    // это правка модели, её надо сохранить.
                    self.after_edit();
                    // Клик по ⋮⋮ без переноса выделяет блок.
                    if drag.mode == FreeDragMode::Move && !drag.moved {
                        if let Some(top) = self.top_level_of(drag.block) {
                            self.select_blocks(vec![top]);
                        }
                    }
                    return EventResult::Handled;
                }
                if let Some(drag) = self.drag.take() {
                    if !drag.started {
                        if let Some(top) = self.top_level_of(drag.block) {
                            self.select_blocks(vec![top]);
                        }
                    }
                    if drag.started {
                        if self.host_took_block(*position, drag.block) {
                            self.mark_dirty(DirtyFlags::RENDER);
                            return EventResult::Handled;
                        }
                        if let Some((target, before)) = drag.target {
                            self.checkpoint(EditClass::Structure);
                            let moved = {
                                let mut model = self.model();
                                edit::move_block(&mut model, drag.block, target, before)
                            };
                            if moved {
                                self.after_edit();
                            } else {
                                self.history().discard_last_checkpoint();
                            }
                        }
                    }
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if self.mouse_selecting {
                    self.mouse_selecting = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            // Перенос блока шёл drag'ом дерева: MouseUp приложение не шлёт,
            // жест заканчивает DragEnd. Если DropArea хоста забрал блок, он
            // уже поставил в очередь `DeleteBlock`; здесь — только свой
            // финал: закрепить (холст) либо переставить (поток).
            Event::DragEnd { .. } => {
                if let Some(drag) = self.free_drag.take() {
                    let _ = drag;
                    self.after_edit();
                    return EventResult::Handled;
                }
                if let Some(drag) = self.drag.take() {
                    if drag.started {
                        if let Some((target, before)) = drag.target {
                            self.checkpoint(EditClass::Structure);
                            let moved = {
                                let mut model = self.model();
                                edit::move_block(&mut model, drag.block, target, before)
                            };
                            if moved {
                                self.after_edit();
                            } else {
                                self.history().discard_last_checkpoint();
                            }
                        }
                    }
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::Drop { position, data } => {
                if self.read_only || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if data.drag_type != crate::input::DragData::TYPE_FILE {
                    let Some(cb) = self.on_drop_data.clone() else {
                        return EventResult::Ignored;
                    };
                    return if cb(*position, data) { EventResult::Handled } else { EventResult::Ignored };
                }
                let Some(cb) = self.on_drop_file.clone() else {
                    return EventResult::Ignored;
                };
                let path = std::path::PathBuf::from(data.payload.clone());
                let token = self.insert_pending_media(*position, &path);
                cb(path, token);
                EventResult::Handled
            }
            Event::DoubleClick { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if let Some(pos) = self.hit_caret(*position) {
                    let model = self.model();
                    let text = edit::find_block(&model.blocks, pos.block)
                        .and_then(|b| b.kind.text())
                        .map(|t| t.text())
                        .unwrap_or_default();
                    drop(model);
                    let (s, e) = edit::word_bounds(&text, pos.offset);
                    self.selection = Some(DocSelection {
                        anchor: CaretPos { block: pos.block, offset: s },
                        head: CaretPos { block: pos.block, offset: e },
                    });
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                EventResult::Handled
            }
            Event::KeyDown(key) => {
                self.menu_pos = None;
                if !self.focused {
                    return EventResult::Ignored;
                }
                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;
                let editable = !self.read_only;
                // Каретка в ячейке таблицы — свой обработчик навигации/правок
                // (ctrl-комбинации, напр. undo, проходят дальше).
                if editable && !ctrl && self.code_caret.is_some() {
                    if self.code_key(key, shift) {
                        return EventResult::Handled;
                    }
                }
                if editable && !ctrl && self.table_caret.is_some() {
                    if self.table_key(key, shift) {
                        return EventResult::Handled;
                    }
                }
                // Автокомплит [[ перехватывает навигацию.
                if let Some(w) = &mut self.wiki {
                    let count = w.candidates.len();
                    match key {
                        Key::Up => {
                            w.selected = w.selected.saturating_sub(1);
                            self.mark_dirty(DirtyFlags::RENDER);
                            return EventResult::Handled;
                        }
                        Key::Down => {
                            if count > 0 {
                                w.selected = (w.selected + 1).min(count - 1);
                            }
                            self.mark_dirty(DirtyFlags::RENDER);
                            return EventResult::Handled;
                        }
                        Key::Enter | Key::Tab => {
                            let idx = w.selected;
                            self.apply_wiki(idx);
                            return EventResult::Handled;
                        }
                        Key::Escape => {
                            self.close_wiki();
                            return EventResult::Handled;
                        }
                        Key::Left | Key::Right => {
                            self.close_wiki();
                        }
                        Key::Backspace => {
                            if w.query.pop().is_none() {
                                self.close_wiki();
                            } else {
                                let q = self.wiki.as_ref().map(|w| w.query.clone());
                                if let Some(q) = q {
                                    let cands = self.wiki_candidates(&q);
                                    if let Some(w) = &mut self.wiki {
                                        w.candidates = cands;
                                        w.selected = 0;
                                    }
                                }
                            }
                            self.checkpoint(EditClass::Deleting);
                            self.backspace();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }
                // Slash-меню перехватывает навигацию.
                if let Some(sl) = &mut self.slash {
                    let count = filter_items(&self.slash_items, &sl.query).len();
                    match key {
                        Key::Up => {
                            sl.selected = sl.selected.saturating_sub(1);
                            self.mark_dirty(DirtyFlags::RENDER);
                            return EventResult::Handled;
                        }
                        Key::Down => {
                            if count > 0 {
                                sl.selected = (sl.selected + 1).min(count - 1);
                            }
                            self.mark_dirty(DirtyFlags::RENDER);
                            return EventResult::Handled;
                        }
                        Key::Enter => {
                            let query = sl.query.clone();
                            let idx = sl.selected;
                            let action = filter_items(&self.slash_items, &query)
                                .get(idx)
                                .map(|it| it.action.clone());
                            if let Some(action) = action {
                                self.apply_slash(action);
                            } else {
                                self.close_slash();
                            }
                            return EventResult::Handled;
                        }
                        Key::Escape => {
                            self.close_slash();
                            return EventResult::Handled;
                        }
                        Key::Left | Key::Right => {
                            self.close_slash();
                            // Дальше — обычная навигация.
                        }
                        Key::Backspace => {
                            if sl.query.pop().is_none() {
                                self.close_slash();
                            } else {
                                sl.selected = 0;
                            }
                            self.checkpoint(EditClass::Deleting);
                            self.backspace();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }
                // Выделение блоков — свой набор клавиш: удалить, буфер,
                // все блоки, шаг выделения вверх/вниз, Esc.
                if editable && !self.block_sel.is_empty() {
                    match key {
                        Key::Delete | Key::Backspace => {
                            let ids = self.block_sel.clone();
                            self.delete_blocks(ids);
                            return EventResult::Handled;
                        }
                        Key::Escape => {
                            self.clear_block_sel();
                            return EventResult::Handled;
                        }
                        Key::C if ctrl => {
                            self.copy_blocks();
                            return EventResult::Handled;
                        }
                        Key::X if ctrl => {
                            self.cut_blocks();
                            return EventResult::Handled;
                        }
                        Key::V if ctrl => {
                            self.paste_blocks();
                            return EventResult::Handled;
                        }
                        Key::A if ctrl => {
                            let all = self.top_order();
                            self.select_blocks(all);
                            return EventResult::Handled;
                        }
                        Key::Up | Key::Down => {
                            let order = self.top_order();
                            let up = matches!(key, Key::Up);
                            let edge = if up {
                                self.block_sel.first().copied()
                            } else {
                                self.block_sel.last().copied()
                            };
                            let next = edge
                                .and_then(|id| order.iter().position(|x| *x == id))
                                .map(|i| if up { i.saturating_sub(1) } else { (i + 1).min(order.len() - 1) })
                                .and_then(|i| order.get(i).copied());
                            if let Some(n) = next {
                                if shift {
                                    self.select_block_range(n);
                                } else {
                                    self.select_blocks(vec![n]);
                                }
                            }
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }
                let handled = match key {
                    Key::A if ctrl => {
                        // Повторный Ctrl+A (весь текст уже выделен)
                        // выделяет блоки целиком.
                        let before = self.selection;
                        self.select_all();
                        if editable && !self.plain && before == self.selection {
                            let all = self.top_order();
                            self.select_blocks(all);
                        }
                        true
                    }
                    Key::C if ctrl => {
                        let text = self.selection_text();
                        if !text.is_empty() {
                            ctx.copy_to_clipboard(&text);
                        }
                        true
                    }
                    Key::X if ctrl && editable => {
                        let text = self.selection_text();
                        if !text.is_empty() {
                            ctx.copy_to_clipboard(&text);
                            self.checkpoint(EditClass::Structure);
                            self.delete_selection_if_any();
                        }
                        true
                    }
                    Key::V if ctrl && editable => {
                        if let Some(text) = ctx.paste_from_clipboard() {
                            self.paste_text(&text);
                        }
                        true
                    }
                    Key::Z if ctrl && editable => {
                        if shift {
                            self.redo();
                        } else {
                            self.undo();
                        }
                        true
                    }
                    Key::Y if ctrl && editable => {
                        self.redo();
                        true
                    }
                    Key::B if ctrl && editable => {
                        self.toggle_inline(|s| s.bold, |s, v| s.bold = v);
                        true
                    }
                    Key::I if ctrl && editable => {
                        self.toggle_inline(|s| s.italic, |s, v| s.italic = v);
                        true
                    }
                    Key::E if ctrl && editable => {
                        self.toggle_inline(|s| s.code, |s, v| s.code = v);
                        true
                    }
                    Key::S if ctrl && shift && editable => {
                        self.toggle_inline(|s| s.strike, |s, v| s.strike = v);
                        true
                    }
                    Key::Tab if editable => self.tab_indent_checkpointed(shift),
                    Key::Backspace if editable => {
                        self.checkpoint(EditClass::Deleting);
                        self.backspace();
                        true
                    }
                    Key::Delete if editable => {
                        self.checkpoint(EditClass::Deleting);
                        self.delete_forward();
                        true
                    }
                    Key::Enter if editable => {
                        self.checkpoint(EditClass::Structure);
                        self.enter(shift);
                        true
                    }
                    Key::Left => {
                        self.move_horizontal(-1, shift);
                        true
                    }
                    Key::Right => {
                        self.move_horizontal(1, shift);
                        true
                    }
                    Key::Up => {
                        self.move_vertical(-1, shift);
                        true
                    }
                    Key::Down => {
                        self.move_vertical(1, shift);
                        true
                    }
                    Key::Home => {
                        self.move_line_edge(false, shift);
                        true
                    }
                    Key::End => {
                        self.move_line_edge(true, shift);
                        true
                    }
                    Key::Escape => {
                        if let Some(sel) = self.selection {
                            self.selection = Some(DocSelection::caret(sel.head));
                            self.mark_dirty(DirtyFlags::RENDER);
                        }
                        true
                    }
                    _ => false,
                };
                if handled {
                    if let Some(rect) = self.caret().and_then(|p| self.caret_rect(p)) {
                        ctx.scroll_into_view(rect);
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::CharInput(c) => {
                // С Ctrl (кроме AltGr = Ctrl+Alt в Windows) и с Cmd текст не
                // набирают: в русской раскладке xkb на Ctrl+Z отдаёт «я»
                // (control-преобразование только для ASCII), и буква
                // вставлялась бы перед самой отменой — «Ctrl+Z не работает».
                let combo = (ctx.modifiers.ctrl && !ctx.modifiers.alt) || ctx.modifiers.meta;
                if !self.focused || self.read_only || c.is_control() || combo {
                    return EventResult::Ignored;
                }
                if self.code_caret.is_some() {
                    let ch = *c;
                    self.checkpoint(EditClass::Typing);
                    let mut buf = [0u8; 4];
                    self.code_insert(ch.encode_utf8(&mut buf));
                    return EventResult::Handled;
                }
                if self.table_caret.is_some() {
                    let ch = *c;
                    self.checkpoint(EditClass::Typing);
                    let mut buf = [0u8; 4];
                    self.table_insert(ch.encode_utf8(&mut buf));
                    return EventResult::Handled;
                }
                if self.caret().is_none() {
                    return EventResult::Ignored;
                }
                let ch = *c;
                self.checkpoint(EditClass::Typing);
                let mut buf = [0u8; 4];
                self.insert_str(ch.encode_utf8(&mut buf));
                // Автокомплит [[: набор уточняет кандидатов.
                if self.wiki.is_some() {
                    if ch.is_whitespace() || ch == ']' {
                        self.close_wiki();
                    } else {
                        let q = {
                            let w = self.wiki.as_mut().expect("wiki активен");
                            w.query.push(ch);
                            w.selected = 0;
                            w.query.clone()
                        };
                        let cands = self.wiki_candidates(&q);
                        if let Some(w) = &mut self.wiki {
                            w.candidates = cands;
                        }
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Handled;
                }
                // Slash-меню: набор уточняет фильтр (пробел закрывает).
                if let Some(sl) = &mut self.slash {
                    if ch.is_whitespace() {
                        self.close_slash();
                    } else {
                        sl.query.push(ch);
                        sl.selected = 0;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Handled;
                }
                match ch {
                    '/' => self.maybe_open_slash(),
                    '[' => self.maybe_open_wiki(),
                    ' ' => {
                        self.maybe_block_shortcut();
                    }
                    '`' | '-' => {
                        // ``` и --- срабатывают целой строкой; иначе `
                        // может замыкать инлайн-код.
                        if !self.maybe_block_shortcut() && ch == '`' {
                            self.maybe_inline_shortcut();
                        }
                    }
                    '*' | '~' => self.maybe_inline_shortcut(),
                    _ => {}
                }
                EventResult::Handled
            }
            Event::ImePreedit { text, .. } => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                self.preedit = if text.is_empty() { None } else { Some(text.clone()) };
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.preedit = None;
                self.checkpoint(EditClass::Typing);
                if self.code_caret.is_some() {
                    self.code_insert(text);
                    return EventResult::Handled;
                }
                if self.table_caret.is_some() {
                    self.table_insert(text);
                } else {
                    self.insert_str(text);
                }
                EventResult::Handled
            }
            Event::FocusGained => {
                if !self.focused && !self.read_only {
                    self.focused = true;
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                EventResult::Ignored
            }
            Event::FocusLost => {
                if self.focused {
                    self.focused = false;
                    self.mouse_selecting = false;
                    self.preedit = None;
                    self.mark_dirty(DirtyFlags::RENDER);
                    if let Some(cb) = self.on_focus_lost.clone() {
                        cb();
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.focused
            || self.read_only
            || (self.selection.is_none()
                && self.table_caret.is_none()
                && self.code_caret.is_none())
        {
            return false;
        }
        self.blink_ms += dt.as_secs_f32() * 1000.0;
        if self.blink_ms >= 530.0 {
            self.blink_ms = 0.0;
            self.caret_on = !self.caret_on;
            self.mark_dirty(DirtyFlags::RENDER);
        }
        true
    }

    fn wants_animate_tick(&self) -> bool {
        self.focused
            && !self.read_only
            && (self.selection.is_some()
                || self.table_caret.is_some()
                || self.code_caret.is_some())
    }

    fn wants_tab(&self) -> bool {
        self.focused && !self.read_only
    }

    fn element_type_name(&self) -> &str {
        "document-editor"
    }

    fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_request_pending)
    }

    /// Роль текстового поля: так приложение переводит фокус на редактор
    /// по клику и шлёт прежнему вводу `FocusLost` — без этого правка
    /// карточки доски не закрывалась бы кликом мимо неё.
    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        if self.read_only {
            return None;
        }
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TextField,
            state: crate::a11y::NodeState { focused: self.focused, ..Default::default() },
            properties: crate::a11y::NodeProperties::default(),
        })
    }

    /// Над врезкой редактор фокус по клику не берёт: там чужие виджеты
    /// (доска, диаграмма), и каретка в прежнем блоке иначе оставалась бы
    /// живой — приложение снимает фокус, а клик уходит виджету врезки.
    fn text_input_hit(&self, point: Point) -> bool {
        self.embed_at(point).is_none()
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
}

impl StyledElement for DocumentEditorElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        let mut doc_style = DocStyle::default();
        doc_style.apply(style);
        // Дети перестраиваются только при реальном изменении стиля.
        if doc_style != *self.style {
            self.style = Arc::new(doc_style);
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
}
