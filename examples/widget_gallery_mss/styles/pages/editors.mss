/* === Раздел «Редакторы» (sections/editors.rs) ===
 * Только переменные темы: раздел перекрашивается вместе с ней. */

/* --- CodeEditor --- */

.ed-code-host {
    height: 420px;
    border-radius: 10px;
    border: 1px solid var(--border);
    overflow: hidden;
}

.ed-code {
    font-size: 13px;
    background-color: var(--input-bg);
    color: var(--text);
    gutter-color: var(--bg-base);
    caret-color: var(--accent);
    accent-color: var(--accent);

    editor-bg: var(--input-bg);
    editor-fg: var(--text);
    editor-gutter-bg: var(--bg-base);
    editor-gutter-fg: var(--text-muted);
    editor-cursor: var(--accent);
    editor-selection: var(--blue-muted);
    editor-current-line: var(--section-hover);
    editor-popup-bg: var(--bg-elevated);
    editor-popup-border: var(--border);
    editor-error: var(--error);
    editor-warning: var(--warning);
    editor-info: var(--info);

    token-keyword: var(--purple);
    token-type: var(--teal);
    token-function: var(--accent);
    token-function-macro: var(--pink);
    token-string: var(--success);
    token-number: var(--orange);
    token-constant: var(--orange);
    token-comment: var(--text-muted);
    token-attribute: var(--warning);
    token-namespace: var(--indigo);
    token-property: var(--info);
    token-operator: var(--text-subtle);
    token-punctuation: var(--text-subtle);
    token-variable: var(--text);
}

.ed-status {
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--bg-base);
}

.ed-status-text {
    font-size: 12px;
    color: var(--text-subtle);
}

.ed-result {
    font-size: 13px;
    color: var(--text-subtle);
}

/* --- CommandPalette: встроенные стили берут --bg-panel/--primary, у галереи свои имена --- */

.palette {
    background-color: var(--bg-elevated);
    border-color: var(--border);
    box-shadow: 0px 12px 32px var(--fx-shadow-lg);
}

.palette-item:hover {
    background-color: var(--section-hover);
}

.palette-item.selected {
    background-color: var(--blue-muted);
}

.palette-item-icon-wrap {
    background-color: var(--bg-base);
}

.palette-item.selected .palette-item-icon-wrap {
    background-color: var(--bg-surface);
}

.palette-item-icon {
    color: var(--text-muted);
}

.palette-item.selected .palette-item-icon {
    color: var(--accent);
}

.palette-item-title.match {
    color: var(--accent);
}

.palette-title,
.palette-item-title {
    color: var(--text);
}

.palette-item-desc,
.palette-item-category,
.palette-empty {
    color: var(--text-muted);
}

.palette-item-shortcut,
.palette-group {
    color: var(--text-subtle);
}

/* --- DocumentEditor: переменные --doc-* --- */

.ed-doc-host {
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--input-bg);
    padding: 8px 4px;
}

.gallery-doc {
    --doc-text-color: var(--text);
    --doc-muted-color: var(--text-muted);
    --doc-heading-color: var(--text);
    --doc-link-color: var(--accent);
    --doc-link-missing-color: var(--error);
    --doc-caret-color: var(--accent);
    --doc-selection-color: var(--blue-muted);
    --doc-menu-bg: var(--bg-elevated);
    --doc-menu-border: var(--border);
    --doc-menu-sel-bg: var(--blue-muted);
    --doc-code-color: var(--pink);
    --doc-code-bg: var(--bg-base);
    --doc-code-block-bg: var(--bg-base);
    --doc-code-block-color: var(--text);
    --doc-quote-border-color: var(--accent);
    --doc-bullet-color: var(--text-muted);
    --doc-number-color: var(--text-muted);
    --doc-checkbox-color: var(--text-muted);
    --doc-checkbox-check-color: var(--accent);
    --doc-toggle-chevron-color: var(--text-muted);
    --doc-divider-color: var(--border);
    --doc-grid-color: var(--border);
    --doc-block-hover-color: var(--section-hover);
    --doc-block-selected-color: var(--blue-muted);
    --doc-media-bg: var(--bg-base);
    --doc-shape-stroke-color: var(--accent);
    --doc-shape-handle-color: var(--accent);
    --doc-embed-border-color: var(--border);
    --doc-embed-bg: var(--bg-base);
    --doc-table-border-color: var(--border);
    --doc-table-header-bg: var(--bg-base);
    --doc-text-size: 14px;
}
