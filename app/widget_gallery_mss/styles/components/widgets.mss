/* === Element-type selectors === */

Button {
    background: var(--button-bg);
    color: var(--text);
    accent-color: var(--accent);
    border-radius: 8px;
    padding: 8px 16px;
    font-size: 14px;
    transition: all 200ms ease;
    &:hover {
        background: var(--bg-overlay);
    }
    &:pressed {
        background: var(--button-pressed);
    }
}

Button.primary {
    background: var(--accent);
    color: #ffffff;
    &:hover { background: var(--accent-hover); }
    &:pressed { background: var(--button-pressed); }
}

Button.secondary {
    background: transparent;
    color: var(--text);
    border: 1px solid var(--border);
    &:hover { background: var(--bg-overlay); border-color: var(--accent); }
    &:pressed { background: var(--bg-elevated); }
}

Button.danger {
    background: #EF4444;
    color: #ffffff;
    &:hover { background: #DC2626; }
    &:pressed { background: #B91C1C; }
}

Button.text {
    background: transparent;
    color: var(--accent);
    &:hover { background: var(--bg-overlay); }
}

ToolButton {
    background: transparent;
    color: var(--text-subtle);
    border-radius: 6px;
    padding: 8px;
    font-size: 18px;
    transition: background-color 150ms ease, color 150ms ease;
    &:hover {
        background: var(--bg-overlay);
        color: var(--text);
    }
}

TextField {
    background: var(--input-bg);
    color: var(--text);
    accent-color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    font-size: 14px;
    &:focus {
        border-color: var(--accent);
        box-shadow: 0 0 0 3px var(--focus-ring);
    }
}

Checkbox {
    color: var(--text);
    accent-color: var(--accent);
    font-size: 14px;
    transition: background-color 200ms ease;
    &:checked {
        background: var(--accent);
        border-color: var(--accent);
    }
}

RadioButton {
    color: var(--text);
    accent-color: var(--accent);
    font-size: 14px;
    border-color: var(--border);
    &:checked {
        background: var(--accent);
        border-color: var(--accent);
    }
}

Toggle {
    width: 48px;
    height: 26px;
    background: var(--bg-elevated);
    color: #ffffff;
    accent-color: var(--accent);
    border-radius: 13px;
    transition: background-color 250ms ease-out;
    &:checked {
        background: var(--success);
    }
}

Slider {
    height: 6px;
    background: var(--bg-overlay);
    color: var(--accent);
    accent-color: var(--accent);
    border-radius: 3px;
}

/* Shared input-like widgets */
MultilineTextEdit, Autocomplete, SpinBox, DatePicker,
TimePicker, ColorPicker {
    background: var(--input-bg);
    color: var(--text);
    accent-color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 8px;
}

Dropdown {
    background: var(--input-bg);
    color: var(--text);
    accent-color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px;
    font-size: 14px;
}

ProgressBar {
    height: 8px;
    background: var(--bg-overlay);
    color: var(--accent);
    accent-color: var(--accent);
    border-radius: 4px;
}

TabBar {
    background: var(--bg-surface);
    accent-color: var(--accent);
    border-bottom: 1px solid var(--border);
}

Tab {
    padding: 12px 24px;
    color: var(--text-subtle);
    accent-color: var(--accent);
    font-size: 14px;
    transition: background-color 150ms ease, color 150ms ease;
    &:hover {
        color: var(--text);
        background: var(--bg-overlay);
    }
    &:selected {
        color: var(--accent);
        border-bottom-color: var(--accent);
    }
}

Toolbar {
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    padding: 8px 16px;
}

Divider {
    background: var(--border);
    height: 1px;
}

ScrollView {
    background: transparent;
}

Combobox, Multiselect {
    background: var(--input-bg);
    color: var(--text);
    accent-color: var(--accent);
    border: 1px solid var(--border);
    border-radius: 8px;
}

Chip {
    /* `background-color` (not `background`) so inline `.style("background-color", ...)`
       can override per-instance — ComputedStyle::background_color() prefers
       `background` when both are present. */
    background-color: var(--bg-overlay);
    color: var(--text);
    accent-color: var(--accent);
    border-radius: 16px;
    padding: 6px 12px;
    font-size: 13px;
}

/* Календарь и попап DatePicker рисуются общей панелью — одна тема на оба.
   Переменные `--cal-*` наследуются, поэтому достаточно объявить их один раз. */
Calendar, DatePicker {
    --cal-panel-bg:       var(--bg-surface);
    --cal-panel-border:   var(--border);
    --cal-muted-color:    var(--text-subtle);
    --cal-outside-color:  var(--text-subtle);
    --cal-weekend-color:  var(--error);
    --cal-today-color:    var(--accent);
    --cal-hover-bg:       var(--bg-overlay);
    --cal-radius:         12px;
    --cal-cell-size:      36px;
}

/* Shared panel-like widgets */
Calendar, ListView, TabView, Dialog,
FloatingWindow, PopupMenu, TableView, TreeView,
PropertyGrid, SegmentedButton {
    background: var(--bg-surface);
    color: var(--text);
    accent-color: var(--accent);
    border-color: var(--border);
}

Carousel {
    background: var(--bg-base);
    accent-color: var(--accent);
    border-color: var(--border);
}

OptionButton {
    background: transparent;
    color: var(--text);
    accent-color: var(--accent);
    border-color: var(--border);
}

TransformBox {
    --tb-border-color: var(--accent);
    --tb-border-width: 1.5;
    --tb-handle-size: 12;
    --tb-handle-color: var(--bg-surface);
    --tb-handle-border-color: var(--accent);
}
