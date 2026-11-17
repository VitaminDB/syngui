/* === FFmpeg Video Player === */

/*
 * Material-style controls bar над VideoView. Базовый layout — Column
 * (canvas сверху, controls bar снизу). На hover bar оставлен видимым:
 * полное auto-hide требует отдельного hover-state на родителе (это можно
 * сделать в следующей итерации через `.ffmpeg-player:hover .ffmpeg-controls`,
 * когда такой селектор будет проверен в MSS-парсере).
 */

.ffmpeg-player {
    background-color: #000000;
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.ffmpeg-canvas {
    background-color: #000000;
    transition: opacity 200ms ease-out;
}

.ffmpeg-canvas:hover {
    opacity: 1.0;
}

.ffmpeg-controls {
    background-color: rgba(20, 20, 22, 0.92);
    padding: 8px 12px 8px 12px;
    color: #ffffff;
    transition: background-color 150ms ease-out;
}

.ffmpeg-controls:hover {
    background-color: rgba(20, 20, 22, 0.98);
}

/* Иконки play/pause/volume — Material Icons font, белые */
.ffmpeg-play {
    color: #ffffff;
    background-color: transparent;
    border-radius: 999px;
    padding: 6px;
    transition: background-color 120ms ease-out;
}

.ffmpeg-play:hover {
    background-color: rgba(255, 255, 255, 0.12);
}

.ffmpeg-mute {
    color: #ffffff;
    background-color: transparent;
    border-radius: 999px;
    padding: 4px;
    transition: background-color 120ms ease-out;
}

.ffmpeg-mute:hover {
    background-color: rgba(255, 255, 255, 0.10);
}

/* Seek-слайдер: занимает всё доступное пространство */
.ffmpeg-seek {
    flex: 1;
    accent-color: #ef4444;
    background-color: rgba(255, 255, 255, 0.18);
    border-radius: 4px;
}

/* Volume slider — компактный */
.ffmpeg-volume {
    accent-color: #ffffff;
    background-color: rgba(255, 255, 255, 0.18);
    border-radius: 4px;
}

/* Time label: tabular monospace для стабильного выравнивания */
.ffmpeg-time {
    color: rgba(255, 255, 255, 0.86);
    font-size: 12px;
    font-family: monospace;
    padding: 0 8px 0 4px;
}

/* Состояние «нет видео» — placeholder в demo-странице */
.ffmpeg-placeholder {
    background-color: var(--bg-surface);
    border: 2px dashed var(--border);
    border-radius: 12px;
    padding: 32px;
    color: var(--text-muted);
}

.ffmpeg-placeholder-icon {
    color: var(--text-muted);
    font-size: 48px;
    padding-bottom: 8px;
}

.ffmpeg-placeholder-text {
    color: var(--text-muted);
    font-size: 14px;
}

/* Сообщение об ошибке открытия */
.ffmpeg-error {
    background-color: rgba(239, 68, 68, 0.10);
    border: 1px solid rgba(239, 68, 68, 0.40);
    color: #b91c1c;
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 13px;
}

/* Form-row для path input + Open button */
.ffmpeg-source-row {
    background-color: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px;
}

.ffmpeg-source-input {
    background-color: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 12px;
    color: var(--text);
    font-size: 13px;
}

.ffmpeg-open-btn {
    background-color: var(--primary);
    color: #ffffff;
    border-radius: 6px;
    padding: 8px 16px;
    font-weight: 600;
    transition: background-color 120ms ease-out;
}

.ffmpeg-open-btn:hover {
    background-color: var(--primary-hover);
}

/* Метаданные блок */
.ffmpeg-meta {
    background-color: var(--bg-surface);
    border-radius: 8px;
    padding: 12px 16px;
    color: var(--text);
    font-size: 12px;
    font-family: monospace;
}

/* Анимация появления плеера после успешного открытия */
@keyframes ffmpeg-fade-in {
    from {
        opacity: 0;
        transform: translateY(8px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}

.ffmpeg-player {
    animation: ffmpeg-fade-in 280ms ease-out;
}
