/* ══ Studio ══════════════════════════════════════════════════════════════
 * Заголовок окна и боковая панель — одна тёмная поверхность (--header-bg),
 * содержимое — скруглённая карточка внутри неё; активная вкладка панели
 * перетекает в карточку (flow-edge). Спокойный рабочий интерфейс.
 */

.window-frame { background: var(--header-bg); border-radius: 12px; border: 1px solid var(--header-line); }
.window-frame:window-maximized, .window-frame:window-fullscreen { border-radius: 0px; border-width: 0px; }

/* ── Заголовок окна ── */
.titlebar { height: 52px; background: var(--header-bg); }
.tb-left { height: 52px; width: 244px; padding: 0px 16px 0px 18px; align-items: center; }
.brand-mark {
    width: 30px; height: 30px; border-radius: 9px;
    background: linear-gradient(135deg, var(--accent), var(--pink));
    align-items: center; justify-content: center;
    box-shadow: 0 4px 14px var(--focus-ring);
}
.brand-icon { icon-size: 18px; icon-color: #ffffff; }
.brand-title { color: var(--header-text); font-size: 14px; font-weight: 700; letter-spacing: 1.5px; }
.brand-sub { color: var(--header-text); opacity: 0.55; font-size: 11px; }
.tb-drag-wrap { height: 52px; }
.tb-drag { height: 52px; flex-grow: 1; min-width: 40px; align-items: center; padding-left: 4px; }
.tb-crumb { color: var(--header-text); opacity: 0.5; font-size: 13px; }
.tb-crumb-sep { color: var(--header-text); opacity: 0.3; font-size: 13px; }
.tb-title { color: var(--header-text); font-size: 13px; font-weight: 600; max-width: 420px; }
.tb-tools { height: 52px; padding: 0px 8px; align-items: center; }
.window-controls { height: 52px; padding: 0px 8px 0px 4px; align-items: center; }
SystemWindowControls { color: var(--header-text); background-color: var(--header-fill); }

.titlebar SegmentedButton {
    background: var(--header-fill);
    color: var(--header-text);
    accent-color: var(--accent);
    --selected-color: var(--on-accent);
    border-color: var(--header-line);
    border-radius: 8px;
    height: 32px;
    font-size: 12px;
}
.tb-lang {
    width: 130px;
    height: 32px;
    padding: 4px 10px;
    font-size: 12px;
    background: var(--header-fill);
    color: var(--header-text);
    border-color: var(--header-line);
    accent-color: var(--header-text);
    --popup-background: var(--bg-surface);
    --popup-color: var(--text);
    --popup-accent: var(--accent);
    --popup-border: var(--border);
}
.tb-btn { color: var(--header-text); &:hover { background: var(--header-fill); color: var(--header-text); } }

/* ── Рамка и карточка ── */
.gallery-frame { background: var(--header-bg); }
.gallery-sidebar {
    width: 244px;
    background: var(--header-bg);
    /* Прокрутка начинается ниже скругления карточки: вкладка у верха
     * перетекает в прямой край карточки, а не в её угол. */
    padding-top: 18px;
    padding-bottom: 30px;
}
.nav-page { padding: 0px 0px 12px 10px; }
.nav-list { padding: 0px; }
.nav-group {
    color: var(--header-text);
    opacity: 0.45;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.8px;
    padding: 14px 10px 4px 10px;
}
/* Пункт — вкладка: скруглён слева, плоский справа у карточки; ушки
 * (flow-edge) красятся фоном пункта. */
.nav-item {
    padding: 8px 10px 8px 10px;
    border-top-left-radius: 10px;
    border-bottom-left-radius: 10px;
    border-top-right-radius: 0px;
    border-bottom-right-radius: 0px;
    flow-edge: right;
    flow-radius: 12px;
    flow-color: #00000000;
    transition: background-color 160ms ease-out;
}
.nav-item:hover { background: var(--header-fill); }
.nav-item-active { background: var(--bg-base); flow-color: var(--bg-base); }
.nav-item-active:hover { background: var(--bg-base); }
.nav-icon { icon-size: 18px; icon-color: var(--header-text); icon-opacity: 0.75; transition: icon-color 160ms ease-out; }
.nav-item-active .nav-icon { icon-color: var(--accent); icon-opacity: 1; }
.nav-text { color: var(--header-text); font-size: 13px; }
.nav-item-active .nav-text { color: var(--text); font-weight: 600; }

.content-frame { padding: 0px 12px 12px 0px; }
.content-card { background: var(--bg-base); border-radius: 18px; overflow: hidden; }
.content { background: var(--bg-base); padding: 24px; }

/* ── Карточки разделов ── */
.section-card {
    background: var(--bg-surface);
    border-radius: 14px;
    padding: 22px;
    border: 1px solid var(--border);
    box-shadow: 0 1px 2px var(--shadow-color), 0 8px 24px var(--shadow-color);
}
.section-title { color: var(--text); font-size: 18px; font-weight: 600; }
