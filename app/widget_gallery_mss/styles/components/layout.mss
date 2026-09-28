/* === Layout: рамка и карточка === */

/* Шапка и боковая панель — одна поверхность (--header-bg), содержимое —
 * скруглённая карточка внутри неё. */
.gallery-frame { background: var(--header-bg); }
.gallery-sidebar {
    width: 244px;
    background: var(--header-bg);
}
.nav-page { padding: 8px 0px 12px 8px; }
.nav-list { padding: 0px; }
.nav-group {
    color: var(--header-text);
    opacity: 0.55;
    font-size: 11px;
    font-weight: 600;
    padding: 14px 10px 4px 10px;
}
.nav-item {
    padding: 7px 10px 7px 10px;
    margin-right: 10px;
    border-radius: 10px;
    transition: background-color 160ms ease-out, margin-right 220ms emphasized;
}
.nav-item:hover { background: rgba(255, 255, 255, 0.08); }
/* Активная вкладка вырастает до края панели и перетекает в карточку. */
.nav-item-active {
    background: var(--bg-base);
    margin-right: 0px;
    border-top-right-radius: 0px;
    border-bottom-right-radius: 0px;
    flow-edge: right;
    flow-radius: 12px;
    flow-color: var(--bg-base);
}
.nav-icon { icon-size: 18px; icon-color: var(--header-text); icon-opacity: 0.8; transition: icon-color 160ms ease-out; }
.nav-item-active .nav-icon { icon-color: var(--accent); icon-opacity: 1; }
.nav-text { color: var(--header-text); font-size: 13px; }
.nav-item-active .nav-text { color: var(--text); font-weight: 600; }

.content-frame { padding: 0px 12px 12px 0px; }
.content-card {
    background: var(--bg-base);
    border-radius: 18px;
    overflow: hidden;
}

.content {
    background: var(--bg-base);
    padding: 20px;
}

