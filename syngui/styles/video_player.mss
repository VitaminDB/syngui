/* Видеоплеер syngui (`widgets::visual::video_view::player`, `VideoPlayerView`):
 * кадр во всю сцену, панель поверх него. Встроенные стили фреймворка (см.
 * `mss::defaults`) — приложение переопределяет их теми же селекторами.
 *
 * Палитра тёмная при любой теме: панель лежит на кадре, под ней чёрный фон
 * или сама картинка — белые иконки и градиент к низу читаются на обоих.
 * Акцент (заливка шкалы, наведение на большую ⏵) — `--primary` темы.
 *
 * Компактный режим (карточки ~360–420 px) — те же классы с суффиксом `-sm`:
 * плеер ставит один класс по режиму, поэтому у `-sm` свои полные правила. */

.vp {
    background-color: #000000;
    border-width: 0;
    padding: 0 0 0 0;
    overflow: hidden;
}

.vp-canvas {
    width: 100%;
    height: 100%;
    background-color: #000000;
}

/* Высокий верхний отступ — зона градиента: панель не обрезает кадр резкой
 * полосой, а темнеет к низу. */
.vp-header {
    padding: 10px 12px 36px 12px;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.8) 0%, rgba(0, 0, 0, 0.45) 55%, rgba(0, 0, 0, 0) 100%);
    border-width: 0;
}

.vp-header-sm {
    padding: 4px 6px 20px 6px;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.8) 0%, rgba(0, 0, 0, 0.45) 55%, rgba(0, 0, 0, 0) 100%);
    border-width: 0;
}

.vp-controls {
    padding: 48px 18px 10px 18px;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0) 0%, rgba(0, 0, 0, 0.55) 45%, rgba(0, 0, 0, 0.85) 100%);
    border-width: 0;
}

/* Slider (см. slider.rs): дорожка — background-color, заливка — color,
 * ползунок — accent-color; толщина дорожки — min-height, размер ползунка —
 * max-height, высота зоны клика — height. */
Slider.vp-seek {
    height: 18px;
    min-height: 4px;
    max-height: 14px;
    color: var(--primary, #4F8CFF);
    background-color: rgba(255, 255, 255, 0.28);
    accent-color: #FFFFFF;
    border-color: #FFFFFF;
    border-width: 0;
    border-radius: 2px;
}

Slider.vp-volume {
    height: 18px;
    min-height: 3px;
    max-height: 12px;
    color: #FFFFFF;
    background-color: rgba(255, 255, 255, 0.28);
    accent-color: #FFFFFF;
    border-color: #FFFFFF;
    border-width: 0;
    border-radius: 2px;
}

/* Три ряда нижней строки лежат друг на друге во всю ширину. */
.vp-row {
    width: 100%;
    height: 48px;
}

.vp-time {
    color: rgba(255, 255, 255, 0.92);
    font-size: 13px;
    font-weight: 500;
    padding-left: 6px;
}

ToolButton.vp-btn {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    background-color: transparent;
    color: #FFFFFF;
    icon-size: 24px;
    transition: background-color 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-btn:hover {
    background-color: rgba(255, 255, 255, 0.16);
    color: #FFFFFF;
}

/* Главная кнопка — белый круг с тёмной иконкой, как в tv_rezka. */
ToolButton.vp-play {
    width: 46px;
    height: 46px;
    border-radius: 50%;
    background-color: #FFFFFF;
    color: #0B0C14;
    icon-size: 30px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    transition: background-color 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0),
                transform 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-play:hover {
    background-color: #FFFFFF;
    color: #0B0C14;
    transform: scale(1.06);
}

/* Большая ⏵ по центру кадра на паузе. */
ToolButton.vp-big-play {
    width: 76px;
    height: 76px;
    border-radius: 50%;
    background-color: rgba(0, 0, 0, 0.5);
    border-width: 2px;
    border-color: rgba(255, 255, 255, 0.28);
    color: #FFFFFF;
    icon-size: 46px;
    transition: background-color 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0),
                transform 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-big-play:hover {
    background-color: var(--primary, #4F8CFF);
    border-color: var(--primary, #4F8CFF);
    color: #FFFFFF;
    transform: scale(1.06);
}

/* Кнопки ±1 с: подпись текстом под прозрачной `vp-btn` (у Material нет
 * иконки «1»). */
.vp-step {
    width: 38px;
    height: 38px;
    justify-content: center;
    align-items: center;
    background-color: transparent;
    border-width: 0;
}

.vp-step-text {
    color: #FFFFFF;
    font-size: 13px;
    font-weight: 700;
}

/* ── Компактный режим ───────────────────────────────────────────────────── */

.vp-controls-sm {
    padding: 28px 8px 4px 8px;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0) 0%, rgba(0, 0, 0, 0.55) 45%, rgba(0, 0, 0, 0.85) 100%);
    border-width: 0;
}

Slider.vp-seek-sm {
    height: 14px;
    min-height: 3px;
    max-height: 11px;
    color: var(--primary, #4F8CFF);
    background-color: rgba(255, 255, 255, 0.28);
    accent-color: #FFFFFF;
    border-color: #FFFFFF;
    border-width: 0;
    border-radius: 2px;
}

.vp-row-sm {
    width: 100%;
    height: 34px;
}

.vp-time-sm {
    color: rgba(255, 255, 255, 0.92);
    font-size: 11px;
    font-weight: 500;
    padding-left: 4px;
}

ToolButton.vp-btn-sm {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background-color: transparent;
    color: #FFFFFF;
    icon-size: 19px;
    transition: background-color 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-btn-sm:hover {
    background-color: rgba(255, 255, 255, 0.16);
    color: #FFFFFF;
}

ToolButton.vp-play-sm {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background-color: #FFFFFF;
    color: #0B0C14;
    icon-size: 23px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.45);
    transition: transform 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-play-sm:hover {
    background-color: #FFFFFF;
    color: #0B0C14;
    transform: scale(1.06);
}

ToolButton.vp-big-play-sm {
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background-color: rgba(0, 0, 0, 0.5);
    border-width: 2px;
    border-color: rgba(255, 255, 255, 0.28);
    color: #FFFFFF;
    icon-size: 32px;
    transition: background-color 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0),
                transform 120ms cubic-bezier(0.2, 0.0, 0.0, 1.0);
}

ToolButton.vp-big-play-sm:hover {
    background-color: var(--primary, #4F8CFF);
    border-color: var(--primary, #4F8CFF);
    color: #FFFFFF;
    transform: scale(1.06);
}

.vp-step-sm {
    width: 28px;
    height: 28px;
    justify-content: center;
    align-items: center;
    background-color: transparent;
    border-width: 0;
}

.vp-step-text-sm {
    color: #FFFFFF;
    font-size: 11px;
    font-weight: 700;
}

/* Превью кадров в рамке (`frames_preview`): компактный плеер 360×202. */
.vp-node-preview {
    width: 360px;
    height: 202px;
    border-radius: 8px;
    overflow: hidden;
    background-color: #000000;
}
