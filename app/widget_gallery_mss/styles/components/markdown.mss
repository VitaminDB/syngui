/* ─────────────────────────────────────────────────────────────────────────────
   Markdown (MarkdownView + MarkdownEditor)
   ───────────────────────────────────────────────────────────────────────────── */

/* === MarkdownView (read-only renderer) === */

.markdown-view {
    /* body-текст */
    color: var(--text);
    font-size: 14px;
    line-height: 1.6;

    /* Headings */
    --md-heading-color: var(--text);
    --md-heading-spacing: 12px;

    /* Links */
    --md-link-color: var(--accent);

    /* Inline code */
    --md-code-bg: var(--bg-elevated);
    --md-code-color: #E11D48;
    --md-code-font-size: 13px;
    --md-code-padding-h: 6px;
    --md-code-radius: 4px;

    /* Code block */
    --md-code-block-bg: #0F172A;
    --md-code-block-color: #E2E8F0;
    --md-code-block-radius: 10px;
    --md-code-block-padding: 16px;

    /* Blockquote */
    --md-quote-bg: var(--bg-elevated);
    --md-quote-text-color: var(--text-muted);
    --md-quote-border-color: var(--accent);
    --md-quote-border-width: 3px;
    --md-quote-padding-left: 16px;
    --md-quote-padding-v: 12px;
    --md-quote-radius: 6px;

    /* Lists */
    --md-list-indent: 24px;
    --md-bullet-color: var(--text-muted);
    --md-checkbox-color: var(--accent);

    /* Tables */
    --md-table-border-color: var(--border);
    --md-table-header-bg: var(--bg-elevated);
    --md-table-header-color: var(--text);
    --md-table-stripe-bg: var(--bg-overlay);

    /* HR */
    --md-hr-color: var(--border);
    --md-hr-thickness: 1px;

    /* Footnotes */
    --md-footnote-color: var(--accent);
    --md-footnote-divider-color: var(--border);

    /* Copy code button */
    --md-copy-bg: rgba(255, 255, 255, 0.06);
    --md-copy-bg-hover: rgba(255, 255, 255, 0.18);
    --md-copy-color: #E2E8F0;
    --md-copy-radius: 6px;
    --md-copy-size: 28px;
    --md-copy-margin: 8px;
    --md-copy-flash-bg: rgba(34, 197, 94, 0.85);
}

/* === MarkdownEditor (Edit / Preview / Split) === */

.markdown-editor {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px;
    transition: border-color 180ms ease, background 180ms ease;
}

.markdown-editor:hover {
    border-color: var(--accent);
}

.markdown-editor .toolbar {
    background: var(--bg-base);
    border-radius: 8px;
    padding: 4px;
    border: 1px solid var(--border);
}

.markdown-editor .toolbar ToolButton {
    transition: background-color 150ms ease-out, color 150ms ease;
}

.markdown-editor .editor-pane {
    background: var(--bg-base);
    border-radius: 8px;
    transition: background 200ms ease;
}

.markdown-editor .preview-pane {
    background: var(--bg-base);
    border-radius: 8px;
    padding: 12px;
}

.markdown-editor .preview-md {
    color: var(--text);
}

.markdown-editor .split-pane {
    background: transparent;
}
