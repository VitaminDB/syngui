/* === MSS Properties Demo === */

/* Margin */
.mss-margin-none { margin: 0; }
.mss-margin-sm { margin: 8px; }
.mss-margin-md { margin: 16px; }
.mss-margin-lg { margin: 24px; }

/* Border */
.mss-border-thin { border: 1px solid var(--border); }
.mss-border-blue { border-width: 2px; border-color: var(--accent); }
.mss-border-red { border-width: 3px; border-color: var(--error); }
.mss-border-green { border-width: 3px; border-color: var(--success); }

/* Font weight */
.mss-fw-normal { font-weight: 400; }
.mss-fw-bold { font-weight: 700; }
.mss-fw-bold-keyword { font-weight: bold; }

/* Text align */
.mss-align-left { text-align: left; }
.mss-align-center { text-align: center; }
.mss-align-right { text-align: right; }

/* Text vertical align */
.mss-valign-top { text-vertical-align: top; }
.mss-valign-center { text-vertical-align: center; }
.mss-valign-bottom { text-vertical-align: bottom; }

/* Text decoration */
.mss-underline { text-decoration: underline; }
.mss-line-through { text-decoration: line-through; }

/* Opacity */
.mss-opacity-100 { opacity: 1.0; }
.mss-opacity-75 { opacity: 0.75; }
.mss-opacity-50 { opacity: 0.5; }
.mss-opacity-25 { opacity: 0.25; }
.mss-opacity-10 { opacity: 0.1; }

/* Overflow */
.mss-overflow-hidden { overflow: hidden; border: 1px solid var(--border); }

/* Cursor */
.mss-cursor-pointer { cursor: pointer; }
.mss-cursor-text { cursor: text; }
.mss-cursor-move { cursor: move; }
.mss-cursor-crosshair { cursor: crosshair; }

/* Font family */
.mss-font-inter { font-family: "Inter"; }
.mss-font-mono { font-family: monospace; }

/* Transition demos */
.mss-transition-fast {
    background: var(--blue-muted);
    transition: background-color 100ms ease;
    &:hover { background: var(--accent); }
}

.mss-transition-normal {
    background: var(--green-muted);
    transition: background-color 300ms ease-in-out;
    &:hover { background: var(--success); }
}

.mss-transition-slow {
    background: var(--amber-muted);
    transition: background-color 800ms ease-out;
    &:hover { background: var(--warning); }
}

.mss-transition-bounce {
    background: var(--purple-muted);
    transition: background-color 500ms ease-out-bounce;
    &:hover { background: var(--purple); }
}

.mss-transition-opacity {
    background: var(--red-muted);
    opacity: 1.0;
    transition: opacity 400ms ease;
    &:hover { opacity: 0.3; }
}

.mss-transition-border {
    background: var(--bg-overlay);
    border-width: 2px;
    border-color: var(--border);
    transition: border-color 300ms ease;
    &:hover { border-color: var(--accent); }
}
