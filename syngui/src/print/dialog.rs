//! Окно печати: слева — предпросмотр листов, справа — принтер и настройки,
//! внизу — «Отмена» и «Печать». На узком экране (телефон) — столбцом:
//! предпросмотр сверху, настройки под ним.
//!
//! Классы MSS (`styles/print.mss`): `print-dialog` (+ `phone`),
//! `print-header`, `print-title`, `print-body`, `print-preview`,
//! `print-page-nav`, `print-settings`, `print-row`, `print-label`,
//! `print-section`, `print-note`, `print-error`, `print-footer`,
//! `print-summary`, `print-printer-state`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::{cups, paper, ppd::Ppd, Fit, PageOrientation, PageRange, PrintDocument, PrintImage, PrintOutcome, PrintSettings, Printer, PrinterState, PDF_DESTINATION};
use crate::async_runtime::run_on_main_thread;
use crate::i18n::{builtin, builtin_args};
use crate::layout::{CrossAxisAlignment, MainAxisAlignment};
use crate::signal::{create_effect, use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::containers::{Column, DecoratedBox, GestureDetector, Reactive, Row};
use crate::widgets::input::{Checkbox, Dropdown, DropdownItem, SpinBox, TextField};
use crate::widgets::overlay::{Portal, PortalAnchor};
use crate::widgets::scroll::ScrollView;
use crate::widgets::visual::{Image, ImageFit};
use crate::widgets::{Button, SegmentedButton};

type W = Box<dyn Widget>;

fn tr(key: &str, en: &str) -> String {
    builtin(&format!("print.{key}"), en)
}

/// Длинная сторона растра предпросмотра.
const PREVIEW_SIDE: u32 = 1000;
/// Уже ширины — раскладка столбцом.
const NARROW_BELOW: f32 = 720.0;

/// Настройки драйвера, у которых своё место в окне (не в «Дополнительно»).
const OWN_OPTIONS: &[&str] = &["PageSize", "PageRegion", "Duplex", "ColorModel", "Collate"];

type DoneFn = Arc<dyn Fn(Result<PrintOutcome, String>) + Send + Sync>;

/// Окно печати документа.
pub struct PrintDialog {
    open: RwSignal<bool>,
    doc: Arc<dyn PrintDocument>,
    on_done: Option<DoneFn>,
}

#[derive(Clone, Copy)]
struct State {
    open: RwSignal<bool>,
    printers: RwSignal<Option<Result<Vec<Printer>, String>>>,
    ppd: RwSignal<Option<Ppd>>,
    /// Описание принтера грузится.
    ppd_loading: RwSignal<bool>,
    settings: RwSignal<PrintSettings>,
    sheet: RwSignal<usize>,
    preview: RwSignal<Option<(u64, Result<PrintImage, String>)>>,
    busy: RwSignal<Option<String>>,
    error: RwSignal<Option<String>>,
    show_extra: RwSignal<bool>,
}

static PREVIEW_GEN: AtomicU64 = AtomicU64::new(0);
static PPD_GEN: AtomicU64 = AtomicU64::new(0);

impl PrintDialog {
    pub fn new(open: RwSignal<bool>, doc: Arc<dyn PrintDocument>) -> Self {
        Self { open, doc, on_done: None }
    }

    /// Итог: отправлено/сохранено или ошибка (окно к этому времени закрыто).
    pub fn on_done(mut self, f: impl Fn(Result<PrintOutcome, String>) + Send + Sync + 'static) -> Self {
        self.on_done = Some(Arc::new(f));
        self
    }

    pub fn build(self) -> Portal {
        let st = State {
            open: self.open,
            printers: use_signal(None),
            ppd: use_signal(None),
            ppd_loading: use_signal(false),
            settings: use_signal(initial_settings(&*self.doc)),
            sheet: use_signal(0),
            preview: use_signal(None),
            busy: use_signal(None),
            error: use_signal(None),
            show_extra: use_signal(false),
        };
        let doc = self.doc.clone();
        let on_done = self.on_done.clone();

        // Открыли окно — свежий список принтеров.
        create_effect(move || {
            if st.open.get() {
                st.error.set(None);
                st.busy.set(None);
                st.sheet.set(0);
                load_printers(st);
            }
        });
        // Любая настройка или другой лист — новый предпросмотр.
        {
            let doc = doc.clone();
            create_effect(move || {
                let s = st.settings.get();
                let idx = st.sheet.get();
                if !st.open.get() {
                    return;
                }
                start_preview(st, doc.clone(), s, idx);
            });
        }

        let narrow = crate::viewport::viewport_below(NARROW_BELOW);
        let doc2 = doc.clone();
        let body = Reactive::new(move || -> Vec<W> {
            let phone = narrow.get();
            let preview = preview_pane(st, doc2.clone());
            let settings = settings_pane(st, doc2.clone());
            let body: W = if phone {
                Box::new(
                    Column::new()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .class("print-body")
                        .child(DecoratedBox::new().class("print-preview-wrap").child(preview))
                        .child(DecoratedBox::new().class("print-grow print-settings-wrap").child(ScrollView::new().vertical().child(settings))),
                )
            } else {
                Box::new(
                    Row::new()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .class("print-body")
                        .child(DecoratedBox::new().class("print-grow print-preview-wrap").child(preview))
                        .child(DecoratedBox::new().class("print-settings-wrap").child(ScrollView::new().vertical().child(settings))),
                )
            };
            vec![Box::new(
                DecoratedBox::new().class(if phone { "print-dialog phone" } else { "print-dialog" }).child(
                    Column::new()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .child(header(st, &doc2.title()))
                        .child(DecoratedBox::new().class("print-grow").child(body))
                        .child(footer(st, doc2.clone(), on_done.clone())),
                ),
            )]
        });

        Portal::new()
            .is_open(self.open)
            .modal(true)
            .backdrop(true)
            .backdrop_color(crate::core::Color::new(0.0, 0.0, 0.0, 0.45))
            .anchor(PortalAnchor::Center)
            .close_on_outside_click(false)
            .child(body)
    }
}

// ─────────────────────────────────────────────────────────────── данные

fn store_get<'a>(store: &'a [(String, String)], key: &str) -> Option<&'a str> {
    store.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn initial_settings(doc: &dyn PrintDocument) -> PrintSettings {
    let store = super::load_store();
    let get = |k: &str| store_get(&store, k);
    let mut s = PrintSettings::default();
    if let Some(v) = get("orientation") {
        s.orientation = match v {
            "portrait" => PageOrientation::Portrait,
            "landscape" => PageOrientation::Landscape,
            _ => PageOrientation::Auto,
        };
    }
    s.gray = get("gray") == Some("1");
    if let Some(v) = get("margin").and_then(|v| v.parse().ok()) {
        s.margin_mm = v;
    }
    if doc.is_images() {
        s.fit = if get("fit") == Some("fill") { Fit::Fill } else { Fit::Fit };
        s.per_sheet = get("per_sheet").and_then(|v| v.parse().ok()).filter(|n| [1, 2, 4, 6, 9].contains(n)).unwrap_or(1);
    }
    s.pdf_path = super::default_pdf_path(&doc.title());
    s.range = doc.default_range();
    s
}

fn remember(s: &PrintSettings) {
    let mut store = super::load_store();
    let mut set = |k: String, v: String| {
        if let Some(e) = store.iter_mut().find(|(sk, _)| *sk == k) {
            e.1 = v;
        } else {
            store.push((k, v));
        }
    };
    set("printer".into(), if s.printer == PDF_DESTINATION { "pdf".into() } else { s.printer.clone() });
    let p = if s.printer == PDF_DESTINATION { "pdf" } else { &s.printer };
    set(format!("paper.{p}"), s.paper.clone());
    set(format!("duplex.{p}"), s.duplex.clone());
    set("orientation".into(), match s.orientation {
        PageOrientation::Auto => "auto",
        PageOrientation::Portrait => "portrait",
        PageOrientation::Landscape => "landscape",
    }
    .into());
    set("gray".into(), if s.gray { "1" } else { "0" }.into());
    set("margin".into(), s.margin_mm.to_string());
    set("fit".into(), if s.fit == Fit::Fill { "fill" } else { "fit" }.into());
    set("per_sheet".into(), s.per_sheet.to_string());
    super::save_store(&store);
}

fn load_printers(st: State) {
    std::thread::spawn(move || {
        let list = cups::printers();
        run_on_main_thread(move || {
            let store = super::load_store();
            let current = st.settings.get_untracked().printer;
            let names: Vec<String> = list.as_ref().map(|l| l.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
            let pick = if !current.is_empty() && (current == PDF_DESTINATION || names.contains(&current)) {
                current
            } else {
                match store_get(&store, "printer") {
                    Some("pdf") => PDF_DESTINATION.to_string(),
                    Some(p) if names.iter().any(|n| n == p) => p.to_string(),
                    _ => list
                        .as_ref()
                        .ok()
                        .and_then(|l| l.iter().find(|p| p.is_default).or(l.first()))
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| PDF_DESTINATION.to_string()),
                }
            };
            st.printers.set(Some(list));
            select_printer(st, pick);
        });
    });
}

/// Выбрать принтер: загрузить его описание и подставить его бумагу, двустороннюю печать, цвет.
fn select_printer(st: State, name: String) {
    let seq = PPD_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    st.settings.update(|s| s.printer = name.clone());
    st.show_extra.set(false);
    if name == PDF_DESTINATION {
        st.ppd.set(None);
        st.ppd_loading.set(false);
        apply_printer(st, None);
        return;
    }
    st.ppd_loading.set(true);
    std::thread::spawn(move || {
        let ppd = cups::printer_ppd(&name).ok();
        let user = cups::user_defaults(&name);
        run_on_main_thread(move || {
            if PPD_GEN.load(Ordering::SeqCst) != seq {
                return;
            }
            if let Some(p) = &ppd {
                if let Some(o) = p.option("PageSize") {
                    for c in &o.choices {
                        if let Some(&(w, h)) = p.dimensions.get(&c.key) {
                            paper::register(&c.key, w, h, &c.label);
                        }
                    }
                }
            }
            st.ppd.set(ppd.clone());
            st.ppd_loading.set(false);
            apply_printer(st, ppd.map(|p| (p, user)));
        });
    });
}

fn apply_printer(st: State, ppd: Option<(Ppd, Vec<(String, String)>)>) {
    let store = super::load_store();
    st.settings.update(|s| {
        let key = if s.printer == PDF_DESTINATION { "pdf".to_string() } else { s.printer.clone() };
        let saved_paper = store_get(&store, &format!("paper.{key}")).map(str::to_string);
        let saved_duplex = store_get(&store, &format!("duplex.{key}")).map(str::to_string);
        s.extra.clear();
        match &ppd {
            None => {
                let sizes = paper::PDF_SIZES;
                s.paper = saved_paper.filter(|p| sizes.contains(&p.as_str())).unwrap_or_else(|| paper::default_key().into());
                s.duplex.clear();
                s.hw_margins = [0.0; 4];
            }
            Some((p, user)) => {
                let user_get = |k: &str| user.iter().find(|(uk, _)| uk == k).map(|(_, v)| v.clone());
                if let Some(o) = p.option("PageSize") {
                    let ok = |v: &String| o.choice(v).is_some() && paper::size_mm(v).is_some();
                    s.paper = saved_paper
                        .filter(ok)
                        .or_else(|| user_get("PageSize").filter(ok))
                        .or_else(|| Some(o.default.clone()).filter(ok))
                        .or_else(|| o.choices.iter().map(|c| c.key.clone()).find(ok))
                        .unwrap_or_else(|| paper::default_key().into());
                }
                s.duplex = match p.option("Duplex") {
                    Some(o) => saved_duplex.filter(|d| o.choice(d).is_some()).or_else(|| user_get("Duplex").filter(|d| o.choice(d).is_some())).unwrap_or_else(|| o.default.clone()),
                    None => String::new(),
                };
                s.hw_margins = p.hw_margins_mm(&s.paper).unwrap_or([0.0; 4]);
                if !p.color {
                    s.gray = true;
                }
                // Дополнительно: сохранённые пользователем значения (`lpoptions`), остальное — по драйверу.
                for o in p.options.iter().filter(|o| !OWN_OPTIONS.contains(&o.key.as_str())) {
                    if let Some(v) = user_get(&o.key).filter(|v| o.choice(v).is_some()) {
                        if v != o.default {
                            s.extra.push((o.key.clone(), v));
                        }
                    }
                }
                sync_color_model(s, p);
            }
        }
    });
    st.sheet.set(0);
}

fn is_gray_choice(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["gray", "grey", "mono", "black", "kgray"].iter().any(|g| k.contains(g))
}

/// Цвет/ч-б → вариант `ColorModel` драйвера.
fn sync_color_model(s: &mut PrintSettings, p: &Ppd) {
    s.extra.retain(|(k, _)| k != "ColorModel");
    let Some(o) = p.option("ColorModel") else { return };
    let want = if s.gray {
        o.choices.iter().find(|c| is_gray_choice(&c.key))
    } else if !is_gray_choice(&o.default) {
        o.choice(&o.default)
    } else {
        o.choices.iter().find(|c| !is_gray_choice(&c.key))
    };
    if let Some(c) = want {
        s.extra.push(("ColorModel".into(), c.key.clone()));
    }
}

fn start_preview(st: State, doc: Arc<dyn PrintDocument>, s: PrintSettings, idx: usize) {
    let seq = PREVIEW_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        // Подряд идущие изменения (щелчки по спину) — рисуем последнее.
        std::thread::sleep(std::time::Duration::from_millis(40));
        if PREVIEW_GEN.load(Ordering::SeqCst) != seq {
            return;
        }
        let list = super::sheets(&*doc, &s);
        let r = match list.get(idx.min(list.len().saturating_sub(1))) {
            Some(items) => super::render_preview(&*doc, items, &s, PREVIEW_SIDE).map(|(_, img)| frame(img)),
            None => Err(tr("nothing", "Nothing to print")),
        };
        if PREVIEW_GEN.load(Ordering::SeqCst) != seq {
            return;
        }
        run_on_main_thread(move || st.preview.set(Some((seq, r))));
    });
}

/// Тонкая рамка по краю листа: белый лист виден и на светлом фоне.
fn frame(img: PrintImage) -> PrintImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let mut px = (*img.rgba).clone();
    for x in 0..w {
        for y in [0, h - 1] {
            px[(y * w + x) * 4..][..3].copy_from_slice(&[190, 190, 190]);
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            px[(y * w + x) * 4..][..3].copy_from_slice(&[190, 190, 190]);
        }
    }
    PrintImage::from_rgba(img.width, img.height, px)
}

// ─────────────────────────────────────────────────────────────── вид

fn header(st: State, title: &str) -> W {
    Box::new(
        DecoratedBox::new().class("print-header").child(
            Row::new()
                .gap(12.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(
                    DecoratedBox::new().class("print-grow").child(
                        Column::new()
                            .gap(2.0)
                            .child(Text::new(tr("title", "Print")).class("print-title"))
                            .child(Text::new(title.to_string()).max_lines(1).elide(crate::widget::Elide::Middle).class("print-subtitle")),
                    ),
                )
                .child(GestureDetector::new().on_click(move || close(st)).child(DecoratedBox::new().class("print-close").child(Text::new("✕").class("print-close-text")))),
        ),
    )
}

fn close(st: State) {
    if st.busy.get_untracked().is_none() {
        st.open.set(false);
    }
}

fn preview_pane(st: State, doc: Arc<dyn PrintDocument>) -> W {
    let image = Reactive::new(move || -> Vec<W> {
        match st.preview.get() {
            Some((seq, Ok(img))) => vec![Box::new(
                Image::from_rgba_shared(format!("syngui-print-preview-{seq}"), img.width, img.height, img.rgba.clone())
                    .fit(ImageFit::Contain)
                    .placeholder(false)
                    .class("print-preview-image"),
            )],
            Some((_, Err(e))) => vec![Box::new(crate::widget::Center::new().child(Text::new(e).class("print-error")))],
            None => vec![Box::new(crate::widget::Center::new().child(Text::new(tr("preparing", "Preparing preview…")).class("print-note")))],
        }
    });
    let nav = Reactive::new(move || -> Vec<W> {
        let s = st.settings.get();
        let n = super::sheets(&*doc, &s).len();
        let i = st.sheet.get().min(n.saturating_sub(1));
        if n < 2 {
            return vec![];
        }
        let arrow = |glyph: &'static str, enabled: bool, delta: isize| -> W {
            Box::new(
                GestureDetector::new()
                    .on_click(move || {
                        let cur = st.sheet.get_untracked() as isize + delta;
                        if cur >= 0 && (cur as usize) < n {
                            st.sheet.set(cur as usize);
                        }
                    })
                    .child(DecoratedBox::new().class(if enabled { "print-nav-btn" } else { "print-nav-btn disabled" }).child(Text::new(glyph).class("print-nav-glyph"))),
            )
        };
        vec![Box::new(
            Row::new()
                .gap(10.0)
                .main_axis_alignment(MainAxisAlignment::Center)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .class("print-page-nav")
                .child(arrow("‹", i > 0, -1))
                .child(Text::new(builtin_args("print.sheet_of", "Sheet {n} of {total}", &[("n", &(i + 1)), ("total", &n)])).class("print-nav-text"))
                .child(arrow("›", i + 1 < n, 1)),
        )]
    });
    Box::new(
        Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .class("print-preview")
            .child(DecoratedBox::new().class("print-grow print-preview-area").child(image))
            .child(nav),
    )
}

fn row<M>(label: String, control: impl crate::widgets::containers::IntoWidget<M>) -> W {
    Box::new(
        Column::new()
            .gap(6.0)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .class("print-row")
            .child(Text::new(label).class("print-label"))
            .child(control),
    )
}

fn segmented(labels: Vec<String>, selected: usize, on: impl Fn(usize) + Send + 'static) -> W {
    Box::new(SegmentedButton::new(labels).selected(selected).on_change(on).class("print-segmented"))
}

fn printer_label(p: &Printer) -> String {
    p.title().to_string()
}

fn state_text(p: &Printer) -> String {
    let mut s = match p.state {
        PrinterState::Idle => tr("state.idle", "Ready"),
        PrinterState::Printing => tr("state.printing", "Printing"),
        PrinterState::Disabled => tr("state.disabled", "Paused"),
    };
    if !p.location.is_empty() {
        s.push_str(" · ");
        s.push_str(&p.location);
    }
    s
}

fn settings_pane(st: State, doc: Arc<dyn PrintDocument>) -> W {
    let mut col = Column::new().gap(14.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("print-settings");

    // Принтер и его состояние.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let printers = st.printers.get();
        let current = st.settings.with(|s| s.printer.clone());
        let mut items: Vec<DropdownItem> = Vec::new();
        let mut note: Option<(String, bool)> = None;
        match &printers {
            None => note = Some((tr("searching", "Looking for printers…"), false)),
            Some(Ok(list)) => {
                for p in list {
                    let mut label = printer_label(p);
                    if p.is_default {
                        label.push_str(&format!(" ({})", tr("default", "default")));
                    }
                    items.push(DropdownItem::new(p.name.clone(), label));
                }
                if let Some(p) = list.iter().find(|p| p.name == current) {
                    let warn = p.state == PrinterState::Disabled || !p.message.is_empty();
                    let mut t = state_text(p);
                    if !p.message.is_empty() {
                        t.push_str(" · ");
                        t.push_str(&p.message);
                    }
                    note = Some((t, warn));
                } else if list.is_empty() {
                    note = Some((tr("no_printers", "No printers set up — add one in Settings → Printers"), false));
                }
            }
            Some(Err(_)) => note = Some((tr("no_cups", "Printing service (CUPS) is not available"), true)),
        }
        items.push(DropdownItem::new(PDF_DESTINATION, tr("save_pdf", "Save as PDF")));
        let dd = Dropdown::with_items(items).selected(current.clone()).on_change(move |v: &str| {
            if v != st.settings.get_untracked().printer {
                select_printer(st, v.to_string());
            }
        });
        let mut c = Column::new().gap(6.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("print-row");
        c = c.child(Text::new(tr("printer", "Printer")).class("print-label")).child(dd);
        if let Some((t, warn)) = note {
            c = c.child(Text::new(t).class(if warn { "print-printer-state warn" } else { "print-printer-state" }));
        }
        vec![Box::new(c)]
    }));

    // Копии и путь PDF — пересоздаются только при смене принтера (иначе поле теряет фокус).
    col = col.child(Reactive::new(move || -> Vec<W> {
        let printer = st.settings.with(|s| s.printer.clone());
        let s = st.settings.get_untracked();
        if printer == PDF_DESTINATION {
            let field = TextField::new().text(s.pdf_path.to_string_lossy().to_string()).on_change(move |t: &str| {
                let t = t.to_string();
                st.settings.update(|s| s.pdf_path = t.clone().into());
            });
            return vec![row(tr("file", "File"), field)];
        }
        let spin = SpinBox::new().range(1.0, 999.0).step(1.0).value(s.copies as f64).on_change(move |v: f64| {
            st.settings.update(|s| s.copies = v.round().max(1.0) as u32);
        });
        vec![row(tr("copies", "Copies"), spin)]
    }));
    col = col.child(Reactive::new(move || -> Vec<W> {
        let (printer, copies, collate) = st.settings.with(|s| (s.printer.clone(), s.copies, s.collate));
        if printer == PDF_DESTINATION || copies < 2 {
            return vec![];
        }
        vec![Box::new(Checkbox::checked(collate).label(tr("collate", "Collate copies")).on_change(move |v| st.settings.update(|s| s.collate = v)))]
    }));

    // Страницы.
    {
        let doc = doc.clone();
        col = col.child(Reactive::new(move || -> Vec<W> {
            let total = doc.len();
            if total < 2 {
                return vec![];
            }
            let range = st.settings.with(|s| s.range.clone());
            let what = if doc.is_images() { tr("pictures", "Pictures") } else { tr("pages", "Pages") };
            let cur_label = if doc.is_images() { tr("range.current_image", "Current picture") } else { tr("range.current", "Current page") };
            let all_label = builtin_args("print.range.all_n", "All ({n})", &[("n", &total)]);
            let items = vec![DropdownItem::new("all", all_label), DropdownItem::new("current", cur_label), DropdownItem::new("custom", tr("range.custom", "Selected…"))];
            let sel = match range {
                PageRange::All => "all",
                PageRange::Current => "current",
                PageRange::Custom(_) => "custom",
            };
            let dd = Dropdown::with_items(items).selected(sel).on_change(move |v: &str| {
                let r = match v {
                    "current" => PageRange::Current,
                    "custom" => PageRange::Custom(match st.settings.get_untracked().range {
                        PageRange::Custom(t) => t,
                        _ => String::new(),
                    }),
                    _ => PageRange::All,
                };
                st.sheet.set(0);
                st.settings.update(|s| s.range = r.clone());
            });
            let mut c = Column::new().gap(6.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("print-row");
            c = c.child(Text::new(what).class("print-label")).child(dd);
            if matches!(range, PageRange::Custom(_)) {
                c = c.child(range_field(st, total));
            }
            vec![Box::new(c)]
        }));
    }

    // Бумага.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let ppd = st.ppd.get();
        let (printer, current) = st.settings.with(|s| (s.printer.clone(), s.paper.clone()));
        if st.ppd_loading.get() {
            return vec![row(tr("paper", "Paper"), Text::new(tr("loading", "Loading…")).class("print-note"))];
        }
        let keys: Vec<(String, Option<String>)> = match (&ppd, printer == PDF_DESTINATION) {
            (Some(p), false) => p.option("PageSize").map(|o| o.choices.iter().map(|c| (c.key.clone(), Some(c.label.clone()))).collect()).unwrap_or_default(),
            _ => paper::PDF_SIZES.iter().map(|k| (k.to_string(), None)).collect(),
        };
        let items: Vec<DropdownItem> = keys.iter().filter_map(|(k, l)| paper::paper(k, l.as_deref())).map(|p| DropdownItem::new(p.key.clone(), p.full_label())).collect();
        if items.is_empty() {
            return vec![];
        }
        let ppd2 = ppd.clone();
        let dd = Dropdown::with_items(items).selected(current).max_height(360.0).on_change(move |v: &str| {
            let v = v.to_string();
            let hw = ppd2.as_ref().and_then(|p| p.hw_margins_mm(&v)).unwrap_or([0.0; 4]);
            st.settings.update(|s| {
                s.paper = v.clone();
                s.hw_margins = hw;
            });
        });
        vec![row(tr("paper", "Paper"), dd)]
    }));

    // Ориентация.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let o = st.settings.with(|s| s.orientation);
        let sel = match o {
            PageOrientation::Auto => 0,
            PageOrientation::Portrait => 1,
            PageOrientation::Landscape => 2,
        };
        vec![row(
            tr("orientation", "Orientation"),
            segmented(vec![tr("orient.auto", "Auto"), tr("orient.portrait", "Portrait"), tr("orient.landscape", "Landscape")], sel, move |i| {
                st.settings.update(|s| {
                    s.orientation = [PageOrientation::Auto, PageOrientation::Portrait, PageOrientation::Landscape][i.min(2)];
                })
            }),
        )]
    }));

    // Цвет.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let ppd = st.ppd.get();
        let (printer, gray) = st.settings.with(|s| (s.printer.clone(), s.gray));
        let color_capable = printer == PDF_DESTINATION || ppd.as_ref().is_some_and(|p| p.color);
        if !color_capable {
            if ppd.is_some() {
                return vec![row(tr("color", "Color"), Text::new(tr("mono_only", "Black and white printer")).class("print-note"))];
            }
            return vec![];
        }
        vec![row(
            tr("color", "Color"),
            segmented(vec![tr("color.color", "Color"), tr("color.gray", "Black and white")], gray as usize, move |i| {
                let ppd = st.ppd.get_untracked();
                st.settings.update(|s| {
                    s.gray = i == 1;
                    if let Some(p) = &ppd {
                        sync_color_model(s, p);
                    }
                })
            }),
        )]
    }));

    // Двусторонняя печать.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let ppd = st.ppd.get();
        let current = st.settings.with(|s| s.duplex.clone());
        let Some(o) = ppd.as_ref().and_then(|p| p.option("Duplex")).cloned() else { return vec![] };
        let items: Vec<DropdownItem> = o.choices.iter().map(|c| DropdownItem::new(c.key.clone(), duplex_label(&c.key, &c.label))).collect();
        let dd = Dropdown::with_items(items).selected(current).on_change(move |v: &str| {
            let v = v.to_string();
            st.settings.update(|s| s.duplex = v.clone());
        });
        vec![row(tr("duplex", "Two-sided"), dd)]
    }));

    // Размещение картинок.
    if doc.is_images() {
        col = col.child(Text::new(tr("layout", "Layout")).class("print-section"));
        col = col.child(Reactive::new(move || -> Vec<W> {
            let (n, fit) = st.settings.with(|s| (s.per_sheet, s.fit));
            const COUNTS: [u32; 5] = [1, 2, 4, 6, 9];
            let sel = COUNTS.iter().position(|c| *c == n).unwrap_or(0);
            let rows: Vec<W> = vec![
                row(
                    tr("per_sheet", "Pictures per sheet"),
                    segmented(COUNTS.iter().map(|c| c.to_string()).collect(), sel, move |i| {
                        st.sheet.set(0);
                        st.settings.update(|s| s.per_sheet = COUNTS[i.min(4)]);
                    }),
                ),
                row(
                    tr("scale", "Scaling"),
                    segmented(vec![tr("fit", "Fit"), tr("fill", "Fill (crop)")], (fit == Fit::Fill) as usize, move |i| {
                        st.settings.update(|s| s.fit = if i == 1 { Fit::Fill } else { Fit::Fit })
                    }),
                ),
            ];
            vec![Box::new(Column::new().gap(14.0).cross_axis_alignment(CrossAxisAlignment::Stretch).children(rows))]
        }));
    }

    // Поля.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let m = st.settings.with(|s| s.margin_mm);
        const MARGINS: [f32; 4] = [0.0, 5.0, 10.0, 20.0];
        let sel = MARGINS.iter().position(|v| (*v - m).abs() < 0.1).unwrap_or(1);
        vec![row(
            tr("margins", "Margins"),
            segmented(vec![tr("margins.none", "None"), tr("margins.narrow", "Narrow"), tr("margins.normal", "Normal"), tr("margins.wide", "Wide")], sel, move |i| {
                st.settings.update(|s| s.margin_mm = MARGINS[i.min(3)])
            }),
        )]
    }));

    // Дополнительно: остальные настройки драйвера.
    col = col.child(Reactive::new(move || -> Vec<W> {
        let ppd = st.ppd.get();
        let Some(p) = ppd else { return vec![] };
        let opts: Vec<_> = p.options.iter().filter(|o| !OWN_OPTIONS.contains(&o.key.as_str()) && o.choices.len() > 1).cloned().collect();
        if opts.is_empty() {
            return vec![];
        }
        let open = st.show_extra.get();
        let mut out: Vec<W> = vec![Box::new(
            GestureDetector::new().on_click(move || st.show_extra.update(|v| *v = !*v)).child(
                DecoratedBox::new().class("print-expander").child(
                    Row::new()
                        .gap(8.0)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .child(Text::new(if open { "▾" } else { "▸" }).class("print-expander-glyph"))
                        .child(Text::new(tr("advanced", "Printer options")).class("print-section"))
                        .child(Text::new(format!("{}", opts.len())).class("print-note")),
                ),
            ),
        )];
        if !open {
            return out;
        }
        let extra = st.settings.get_untracked().extra;
        for o in opts {
            let value = extra.iter().find(|(k, _)| *k == o.key).map(|(_, v)| v.clone()).unwrap_or_else(|| o.default.clone());
            let items: Vec<DropdownItem> = o.choices.iter().map(|c| DropdownItem::new(c.key.clone(), choice_label(&o.key, &c.key, &c.label))).collect();
            let key = o.key.clone();
            let default = o.default.clone();
            let dd = Dropdown::with_items(items).selected(value).max_height(320.0).on_change(move |v: &str| {
                let (key, default, v) = (key.clone(), default.clone(), v.to_string());
                st.settings.update(|s| {
                    s.extra.retain(|(k, _)| *k != key);
                    if v != default {
                        s.extra.push((key.clone(), v.clone()));
                    }
                });
            });
            out.push(row(option_label(&o.key, &o.label), dd));
        }
        vec![Box::new(Column::new().gap(14.0).cross_axis_alignment(CrossAxisAlignment::Stretch).children(out))]
    }));

    Box::new(col)
}

fn range_field(st: State, total: usize) -> W {
    let text = match st.settings.get_untracked().range {
        PageRange::Custom(t) => t,
        _ => String::new(),
    };
    let field = TextField::new().text(text).placeholder(format!("1-{}, …", total.min(3))).on_change(move |t: &str| {
        let t = t.to_string();
        st.sheet.set(0);
        st.settings.update(|s| s.range = PageRange::Custom(t.clone()));
    });
    Box::new(field)
}

fn duplex_label(key: &str, fallback: &str) -> String {
    match key {
        "None" => tr("duplex.none", "One-sided"),
        "DuplexNoTumble" => tr("duplex.long", "Two-sided, flip on long edge"),
        "DuplexTumble" => tr("duplex.short", "Two-sided, flip on short edge"),
        _ => fallback.to_string(),
    }
}

/// Подпись настройки драйвера: перевод известных, иначе как в PPD.
pub(crate) fn option_label(key: &str, fallback: &str) -> String {
    builtin(&format!("print.opt.{key}"), fallback)
}

/// Подпись варианта настройки драйвера.
pub(crate) fn choice_label(option: &str, key: &str, fallback: &str) -> String {
    let k = format!("print.choice.{option}.{key}");
    let v = builtin(&k, "");
    if !v.is_empty() {
        return v;
    }
    let v = builtin(&format!("print.choice.{key}"), "");
    if v.is_empty() {
        fallback.to_string()
    } else {
        v
    }
}

fn footer(st: State, doc: Arc<dyn PrintDocument>, on_done: Option<DoneFn>) -> W {
    let doc2 = doc.clone();
    let summary = Reactive::new(move || -> Vec<W> {
        if let Some(b) = st.busy.get() {
            return vec![Box::new(Text::new(b).class("print-summary"))];
        }
        if let Some(e) = st.error.get() {
            return vec![Box::new(Text::new(e).max_lines(3).class("print-error"))];
        }
        let s = st.settings.get();
        if let PageRange::Custom(t) = &s.range {
            if super::parse_range(t, doc2.len()).is_none() {
                return vec![Box::new(Text::new(tr("bad_range", "Check the page numbers: e.g. 1-3, 5")).class("print-error"))];
            }
        }
        let n = super::sheets(&*doc2, &s).len();
        let mut t = builtin_args("print.sheets", "Sheets: {n}", &[("n", &n)]);
        if s.printer != PDF_DESTINATION && s.copies > 1 {
            t.push_str(&builtin_args("print.copies_x", " · copies: {n}", &[("n", &s.copies)]));
        }
        if let Some(p) = paper::paper(&s.paper, None) {
            t.push_str(" · ");
            t.push_str(&p.label);
        }
        vec![Box::new(Text::new(t).max_lines(2).class("print-summary"))]
    });
    let buttons = Reactive::new(move || -> Vec<W> {
        let busy = st.busy.get().is_some();
        let is_pdf = st.settings.with(|s| s.printer == PDF_DESTINATION);
        let loading = st.printers.get().is_none() || st.ppd_loading.get();
        let doc = doc.clone();
        let on_done = on_done.clone();
        let go = Button::new(if is_pdf { tr("save", "Save") } else { tr("print", "Print") })
            .class("primary print-go")
            .disabled(busy || loading)
            .on_click(move || start_print(st, doc.clone(), on_done.clone()));
        vec![Box::new(
            Row::new()
                .gap(8.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Button::new(tr("cancel", "Cancel")).class("flat print-cancel").disabled(busy).on_click(move || close(st)))
                .child(go),
        )]
    });
    Box::new(
        DecoratedBox::new().class("print-footer").child(
            Row::new()
                .gap(12.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(DecoratedBox::new().class("print-grow").child(summary))
                .child(buttons),
        ),
    )
}

fn start_print(st: State, doc: Arc<dyn PrintDocument>, on_done: Option<DoneFn>) {
    if st.busy.get_untracked().is_some() {
        return;
    }
    let s = st.settings.get_untracked();
    if let PageRange::Custom(t) = &s.range {
        if super::parse_range(t, doc.len()).is_none_or(|v| v.is_empty()) {
            st.error.set(Some(tr("bad_range", "Check the page numbers: e.g. 1-3, 5")));
            return;
        }
    }
    st.error.set(None);
    st.busy.set(Some(tr("preparing_job", "Preparing…")));
    remember(&s);
    std::thread::spawn(move || {
        let progress = move |i: usize, n: usize| {
            let t = builtin_args("print.progress", "Preparing sheet {i} of {n}…", &[("i", &(i + 1)), ("n", &n)]);
            run_on_main_thread(move || {
                if st.busy.get_untracked().is_some() {
                    st.busy.set(Some(t));
                }
            });
        };
        let r = super::print(&*doc, &s, &progress);
        run_on_main_thread(move || {
            st.busy.set(None);
            match &r {
                Ok(_) => st.open.set(false),
                Err(e) => st.error.set(Some(e.clone())),
            }
            if let Some(f) = on_done {
                if r.is_ok() {
                    f(r);
                }
            }
        });
    });
}

/// Сообщение об итоге для программы («Отправлено на HP LaserJet», «Сохранено: …»).
pub fn outcome_text(o: &PrintOutcome) -> String {
    match o {
        PrintOutcome::Sent { printer, sheets, .. } => {
            let title = cups::printers().ok().and_then(|l| l.into_iter().find(|p| &p.name == printer)).map(|p| p.title().to_string()).unwrap_or_else(|| printer.clone());
            builtin_args("print.sent", "Sent to {printer} · sheets: {n}", &[("printer", &title), ("n", sheets)])
        }
        PrintOutcome::Saved(p) => builtin_args("print.saved", "Saved: {path}", &[("path", &p.display())]),
    }
}
