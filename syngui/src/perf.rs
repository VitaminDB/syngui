use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;
use web_time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();

#[inline]
pub fn is_enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("MGUI_PROFILE").is_ok())
}

#[derive(Default)]
struct PerfCounters {
    frames: u64,
    first_frame_printed: bool,
    frame_times_us: Vec<u64>,

    rebuild_us: u64,
    layout_us: u64,
    dl_us: u64,
    render_us: u64,
    apply_styles_us: u64,
    animate_us: u64,
    mm_handle_event_us: u64,
    button_text_measure_us: u64,

    rebuild_visits: u64,

    animate_visits: u64,
    animate_true: u64,
    animate_ticking: u64,

    measure_visits: u64,
    measure_cache_hit: u64,
    measure_grid_calls: u64,
    measure_grid_children: u64,
    measure_grid_estimated: u64,
    measure_grid_probe: u64,

    button_layout: u64,
    button_text_measure: u64,

    dl_visits: u64,
    dl_culled: u64,
    dl_invisible_skip: u64,

    dispatch_visits: u64,
    mm_dispatch_visits: u64,
    mm_dispatches: u64,

    apply_styles_calls: u64,
    apply_styles_iter: u64,
    apply_styles_rule_test: u64,

    draw_calls: u64,
    vertex_count: u64,
    dl_commands: u64,

    last_flush: Option<Instant>,
}

thread_local! {
    static PERF: RefCell<PerfCounters> = RefCell::new(PerfCounters::default());
}

#[inline]
pub fn add(counter: Counter, n: u64) {
    if !is_enabled() {
        return;
    }
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        match counter {
            Counter::RebuildVisit => p.rebuild_visits += n,
            Counter::AnimateVisit => p.animate_visits += n,
            Counter::AnimateTrue => p.animate_true += n,
            Counter::AnimateTicking => p.animate_ticking += n,
            Counter::MeasureVisit => p.measure_visits += n,
            Counter::MeasureCacheHit => p.measure_cache_hit += n,
            Counter::MeasureGridCall => p.measure_grid_calls += n,
            Counter::MeasureGridChild => p.measure_grid_children += n,
            Counter::MeasureGridEstimated => p.measure_grid_estimated += n,
            Counter::MeasureGridProbe => p.measure_grid_probe += n,
            Counter::ButtonLayout => p.button_layout += n,
            Counter::ButtonTextMeasure => p.button_text_measure += n,
            Counter::DlVisit => p.dl_visits += n,
            Counter::DlCulled => p.dl_culled += n,
            Counter::DlInvisibleSkip => p.dl_invisible_skip += n,
            Counter::DispatchVisit => p.dispatch_visits += n,
            Counter::MmDispatchVisit => p.mm_dispatch_visits += n,
            Counter::MmDispatch => p.mm_dispatches += n,
            Counter::ApplyStylesCall => p.apply_styles_calls += n,
            Counter::ApplyStylesIter => p.apply_styles_iter += n,
            Counter::ApplyStylesRuleTest => p.apply_styles_rule_test += n,
        }
    });
}

#[inline]
pub fn incr(counter: Counter) {
    if !is_enabled() {
        return;
    }
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        match counter {
            Counter::RebuildVisit => p.rebuild_visits += 1,
            Counter::AnimateVisit => p.animate_visits += 1,
            Counter::AnimateTrue => p.animate_true += 1,
            Counter::AnimateTicking => p.animate_ticking += 1,
            Counter::MeasureVisit => p.measure_visits += 1,
            Counter::MeasureCacheHit => p.measure_cache_hit += 1,
            Counter::MeasureGridCall => p.measure_grid_calls += 1,
            Counter::MeasureGridChild => p.measure_grid_children += 1,
            Counter::MeasureGridEstimated => p.measure_grid_estimated += 1,
            Counter::MeasureGridProbe => p.measure_grid_probe += 1,
            Counter::ButtonLayout => p.button_layout += 1,
            Counter::ButtonTextMeasure => p.button_text_measure += 1,
            Counter::DlVisit => p.dl_visits += 1,
            Counter::DlCulled => p.dl_culled += 1,
            Counter::DlInvisibleSkip => p.dl_invisible_skip += 1,
            Counter::DispatchVisit => p.dispatch_visits += 1,
            Counter::MmDispatchVisit => p.mm_dispatch_visits += 1,
            Counter::MmDispatch => p.mm_dispatches += 1,
            Counter::ApplyStylesCall => p.apply_styles_calls += 1,
            Counter::ApplyStylesIter => p.apply_styles_iter += 1,
            Counter::ApplyStylesRuleTest => p.apply_styles_rule_test += 1,
        }
    });
}

#[inline]
pub fn add_time(kind: TimeKind, dur: Duration) {
    if !is_enabled() {
        return;
    }
    let us = dur.as_micros() as u64;
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        match kind {
            TimeKind::ApplyStyles => p.apply_styles_us += us,
            TimeKind::Animate => p.animate_us += us,
            TimeKind::MouseMoveHandleEvent => p.mm_handle_event_us += us,
            TimeKind::ButtonTextMeasure => p.button_text_measure_us += us,
        }
    });
}

pub fn record_frame(
    rebuild: Duration,
    layout: Duration,
    dl: Duration,
    render: Duration,
    draw_calls: usize,
    vertex_count: usize,
    dl_commands: usize,
) {
    if !is_enabled() {
        return;
    }
    let frame_total_us = (rebuild + layout + dl + render).as_micros() as u64;
    PERF.with(|p| {
        let mut p = p.borrow_mut();
        p.frames += 1;
        p.rebuild_us += rebuild.as_micros() as u64;
        p.layout_us += layout.as_micros() as u64;
        p.dl_us += dl.as_micros() as u64;
        p.render_us += render.as_micros() as u64;
        p.draw_calls += draw_calls as u64;
        p.vertex_count += vertex_count as u64;
        p.dl_commands += dl_commands as u64;
        p.frame_times_us.push(frame_total_us);

        if !p.first_frame_printed {
            p.first_frame_printed = true;
            eprintln!(
                "[PROFILE first-frame] rebuild={}us layout={}us dl={}us render={}us TOTAL={}us  draws={} verts={} cmds={}\n\
                 [PROFILE first-frame]   measure_visits={} Button_layout={} text_measure={} measure_grid_calls={} probe={} estimated={}\n\
                 [PROFILE first-frame]   apply_styles_calls={} iter={} rule_test={}  dl_visits={} culled={} invisible={}",
                rebuild.as_micros(), layout.as_micros(), dl.as_micros(), render.as_micros(), frame_total_us,
                draw_calls, vertex_count, dl_commands,
                p.measure_visits, p.button_layout, p.button_text_measure,
                p.measure_grid_calls, p.measure_grid_probe, p.measure_grid_estimated,
                p.apply_styles_calls, p.apply_styles_iter, p.apply_styles_rule_test,
                p.dl_visits, p.dl_culled, p.dl_invisible_skip,
            );
        }

        let now = Instant::now();
        let due = match p.last_flush {
            None => {
                p.last_flush = Some(now);
                false
            }
            Some(t) => now.duration_since(t) >= Duration::from_secs(1),
        };
        if due {
            flush(&mut p, now);
        }
    });
}

fn flush(p: &mut PerfCounters, now: Instant) {
    let frames = p.frames.max(1);
    let mut times = std::mem::take(&mut p.frame_times_us);
    times.sort_unstable();
    let p50 = pct(&times, 0.50);
    let p95 = pct(&times, 0.95);
    let p99 = pct(&times, 0.99);
    let avg = times.iter().sum::<u64>() / times.len().max(1) as u64;

    eprintln!(
        "[PROFILE 1s] frames={} frame_us(avg/p50/p95/p99)={}/{}/{}/{}\n  \
         rebuild: {}us tot ({}us/frame, visits={})\n  \
         animate: {}us tot ({}us/frame, visits={}, ticking={}, returning_true={})\n  \
         layout:  {}us tot ({}us/frame, measure_visits={}, cache_hit={}, Button_layouts={}, text_measures={}[{}us], grid_calls={}, grid_children={}, grid_estimated={}, grid_probe={})\n  \
         styles:  {}us tot ({}us/frame, calls={}, iter={}, rule_test={})\n  \
         dl:      {}us tot ({}us/frame, visits={}, culled={}, invisible={}, commands={})\n  \
         render:  {}us tot ({}us/frame, draws={}, verts={})\n  \
         events:  mousemove_dispatches={} ({}us tot, {}us/event, dfs_visits={}, avg_visits/event={})\n  \
         total_dispatch_visits={}",
        frames, avg, p50, p95, p99,
        p.rebuild_us, p.rebuild_us / frames, p.rebuild_visits,
        p.animate_us, p.animate_us / frames, p.animate_visits, p.animate_ticking, p.animate_true,
        p.layout_us, p.layout_us / frames, p.measure_visits, p.measure_cache_hit,
            p.button_layout, p.button_text_measure, p.button_text_measure_us,
            p.measure_grid_calls, p.measure_grid_children, p.measure_grid_estimated, p.measure_grid_probe,
        p.apply_styles_us, p.apply_styles_us / frames, p.apply_styles_calls, p.apply_styles_iter, p.apply_styles_rule_test,
        p.dl_us, p.dl_us / frames, p.dl_visits, p.dl_culled, p.dl_invisible_skip, p.dl_commands,
        p.render_us, p.render_us / frames, p.draw_calls, p.vertex_count,
        p.mm_dispatches, p.mm_handle_event_us,
            p.mm_handle_event_us / p.mm_dispatches.max(1),
            p.mm_dispatch_visits,
            p.mm_dispatch_visits / p.mm_dispatches.max(1),
        p.dispatch_visits,
    );

    *p = PerfCounters {
        first_frame_printed: true,
        last_flush: Some(now),
        ..Default::default()
    };
}

fn pct(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[derive(Clone, Copy)]
pub enum Counter {
    RebuildVisit,
    AnimateVisit,
    AnimateTrue,
    AnimateTicking,
    MeasureVisit,
    MeasureCacheHit,
    MeasureGridCall,
    MeasureGridChild,
    MeasureGridEstimated,
    MeasureGridProbe,
    ButtonLayout,
    ButtonTextMeasure,
    DlVisit,
    DlCulled,
    DlInvisibleSkip,
    DispatchVisit,
    MmDispatchVisit,
    MmDispatch,
    ApplyStylesCall,
    ApplyStylesIter,
    ApplyStylesRuleTest,
}

#[derive(Clone, Copy)]
pub enum TimeKind {
    ApplyStyles,
    Animate,
    MouseMoveHandleEvent,
    ButtonTextMeasure,
}

/// Счётчики работы дерева для тестов и бенчей: сколько элементов создано и
/// удалено, сколько раз пересобирались `Reactive`, сколько markdown
/// разобрано, сколько раз звалась подсветка кода. В отличие от профиля
/// `MGUI_PROFILE` считают всегда: инкремент — `Cell` потока без проверок и
/// блокировок. Счётчики потоковые, как и runtime сигналов: дерево, сигналы и
/// отрисовка живут в UI-потоке, а параллельные тесты не сбивают друг другу
/// числа.
pub mod counters {
    use std::cell::Cell;
    use std::fmt;

    #[derive(Clone, Copy)]
    pub enum Tally {
        /// Элемент вставлен в дерево (`ElementTree::insert*`).
        ElementsCreated,
        /// Элемент удалён из дерева вместе с поддеревом.
        ElementsRemoved,
        /// `Reactive` вызвал свой builder.
        ReactiveBuilds,
        /// Вызов `parse_markdown`.
        MdParseCalls,
        /// Байт исходника, отданных `parse_markdown`.
        MdParseBytes,
        /// Вызов `SyntectHighlighter::highlight`.
        HighlightCalls,
        /// Байт кода, отданных подсветке.
        HighlightBytes,
        /// Вызов `ElementTree::rebuild_if_needed`.
        RebuildCalls,
        /// Проход внутри `rebuild_if_needed` с непустым реестром пересборки.
        RebuildPasses,
        /// Построение индекса правил MSS (`RuleIndex`) по всему stylesheet.
        StyleIndexBuilds,
        /// `MarkdownView::update` с настоящей работой: документ или
        /// настройки изменились (перемер, сброс выделения, пересборка меню).
        MdViewFullUpdates,
    }

    const TALLIES: usize = 11;
    const ZERO: Cell<u64> = Cell::new(0);

    thread_local! {
        static TALLY: [Cell<u64>; TALLIES] = const { [ZERO; TALLIES] };
    }

    #[inline]
    pub fn add(tally: Tally, n: u64) {
        TALLY.with(|t| {
            let c = &t[tally as usize];
            c.set(c.get().wrapping_add(n));
        });
    }

    #[inline]
    pub fn incr(tally: Tally) {
        add(tally, 1);
    }

    /// Снимок счётчиков потока. Разница двух снимков — [`Snapshot::since`].
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Snapshot {
        pub elements_created: u64,
        pub elements_removed: u64,
        pub reactive_builds: u64,
        pub md_parse_calls: u64,
        pub md_parse_bytes: u64,
        pub highlight_calls: u64,
        pub highlight_bytes: u64,
        pub rebuild_calls: u64,
        pub rebuild_passes: u64,
        pub style_index_builds: u64,
        pub md_view_full_updates: u64,
        /// Слотов сигналов в runtime потока. Слоты не освобождаются, так что
        /// в разнице снимков это число заведённых за интервал сигналов.
        pub signal_slots: u64,
    }

    impl Snapshot {
        /// Сколько набежало с `earlier`.
        pub fn since(&self, earlier: &Snapshot) -> Snapshot {
            Snapshot {
                elements_created: self.elements_created.saturating_sub(earlier.elements_created),
                elements_removed: self.elements_removed.saturating_sub(earlier.elements_removed),
                reactive_builds: self.reactive_builds.saturating_sub(earlier.reactive_builds),
                md_parse_calls: self.md_parse_calls.saturating_sub(earlier.md_parse_calls),
                md_parse_bytes: self.md_parse_bytes.saturating_sub(earlier.md_parse_bytes),
                highlight_calls: self.highlight_calls.saturating_sub(earlier.highlight_calls),
                highlight_bytes: self.highlight_bytes.saturating_sub(earlier.highlight_bytes),
                rebuild_calls: self.rebuild_calls.saturating_sub(earlier.rebuild_calls),
                rebuild_passes: self.rebuild_passes.saturating_sub(earlier.rebuild_passes),
                style_index_builds: self
                    .style_index_builds
                    .saturating_sub(earlier.style_index_builds),
                md_view_full_updates: self
                    .md_view_full_updates
                    .saturating_sub(earlier.md_view_full_updates),
                signal_slots: self.signal_slots.saturating_sub(earlier.signal_slots),
            }
        }

        /// Сложить интервалы (сумма по нескольким кадрам).
        pub fn plus(&self, other: &Snapshot) -> Snapshot {
            Snapshot {
                elements_created: self.elements_created + other.elements_created,
                elements_removed: self.elements_removed + other.elements_removed,
                reactive_builds: self.reactive_builds + other.reactive_builds,
                md_parse_calls: self.md_parse_calls + other.md_parse_calls,
                md_parse_bytes: self.md_parse_bytes + other.md_parse_bytes,
                highlight_calls: self.highlight_calls + other.highlight_calls,
                highlight_bytes: self.highlight_bytes + other.highlight_bytes,
                rebuild_calls: self.rebuild_calls + other.rebuild_calls,
                rebuild_passes: self.rebuild_passes + other.rebuild_passes,
                style_index_builds: self.style_index_builds + other.style_index_builds,
                md_view_full_updates: self.md_view_full_updates + other.md_view_full_updates,
                signal_slots: self.signal_slots + other.signal_slots,
            }
        }
    }

    impl fmt::Display for Snapshot {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "elem +{} -{} | reactive {} | md {}x {:.1}KB | hl {}x {:.1}KB | rebuild {}/{} | mss-idx {} | md-upd {} | slots +{}",
                self.elements_created,
                self.elements_removed,
                self.reactive_builds,
                self.md_parse_calls,
                self.md_parse_bytes as f64 / 1024.0,
                self.highlight_calls,
                self.highlight_bytes as f64 / 1024.0,
                self.rebuild_calls,
                self.rebuild_passes,
                self.style_index_builds,
                self.md_view_full_updates,
                self.signal_slots,
            )
        }
    }

    pub fn snapshot() -> Snapshot {
        let v = TALLY.with(|t| std::array::from_fn::<u64, TALLIES, _>(|i| t[i].get()));
        Snapshot {
            elements_created: v[Tally::ElementsCreated as usize],
            elements_removed: v[Tally::ElementsRemoved as usize],
            reactive_builds: v[Tally::ReactiveBuilds as usize],
            md_parse_calls: v[Tally::MdParseCalls as usize],
            md_parse_bytes: v[Tally::MdParseBytes as usize],
            highlight_calls: v[Tally::HighlightCalls as usize],
            highlight_bytes: v[Tally::HighlightBytes as usize],
            rebuild_calls: v[Tally::RebuildCalls as usize],
            rebuild_passes: v[Tally::RebuildPasses as usize],
            style_index_builds: v[Tally::StyleIndexBuilds as usize],
            md_view_full_updates: v[Tally::MdViewFullUpdates as usize],
            signal_slots: crate::signal::signal_slot_count() as u64,
        }
    }

    /// Обнулить счётчики потока. Число слотов сигналов — состояние runtime,
    /// оно не сбрасывается.
    pub fn reset() {
        TALLY.with(|t| t.iter().for_each(|c| c.set(0)));
    }
}

#[cfg(test)]
mod tests {
    use super::counters::{self, snapshot};
    use crate::prelude::*;
    use crate::signal::use_signal;
    use crate::testing::TestHarness;
    use crate::widgets::containers::reactive::Reactive;

    /// Счётчики видят работу дерева: пересборку `Reactive`, созданные и
    /// удалённые элементы, вызовы и проходы пересборки, новые слоты сигналов.
    #[test]
    fn counters_track_tree_work() {
        crate::signal::allow_signal_reads_on_this_thread();
        let before = snapshot();
        let n = use_signal(1usize);
        assert_eq!(snapshot().since(&before).signal_slots, 1);

        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            (0..n.get())
                .map(|i| Box::new(Text::new(format!("{i}"))) as Box<dyn Widget>)
                .collect()
        })));
        let start = snapshot();
        h.rebuild();
        let d = snapshot().since(&start);
        assert_eq!(d.reactive_builds, 1);
        assert_eq!(d.elements_created, 1);
        assert_eq!((d.rebuild_calls, d.rebuild_passes), (1, 1));

        let start = snapshot();
        n.set(3);
        h.rebuild();
        let d = snapshot().since(&start);
        assert_eq!((d.reactive_builds, d.elements_created, d.elements_removed), (1, 2, 0));

        let start = snapshot();
        n.set(0);
        h.rebuild();
        let d = snapshot().since(&start);
        assert_eq!((d.elements_created, d.elements_removed), (0, 3));

        // Без изменений пересборки нет: вызов есть, проходов нет.
        let start = snapshot();
        h.rebuild();
        let d = snapshot().since(&start);
        assert_eq!((d.rebuild_calls, d.rebuild_passes, d.reactive_builds), (1, 0, 0));

        counters::reset();
        let z = snapshot();
        assert_eq!((z.elements_created, z.reactive_builds, z.rebuild_calls), (0, 0, 0));
        assert!(z.signal_slots > 0, "слоты сигналов — состояние runtime, reset их не трогает");
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn counters_count_markdown_parse() {
        let src = "# Заголовок\n\nабзац с `кодом`\n";
        let start = snapshot();
        let _ = crate::widgets::visual::markdown_view::parse_markdown(src);
        let d = snapshot().since(&start);
        assert_eq!((d.md_parse_calls, d.md_parse_bytes), (1, src.len() as u64));
    }

    #[cfg(feature = "markdown-syntax")]
    #[test]
    fn counters_count_highlight_calls() {
        use crate::widgets::visual::markdown_view::{CodeHighlighter, SyntectHighlighter};
        let code = "fn main() {\n    println!(\"hi\");\n}\n";
        let h = SyntectHighlighter::new();
        let start = snapshot();
        let _ = h.highlight(code, Some("rust"));
        let _ = h.highlight(code, None);
        let d = snapshot().since(&start);
        assert_eq!((d.highlight_calls, d.highlight_bytes), (2, 2 * code.len() as u64));
    }
}
