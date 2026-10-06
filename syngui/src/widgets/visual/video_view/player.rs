//! Видеоплеер: кадр во всю отведённую площадь, панель управления поверх
//! него с автоскрытием (по образцу плеера tv_rezka). Общий для приложений:
//! просмотрщик вложений и ноды synthos, видеоплеер synshell, tv_rezka.
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                                                  │
//! │                    ( ▶ )                         │ ← на паузе: большая ⏵
//! │                                                  │
//! │▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│ ← градиент к низу
//! │━━━━━━━━━━━━━━━━━━●───────────────────────────────│ ← перемотка во всю ширину
//! │ 0:12 / 1:04   −1  ⟲10  ( ⏸ )  10⟳  +1   🔊━━━  ⛶ │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! Что играет — [`MediaSource`]: файл через ffmpeg ([`FileSource`]) или
//! кадры из памяти со звуком ([`FramesSource`]). Компактный режим
//! ([`VideoPlayerView::compact`]) — для карточек ~360–420 px: кнопки мельче,
//! без ползунка громкости.
//!
//! Мышь: клик по кадру — пауза, двойной — во весь экран (если владелец дал
//! [`FullscreenCtl`]). Касания: тап по кадру при скрытой панели только
//! показывает её, при видимой — пауза/продолжить (решение ждёт окно двойного
//! тапа); двойной тап по левой/правой трети — ∓10 с (тапы следом по той же
//! стороне добавляют ещё по 10 с), по средней — во весь экран. Панель
//! прячется через [`HIDE_AFTER`] без движения мыши или касаний, пока идёт
//! воспроизведение; на паузе видна всегда. Клавиши (когда фокус в плеере или
//! он в модальном окне): пробел/K — пауза, ←/→ — ∓5 с, Shift+←/→ — ∓1 с,
//! J/L — ∓10 с, ↑/↓ — громкость, M — звук, F — во весь экран, 0–9 — доля.
//!
//! Фоновый поток-тикер (как `install_autohide` в tv_rezka) прячет панель,
//! ловит конец ролика, доводит отложенную перемотку, исполняет отложенный
//! тап и подхватывает паузу, поставленную снаружи. Источник он держит слабой
//! ссылкой, а живёт, пока жив «жетон» в обработчике клавиш: пересобрали
//! виджет или закрыли просмотр — поток завершается сам.
//!
//! Стили классов `vp-*` встроены во фреймворк (`styles/video_player.mss`,
//! см. `mss::defaults`); приложение переопределяет их теми же селекторами.
//!
//! ```ignore
//! let player = Arc::new(Mutex::new(VideoPlayer::open(path)?));
//! VideoPlayerView::file(player)
//!     .fullscreen(FullscreenCtl { active, toggle: Arc::new(move || ...) })
//!     .build()
//! ```

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crate::audio::{AudioBuffer, AudioPlayer};
use crate::core::sync::Mutex;
use crate::core::{Point, Rect, Size};
use crate::i18n::builtin;
use crate::input::{CursorIcon, Event, EventResult, Key, Modifiers};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::render::DisplayList;
use crate::signal::{use_signal, RwSignal};
use crate::video::{VideoFrame, VideoPlayer};
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, Text, UpdateContext, Widget, WidgetExt,
};
use crate::widgets::containers::{
    Column, DecoratedBox, GestureDetector, Reactive, Row, Stack, StackFit,
};
use crate::widgets::input::{EventHook, KeyReply};
use crate::widgets::visual::FramesView;
use crate::widgets::{ImageFit, Slider, ToolButton};

use super::VideoView;

// Иконки Material Icons (шрифт — фича `material-icons`).
const MI_PLAY_ARROW: &str = "\u{E037}";
const MI_PAUSE: &str = "\u{E034}";
const MI_REPLAY: &str = "\u{E042}";
const MI_REPLAY_10: &str = "\u{E059}";
const MI_FORWARD_10: &str = "\u{E056}";
const MI_VOLUME_UP: &str = "\u{E050}";
const MI_VOLUME_DOWN: &str = "\u{E04D}";
const MI_VOLUME_OFF: &str = "\u{E04F}";
const MI_FULLSCREEN: &str = "\u{E5D0}";
const MI_FULLSCREEN_EXIT: &str = "\u{E5D1}";

/// Сколько панель держится без движения мыши при воспроизведении.
const HIDE_AFTER: Duration = Duration::from_millis(2500);
/// Курсор ушёл с плеера — панель прячется через столько.
const LEAVE_GRACE: Duration = Duration::from_millis(600);
/// Шаг тикера: отложенная перемотка и автоскрытие.
const TICK: Duration = Duration::from_millis(50);
/// Перемотка при перетаскивании ползунка — не чаще раза в столько: каждый
/// `seek` файла пересоздаёт аудиовывод, и на каждое движение мыши звук
/// заикался бы.
const SEEK_THROTTLE: Duration = Duration::from_millis(90);
/// После последнего движения ползунка он ещё столько показывает цель, а не
/// часы плеера — те догоняют перемотку не сразу.
const SCRUB_SETTLE: Duration = Duration::from_millis(250);
/// Шаги перемотки: кнопки по краям и Shift+стрелки, стрелки, кнопки ⟲/⟳ и J/L.
const FINE_STEP: f64 = 1.0;
const ARROW_STEP: f64 = 5.0;
const BUTTON_STEP: f64 = 10.0;
const VOLUME_STEP: f32 = 0.1;
/// Тап по той же стороне в течение стольких после перемотки двойным тапом
/// — ещё шаг (серия, как в плеерах телефонов).
const TAP_SERIES: Duration = Duration::from_millis(800);

/// Громкость между роликами: следующий открывается с той же, а «Включить
/// звук» возвращает уровень до выключения.
static LAST_VOLUME: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0
static UNMUTE_VOLUME: AtomicU32 = AtomicU32::new(0x3F80_0000);

// ─────────────────────────────────────────────────────────────────────────────
// Источники
// ─────────────────────────────────────────────────────────────────────────────

/// Что играет плеер. Методы зовутся и из потока-тикера — реализации
/// потокобезопасны.
pub trait MediaSource: Send + Sync + 'static {
    fn duration(&self) -> f64;
    fn position(&self) -> f64;
    fn is_paused(&self) -> bool;
    fn play(&self);
    fn pause(&self);
    fn seek(&self, t: f64);
    fn volume(&self) -> f32;
    fn set_volume(&self, v: f32);
    fn has_audio(&self) -> bool;
    /// Холст с кадром. `pos` — позиция плеера в секундах.
    fn canvas(self: Arc<Self>, pos: RwSignal<f32>) -> Box<dyn Widget>;
    /// Холст сам пишет позицию в `pos` (как `VideoView`); иначе её раз в
    /// кадр переносят часы плеера ([`ClockTicker`]).
    fn drives_position(&self) -> bool;
    /// Раз в шаг тикера, из его потока: досинхронизировать то, что не
    /// удалось сразу (звук, который ещё готовился при перемотке).
    fn tick(&self) {}
}

/// Файл через ffmpeg: `VideoPlayer` с его часами и звуком.
pub struct FileSource(pub Arc<Mutex<VideoPlayer>>);

impl MediaSource for FileSource {
    fn duration(&self) -> f64 {
        self.0
            .lock()
            .map(|p| p.duration_sec().max(0.0))
            .unwrap_or(0.0)
    }

    fn position(&self) -> f64 {
        self.0.lock().map(|p| p.position_sec()).unwrap_or(0.0)
    }

    fn is_paused(&self) -> bool {
        self.0.lock().map(|p| p.is_paused()).unwrap_or(true)
    }

    fn play(&self) {
        if let Ok(mut p) = self.0.lock() {
            p.play();
        }
    }

    fn pause(&self) {
        if let Ok(mut p) = self.0.lock() {
            p.pause();
        }
    }

    fn seek(&self, t: f64) {
        if let Ok(mut p) = self.0.lock() {
            if let Err(e) = p.seek(t) {
                log::warn!("[video-player] seek {t:.2}s: {e}");
            }
        }
    }

    fn volume(&self) -> f32 {
        self.0.lock().map(|p| p.volume()).unwrap_or(1.0)
    }

    fn set_volume(&self, v: f32) {
        if let Ok(p) = self.0.lock() {
            p.set_volume(v);
        }
    }

    fn has_audio(&self) -> bool {
        self.0.lock().map(|p| p.meta().has_audio).unwrap_or(false)
    }

    fn canvas(self: Arc<Self>, pos: RwSignal<f32>) -> Box<dyn Widget> {
        Box::new(
            VideoView::new(self.0.clone())
                .fit(ImageFit::Contain)
                .position_signal(pos)
                .class("vp-canvas"),
        )
    }

    fn drives_position(&self) -> bool {
        true
    }
}

type Frames = Arc<Vec<Arc<VideoFrame>>>;

/// Кадры из памяти (например, выход генератора видео) и, если есть, их звук. Часы — системные:
/// позиция = точка старта + прошедшее время; звук стартует и ставится на
/// паузу вместе с ними.
pub struct FramesSource {
    frames: Frames,
    fps: f32,
    duration: f64,
    audio: Option<Arc<AudioBuffer>>,
    clock: std::sync::Mutex<FramesClock>,
}

struct FramesClock {
    /// Позиция на момент последнего старта/паузы/перемотки.
    base: f64,
    /// Идёт воспроизведение — с этого момента.
    started: Option<Instant>,
    volume: f32,
    out: Option<AudioPlayer>,
    /// Звук ещё готовится (ресэмплинг в потоке `AudioPlayer`) и перемотку
    /// не принимает — тикер повторит её, когда он будет готов.
    audio_seek_pending: bool,
}

/// Живые источники кадров: пересборка владельца (тела ноды, карточки) не
/// должна сбрасывать позицию и запускать второй звук поверх первого.
static FRAMES_SOURCES: std::sync::Mutex<Vec<(usize, usize, Weak<FramesSource>)>> =
    std::sync::Mutex::new(Vec::new());

fn frames_key(frames: &Frames, audio: Option<&Arc<AudioBuffer>>) -> (usize, usize) {
    (
        Arc::as_ptr(frames) as *const () as usize,
        audio.map_or(0, |a| Arc::as_ptr(a) as *const () as usize),
    )
}

impl FramesSource {
    /// Источник этих кадров и звука: уже открытый, пока его кто-то держит,
    /// иначе новый (на паузе, с начала).
    pub fn shared(frames: &Frames, fps: f32, audio: Option<Arc<AudioBuffer>>) -> Arc<Self> {
        let key = frames_key(frames, audio.as_ref());
        let mut list = FRAMES_SOURCES.lock().unwrap_or_else(|e| e.into_inner());
        list.retain(|(_, _, w)| w.strong_count() > 0);
        if let Some(src) = list
            .iter()
            .find(|(f, a, _)| (*f, *a) == key)
            .and_then(|(_, _, w)| w.upgrade())
        {
            return src;
        }
        let fps = fps.max(1.0);
        let src = Arc::new(Self {
            frames: frames.clone(),
            fps,
            duration: frames.len() as f64 / fps as f64,
            audio,
            clock: std::sync::Mutex::new(FramesClock {
                base: 0.0,
                started: None,
                volume: f32::from_bits(LAST_VOLUME.load(Ordering::Relaxed)),
                out: None,
                audio_seek_pending: false,
            }),
        });
        list.push((key.0, key.1, Arc::downgrade(&src)));
        src
    }

    /// Открытый источник этих кадров (любого звука), если он есть: внешняя
    /// кнопка (▶ ноды) управляет тем же плеером, что и кнопки на кадре.
    pub fn find(frames: &Frames) -> Option<Arc<Self>> {
        let key = Arc::as_ptr(frames) as *const () as usize;
        let list = FRAMES_SOURCES.lock().ok()?;
        list.iter()
            .filter(|(f, _, _)| *f == key)
            .find_map(|(_, _, w)| w.upgrade())
    }

    fn clock(&self) -> std::sync::MutexGuard<'_, FramesClock> {
        self.clock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn position_of(&self, c: &FramesClock) -> f64 {
        let t = match c.started {
            Some(at) => c.base + at.elapsed().as_secs_f64(),
            None => c.base,
        };
        t.clamp(0.0, self.duration)
    }

    /// Звук на позицию `t`; не готов — повторит тикер.
    fn seek_audio(c: &mut FramesClock, t: f64) {
        if let Some(out) = c.out.as_ref() {
            c.audio_seek_pending = out.seek_seconds(t).is_err();
        }
    }
}

impl MediaSource for FramesSource {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn position(&self) -> f64 {
        let c = self.clock();
        self.position_of(&c)
    }

    fn is_paused(&self) -> bool {
        self.clock().started.is_none()
    }

    fn play(&self) {
        let mut c = self.clock();
        if c.started.is_some() {
            return;
        }
        c.started = Some(Instant::now());
        let base = c.base;
        if c.out.is_none() {
            if let Some(buf) = self.audio.as_ref() {
                let started = if buf.channels >= 2 {
                    AudioPlayer::start_stereo(buf.pcm.clone(), buf.sample_rate)
                } else {
                    AudioPlayer::start(buf.pcm.clone(), buf.sample_rate)
                };
                match started {
                    Ok(out) => {
                        out.set_volume(c.volume);
                        c.out = Some(out);
                        if base > 0.0 {
                            Self::seek_audio(&mut c, base);
                        }
                    }
                    Err(e) => log::warn!("[video-player] звук кадров: {e:?}"),
                }
            }
        } else if let Some(out) = c.out.as_ref() {
            out.resume();
        }
    }

    fn pause(&self) {
        let mut c = self.clock();
        let pos = self.position_of(&c);
        c.base = pos;
        c.started = None;
        if let Some(out) = c.out.as_ref() {
            out.pause();
        }
    }

    fn seek(&self, t: f64) {
        let mut c = self.clock();
        let t = t.clamp(0.0, self.duration);
        c.base = t;
        if c.started.is_some() {
            c.started = Some(Instant::now());
        }
        Self::seek_audio(&mut c, t);
    }

    fn volume(&self) -> f32 {
        self.clock().volume
    }

    fn set_volume(&self, v: f32) {
        let mut c = self.clock();
        c.volume = v;
        if let Some(out) = c.out.as_ref() {
            out.set_volume(v);
        }
    }

    fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    fn canvas(self: Arc<Self>, pos: RwSignal<f32>) -> Box<dyn Widget> {
        Box::new(
            FramesView::new(self.frames.clone(), self.fps)
                .fit(ImageFit::Contain)
                .position_signal(pos)
                .loop_playback(false)
                .click_to_toggle(false)
                .class("vp-canvas"),
        )
    }

    fn drives_position(&self) -> bool {
        false
    }

    fn tick(&self) {
        let mut c = self.clock();
        if c.audio_seek_pending {
            let t = self.position_of(&c);
            Self::seek_audio(&mut c, t);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Виджет
// ─────────────────────────────────────────────────────────────────────────────

/// Кнопка «во весь экран»: состояние и переключатель — у владельца плеера
/// (он меняет раскладку вокруг и режим окна).
#[derive(Clone)]
pub struct FullscreenCtl {
    pub active: bool,
    pub toggle: Arc<dyn Fn() + Send + Sync>,
}

/// Плеер на всю отведённую площадь.
pub struct VideoPlayerView {
    source: Arc<dyn MediaSource>,
    fullscreen: Option<FullscreenCtl>,
    compact: bool,
    volume: Option<RwSignal<f32>>,
    position: Option<RwSignal<f32>>,
    header: Option<HeaderFn>,
}

/// Строит содержимое верхней полосы (см. [`VideoPlayerView::header`]).
pub type HeaderFn = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

impl VideoPlayerView {
    pub fn new(source: Arc<dyn MediaSource>) -> Self {
        Self {
            source,
            fullscreen: None,
            compact: false,
            volume: None,
            position: None,
            header: None,
        }
    }

    /// Файл, уже открытый в `VideoPlayer`.
    pub fn file(player: Arc<Mutex<VideoPlayer>>) -> Self {
        Self::new(Arc::new(FileSource(player)))
    }

    pub fn fullscreen(mut self, ctl: FullscreenCtl) -> Self {
        self.fullscreen = Some(ctl);
        self
    }

    /// Мелкие кнопки и без ползунка громкости — для карточек ~360–420 px.
    pub fn compact(mut self, on: bool) -> Self {
        self.compact = on;
        self
    }

    /// Громкость живёт у владельца (он её сохраняет): берётся отсюда и
    /// пишется сюда же, а не в общую «громкость прошлого ролика».
    pub fn volume_signal(mut self, sig: RwSignal<f32>) -> Self {
        self.volume = Some(sig);
        self
    }

    /// Позиция в секундах наружу — рядом с плеером что-то идёт в такт
    /// (волна звука под кадром).
    pub fn position_signal(mut self, sig: RwSignal<f32>) -> Self {
        self.position = Some(sig);
        self
    }

    /// Верхняя полоса поверх кадра (назад, название, свои кнопки): видна и
    /// прячется вместе с панелью управления, фон — градиент `.vp-header`.
    /// Содержимое строится заново при каждом показе.
    pub fn header(mut self, f: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        self.header = Some(Arc::new(f));
        self
    }

    pub fn build(self) -> impl Widget {
        build(self)
    }
}

/// Превью кадров из памяти: компактный плеер в рамке `.vp-node-preview`
/// (360×202). Источник общий на кадры ([`FramesSource::shared`]) —
/// пересборка владельца не сбрасывает позицию.
pub fn frames_preview(
    frames: &Frames,
    fps: f32,
    audio: Option<Arc<AudioBuffer>>,
) -> Box<dyn Widget> {
    Box::new(
        DecoratedBox::new().class("vp-node-preview").child(
            VideoPlayerView::new(FramesSource::shared(frames, fps, audio))
                .compact(true)
                .build(),
        ),
    )
}

/// Состояние, которое читает поток-тикер: сигналы syngui живут в
/// thread-local рантайме главного потока, из фона их можно только `set`.
struct Shared {
    last_activity: std::sync::Mutex<Instant>,
    /// Зеркало сигнала `visible`.
    shown: AtomicBool,
    /// Курсор над панелью — не прятать её из-под руки.
    over_controls: AtomicBool,
    scrub: std::sync::Mutex<Scrub>,
    /// Касания (см. [`Ctl::tap`]).
    touch: std::sync::Mutex<TouchState>,
}

/// Касания по кадру.
#[derive(Default)]
struct TouchState {
    /// Тап при видимой панели: пауза/продолжить, если за окно двойного тапа
    /// не придёт второй. Исполняет тикер.
    pending: Option<Instant>,
    /// Последняя перемотка двойным тапом: когда и в какую сторону (−1/+1) —
    /// тапы следом по той же стороне добавляют ещё шаг.
    series: Option<(Instant, i8)>,
}

/// Перетаскивание ползунка и перемотка клавишами.
#[derive(Default)]
struct Scrub {
    /// Цель, до которой ещё не дошли из-за [`SEEK_THROTTLE`].
    pending: Option<f64>,
    last_seek: Option<Instant>,
    last_change: Option<Instant>,
    /// Ползунок показывает `scrub`, а не позицию плеера.
    active: bool,
}

#[derive(Clone)]
struct Ctl {
    source: Arc<dyn MediaSource>,
    shared: Arc<Shared>,
    duration: f64,
    compact: bool,
    has_audio: bool,
    pos: RwSignal<f32>,
    paused: RwSignal<bool>,
    ended: RwSignal<bool>,
    visible: RwSignal<bool>,
    volume: RwSignal<f32>,
    /// Громкость владельца (см. [`VideoPlayerView::volume_signal`]).
    owner_volume: Option<RwSignal<f32>>,
    /// Цель перемотки, пока её тащат (см. [`Scrub::active`]).
    scrub: RwSignal<Option<f32>>,
    fullscreen: Option<FullscreenCtl>,
}

impl Ctl {
    /// Класс по режиму: `base` или `base-sm` в компактном.
    fn cls(&self, base: &str) -> String {
        if self.compact {
            format!("{base}-sm")
        } else {
            base.to_string()
        }
    }
}

fn build(view: VideoPlayerView) -> impl Widget {
    let source = view.source;
    let duration = source.duration();
    let volume = match view.volume {
        Some(sig) => sig.get_untracked().clamp(0.0, 1.0),
        None => f32::from_bits(LAST_VOLUME.load(Ordering::Relaxed)),
    };
    source.set_volume(volume);

    let ctl = Ctl {
        source: source.clone(),
        shared: Arc::new(Shared {
            last_activity: std::sync::Mutex::new(Instant::now()),
            shown: AtomicBool::new(true),
            over_controls: AtomicBool::new(false),
            scrub: std::sync::Mutex::new(Scrub::default()),
            touch: std::sync::Mutex::new(TouchState::default()),
        }),
        duration,
        compact: view.compact,
        has_audio: source.has_audio(),
        pos: {
            let start = source.position().min(duration) as f32;
            match view.position {
                Some(sig) => {
                    sig.set(start);
                    sig
                }
                None => use_signal(start),
            }
        },
        paused: use_signal(source.is_paused()),
        ended: use_signal(false),
        visible: use_signal(true),
        volume: use_signal(volume),
        owner_volume: view.volume,
        scrub: use_signal(None),
        fullscreen: view.fullscreen,
    };
    let token = Arc::new(());
    spawn_ticker(&ctl, Arc::downgrade(&token));

    // Касание приходит синтезированным щелчком (`input::touch`): по
    // `is_synthesized_mouse` его отличаем от мыши.
    let video = {
        let click = ctl.clone();
        let dbl = ctl.clone();
        GestureDetector::new()
            .cursor(CursorIcon::Default)
            .on_click_with_bounds(move |at, bounds| {
                if crate::input::is_synthesized_mouse() {
                    click.tap(at, bounds);
                } else {
                    click.toggle();
                }
            })
            .on_double_click_with_bounds(move |at, bounds| {
                if crate::input::is_synthesized_mouse() {
                    dbl.double_tap(at, bounds);
                } else {
                    dbl.toggle_fullscreen();
                }
            })
            .child(source.clone().canvas(ctl.pos))
    };

    let mut stage = Stack::new()
        .fit(StackFit::Expand)
        .child(video)
        .child(center_badge(ctl.clone()))
        .child(controls_layer(ctl.clone()));
    if let Some(h) = view.header {
        stage = stage.child(header_layer(ctl.clone(), h));
    }
    if !source.drives_position() {
        stage = stage.child(clock_layer(ctl.clone()));
    }

    let keys = ctl.clone();
    let captured = ctl.clone();
    let mover = ctl.clone();
    EventHook::new()
        .on_key_down(move |key, mods| {
            // Жетон живёт в обработчике: элемент удалили — поток-тикер
            // видит мёртвую слабую ссылку и выходит.
            let _alive = &token;
            keys.on_key(key, mods)
        })
        .capture_keys(move |key| captured.handles_key(key))
        .on_mouse_move(move |inside| {
            if inside {
                mover.wake();
            } else {
                mover.leave();
            }
        })
        .child(DecoratedBox::new().class("vp").child(stage))
}

// ─────────────────────────────────────────────────────────────────────────────
// Слои поверх кадра
// ─────────────────────────────────────────────────────────────────────────────

/// Большая ⏵ по центру на паузе и ⟲ в конце ролика. Колонка — прямой
/// ребёнок `Stack` и растянута им на всю сцену (`Reactive` отдаёт ребёнку
/// свободные ограничения — внутри него колонка сжалась бы к верху). Мимо
/// кнопки колонка пропускает клики к кадру под ней.
fn center_badge(ctl: Ctl) -> impl Widget {
    let badge = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let ended = ctl.ended.get();
        if !ctl.paused.get() && !ended {
            return vec![];
        }
        let c = ctl.clone();
        let (icon, tip) = if ended {
            (
                MI_REPLAY,
                builtin("video_player.replay.tooltip", "Watch again"),
            )
        } else {
            (
                MI_PLAY_ARROW,
                builtin("video_player.play.tooltip", "Play (Space)"),
            )
        };
        vec![Box::new(
            ToolButton::new(icon)
                .tooltip(tip)
                .on_click(move || c.toggle())
                .class(ctl.cls("vp-big-play")),
        )]
    });
    Column::new()
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(badge)
}

/// Панель у нижнего края. Как и у [`center_badge`], колонка снаружи
/// `Reactive`; `End` без распорки — над панелью клики проходят к кадру
/// (распорка-`DecoratedBox` перехватила бы их).
fn controls_layer(ctl: Ctl) -> impl Widget {
    let panel = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let shown = ctl.visible.get() || ctl.paused.get();
        if !shown {
            return vec![];
        }
        let hover = ctl.clone();
        vec![Box::new(
            GestureDetector::new()
                .cursor(CursorIcon::Default)
                .on_hover_change(move |over| {
                    log::debug!("[video-player] наведение на панель: {over}");
                    hover.shared.over_controls.store(over, Ordering::Relaxed);
                    if over {
                        hover.wake();
                    }
                })
                .child(
                    DecoratedBox::new().class(ctl.cls("vp-controls")).child(
                        Column::new()
                            .gap(if ctl.compact { 2.0 } else { 6.0 })
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .child(seek_bar(ctl.clone()))
                            .child(bottom_row(ctl.clone())),
                    ),
                ),
        )]
    });
    Column::new()
        .main_axis_alignment(MainAxisAlignment::End)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(panel)
}

/// Верхняя полоса владельца ([`VideoPlayerView::header`]) — видна вместе с
/// панелью; под ней клики проходят к кадру.
fn header_layer(ctl: Ctl, header: HeaderFn) -> impl Widget {
    let bar = Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let shown = ctl.visible.get() || ctl.paused.get();
        if !shown {
            return vec![];
        }
        let hover = ctl.clone();
        vec![Box::new(
            GestureDetector::new()
                .cursor(CursorIcon::Default)
                .on_hover_change(move |over| {
                    log::debug!("[video-player] наведение на панель: {over}");
                    hover.shared.over_controls.store(over, Ordering::Relaxed);
                    if over {
                        hover.wake();
                    }
                })
                .child(DecoratedBox::new().class(ctl.cls("vp-header")).child(header())),
        )]
    });
    Column::new()
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(bar)
}

/// Часы для источников без своих (кадры из памяти): пока идёт
/// воспроизведение, раз в кадр переносят позицию источника в `pos`.
/// Элемент вставляется заново на каждый старт — реестр анимаций syngui
/// читает заявку `wants_animate_tick` при вставке.
fn clock_layer(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        if ctl.paused.get() {
            return vec![];
        }
        vec![Box::new(ClockTicker {
            source: ctl.source.clone(),
            pos: ctl.pos,
        })]
    })
}

fn seek_bar(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let value = ctl.scrub.get().unwrap_or_else(|| ctl.pos.get());
        let dur = ctl.duration as f32;
        let c = ctl.clone();
        vec![Box::new(
            Slider::new()
                .value(value.clamp(0.0, dur.max(0.0)))
                .range(0.0, dur.max(0.1))
                .step(0.02)
                .on_change(move |t: f32| c.request_seek(t as f64))
                .class(ctl.cls("vp-seek")),
        )]
    })
}

/// Время слева, транспорт строго по центру, звук и экран справа — три ряда
/// во всю ширину в одном `Stack` (как в tv_rezka): центр не съезжает от
/// ширины боковых групп, а пустые места рядов пропускают клики к соседям.
/// Уже этого окна (телефон в портрете) ряд не вмещает всё: без ползунка
/// громкости (у телефона кнопки громкости) и шагов ∓1 с.
const NARROW_WIDTH: f32 = 560.0;

fn bottom_row(ctl: Ctl) -> impl Widget {
    let row_cls = ctl.cls("vp-row");
    // Строится внутри `Reactive` панели — подписка на размер окна пересобирает ряд.
    let narrow = crate::viewport::viewport_size().get().width < NARROW_WIDTH;
    let left = Row::new()
        .main_axis_alignment(MainAxisAlignment::Start)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(time_label(ctl.clone()))
        .class(row_cls.clone());

    let mut right_items: Vec<Box<dyn Widget>> = Vec::new();
    if ctl.has_audio {
        right_items.push(Box::new(mute_button(ctl.clone())));
        if !ctl.compact && !narrow {
            right_items.push(Box::new(volume_slider(ctl.clone())));
        }
    }
    if let Some(fs) = ctl.fullscreen.clone() {
        let c = ctl.clone();
        let (icon, tip) = if fs.active {
            (
                MI_FULLSCREEN_EXIT,
                builtin(
                    "video_player.exit_fullscreen.tooltip",
                    "Exit full screen (Esc)",
                ),
            )
        } else {
            (
                MI_FULLSCREEN,
                builtin("video_player.fullscreen.tooltip", "Full screen (F)"),
            )
        };
        right_items.push(Box::new(
            ToolButton::new(icon)
                .tooltip(tip)
                .on_click(move || c.toggle_fullscreen())
                .class(ctl.cls("vp-btn")),
        ));
    }
    let right = Row::new()
        .gap(if ctl.compact { 2.0 } else { 4.0 })
        .main_axis_alignment(MainAxisAlignment::End)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .children(right_items)
        .class(row_cls.clone());

    let back = ctl.clone();
    let fwd = ctl.clone();
    let mut center = Row::new()
        .gap(if ctl.compact { 6.0 } else { 12.0 })
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center);
    if !narrow {
        center = center.child(step_button(
            &ctl,
            "−1",
            -FINE_STEP,
            builtin("video_player.back_1.tooltip", "Back 1 s (Shift+←)"),
        ));
    }
    center = center
        .child(
            ToolButton::new(MI_REPLAY_10)
                .tooltip(builtin("video_player.back.tooltip", "Back 10 s (J)"))
                .on_click(move || back.seek_by(-BUTTON_STEP))
                .class(ctl.cls("vp-btn")),
        )
        .child(play_button(ctl.clone()))
        .child(
            ToolButton::new(MI_FORWARD_10)
                .tooltip(builtin("video_player.forward.tooltip", "Forward 10 s (L)"))
                .on_click(move || fwd.seek_by(BUTTON_STEP))
                .class(ctl.cls("vp-btn")),
        );
    if !narrow {
        center = center.child(step_button(
            &ctl,
            "+1",
            FINE_STEP,
            builtin("video_player.forward_1.tooltip", "Forward 1 s (Shift+→)"),
        ));
    }
    let center = center.class(row_cls);

    Stack::new().child(left).child(right).child(center)
}

/// Шаг на секунду: у Material нет иконки «1», поэтому подпись — текстом под
/// прозрачной кнопкой (кнопка сверху — ей hover, клик и подсказка).
fn step_button(ctl: &Ctl, label: &str, delta: f64, tip: String) -> impl Widget {
    let c = ctl.clone();
    Stack::new()
        .child(
            DecoratedBox::new()
                .class(ctl.cls("vp-step"))
                .child(Text::new(label.to_string()).class(ctl.cls("vp-step-text"))),
        )
        .child(
            ToolButton::new("")
                .tooltip(tip)
                .on_click(move || c.seek_by(delta))
                .class(ctl.cls("vp-btn")),
        )
}

/// Отдельный `Reactive` на кнопку: позиция тикает много раз в секунду, и
/// общий блок пересобирал бы кнопку между press и release — клик терялся
/// (так же, как с ⏵ аудио в просмотрщике вложений synthos).
fn play_button(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let paused = ctl.paused.get();
        let ended = ctl.ended.get();
        let (icon, tip) = match (ended, paused) {
            (true, _) => (
                MI_REPLAY,
                builtin("video_player.replay.tooltip", "Watch again"),
            ),
            (false, true) => (
                MI_PLAY_ARROW,
                builtin("video_player.play.tooltip", "Play (Space)"),
            ),
            (false, false) => (
                MI_PAUSE,
                builtin("video_player.pause.tooltip", "Pause (Space)"),
            ),
        };
        let c = ctl.clone();
        vec![Box::new(
            ToolButton::new(icon)
                .tooltip(tip)
                .on_click(move || c.toggle())
                .class(ctl.cls("vp-play")),
        )]
    })
}

fn time_label(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let cur = ctl.scrub.get().unwrap_or_else(|| ctl.pos.get()) as f64;
        let cur = cur.clamp(0.0, ctl.duration);
        let text = format!(
            "{} / {}",
            format_duration((cur * 1000.0) as u64),
            format_duration((ctl.duration * 1000.0) as u64)
        );
        vec![Box::new(Text::new(text).class(ctl.cls("vp-time")))]
    })
}

fn mute_button(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let v = ctl.volume.get();
        let (icon, tip) = if v <= 0.0 {
            (
                MI_VOLUME_OFF,
                builtin("video_player.unmute.tooltip", "Unmute (M)"),
            )
        } else if v < 0.5 {
            (
                MI_VOLUME_DOWN,
                builtin("video_player.mute.tooltip", "Mute (M)"),
            )
        } else {
            (
                MI_VOLUME_UP,
                builtin("video_player.mute.tooltip", "Mute (M)"),
            )
        };
        let c = ctl.clone();
        vec![Box::new(
            ToolButton::new(icon)
                .tooltip(tip)
                .on_click(move || c.toggle_mute())
                .class(ctl.cls("vp-btn")),
        )]
    })
}

fn volume_slider(ctl: Ctl) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let v = ctl.volume.get();
        let c = ctl.clone();
        vec![Box::new(
            Slider::new()
                .value(v)
                .range(0.0, 1.0)
                .step(0.01)
                .width(88.0)
                .on_change(move |nv: f32| c.set_volume(nv))
                .class("vp-volume"),
        )]
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Действия
// ─────────────────────────────────────────────────────────────────────────────

impl Ctl {
    /// Активность пользователя: панель показать и отложить автоскрытие.
    fn wake(&self) {
        wake_shared(&self.shared, self.visible);
    }

    /// Курсор ушёл с плеера: при воспроизведении панель прячется почти
    /// сразу, а не через полный [`HIDE_AFTER`].
    fn leave(&self) {
        if let Ok(mut t) = self.shared.last_activity.lock() {
            if let Some(early) = Instant::now().checked_sub(HIDE_AFTER - LEAVE_GRACE) {
                *t = (*t).min(early);
            }
        }
    }

    fn toggle(&self) {
        self.wake();
        toggle_playback(&*self.source, self.ended, self.paused, self.pos);
    }

    /// Тап по кадру. Панель скрыта — только показать её (как OK на пульте в
    /// tv_rezka: иначе пауза включалась бы, стоит тронуть экран). Видна —
    /// пауза/продолжить, но не сразу: это может быть первый тап двойного,
    /// поэтому решение ждёт окно двойного тапа (исполняет тикер). Тап по
    /// той же стороне вскоре после перемотки двойным тапом — ещё шаг.
    fn tap(&self, at: Point, bounds: Rect) {
        log::debug!("[video-player] касание {at:?}");
        // Пальцем над панелью «не стоят»: эмуляция указателя у композитора
        // могла оставить флаг наведения, и панель не пряталась бы.
        self.shared.over_controls.store(false, Ordering::Relaxed);
        let side = tap_side(at, bounds);
        let series = side != 0
            && self.shared.touch.lock().is_ok_and(|t| {
                t.series
                    .is_some_and(|(when, s)| s == side && when.elapsed() < TAP_SERIES)
            });
        if series {
            self.seek_by_tap(side);
            return;
        }
        let shown = self.visible.get_untracked() || self.paused.get_untracked();
        self.wake();
        if shown {
            if let Ok(mut t) = self.shared.touch.lock() {
                t.pending = Some(Instant::now());
            }
        }
    }

    /// Двойной тап: по левой/правой трети — ∓10 с, по средней — во весь
    /// экран. Отложенный первый тап отменяется.
    fn double_tap(&self, at: Point, bounds: Rect) {
        self.shared.over_controls.store(false, Ordering::Relaxed);
        if let Ok(mut t) = self.shared.touch.lock() {
            t.pending = None;
        }
        match tap_side(at, bounds) {
            0 => {
                self.wake();
                self.toggle_fullscreen();
            }
            side => self.seek_by_tap(side),
        }
    }

    fn seek_by_tap(&self, side: i8) {
        if let Ok(mut t) = self.shared.touch.lock() {
            t.pending = None;
            t.series = Some((Instant::now(), side));
        }
        self.seek_by(side as f64 * BUTTON_STEP);
    }

    fn toggle_fullscreen(&self) {
        if let Some(fs) = &self.fullscreen {
            (fs.toggle)();
        }
    }

    /// Перемотка ползунком или клавишами. Первая — сразу, дальше не чаще
    /// [`SEEK_THROTTLE`]; хвост доводит тикер.
    fn request_seek(&self, target: f64) {
        self.wake();
        let target = target.clamp(0.0, self.duration);
        self.scrub.set(Some(target as f32));
        self.ended.set(false);
        let now = Instant::now();
        let seek_now = {
            let Ok(mut s) = self.shared.scrub.lock() else {
                return;
            };
            s.active = true;
            s.last_change = Some(now);
            let due = s
                .last_seek
                .is_none_or(|at| now.duration_since(at) >= SEEK_THROTTLE);
            if due {
                s.last_seek = Some(now);
                s.pending = None;
            } else {
                s.pending = Some(target);
            }
            due
        };
        if seek_now {
            self.source.seek(target);
            self.pos.set(target as f32);
        }
    }

    fn seek_by(&self, delta: f64) {
        let base = self
            .scrub
            .get_untracked()
            .unwrap_or_else(|| self.pos.get_untracked()) as f64;
        self.request_seek(base + delta);
    }

    fn set_volume(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.source.set_volume(v);
        match self.owner_volume {
            Some(sig) => sig.set(v),
            None => LAST_VOLUME.store(v.to_bits(), Ordering::Relaxed),
        }
        if v > 0.0 {
            UNMUTE_VOLUME.store(v.to_bits(), Ordering::Relaxed);
        }
        self.volume.set(v);
        self.wake();
    }

    fn toggle_mute(&self) {
        if self.volume.get_untracked() > 0.0 {
            self.set_volume(0.0);
        } else {
            let back = f32::from_bits(UNMUTE_VOLUME.load(Ordering::Relaxed));
            self.set_volume(if back > 0.0 { back } else { 1.0 });
        }
    }

    fn handles_key(&self, key: Key) -> bool {
        match key {
            Key::Space
            | Key::K
            | Key::MediaPlayPause
            | Key::Left
            | Key::Right
            | Key::J
            | Key::L
            | Key::Home
            | Key::Num0
            | Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9 => true,
            Key::Up | Key::Down | Key::M => self.has_audio,
            Key::F => self.fullscreen.is_some(),
            // Esc во весь экран — выход из него; иначе Esc закрывает
            // просмотрщик (его ловит `Portal`).
            Key::Escape => self.fullscreen.as_ref().is_some_and(|f| f.active),
            _ => false,
        }
    }

    fn on_key(&self, key: Key, mods: Modifiers) -> KeyReply {
        if mods.ctrl || mods.alt || mods.meta || !self.handles_key(key) {
            return KeyReply::Ignore;
        }
        let arrow = if mods.shift { FINE_STEP } else { ARROW_STEP };
        match key {
            Key::Space | Key::K | Key::MediaPlayPause => self.toggle(),
            Key::Left => self.seek_by(-arrow),
            Key::Right => self.seek_by(arrow),
            Key::J => self.seek_by(-BUTTON_STEP),
            Key::L => self.seek_by(BUTTON_STEP),
            Key::Up => self.set_volume(self.volume.get_untracked() + VOLUME_STEP),
            Key::Down => self.set_volume(self.volume.get_untracked() - VOLUME_STEP),
            Key::M => self.toggle_mute(),
            Key::F | Key::Escape => self.toggle_fullscreen(),
            Key::Home => self.request_seek(0.0),
            other => {
                if let Some(n) = digit(other) {
                    self.request_seek(self.duration * n as f64 / 10.0);
                }
            }
        }
        KeyReply::Handled
    }
}

/// Пауза/продолжить; в конце ролика — с начала. Общее для [`Ctl::toggle`]
/// и отложенного тапа из тикера.
fn toggle_playback(
    source: &dyn MediaSource,
    ended: RwSignal<bool>,
    paused: RwSignal<bool>,
    pos: RwSignal<f32>,
) {
    if ended.get_untracked() {
        source.seek(0.0);
        source.play();
        ended.set(false);
        pos.set(0.0);
    } else if source.is_paused() {
        source.play();
    } else {
        source.pause();
    }
    paused.set(source.is_paused());
}

/// Треть кадра под касанием: −1 — левая, +1 — правая, 0 — средняя.
fn tap_side(at: Point, bounds: Rect) -> i8 {
    let w = bounds.size.width;
    if w <= 0.0 {
        return 0;
    }
    let x = (at.x - bounds.origin.x) / w;
    if x < 1.0 / 3.0 {
        -1
    } else if x > 2.0 / 3.0 {
        1
    } else {
        0
    }
}

/// «1:05» / «1:02:03».
fn format_duration(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn digit(key: Key) -> Option<u8> {
    Some(match key {
        Key::Num0 => 0,
        Key::Num1 => 1,
        Key::Num2 => 2,
        Key::Num3 => 3,
        Key::Num4 => 4,
        Key::Num5 => 5,
        Key::Num6 => 6,
        Key::Num7 => 7,
        Key::Num8 => 8,
        Key::Num9 => 9,
        _ => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Тикер
// ─────────────────────────────────────────────────────────────────────────────

/// Показать панель и отложить автоскрытие (главный поток).
fn wake_shared(shared: &Shared, visible: RwSignal<bool>) {
    if let Ok(mut t) = shared.last_activity.lock() {
        *t = Instant::now();
    }
    shared.shown.store(true, Ordering::Relaxed);
    visible.set(true);
}

/// Сигнал из потока тикера — только через очередь главного потока. Сам
/// `RwSignal::set` из фона тоже маршалит туда, но полагается на
/// зарегистрированный главный поток; в `TestHarness` его нет, и `set` ушёл
/// бы в thread-local рантайм тикера, где такого слота нет.
fn set_on_main<T: PartialEq + Send + Clone + 'static>(sig: RwSignal<T>, value: T) {
    crate::async_runtime::run_on_main_thread(move || sig.set(value));
}

/// Фоновый поток плеера: хвост перемотки, конец ролика, автоскрытие,
/// отложенный тап, пауза/старт, поставленные снаружи (▶ ноды).
fn spawn_ticker(ctl: &Ctl, token: Weak<()>) {
    let source = Arc::downgrade(&ctl.source);
    let shared = ctl.shared.clone();
    let duration = ctl.duration;
    let (pos_sig, paused_sig, ended_sig, visible_sig, scrub_sig) =
        (ctl.pos, ctl.paused, ctl.ended, ctl.visible, ctl.scrub);
    let initially_paused = ctl.paused.get_untracked();
    let spawned = std::thread::Builder::new()
        .name("video-player-tick".into())
        .spawn(move || {
            let mut last_paused = initially_paused;
            let mut last_pos = f64::NAN;
            let mut last_move = Instant::now();
            let mut moved = false;
            log::debug!("[video-player] тикер запущен");
            loop {
                std::thread::sleep(TICK);
                if token.upgrade().is_none() {
                    log::debug!("[video-player] тикер: виджет удалён");
                    break;
                }
                let Some(source) = source.upgrade() else {
                    log::debug!("[video-player] тикер: источник закрыт");
                    break;
                };
                source.tick();

                // Хвост перемотки: последняя цель, придержанная троттлингом.
                let (pending, settled) = match shared.scrub.lock() {
                    Ok(mut s) => {
                        let due = s.last_seek.is_none_or(|at| at.elapsed() >= SEEK_THROTTLE);
                        let pending = if due { s.pending.take() } else { None };
                        if pending.is_some() {
                            s.last_seek = Some(Instant::now());
                        }
                        let settled = s.active
                            && s.pending.is_none()
                            && s.last_change.is_some_and(|t| t.elapsed() >= SCRUB_SETTLE);
                        if settled {
                            s.active = false;
                        }
                        (pending, settled)
                    }
                    Err(_) => (None, false),
                };
                if let Some(t) = pending {
                    source.seek(t);
                    set_on_main(pos_sig, t as f32);
                }
                if settled {
                    set_on_main(scrub_sig, None);
                }

                // Тап, который не стал двойным, — пауза/продолжить. Сам
                // переключатель — на главном потоке (сигналы читаются там);
                // источник туда уходит слабой ссылкой.
                let tap_due = shared.touch.lock().is_ok_and(|mut t| {
                    let due = t
                        .pending
                        .is_some_and(|at| at.elapsed() >= crate::input::touch_config().double_tap);
                    if due {
                        t.pending = None;
                    }
                    due
                });
                if tap_due {
                    log::debug!("[video-player] тап: пауза/продолжить");
                    let weak = Arc::downgrade(&source);
                    let shared = shared.clone();
                    crate::async_runtime::run_on_main_thread(move || {
                        if let Some(src) = weak.upgrade() {
                            wake_shared(&shared, visible_sig);
                            toggle_playback(&*src, ended_sig, paused_sig, pos_sig);
                        }
                    });
                }
                let scrubbing = shared.scrub.lock().map(|s| s.active).unwrap_or(false);

                let paused = source.is_paused();
                let pos = source.position();
                if last_paused != paused {
                    last_paused = paused;
                    set_on_main(paused_sig, paused);
                    if !paused {
                        set_on_main(ended_sig, false);
                    }
                    last_move = Instant::now();
                    moved = false;
                }

                // Конец ролика: без звука часы идут дальше длительности, со
                // звуком встают чуть раньше неё — ловим оба случая.
                if !paused && !scrubbing && duration > 0.0 {
                    if (pos - last_pos).abs() > 1e-3 {
                        if !last_pos.is_nan() {
                            moved = true;
                        }
                        last_pos = pos;
                        last_move = Instant::now();
                    }
                    let stalled = moved && last_move.elapsed() >= Duration::from_millis(700);
                    if pos >= duration - 0.05 || (pos >= duration - 1.0 && stalled) {
                        source.pause();
                        last_paused = true;
                        set_on_main(paused_sig, true);
                        set_on_main(ended_sig, true);
                        set_on_main(pos_sig, duration as f32);
                        shared.shown.store(true, Ordering::Relaxed);
                        set_on_main(visible_sig, true);
                        continue;
                    }
                }
                drop(source);

                let idle = shared
                    .last_activity
                    .lock()
                    .map(|t| t.elapsed())
                    .unwrap_or_default();
                if !paused
                    && !scrubbing
                    && idle >= HIDE_AFTER
                    && !shared.over_controls.load(Ordering::Relaxed)
                    && shared.shown.swap(false, Ordering::Relaxed)
                {
                    log::debug!("[video-player] панель спрятана");
                    set_on_main(visible_sig, false);
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("[video-player] поток тикера не запустился: {e}");
    }
}

/// Часы кадра для [`FramesSource`]: раз в кадр UI — позиция источника в
/// `pos`, по ней `FramesView` выбирает кадр. Невидимый, 0×0.
struct ClockTicker {
    source: Arc<dyn MediaSource>,
    pos: RwSignal<f32>,
}

impl Widget for ClockTicker {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ClockTickerElement {
            id: ElementId::new(),
            source: self.source.clone(),
            pos: self.pos,
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT,
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

    fn mount(&self, _tree: &mut ElementTree, _parent: ElementId) {}
}

struct ClockTickerElement {
    id: ElementId,
    source: Arc<dyn MediaSource>,
    pos: RwSignal<f32>,
    bounds: Rect,
    dirty: DirtyFlags,
}

impl Element for ClockTickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ClockTicker>() {
            self.source = w.source.clone();
            self.pos = w.pos;
        }
    }

    fn layout(&mut self, _c: Constraints) -> Size {
        self.bounds = Rect::new(self.bounds.origin, Size::zero());
        Size::zero()
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _e: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        if self.source.is_paused() {
            return false;
        }
        let t = self.source.position() as f32;
        if (self.pos.get_untracked() - t).abs() > 1e-3 {
            self.pos.set(t);
        }
        true
    }

    fn wants_animate_tick(&self) -> bool {
        !self.source.is_paused()
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, p: Point) {
        self.bounds.origin = p;
    }

    fn mark_dirty(&mut self, f: DirtyFlags) {
        self.dirty |= f;
    }

    fn clear_dirty(&mut self, f: DirtyFlags) {
        self.dirty.remove(f);
    }

    fn is_dirty(&self, f: DirtyFlags) -> bool {
        self.dirty.contains(f)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, i: ElementId) {
        self.id = i;
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "VideoPlayerClock"
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{click_at, TestHarness};

    const W: f32 = 640.0;
    const H: f32 = 360.0;

    /// 30 с крошечных кадров при 24 fps, без звука.
    fn frames() -> Frames {
        Arc::new(
            (0..720)
                .map(|i| {
                    Arc::new(VideoFrame {
                        width: 4,
                        height: 4,
                        rgba: vec![(i % 255) as u8; 64].into(),
                        pts_sec: i as f64 / 24.0,
                        seek_generation: 0,
                        surface: None,
                    })
                })
                .collect(),
        )
    }

    fn settle(h: &mut TestHarness, engine: &crate::mss::StyleEngine) {
        crate::async_runtime::drain_main_thread_callbacks();
        h.rebuild();
        h.apply_styles(engine);
        h.layout(W, H);
    }

    fn tap(h: &mut TestHarness, at: Point) {
        h.touch_down(1, at);
        h.touch_up(1);
    }

    /// Дождаться, пока тикер исполнит отложенный тап (окно двойного тапа).
    fn wait_tap(h: &mut TestHarness, engine: &crate::mss::StyleEngine) {
        std::thread::sleep(crate::input::touch_config().double_tap + Duration::from_millis(150));
        settle(h, engine);
    }

    /// Владелец пересобирает плеер (Reactive по своему сигналу): тап по-прежнему
    /// ставит паузу, панель прячется по таймеру.
    #[test]
    fn rebuild_keeps_ticker() {
        crate::signal::allow_signal_reads_on_this_thread();
        let frames = frames();
        let src = FramesSource::shared(&frames, 24.0, None);
        let rev = use_signal(0u32);
        let inner = src.clone();
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let _ = rev.get();
            vec![Box::new(VideoPlayerView::new(inner.clone()).build())]
        })));
        let engine = h.apply_mss("");
        settle(&mut h, &engine);
        for i in 1..=3 {
            rev.set(i);
            settle(&mut h, &engine);
        }
        assert!(src.is_paused());
        let mid = Point::new(W / 2.0, H * 0.3);
        tap(&mut h, mid);
        settle(&mut h, &engine);
        wait_tap(&mut h, &engine);
        assert!(!src.is_paused(), "тап после пересборки — старт");
        std::thread::sleep(HIDE_AFTER + Duration::from_millis(400));
        settle(&mut h, &engine);
        assert!(h.find_by_class("vp-controls").is_empty(), "панель спряталась после пересборки");
        tap(&mut h, mid);
        settle(&mut h, &engine);
        wait_tap(&mut h, &engine);
        assert_eq!(h.find_by_class("vp-controls").len(), 1, "панель показалась");
        tap(&mut h, mid);
        settle(&mut h, &engine);
        wait_tap(&mut h, &engine);
        assert!(src.is_paused(), "второй тап при видимой панели — пауза");
    }

    #[test]
    fn builtin_styles_and_touch() {
        // Виджет строится до `TestHarness::new` — сигналы читаются уже здесь.
        crate::signal::allow_signal_reads_on_this_thread();
        let frames = frames();
        let src = FramesSource::shared(&frames, 24.0, None);
        let mut h = TestHarness::new(Box::new(VideoPlayerView::new(src.clone()).build()));
        // Без стилей приложения — только встроенные (`styles/video_player.mss`).
        let engine = h.apply_mss("");
        settle(&mut h, &engine);

        let one = |h: &TestHarness, class: &str| {
            let ids = h.find_by_class(class);
            assert_eq!(ids.len(), 1, "ровно один .{class}");
            h.element_bounds(ids[0])
        };
        let canvas = one(&h, "vp-canvas");
        assert_eq!(
            (canvas.size.width, canvas.size.height),
            (W, H),
            "кадр во всю сцену"
        );
        let controls = one(&h, "vp-controls");
        assert!(
            (controls.origin.y + controls.size.height - H).abs() < 1.0
                && controls.size.height < 170.0,
            "панель у низа и не съедает кадр: {controls:?}"
        );
        let play = one(&h, "vp-play");
        assert!(
            (play.size.width - 46.0).abs() < 0.5,
            "встроенный размер ⏯: {play:?}"
        );

        // На паузе панель видна: тап по кадру — старт, но после окна
        // двойного тапа.
        assert!(src.is_paused());
        let mid = Point::new(W / 2.0, H * 0.3);
        tap(&mut h, mid);
        settle(&mut h, &engine);
        assert!(src.is_paused(), "тап ждёт окно двойного");
        wait_tap(&mut h, &engine);
        assert!(!src.is_paused(), "одиночный тап при видимой панели — старт");

        // Двойной тап по правой трети — +10 с, без паузы (первый тап отменён).
        let before = src.position();
        tap(&mut h, Point::new(W * 0.85, H * 0.3));
        tap(&mut h, Point::new(W * 0.85, H * 0.3));
        settle(&mut h, &engine);
        wait_tap(&mut h, &engine);
        assert!(!src.is_paused(), "двойной тап не ставит паузу");
        assert!(
            src.position() >= before + 9.5,
            "+10 с: {before} → {}",
            src.position()
        );
        // Тап следом по той же стороне — ещё +10 с.
        let before = src.position();
        tap(&mut h, Point::new(W * 0.85, H * 0.3));
        settle(&mut h, &engine);
        assert!(
            src.position() >= before + 9.5,
            "серия: {before} → {}",
            src.position()
        );

        // Двойной тап по левой трети — назад.
        std::thread::sleep(TAP_SERIES);
        let before = src.position();
        tap(&mut h, Point::new(W * 0.1, H * 0.3));
        tap(&mut h, Point::new(W * 0.1, H * 0.3));
        // Вторая перемотка подряд придержана троттлингом — её доводит тикер.
        std::thread::sleep(SEEK_THROTTLE * 3);
        settle(&mut h, &engine);
        assert!(
            src.position() <= before - 9.0,
            "−10 с: {before} → {}",
            src.position()
        );

        // Панель спряталась по таймеру — тап только показывает её.
        std::thread::sleep(HIDE_AFTER + Duration::from_millis(300));
        settle(&mut h, &engine);
        assert!(
            h.find_by_class("vp-controls").is_empty(),
            "панель спряталась"
        );
        tap(&mut h, mid);
        settle(&mut h, &engine);
        wait_tap(&mut h, &engine);
        assert!(!src.is_paused(), "тап по скрытой панели не ставит паузу");
        assert_eq!(h.find_by_class("vp-controls").len(), 1, "панель показалась");

        // Мышь — как раньше: щелчок по кадру сразу ставит паузу.
        h.send_events(&click_at(mid));
        settle(&mut h, &engine);
        assert!(src.is_paused(), "щелчок мышью — пауза сразу");
    }
}
