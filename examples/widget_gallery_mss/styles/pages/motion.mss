/* === Перетекания === */

.demo-title { color: var(--text); font-size: 15px; font-weight: 600; }
.demo-code {
    background: var(--bg-overlay);
    border-radius: 8px;
    padding: 8px 12px;
    border: 1px solid var(--border);
}
.demo-code-text { color: var(--text-subtle); font-size: 12px; font-family: "monospace"; }

/* Панель с вкладками: размер перетекает под содержимое */
.motion-panel {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 18px 20px;
    box-shadow: 0 8px 28px var(--shadow-color);
}
.motion-stat-icon { icon-size: 28px; icon-color: var(--accent); }
.motion-stat-value { color: var(--text); font-size: 15px; font-weight: 600; }
.motion-cal-cell {
    width: 30px; height: 26px;
    border-radius: 6px;
    justify-content: center; align-items: center;
}
.motion-cal-head { color: var(--text-muted); font-size: 11px; }
.motion-cal-day { color: var(--text-subtle); font-size: 12px; }
.motion-cal-today { background: var(--accent); }
.motion-cal-today .motion-cal-day { color: #ffffff; font-weight: 600; }
.motion-cover {
    width: 96px; height: 96px;
    border-radius: 48px;
    background: linear-gradient(135deg, var(--purple), var(--pink));
    justify-content: center; align-items: center;
}
.motion-cover-icon { icon-size: 40px; icon-color: #ffffff; }
.motion-progress { width: 260px; }
.motion-ring { accent-color: var(--accent); }
.motion-ring-icon { icon-size: 22px; icon-color: var(--text-subtle); }

/* Панель и карточка, перетекающая в неё */
.motion-bar {
    background: var(--header-bg);
    border-radius: 12px 12px 0px 0px;
    padding: 6px 10px;
    width: 560px;
}
.motion-bar-text { color: var(--header-text); font-size: 13px; }
.motion-bar-btn { color: var(--header-text); }
.motion-popup {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px;
    box-shadow: 0 12px 32px var(--shadow-color);
}
.motion-popup.popup-flow-top {
    border-top-left-radius: 0px; border-top-right-radius: 0px;
    border-top-width: 0px;
    flow-edge: top; flow-radius: 14px; flow-color: var(--header-bg);
}
.motion-menu-row { padding: 6px 8px; border-radius: 8px; transition: background-color 120ms ease-out; }
.motion-menu-row:hover { background: var(--section-hover); }
.motion-menu-icon { icon-size: 20px; icon-color: var(--accent); }
.motion-menu-text { color: var(--text); font-size: 13px; }

/* Уведомления */
.motion-notes-area { min-height: 120px; }
.motion-note {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 10px 12px;
    box-shadow: 0 4px 14px var(--shadow-color);
}
.motion-note-icon { icon-size: 24px; icon-color: var(--accent); }
.motion-note-close { icon-size: 16px; }

/* Размер */
.motion-box-a { background: var(--accent); border-radius: 12px; }
.motion-box-b { background: var(--success); border-radius: 12px; }
.motion-box-c { background: var(--purple); border-radius: 12px; }
.motion-size-mss { transition: size 320ms spring(420, 40); }

/* Кривые */
.motion-curve {
    background: var(--bg-overlay);
    border-radius: 10px;
    color: var(--text-subtle);
    accent-color: var(--accent);
}
