/* === Раздел «Продвинутые виджеты» (sections/advanced.rs) ===
 * Только переменные темы: раздел перекрашивается вместе с ней. */

.adv-result {
    font-size: 13px;
    color: var(--text-subtle);
}

.adv-caption {
    font-size: 12px;
    color: var(--text-muted);
}

/* --- MenuBar --- */

.adv-menubar-host {
    padding: 4px 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-base);
}

.adv-menubar-host .menubar-item:hover,
.adv-menubar-host .menubar-item.open {
    background-color: var(--section-hover);
}

.adv-menubar-host .menubar-title {
    color: var(--text);
}

/* --- GraphView --- */

.adv-graph-host {
    height: 380px;
    border-radius: 10px;
    border: 1px solid var(--border);
    overflow: hidden;
}

.adv-graph {
    background-color: var(--bg-base);
    color: var(--text);
    border-color: var(--border);
    accent-color: var(--accent);
    selection-color: var(--bg-surface);
}

/* --- TickSlider --- */

.adv-tick {
    color: var(--accent);
    background-color: var(--border);
    border-color: var(--accent);
    accent-color: var(--bg-surface);
    tick-color: var(--text-muted);
    tick-label-color: var(--text-subtle);
    value-color: var(--text);
}

/* --- SegmentedProgressBar --- */

.adv-segments {
    height: 12px;
    gap: 3px;
    border-radius: 3px;
    accent-color: var(--accent);
    background-color: var(--border);
    color: var(--text-muted);
    outline-color: var(--error);
}

/* --- Dismissible --- */

.adv-note {
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
}

.adv-note-title {
    font-size: 14px;
    color: var(--text);
}

.adv-note-body {
    font-size: 12px;
    color: var(--text-subtle);
}

/* --- VirtualList --- */

.adv-vlist-host {
    height: 372px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg-base);
    padding: 6px;
    overflow: hidden;
}

.adv-row {
    width: 100%;
    padding: 7px 10px;
    border-radius: 6px;
    background: var(--bg-surface);
}

.adv-row-num {
    font-size: 12px;
    font-family: monospace;
    color: var(--accent);
}

.adv-row-title {
    font-size: 13px;
    color: var(--text);
}

.adv-row-sub {
    font-size: 12px;
    color: var(--text-muted);
}

/* --- OnScreenKeyboard (классы .osk-* — встроенные стили рассчитаны на ТВ, здесь — по теме) --- */

.adv-osk-host {
    padding: 10px;
    border-radius: 12px;
    background: var(--bg-base);
    border: 1px solid var(--border);
}

.osk-key {
    border-radius: 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    justify-content: center;
    align-items: center;
    transition: background 120ms ease-out, border-color 120ms ease-out;
}

.osk-key:hover {
    border-color: var(--accent);
}

.osk-key-action {
    background: var(--bg-elevated);
}

.osk-key-on {
    border-color: var(--accent);
}

.osk-key-focused {
    background: var(--accent);
    border-color: var(--accent);
}

.osk-key-text {
    color: var(--text);
    font-size: 15px;
}

.osk-key-text-focused {
    color: var(--button-hover-text);
}

.osk-field {
    width: 100%;
    height: 44px;
    padding: 0px 14px;
    border-radius: 10px;
    background: var(--input-bg);
    border: 1px solid var(--border);
    align-items: center;
}

.osk-field-focused {
    border-color: var(--accent);
}

.osk-field-text {
    color: var(--text);
    font-size: 16px;
}

.osk-field-placeholder {
    color: var(--text-muted);
    font-size: 16px;
}

.osk-field-caret {
    width: 2px;
    height: 22px;
    background: var(--accent);
}

/* --- PanZoomViewport / ImageViewport --- */

.adv-viewport-host {
    height: 340px;
    border-radius: 10px;
    border: 1px solid var(--border);
    overflow: hidden;
}

.adv-panzoom {
    background-color: var(--bg-base);
    color: var(--text-muted);
}

.adv-imageview {
    background-color: var(--bg-base);
    color: var(--text-muted);
}

.adv-sticker {
    width: 170px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    box-shadow: 0 4px 12px var(--shadow-color);
}

.adv-board-pic {
    width: 200px;
    height: 130px;
    border-radius: 10px;
}

/* --- StaticWaveform --- */

.adv-wave {
    accent-color: var(--accent);
    color: var(--text-muted);
    background-color: var(--bg-base);
    border-radius: 8px;
}

/* --- AspectRatio / ScaleBox --- */

.adv-ratio {
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg-base);
}

.adv-ratio-pic {
    border-radius: 10px;
}

.adv-tile {
    width: 56px;
    height: 56px;
    border-radius: 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    justify-content: center;
    align-items: center;
}

.adv-tile-accent {
    background: var(--accent);
    border-color: var(--accent);
}

.adv-tile-text {
    font-size: 20px;
    font-weight: 600;
    color: var(--text);
}

.adv-tile-text-on {
    font-size: 20px;
    font-weight: 600;
    color: var(--button-hover-text);
}

/* --- DockArea --- */

.adv-dock-host {
    height: 340px;
    border-radius: 10px;
    border: 1px solid var(--border);
    overflow: hidden;
}

.adv-dock-host .dock-group {
    background-color: var(--bg-surface);
}

.adv-dock-host .dock-header {
    background-color: var(--bg-base);
    border-color: var(--border);
}

.adv-dock-host .dock-tab:hover {
    background-color: var(--section-hover);
}

.adv-dock-host .dock-tab.active {
    border-bottom-color: var(--accent);
    background-color: var(--bg-surface);
}

.adv-dock-host .dock-tab-title {
    color: var(--text-muted);
}

.adv-dock-host .dock-tab.active .dock-tab-title {
    color: var(--text);
}

.adv-dock-host .dock-tab-close {
    color: var(--text-subtle);
}

.adv-dock-host .dock-drop-hint {
    background-color: var(--blue-muted);
    border-color: var(--accent);
}

.adv-dock-body {
    padding: 10px 12px;
}

.adv-dock-line {
    font-size: 13px;
    font-family: monospace;
    color: var(--text);
}
