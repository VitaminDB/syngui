/* === Lottie и частицы === */
/* Все цвета — переменные темы: раздел перекрашивается вместе с галереей. */

.lt-block-title { color: var(--text); font-size: 15px; font-weight: 600; }
.lt-card-title { color: var(--text); font-size: 14px; font-weight: 600; }
.lt-card-caption { color: var(--text-muted); font-size: 11px; }
.lt-code {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 12px;
}
.lt-code-text { color: var(--text-subtle); font-size: 12px; font-family: "monospace"; }
.lt-slider { width: 160px; }

/* Карточка витрины */
.lt-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 12px;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: box-shadow 200ms ease-out, border-color 200ms ease-out;
}
.lt-card:hover {
    border-color: var(--accent);
    box-shadow: 0 8px 24px var(--shadow-color);
}
.lt-card-wide { width: 248px; }
.lt-card-grid { width: 222px; }

/* Сцена под анимацией */
.lt-stage {
    min-width: 196px;
    min-height: 156px;
    padding: 14px;
    border-radius: 12px;
    background: linear-gradient(160deg, var(--bg-surface), var(--bg-base));
    justify-content: center;
    align-items: center;
}
.lt-stage-wide { width: 224px; height: 150px; }
.lt-idle-icon { icon-size: 56px; icon-color: var(--accent); }
.lt-like-count { color: var(--pink); font-size: 15px; font-weight: 600; }

/* Сцена частиц */
.lt-pstage {
    overflow: hidden;
    width: 196px;
    height: 140px;
    border-radius: 12px;
    background: linear-gradient(160deg, var(--bg-surface), var(--bg-base));
}
.lt-pstage-ps { width: 280px; height: 150px; }
.lt-card-ps { width: 304px; }
.lt-pstage-center { justify-content: center; align-items: center; }
.lt-stage-text { color: var(--text-subtle); font-size: 12px; }
.lt-glass {
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
    box-shadow: 0 6px 18px var(--shadow-color);
}
.lt-glass-text { color: var(--text); font-size: 12px; font-family: "monospace"; }

/* Пресеты + переопределения из MSS */
.lt-p-magic { particle-preset: magic; particle-rate: 28; particle-colors: "var(--purple) var(--pink) var(--accent)"; }
.lt-p-rain { particle-preset: rain; particle-rate: 70; particle-color: var(--info); }
.lt-p-bubbles { particle-preset: bubbles; particle-rate: 7; particle-color: var(--teal); }
.lt-p-embers {
    particle-preset: embers;
    particle-rate: 30;
    particle-emitter: point 50% 92%;
    particle-colors: "var(--warning) var(--orange) var(--error)";
}
.lt-p-dust { particle-preset: dust; particle-rate: 16; particle-color: var(--text-subtle); }
.lt-p-portal {
    particle-rate: 60;
    particle-emitter: ring;
    particle-shape: glow;
    particle-size: 3px 7px;
    particle-size-end: 0;
    particle-lifetime: 0.6s 1.4s;
    particle-speed: 4 18;
    particle-spread: 360deg;
    particle-glow: 6px;
    particle-twinkle: 0.4;
    particle-colors: "var(--accent) var(--purple)";
}
.lt-p-trail {
    particle-emitter: pointer;
    particle-hover-rate: 90;
    particle-shape: glow;
    particle-size: 4px 9px;
    particle-size-end: 0;
    particle-lifetime: 0.5s 1.1s;
    particle-speed: 4 24;
    particle-spread: 360deg;
    particle-gravity: 0 -16;
    particle-glow: 4px;
    particle-colors: "var(--accent) var(--pink)";
}
.lt-p-back { particle-preset: stars; particle-rate: 14; particle-layer: back; particle-colors: "var(--warning) var(--accent)"; }
.lt-like-fx { particle-preset: hearts; particle-color: var(--pink); particle-burst: 24; }

/* Формы частиц */
.lt-shape-stage {
    overflow: hidden;
    width: 84px;
    height: 96px;
    border-radius: 12px;
    background: linear-gradient(160deg, var(--bg-elevated), var(--bg-base));
    border: 1px solid var(--border);
}
.lt-shape-fx {
    particle-rate: 12;
    particle-emitter: point 50% 82%;
    particle-direction: -90deg;
    particle-spread: 50deg;
    particle-speed: 12 30;
    particle-size: 6px 10px;
    particle-size-end: 0.3;
    particle-lifetime: 1.2s 2s;
    particle-spin: 120deg;
    particle-gravity: 0 -8;
}
.lt-shape-circle { particle-shape: circle; }
.lt-shape-square { particle-shape: square; }
.lt-shape-triangle { particle-shape: triangle; }
.lt-shape-star { particle-shape: star; }
.lt-shape-spark { particle-shape: spark; }
.lt-shape-ring { particle-shape: ring; }
.lt-shape-glow { particle-shape: glow; }
.lt-shape-heart { particle-shape: heart; }
.lt-shape-mixed { particle-shape: mixed; }
.lt-tint-0 { particle-color: var(--accent); }
.lt-tint-1 { particle-color: var(--purple); }
.lt-tint-2 { particle-color: var(--pink); }
.lt-tint-3 { particle-color: var(--teal); }
.lt-tint-4 { particle-color: var(--orange); }
