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
