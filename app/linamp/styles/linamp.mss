/* LinAmp — Winamp-style dark theme */

:root {
    --bg: #232323;
    --bg-darker: #1a1a1a;
    --bg-lighter: #2d2d2d;
    --accent: #00C850;
    --accent-dim: #009030;
    --text: #E0E0E0;
    --text-dim: #808080;
    --text-bright: #FFFFFF;
    --border: #3a3a3a;
    --highlight: #3a3a3a;
}

/* Common */
Column {
    background-color: var(--bg);
}

Row {
    background-color: transparent;
}

Text {
    color: var(--text);
    font-size: 12;
}

Text.title {
    color: var(--accent);
    font-size: 11;
    font-weight: bold;
}

Text.bright {
    color: var(--text-bright);
}

Text.dim {
    color: var(--text-dim);
    font-size: 10;
}

Text.display {
    color: var(--accent);
    font-size: 14;
    font-weight: bold;
}

Text.time {
    color: var(--accent);
    font-size: 18;
    font-weight: bold;
}

/* Buttons */
Button {
    background-color: var(--bg-lighter);
    color: var(--text);
    border-radius: 3;
    padding-left: 8;
    padding-right: 8;
    padding-top: 4;
    padding-bottom: 4;
    font-size: 11;
    border-width: 1;
    border-color: var(--border);
    height: 24;
}

Button:hover {
    background-color: #404040;
}

Button:active {
    background-color: var(--accent-dim);
    color: var(--text-bright);
}

Button.transport {
    width: 32;
    height: 28;
    font-size: 16;
    padding-left: 0;
    padding-right: 0;
    border-radius: 4;
}

Button.transport:hover {
    background-color: #404040;
    border-color: var(--accent-dim);
}

Button.transport.active {
    background-color: var(--accent-dim);
    color: var(--accent);
}

Button.preset {
    height: 22;
    font-size: 10;
    padding-left: 6;
    padding-right: 6;
    padding-top: 2;
    padding-bottom: 2;
}

Button.preset.active {
    background-color: var(--accent-dim);
    color: var(--accent);
    border-color: var(--accent);
}

/* Slider */
Slider {
    height: 6;
}

Slider.progress {
    height: 4;
}

Slider.volume {
    width: 60;
    height: 4;
}

Slider.eq-band {
    width: 20;
    height: 60;
}

/* Divider */
DecoratedBox.divider {
    background-color: var(--border);
    height: 1;
}

/* Header bar */
DecoratedBox.header {
    background-color: var(--bg-darker);
    padding-left: 8;
    padding-right: 8;
    padding-top: 4;
    padding-bottom: 4;
}

/* Panel area */
DecoratedBox.panel {
    background-color: var(--bg);
    padding-left: 6;
    padding-right: 6;
    padding-top: 4;
    padding-bottom: 4;
}

/* Track list */
DecoratedBox.track {
    padding-left: 8;
    padding-right: 8;
    padding-top: 3;
    padding-bottom: 3;
    background-color: transparent;
}

DecoratedBox.track:hover {
    background-color: var(--highlight);
}

DecoratedBox.track.current {
    background-color: var(--accent-dim);
}

/* Toggle */
Toggle {
    width: 32;
    height: 16;
}

/* Flex layout helpers */
.grow   { flex-grow: 1; }
.grow-2 { flex-grow: 2; }
.grow-3 { flex-grow: 3; }
