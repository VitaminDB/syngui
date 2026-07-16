use syngui::animation::{Animation, Easing};
use syngui::mgui;
use syngui::prelude::*;
use syngui::widgets::*;

const STYLES: &str = include_str!("../styles/calculator.mss");

// ── Context: shared calculator state ────────────────────────────────────────

#[derive(Clone, Copy)]
struct CalcCtx {
    display: RwSignal<String>,
    expression: RwSignal<String>,
    anim_trigger: RwSignal<u32>,
    saved: RwSignal<f64>,
    operation: RwSignal<Option<char>>,
    new_number: RwSignal<bool>,
    just_evaluated: RwSignal<bool>,
}

impl CalcCtx {
    fn new() -> Self {
        Self {
            display: use_signal("0".to_string()),
            expression: use_signal(String::new()),
            anim_trigger: use_signal(0u32),
            saved: use_signal(0.0f64),
            operation: use_signal(None::<char>),
            new_number: use_signal(true),
            just_evaluated: use_signal(false),
        }
    }

    fn reset(&self) {
        self.display.set("0".into());
        self.expression.set(String::new());
        self.saved.set(0.0);
        self.operation.set(None);
        self.new_number.set(true);
        self.just_evaluated.set(false);
    }
}

fn format_number(n: f64) -> String {
    if n == n.floor() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{:.10}", n);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ── Desktop entry ──────────────────────────────────────────────────────────

pub fn run_desktop() {
    let ctx = CalcCtx::new();
    provide_context(ctx);

    App::new()
        .title("Calculator")
        .size(380, 500)
        .min_size(320, 480)
        .vsync(true)
        .gpu_backend(GpuBackend::Auto)
        .gpu_power(GpuPowerPreference::LowPower)
        .with_styles_str(STYLES)
        .with_debug_overlay(false)
        .run(|_| Box::new(build_calculator()));
}

// ── Android entry ──────────────────────────────────────────────────────────

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: syngui::app::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info),
    );
    log::info!("Calculator Android starting");

    let ctx = CalcCtx::new();
    provide_context(ctx);

    App::new()
        .title("Calculator")
        .vsync(true)
        .gpu_backend(GpuBackend::Gl)
        .gpu_power(GpuPowerPreference::LowPower)
        .with_android_app(app)
        .with_styles_str(STYLES)
        .run(|_| Box::new(build_calculator()));
}

// ── UI ──────────────────────────────────────────────────────────────────────

fn build_calculator() -> impl Widget {
    mgui! {
        Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch).class("calculator-root") => [
            Padding::only(16.0, 16.0, 16.0, 8.0) => [
                build_display(),
            ],
            DecoratedBox::new().class("grow") => [
                build_keypad(),
            ],
        ]
    }
}

fn build_display() -> impl Widget {
    let ctx = use_context::<CalcCtx>();
    let display = ctx.display;
    let expression = ctx.expression;
    let anim_trigger = ctx.anim_trigger;

    DecoratedBox::new()
        .clip(true)
        .child(mgui! {
            Column::new().gap(4.0) => [
                move || {
                    let expr = expression.get();
                    Text::new(&expr).class("display-expression")
                },
                move || {
                    let value = display.get();
                    let _t = anim_trigger.get();
                    Animated::new(
                        Text::new(&value).class("display-value")
                    )
                    .scale(Animation::tween(Easing::EaseOutBack)
                        .from(0.5).to(1.0).duration_ms(800).build())
                    .opacity(Animation::tween(Easing::EaseOutQuad)
                        .from(0.0).to(1.0).duration_ms(600).build())
                },
            ]
        })
        .class("display-container")
}

fn calc_button(label: &str, class: &str) -> impl Widget {
    let label_s = label.to_string();
    Button::new(label)
        .on_click(move || handle_input(&label_s))
        .class(class)
}

fn build_keypad() -> impl Widget {
    Padding::only(12.0, 8.0, 12.0, 12.0).child(mgui! {
        Grid::new(4).gap(10.0) => [
            calc_button("AC",  "btn-func"),
            calc_button("+/−", "btn-func"),
            calc_button("%",   "btn-func"),
            calc_button("÷",   "btn-operator"),
            calc_button("7", "btn-number"),
            calc_button("8", "btn-number"),
            calc_button("9", "btn-number"),
            calc_button("×", "btn-operator"),
            calc_button("4", "btn-number"),
            calc_button("5", "btn-number"),
            calc_button("6", "btn-number"),
            calc_button("−", "btn-operator"),
            calc_button("1", "btn-number"),
            calc_button("2", "btn-number"),
            calc_button("3", "btn-number"),
            calc_button("+", "btn-operator"),
            calc_button("0",  "btn-number"),
            calc_button("00", "btn-number"),
            calc_button(".",  "btn-number"),
            calc_button("=",  "btn-equals"),
        ]
    })
}

// ── Logic ───────────────────────────────────────────────────────────────────

fn handle_input(input: &str) {
    let ctx = use_context::<CalcCtx>();

    match input {
        "AC" => ctx.reset(),

        "+/−" => {
            if let Ok(n) = ctx.display.get_untracked().parse::<f64>() {
                let s = format_number(-n);
                ctx.display.set(s);
            }
        }

        "%" => {
            if let Ok(n) = ctx.display.get_untracked().parse::<f64>() {
                let s = format_number(n / 100.0);
                ctx.display.set(s);
            }
        }

        "+" | "−" | "×" | "÷" => {
            let op_char = match input {
                "+" => '+',
                "−" => '-',
                "×" => '×',
                "÷" => '÷',
                _ => unreachable!(),
            };

            if ctx.operation.get_untracked().is_some() && !ctx.new_number.get_untracked() {
                evaluate(&ctx);
            } else {
                let v = ctx.display.get_untracked().parse().unwrap_or(0.0);
                ctx.saved.set(v);
            }

            let saved = ctx.saved.get_untracked();
            ctx.expression
                .set(format!("{} {}", format_number(saved), input));
            ctx.operation.set(Some(op_char));
            ctx.new_number.set(true);
            ctx.just_evaluated.set(false);
        }

        "=" => {
            if let Some(op) = ctx.operation.get_untracked() {
                let op_sym = match op {
                    '+' => "+",
                    '-' => "−",
                    '×' => "×",
                    '÷' => "÷",
                    _ => "?",
                };
                let saved = ctx.saved.get_untracked();
                let current = ctx.display.get_untracked();
                let expr = format!("{} {} {}", format_number(saved), op_sym, current);

                evaluate(&ctx);

                let result_display = ctx.display.get_untracked();
                ctx.expression.set(format!("{} = {}", expr, result_display));
                ctx.operation.set(None);
                ctx.just_evaluated.set(true);

                let c = ctx.anim_trigger.get_untracked().wrapping_add(1);
                ctx.anim_trigger.set(c);
            }
        }

        "." => {
            if ctx.new_number.get_untracked() {
                ctx.display.set("0.".into());
                ctx.new_number.set(false);
            } else {
                let d = ctx.display.get_untracked();
                if !d.contains('.') {
                    ctx.display.set(format!("{}.", d));
                }
            }
        }

        digit => {
            let is_new = ctx.new_number.get_untracked();
            let is_eval = ctx.just_evaluated.get_untracked();
            let mut d = ctx.display.get_untracked();

            if is_new || is_eval {
                d = if digit == "00" {
                    "0".into()
                } else {
                    digit.into()
                };
                ctx.new_number.set(false);
                ctx.just_evaluated.set(false);
            } else if d == "0" && digit != "00" {
                d = digit.into();
            } else if !(d == "0" && digit == "00") {
                d.push_str(digit);
            }

            if d.len() > 15 {
                d.truncate(15);
            }

            ctx.display.set(d);
            let c = ctx.anim_trigger.get_untracked().wrapping_add(1);
            ctx.anim_trigger.set(c);
        }
    }
}

fn evaluate(ctx: &CalcCtx) {
    let current = ctx.display.get_untracked().parse::<f64>().unwrap_or(0.0);
    let saved = ctx.saved.get_untracked();

    let result = match ctx.operation.get_untracked() {
        Some('+') => saved + current,
        Some('-') => saved - current,
        Some('×') => saved * current,
        Some('÷') if current != 0.0 => saved / current,
        Some('÷') => f64::INFINITY,
        _ => current,
    };

    if result.is_infinite() {
        ctx.display.set("Error".into());
    } else {
        ctx.display.set(format_number(result));
    }
    ctx.saved.set(result);
    ctx.new_number.set(true);
}
