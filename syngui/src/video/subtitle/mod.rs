//! Субтитры видео: дорожки файла (SRT, ASS/SSA, WebVTT, mov_text, PGS, VobSub…)
//! и внешние файлы рядом с роликом (`.srt`, `.ass`, `.ssa`, `.vtt`).
//!
//! Текстовые дорожки контейнера декодер видео разбирает все сразу, по мере
//! чтения файла: переключение между ними мгновенное. Картинки (PGS, VobSub)
//! — только у выбранной дорожки: они тяжёлые. Внешние файлы читаются целиком
//! при подключении ([`SubtitleStore::add_external`]).
//!
//! Текст хранится разобранным ([`SubText`]: строки из кусков с курсивом,
//! жирным, цветом; выравнивание и позиция из ASS) — рисует его
//! [`render`] своим оформлением ([`SubtitleStyle`]), а показывает поверх
//! кадра слой субтитров виджета плеера.

pub mod ass;
pub mod external;
pub(crate) mod ffdec;
pub mod render;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub use render::{SubtitleAnimation, SubtitleStyle};

/// Откуда дорожка: поток контейнера (номер потока) или внешний файл.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SubtitleSource {
    Embedded(usize),
    External(PathBuf),
}

/// Дорожка субтитров — для выбора пользователем.
#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleTrackInfo {
    pub source: SubtitleSource,
    /// Язык («rus», «eng», «ru»…), если указан.
    pub language: Option<String>,
    /// Название («Надписи», «Full», «SDH»…), если указано.
    pub title: Option<String>,
    /// Формат для людей: «SRT», «ASS», «PGS», «VobSub», «WebVTT»…
    pub codec: String,
    /// Картинки (PGS, VobSub): оформление к ним не применяется.
    pub bitmap: bool,
    /// Помечена в файле как основная.
    pub default: bool,
    /// Только надписи и иноязычные реплики («forced»).
    pub forced: bool,
    /// Для слабослышащих (SDH, CC): с описанием звуков.
    pub hearing_impaired: bool,
}

impl SubtitleTrackInfo {
    pub fn is_external(&self) -> bool {
        matches!(self.source, SubtitleSource::External(_))
    }
}

/// Кусок строки с одним оформлением.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Цвет из файла (`{\c&H…&}`, `<font color>`), если задан.
    pub color: Option<[u8; 3]>,
}

/// Разобранный текст события.
#[derive(Clone, Debug, PartialEq)]
pub struct SubText {
    /// Строки; каждая — куски с оформлением. Пустых строк нет.
    pub lines: Vec<Vec<Span>>,
    /// Выравнивание как на цифровом блоке клавиатуры (ASS `\an`): 1–3 —
    /// снизу, 4–6 — посередине, 7–9 — сверху; 1/4/7 — слева, 3/6/9 — справа.
    pub align: u8,
    /// Точка привязки из файла (ASS `\pos`, `\move`) в долях кадра (0…1).
    pub pos: Option<(f32, f32)>,
    /// Слой ASS: выше — рисуется поверх.
    pub layer: i32,
}

impl SubText {
    /// Текст без оформления (строки через `\n`) — для тестов и подсказок.
    pub fn plain(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Картинка субтитров (PGS, VobSub) в координатах холста дорожки.
#[derive(Clone, Debug)]
pub struct SubBitmap {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// RGBA8, альфа не умножена на цвет.
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub enum SubContent {
    Text(SubText),
    /// Картинки и размер холста, к которому отнесены их координаты (обычно
    /// размер кадра видео).
    Bitmap { rects: Vec<SubBitmap>, canvas: (u32, u32) },
}

/// Событие: что показывать и когда (секунды в шкале времени видео).
#[derive(Clone, Debug)]
pub struct SubtitleEvent {
    /// Уникален в пределах хранилища — ключ готовой картинки.
    pub id: u64,
    pub start: f64,
    /// `f64::INFINITY` — до следующего события дорожки (у PGS конец
    /// приходит отдельным пустым пакетом).
    pub end: f64,
    pub content: Arc<SubContent>,
}

/// Без конца событие текстовой дорожки висит не дольше этого.
const OPEN_TEXT_MAX: f64 = 8.0;
/// Картинок одной дорожки держать не больше: PGS-фильм — тысячи событий по
/// сотне килобайт.
const BITMAP_EVENTS_MAX: usize = 96;

struct TrackData {
    info: SubtitleTrackInfo,
    /// По возрастанию начала.
    events: Vec<SubtitleEvent>,
}

#[derive(Default)]
struct StoreInner {
    tracks: Vec<TrackData>,
    selected: Option<usize>,
    /// Сдвиг субтитров, с: плюс — позже.
    delay: f64,
}

/// Дорожки и события одного ролика: пишут декодер и загрузчик внешних
/// файлов, читают слой показа и интерфейс выбора.
pub struct SubtitleStore {
    inner: Mutex<StoreInner>,
    next_id: AtomicU64,
    /// Растёт при любой перемене — по нему слой показа узнаёт, что пора
    /// пересчитать видимое.
    revision: AtomicU64,
    /// Начало ролика в шкале видео (`start_time` контейнера), с: время
    /// внешних файлов отсчитывается от него.
    start_sec: f64,
}

/// Субтитры на момент времени: что видно (с запасом на анимации).
#[derive(Clone, Debug, Default)]
pub struct ActiveSubtitles {
    /// Время показа, уже со сдвигом пользователя: сравнивать с `start`/`end`.
    pub time: f64,
    pub events: Vec<SubtitleEvent>,
    /// Следующие события (для заблаговременной отрисовки).
    pub upcoming: Vec<SubtitleEvent>,
}

impl SubtitleStore {
    pub fn new(tracks: Vec<SubtitleTrackInfo>, start_sec: f64) -> Self {
        Self {
            inner: Mutex::new(StoreInner {
                tracks: tracks.into_iter().map(|info| TrackData { info, events: Vec::new() }).collect(),
                ..Default::default()
            }),
            next_id: AtomicU64::new(1),
            revision: AtomicU64::new(1),
            start_sec,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bump(&self) {
        self.revision.fetch_add(1, Ordering::Release);
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn tracks(&self) -> Vec<SubtitleTrackInfo> {
        self.lock().tracks.iter().map(|t| t.info.clone()).collect()
    }

    pub fn selected(&self) -> Option<SubtitleSource> {
        let g = self.lock();
        g.selected.and_then(|i| g.tracks.get(i)).map(|t| t.info.source.clone())
    }

    /// Показывать дорожку `source` (`None` — выключить). `false` — такой нет.
    pub fn select(&self, source: Option<&SubtitleSource>) -> bool {
        let mut g = self.lock();
        let idx = match source {
            None => None,
            Some(s) => match g.tracks.iter().position(|t| t.info.source == *s) {
                Some(i) => Some(i),
                None => return false,
            },
        };
        if g.selected != idx {
            g.selected = idx;
            drop(g);
            self.bump();
        }
        true
    }

    pub fn delay(&self) -> f64 {
        self.lock().delay
    }

    pub fn set_delay(&self, sec: f64) {
        let sec = (sec * 1000.0).round() / 1000.0;
        let mut g = self.lock();
        if g.delay != sec {
            g.delay = sec;
            drop(g);
            self.bump();
        }
    }

    /// Номер дорожки в списке по источнику.
    fn index_of(&self, g: &StoreInner, source: &SubtitleSource) -> Option<usize> {
        g.tracks.iter().position(|t| t.info.source == *source)
    }

    /// Подключить внешний файл: разобрать целиком и добавить дорожкой
    /// (уже подключённый — не повторно). Возвращает источник дорожки.
    pub fn add_external(&self, path: &Path) -> Result<SubtitleSource, String> {
        let source = SubtitleSource::External(path.to_path_buf());
        if self.index_of(&self.lock(), &source).is_some() {
            return Ok(source);
        }
        let parsed = external::load(path)?;
        let mut info = external::describe(path, None);
        info.codec = parsed.format.to_string();
        let mut events: Vec<SubtitleEvent> = parsed
            .events
            .into_iter()
            .map(|(start, end, text)| SubtitleEvent {
                id: self.next_id.fetch_add(1, Ordering::Relaxed),
                start: start + self.start_sec,
                end: end + self.start_sec,
                content: Arc::new(SubContent::Text(text)),
            })
            .collect();
        events.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut g = self.lock();
        g.tracks.push(TrackData { info, events });
        drop(g);
        self.bump();
        Ok(source)
    }

    /// Найти рядом с роликом файлы субтитров ([`external::find`]) и
    /// подключить их. Возвращает, сколько подключено.
    pub fn add_external_near(&self, video: &Path) -> usize {
        let mut n = 0;
        for found in external::find(video) {
            match self.add_external(&found.path) {
                Ok(src) => {
                    // язык и пометки из имени файла и папки
                    let mut g = self.lock();
                    if let Some(i) = self.index_of(&g, &src) {
                        let codec = std::mem::take(&mut g.tracks[i].info.codec);
                        g.tracks[i].info = SubtitleTrackInfo { codec, ..found.info.clone() };
                    }
                    n += 1;
                }
                Err(e) => log::warn!("субтитры {}: {e}", found.path.display()),
            }
        }
        if n > 0 {
            self.bump();
        }
        n
    }

    /// Дорожка картинок (PGS, VobSub) — декодер разбирает только выбранную.
    pub(crate) fn wants_stream(&self, stream_index: usize) -> bool {
        let g = self.lock();
        let Some(t) = g.tracks.iter().find(|t| t.info.source == SubtitleSource::Embedded(stream_index)) else {
            return false;
        };
        !t.info.bitmap || g.selected.and_then(|i| g.tracks.get(i)).is_some_and(|s| std::ptr::eq(s, t))
    }

    /// Событие из потока контейнера. Повтор (то же начало и содержимое —
    /// после перемотки назад пакеты читаются снова) не добавляется.
    pub(crate) fn push_embedded(&self, stream_index: usize, start: f64, end: f64, content: Option<SubContent>) {
        let mut g = self.lock();
        let Some(ti) = self.index_of(&g, &SubtitleSource::Embedded(stream_index)) else { return };
        let track = &mut g.tracks[ti];
        let bitmap = track.info.bitmap;
        // Незакрытые события дорожки кончаются там, где начинается новое.
        for e in track.events.iter_mut() {
            if e.end.is_infinite() && e.start < start {
                e.end = start;
            }
        }
        let Some(content) = content else {
            drop(g);
            self.bump();
            return;
        };
        let dup = track.events.iter().any(|e| {
            (e.start - start).abs() < 0.002
                && match (&*e.content, &content) {
                    (SubContent::Text(a), SubContent::Text(b)) => a == b,
                    (SubContent::Bitmap { rects: a, .. }, SubContent::Bitmap { rects: b, .. }) => {
                        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x.x, x.y, x.w, x.h) == (y.x, y.y, y.w, y.h))
                    }
                    _ => false,
                }
        });
        if dup {
            return;
        }
        let ev = SubtitleEvent {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            start,
            end,
            content: Arc::new(content),
        };
        let at = track.events.partition_point(|e| e.start <= start);
        track.events.insert(at, ev);
        if bitmap && track.events.len() > BITMAP_EVENTS_MAX {
            // дальние от нового события — прочь
            let far = track
                .events
                .iter()
                .enumerate()
                .max_by(|a, b| (a.1.start - start).abs().total_cmp(&(b.1.start - start).abs()))
                .map(|(i, _)| i);
            if let Some(i) = far {
                track.events.remove(i);
            }
        }
        drop(g);
        self.bump();
    }

    /// Картинки дорожки после перемотки: незакрытые события старой позиции
    /// не должны висеть на новой.
    pub(crate) fn on_seek(&self) {
        let mut g = self.lock();
        let mut changed = false;
        for t in g.tracks.iter_mut().filter(|t| t.info.bitmap) {
            let before = t.events.len();
            t.events.retain(|e| e.end.is_finite());
            changed |= t.events.len() != before;
        }
        drop(g);
        if changed {
            self.bump();
        }
    }

    /// Что видно в момент `clock` (время видео) выбранной дорожки: события,
    /// которые идут, начнутся в ближайшие `lead` секунд или кончились не
    /// раньше `trail` секунд назад (для анимаций появления и исчезновения).
    pub fn active(&self, clock: f64, lead: f64, trail: f64) -> ActiveSubtitles {
        let g = self.lock();
        let time = clock - g.delay;
        let Some(track) = g.selected.and_then(|i| g.tracks.get(i)) else {
            return ActiveSubtitles { time, ..Default::default() };
        };
        let mut events = Vec::new();
        let mut upcoming = Vec::new();
        // События отсортированы по началу: всё, что начинается позже
        // `time + lead`, не нужно.
        let hi = track.events.partition_point(|e| e.start <= time + lead);
        for e in &track.events[..hi] {
            let mut e = e.clone();
            // У текста без конца — не дольше OPEN_TEXT_MAX (картинка PGS
            // ждёт своего пустого пакета).
            if e.end.is_infinite() && !track.info.bitmap {
                e.end = e.start + OPEN_TEXT_MAX;
            }
            if e.end + trail < time {
                continue;
            }
            if e.start <= time {
                events.push(e);
            } else {
                upcoming.push(e);
            }
        }
        ActiveSubtitles { time, events, upcoming }
    }

    /// Все события выбранной дорожки (для тестов и проверки).
    pub fn events(&self) -> Vec<SubtitleEvent> {
        let g = self.lock();
        g.selected.and_then(|i| g.tracks.get(i)).map(|t| t.events.clone()).unwrap_or_default()
    }
}

/// Язык для людей по коду ISO 639 («rus», «ru», «eng»…): название на
/// языке интерфейса не знаем — отдаём код как есть, если не узнали.
pub fn language_code_normalized(code: &str) -> String {
    let c = code.trim().to_ascii_lowercase();
    let two = match c.as_str() {
        "rus" => "ru",
        "eng" => "en",
        "ukr" => "uk",
        "bel" => "be",
        "kaz" => "kk",
        "ger" | "deu" => "de",
        "fre" | "fra" => "fr",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "pol" => "pl",
        "jpn" => "ja",
        "kor" => "ko",
        "chi" | "zho" => "zh",
        "tur" => "tr",
        "cze" | "ces" => "cs",
        "dut" | "nld" => "nl",
        "swe" => "sv",
        "fin" => "fi",
        "nor" => "no",
        "dan" => "da",
        "hun" => "hu",
        "gre" | "ell" => "el",
        "heb" => "he",
        "ara" => "ar",
        "hin" => "hi",
        "tha" => "th",
        "vie" => "vi",
        "ind" => "id",
        "rum" | "ron" => "ro",
        "bul" => "bg",
        "srp" => "sr",
        "hrv" => "hr",
        "slv" => "sl",
        "slo" | "slk" => "sk",
        "lit" => "lt",
        "lav" => "lv",
        "est" => "et",
        "geo" | "kat" => "ka",
        "arm" | "hye" => "hy",
        "aze" => "az",
        "uzb" => "uz",
        other => other,
    };
    two.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> SubContent {
        SubContent::Text(SubText { lines: vec![vec![Span { text: s.into(), ..Default::default() }]], align: 2, pos: None, layer: 0 })
    }

    fn store() -> SubtitleStore {
        SubtitleStore::new(
            vec![SubtitleTrackInfo {
                source: SubtitleSource::Embedded(2),
                language: Some("rus".into()),
                title: None,
                codec: "SRT".into(),
                bitmap: false,
                default: true,
                forced: false,
                hearing_impaired: false,
            }],
            0.0,
        )
    }

    #[test]
    fn events_are_sorted_deduplicated_and_windowed() {
        let s = store();
        s.select(Some(&SubtitleSource::Embedded(2)));
        s.push_embedded(2, 5.0, 7.0, Some(text("б")));
        s.push_embedded(2, 1.0, 3.0, Some(text("а")));
        // перемотка назад — тот же пакет ещё раз
        s.push_embedded(2, 1.0, 3.0, Some(text("а")));
        assert_eq!(s.events().len(), 2);
        let a = s.active(2.0, 0.5, 0.3);
        assert_eq!(a.events.len(), 1);
        assert!(a.upcoming.is_empty());
        let a = s.active(4.8, 0.5, 0.3);
        assert!(a.events.is_empty());
        assert_eq!(a.upcoming.len(), 1);
        // хвост анимации исчезновения
        assert_eq!(s.active(3.2, 0.0, 0.3).events.len(), 1);
        // сдвиг: субтитры на 1 с позже
        s.set_delay(1.0);
        assert_eq!(s.active(3.5, 0.0, 0.0).events.len(), 1);
    }

    #[test]
    fn open_events_close_at_next() {
        let s = store();
        s.select(Some(&SubtitleSource::Embedded(2)));
        s.push_embedded(2, 1.0, f64::INFINITY, Some(text("а")));
        s.push_embedded(2, 2.5, f64::INFINITY, None);
        let ev = s.events();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].end, 2.5);
    }
}

/// Живой прогон декодера: `SUBS_VIDEO=ролик [SUBS_TRACK=n] [SUBS_SEEK=с] cargo test --features
/// ffmpeg --lib e2e_decode -- --ignored --nocapture` — дорожки и события за первые секунды.
#[cfg(test)]
#[test]
#[ignore]
fn e2e_decode() {
    let Ok(path) = std::env::var("SUBS_VIDEO") else { return };
    let mut p = crate::video::VideoPlayer::open(&path).expect("ролик открылся");
    let store = p.subtitles();
    let n = store.add_external_near(Path::new(&path));
    let tracks = store.tracks();
    eprintln!("внешних: {n}");
    for (i, t) in tracks.iter().enumerate() {
        eprintln!("{i}: {:?} {:?} {:?} {} forced={} sdh={} default={}", t.source, t.language, t.title, t.codec, t.forced, t.hearing_impaired, t.default);
    }
    let idx: usize = std::env::var("SUBS_TRACK").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    assert!(p.set_subtitle_track(Some(&tracks[idx].source)));
    if let Some(t) = std::env::var("SUBS_SEEK").ok().and_then(|v| v.parse::<f64>().ok()) {
        p.seek(t).unwrap();
    }
    p.play();
    let secs: f64 = std::env::var("SUBS_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(6.0);
    let t0 = std::time::Instant::now();
    while t0.elapsed().as_secs_f64() < secs {
        let _ = p.poll_frame();
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    let pos = p.position_sec();
    eprintln!("позиция {pos:.2}");
    for e in store.events().iter().take(12) {
        let what = match &*e.content {
            SubContent::Text(t) => format!("{:?} align={} pos={:?}", t.plain(), t.align, t.pos),
            SubContent::Bitmap { rects, canvas } => format!("картинок {} холст {canvas:?}", rects.len()),
        };
        eprintln!("  {:.3}–{:.3}: {what}", e.start, e.end);
    }
    let a = store.active(pos, 0.0, 0.0);
    eprintln!("видно сейчас: {:?}", a.events.iter().map(|e| e.id).collect::<Vec<_>>());
}
