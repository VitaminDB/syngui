/* === Layout boxes === */

.layout-box-blue { background: var(--blue-muted); border-radius: 6px; }
.layout-box-amber { background: var(--amber-muted); border-radius: 6px; }
.layout-box-green { background: var(--green-muted); border-radius: 6px; }
.layout-box-text { color: var(--text); font-size: 12px; }

/* === Stack boxes === */

.stack-blue, .stack-red, .stack-green {
    border-radius: 8px;
}
.stack-blue { background: rgba(59, 130, 246, 0.3); }
.stack-red { background: rgba(239, 68, 68, 0.3); }
.stack-green { background: rgba(16, 185, 129, 0.5); }

/* === Animation boxes === */

.anim-box-blue, .anim-box-red, .anim-box-purple, .anim-box-amber,
.anim-box-pink, .anim-box-green, .anim-box-cyan, .anim-box-orange,
.anim-box-teal, .anim-box-indigo {
    border-radius: 6px;
}
.anim-box-blue { background: var(--accent); }
.anim-box-red { background: var(--error); }
.anim-box-purple { background: var(--purple); }
.anim-box-amber { background: var(--warning); }
.anim-box-pink { background: var(--pink); }
.anim-box-green { background: var(--success); }
.anim-box-cyan { background: var(--info); }
.anim-box-orange { background: var(--orange); }
.anim-box-teal { background: var(--teal); }
.anim-box-indigo { background: var(--indigo); }

/* === Color palette swatches === */

.color-swatch-blue, .color-swatch-red, .color-swatch-green,
.color-swatch-amber, .color-swatch-purple {
    border-radius: 8px;
}
.color-swatch-blue { background: var(--accent); }
.color-swatch-red { background: var(--error); }
.color-swatch-green { background: var(--success); }
.color-swatch-amber { background: var(--warning); }
.color-swatch-purple { background: var(--purple); }

/* === Drag & Drop === */

.drag-item {
    background: var(--blue-muted);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 16px;
    width: 120px;
    height: 40px;
}

.drag-item-text {
    color: var(--accent);
    font-size: 13px;
}
