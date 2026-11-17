/* ══ Brutal ══════════════════════════════════════════════════════════════
 * Необрутализм: толстые рамки цвета чернил, жёсткие тени без размытия,
 * плоские насыщенные цвета, прямые углы. Space Grotesk для текста,
 * JetBrains Mono — номера разделов, подписи и служебные надписи КАПСОМ.
 * Нажатая кнопка «вдавливается»: сдвигается на место своей тени.
 */

.window-frame {
    background: var(--bg-base);
    border: 3px solid var(--ink);
    font-family: "Space Grotesk";
}
.window-frame:window-maximized, .window-frame:window-fullscreen { border-width: 0px; }

/* ── Заголовок окна: сплошная полоса ── */
.titlebar { height: 60px; background: var(--header-bg); border-bottom: 3px solid var(--ink); }
.tb-left { height: 57px; width: 252px; padding: 0px 16px; align-items: center; border-right: 3px solid var(--header-text); }
.brand-mark {
    width: 34px; height: 34px;
    background: var(--glow-1);
    border: 3px solid var(--header-text);
    align-items: center; justify-content: center;
    rotate: -6;
}
.brand-icon { icon-size: 18px; icon-color: #0f0f0f; }
.brand-title { color: var(--header-text); font-size: 20px; font-weight: 700; letter-spacing: -0.5px; }
.brand-sub { color: var(--header-text); font-family: "JetBrains Mono"; font-size: 10px; letter-spacing: 1px; opacity: 0.8; }
.tb-drag-wrap { height: 57px; }
.tb-drag { height: 57px; flex-grow: 1; min-width: 40px; align-items: center; padding-left: 18px; }
.tb-crumb { color: var(--header-text); font-family: "JetBrains Mono"; font-size: 12px; opacity: 0.7; }
.tb-crumb-sep { color: var(--glow-2); font-family: "JetBrains Mono"; font-size: 14px; font-weight: 700; }
.tb-title { color: var(--header-text); font-size: 18px; font-weight: 700; max-width: 460px; }
.tb-tools { height: 57px; padding: 0px 10px; align-items: center; }
.window-controls { height: 57px; padding: 0px 10px 0px 6px; align-items: center; }
SystemWindowControls { color: var(--header-text); background-color: var(--glow-2); }

.titlebar SegmentedButton {
    background: var(--header-bg);
    color: var(--header-text);
    accent-color: var(--header-text);
    --selected-color: var(--header-bg);
    border: 2px solid var(--header-text);
    border-radius: 0px;
    height: 34px;
    font-family: "JetBrains Mono";
    font-size: 12px;
}
.tb-lang {
    width: 130px;
    height: 34px;
    padding: 4px 10px;
    font-family: "JetBrains Mono";
    font-size: 12px;
    border-radius: 0px;
    background: var(--header-bg);
    color: var(--header-text);
    border: 2px solid var(--header-text);
    accent-color: var(--header-text);
    --popup-background: var(--bg-surface);
    --popup-color: var(--text);
    --popup-accent: var(--accent);
    --popup-border: var(--ink);
}
.tb-btn { color: var(--header-text); border-radius: 0px; border: 2px solid var(--header-text); &:hover { background: var(--glow-1); color: #0f0f0f; } }

/* ── Боковая панель: жёлтый блок с нумерованным списком ── */
.gallery-sidebar { width: 252px; background: var(--sidebar-bg); border-right: 3px solid var(--ink); }
.nav-page { padding: 6px 14px 16px 12px; }
.nav-group {
    color: var(--text);
    font-family: "JetBrains Mono";
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 2px;
    padding: 18px 6px 6px 6px;
    text-transform: uppercase;
}
.nav-item {
    padding: 7px 10px;
    margin: 2px 0px;
    border: 2px solid transparent;
    transition: background-color 120ms linear;
}
.nav-item:hover { background: var(--bg-surface); border-color: var(--ink); }
.nav-item-active {
    background: var(--ink);
    border-color: var(--ink);
    box-shadow: 4px 4px 0px var(--glow-2);
}
.nav-item-active:hover { background: var(--ink); }
.nav-num { color: var(--text); font-family: "JetBrains Mono"; font-size: 11px; font-weight: 700; opacity: 0.55; min-width: 18px; }
.nav-item-active .nav-num { color: var(--glow-2); opacity: 1; }
.nav-text { color: var(--text); font-size: 14px; font-weight: 500; }
.nav-item-active .nav-text { color: var(--bg-base); font-weight: 700; }

/* ── Бегущая строка ── */
.ticker { height: 34px; background: var(--glow-2); border-top: 3px solid var(--ink); }
.ticker-track { height: 31px; animation: brutal-ticker 20s linear infinite; }
.ticker-chunk { width: 1040px; height: 31px; justify-content: center; }
.ticker-text { color: #0f0f0f; font-family: "JetBrains Mono"; font-size: 13px; font-weight: 700; letter-spacing: 2px; }
@keyframes brutal-ticker {
    from { translate-x: 0px; }
    to   { translate-x: -1040px; }
}

/* ── Страница ── */
.content-frame { padding: 0px; background: var(--bg-base); }
.content-card { background: var(--bg-base); overflow: hidden; }
.content { background: var(--bg-base); padding: 30px 34px 34px 30px; }

.section-card {
    background: var(--bg-surface);
    border: 3px solid var(--ink);
    border-radius: 0px;
    padding: 22px;
    box-shadow: 6px 6px 0px var(--hard-shadow);
}
.section-title { color: var(--text); font-size: 26px; font-weight: 700; letter-spacing: -0.8px; }
.label { color: var(--text); font-family: "JetBrains Mono"; font-size: 11px; font-weight: 700; letter-spacing: 1.2px; text-transform: uppercase; }

/* ── Виджеты: рамка + жёсткая тень, нажатие вдавливает ── */
Button {
    background: var(--bg-surface);
    color: var(--text);
    border: 3px solid var(--ink);
    border-radius: 0px;
    padding: 8px 18px;
    font-weight: 700;
    box-shadow: 4px 4px 0px var(--hard-shadow);
    transition: translate-x 80ms linear, translate-y 80ms linear, box-shadow 80ms linear, background-color 120ms linear;
    &:hover { background: var(--glow-1); color: #0f0f0f; }
    &:pressed { translate-x: 4px; translate-y: 4px; box-shadow: 0px 0px 0px var(--hard-shadow); }
}
Button.primary { background: var(--accent); color: var(--on-accent); &:hover { background: var(--accent-hover); } }
Button.secondary { background: var(--bg-surface); border-color: var(--ink); }
Button.danger { background: var(--error); color: #ffffff; border-color: var(--ink); }
Button.text { border-width: 0px; box-shadow: 0px 0px 0px transparent; text-decoration: underline; }
ToolButton { border-radius: 0px; &:hover { background: var(--glow-1); color: #0f0f0f; } }
TextField, Dropdown, MultilineTextEdit, Autocomplete, SpinBox, DatePicker, TimePicker, ColorPicker, Combobox, Multiselect {
    background: var(--input-bg);
    border: 3px solid var(--ink);
    border-radius: 0px;
}
TextField { &:focus { border-color: var(--ink); box-shadow: 4px 4px 0px var(--accent); } }
SegmentedButton { border: 3px solid var(--ink); border-radius: 0px; background: var(--bg-surface); font-weight: 700; }
Checkbox, RadioButton { border-color: var(--ink); }
Toggle { border: 2px solid var(--ink); background: var(--bg-surface); &:checked { background: var(--accent); } }
ProgressBar { height: 14px; border: 2px solid var(--ink); border-radius: 0px; background: var(--bg-surface); color: var(--accent); }
Slider { height: 8px; border-radius: 0px; background: var(--bg-overlay); }
Chip { border-radius: 0px; border: 2px solid var(--ink); background-color: var(--glow-1); color: #0f0f0f; font-weight: 700; }
Tab { font-weight: 700; &:selected { color: var(--text); border-bottom-color: var(--ink); background: var(--glow-1); } }
TabBar { border-bottom: 3px solid var(--ink); }
Calendar, ListView, TabView, Dialog, FloatingWindow, PopupMenu, TableView, TreeView, PropertyGrid {
    border-color: var(--ink);
}
Calendar, DatePicker { --cal-radius: 0px; --cal-day-shape: square; --cal-panel-border: var(--ink); }
Divider { background: var(--ink); height: 3px; }
Badge { border-radius: 0px; border: 2px solid var(--ink); }
