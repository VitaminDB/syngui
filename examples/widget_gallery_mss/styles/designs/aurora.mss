/* ══ Aurora ══════════════════════════════════════════════════════════════
 * Стекло над северным сиянием: три пятна радиальных градиентов медленно
 * плывут под плавающими полупрозрачными панелями с размытием подложки.
 * Пилюли, градиенты, мягкое свечение, крупные скругления; шрифт Outfit.
 */

.window-frame {
    background: var(--bg-base);
    border-radius: 18px;
    font-family: "Outfit";
}
.window-frame:window-maximized, .window-frame:window-fullscreen { border-radius: 0px; }

/* ── Сияние ── */
.aurora-base { background: linear-gradient(160deg, var(--bg-base), var(--bg-surface)); }
.aurora-blob { pointer-events: none; }
.aurora-blob-1 {
    background: radial-gradient(circle at 14% 18%, var(--glow-1), transparent 46%);
    animation: aurora-drift-1 22s ease-in-out infinite;
}
.aurora-blob-2 {
    background: radial-gradient(circle at 86% 30%, var(--glow-2), transparent 42%);
    opacity: 0.8;
    animation: aurora-drift-2 28s ease-in-out infinite;
}
.aurora-blob-3 {
    background: radial-gradient(circle at 55% 95%, var(--glow-3), transparent 50%);
    opacity: 0.85;
    animation: aurora-drift-3 34s ease-in-out infinite;
}
.aurora-grain { pointer-events: none; }

@keyframes aurora-drift-1 {
    0%   { translate-x: 0px;   translate-y: 0px;  scale: 1.0; }
    50%  { translate-x: 160px; translate-y: 90px; scale: 1.15; }
    100% { translate-x: 0px;   translate-y: 0px;  scale: 1.0; }
}
@keyframes aurora-drift-2 {
    0%   { translate-x: 0px;    translate-y: 0px;   scale: 1.1; }
    50%  { translate-x: -180px; translate-y: 120px; scale: 0.95; }
    100% { translate-x: 0px;    translate-y: 0px;   scale: 1.1; }
}
@keyframes aurora-drift-3 {
    0%   { translate-x: 0px;   translate-y: 0px;    scale: 1.0; }
    50%  { translate-x: -90px; translate-y: -140px; scale: 1.2; }
    100% { translate-x: 0px;   translate-y: 0px;    scale: 1.0; }
}

/* ── Плавающие стеклянные панели ── */
.shell { padding: 12px; }
.titlebar, .gallery-sidebar, .content-card {
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
    backdrop-filter: blur(28px);
    box-shadow: 0 18px 50px var(--shadow-color);
}

.titlebar { height: 58px; border-radius: 20px; }
.tb-left { height: 56px; width: 240px; padding: 0px 14px 0px 14px; align-items: center; }
.brand-mark {
    width: 34px; height: 34px; border-radius: 17px;
    background: conic-gradient(from 0deg at center, var(--glow-1), var(--glow-2), var(--glow-3), var(--glow-1));
    align-items: center; justify-content: center;
    box-shadow: 0 0 22px var(--focus-ring);
}
.brand-icon { icon-size: 18px; icon-color: #ffffff; }
.brand-title {
    font-size: 16px; font-weight: 700; letter-spacing: 2px;
    text-gradient: linear-gradient(90deg, var(--accent), var(--pink), var(--info), var(--accent));
    text-animation: gradient 6s;
}
.brand-sub { color: var(--text-subtle); font-size: 11px; }
.tb-drag-wrap { height: 56px; }
.tb-drag { height: 56px; flex-grow: 1; min-width: 40px; align-items: center; justify-content: center; }
.tb-crumb { color: var(--text-muted); font-size: 13px; }
.tb-crumb-sep { color: var(--text-muted); font-size: 13px; }
.tb-title { color: var(--text); font-size: 14px; font-weight: 600; max-width: 420px; }
.tb-tools { height: 56px; padding: 0px 6px; align-items: center; }
.window-controls { height: 56px; padding: 0px 12px 0px 4px; align-items: center; }
SystemWindowControls { color: var(--text); background-color: var(--header-fill); }

.titlebar SegmentedButton {
    background: var(--header-fill);
    color: var(--text);
    accent-color: var(--accent);
    border-color: var(--glass-border);
    border-radius: 16px;
    height: 32px;
    font-size: 12px;
}
.tb-lang {
    width: 130px;
    height: 32px;
    padding: 4px 12px;
    font-size: 12px;
    border-radius: 16px;
    background: var(--header-fill);
    color: var(--text);
    border-color: var(--glass-border);
    accent-color: var(--accent);
    --popup-background: var(--bg-surface);
    --popup-color: var(--text);
    --popup-accent: var(--accent);
    --popup-border: var(--border);
}
.tb-btn { color: var(--text); border-radius: 16px; &:hover { background: var(--header-fill); color: var(--accent); } }

/* ── Боковая панель: пилюли ── */
.gallery-sidebar { width: 240px; border-radius: 24px; padding: 8px 0px 8px 0px; }
.nav-page { padding: 4px 10px 10px 10px; }
.nav-group {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 1.5px;
    padding: 16px 12px 6px 12px;
}
.nav-item {
    padding: 9px 14px;
    border-radius: 14px;
    transition: background-color 200ms ease-out;
}
.nav-item:hover { background: var(--section-hover); }
.nav-item-active {
    background: linear-gradient(100deg, var(--accent), var(--pink));
    box-shadow: 0 8px 24px var(--focus-ring);
}
.nav-item-active:hover { background: linear-gradient(100deg, var(--accent-hover), var(--pink)); }
.nav-icon { icon-size: 18px; icon-color: var(--text-subtle); }
.nav-item-active .nav-icon { icon-color: #ffffff; }
.nav-text { color: var(--text-subtle); font-size: 14px; }
.nav-item-active .nav-text { color: #ffffff; font-weight: 600; }

/* ── Страница ── */
.content-frame { padding: 0px; }
.content-card { border-radius: 26px; overflow: hidden; }
.content { background: transparent; padding: 28px; }

.section-card {
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
    border-radius: 22px;
    padding: 24px;
    box-shadow: 0 10px 30px var(--shadow-color);
}
.section-title {
    font-size: 22px;
    font-weight: 700;
    text-gradient: linear-gradient(90deg, var(--text), var(--accent));
}
.label { color: var(--text-subtle); font-size: 13px; letter-spacing: 0.4px; }

/* ── Виджеты: пилюли и свечение ── */
Button { border-radius: 20px; padding: 9px 20px; background: var(--button-bg); border: 1px solid var(--border); }
Button.primary {
    background: linear-gradient(100deg, var(--accent), var(--pink));
    color: #ffffff;
    border-width: 0px;
    box-shadow: 0 8px 22px var(--focus-ring);
    &:hover { background: linear-gradient(100deg, var(--accent-hover), var(--pink)); box-shadow: 0 10px 30px var(--focus-ring); }
}
Button.secondary { border-radius: 20px; border-color: var(--border); background: var(--glass-bg); }
Button.text { border-width: 0px; background: transparent; }
Button:disabled, Button.primary:disabled, Button.danger:disabled {
    background: var(--bg-overlay);
    color: var(--text-muted);
    box-shadow: 0px 0px 0px transparent;
}
Button.danger { border-radius: 20px; background: linear-gradient(100deg, var(--error), var(--orange)); border-width: 0px; }
ToolButton { border-radius: 14px; }
TextField, Dropdown, MultilineTextEdit, Autocomplete, SpinBox, DatePicker, TimePicker, ColorPicker, Combobox, Multiselect {
    background: var(--input-bg);
    border-color: var(--border);
    border-radius: 14px;
}
TextField { &:focus { border-color: var(--accent); box-shadow: 0 0 0 4px var(--focus-ring), 0 0 24px var(--focus-ring); } }
SegmentedButton { border-radius: 16px; background: var(--glass-bg); border-color: var(--border); --selected-color: #ffffff; }
Toggle { &:checked { background: linear-gradient(90deg, var(--accent), var(--pink)); } }
ProgressBar { height: 8px; border-radius: 4px; background: var(--bg-overlay); color: var(--accent); }
Slider { background: var(--bg-overlay); }
Chip { border-radius: 16px; background-color: var(--glass-bg); }
Calendar, ListView, TabView, Dialog, FloatingWindow, PopupMenu, TableView, TreeView, PropertyGrid {
    border-color: var(--glass-border);
}
TabBar { background: transparent; }
Divider { background: var(--glass-border); }
