/* Док-панели (`widgets::containers::dock_area`): встроенные стили, цвета —
 * из переменных темы с запасными значениями. Приложение переопределяет их
 * теми же классами. */

.dock-group {
    background-color: var(--bg-panel, #151922);
}

.dock-header {
    height: 34px;
    padding: 0px 4px;
    background-color: var(--bg-panel-2, #1A1F2A);
    border-color: var(--border, #252B37);
    border-bottom-width: 1px;
    align-items: center;
}

.dock-tab {
    height: 34px;
    padding: 0px 12px;
    align-items: center;
    border-color: transparent;
    border-bottom-width: 2px;
}

.dock-tab:hover {
    background-color: var(--surface-hover, #FFFFFF0F);
}

.dock-tab.active {
    border-color: var(--primary, #4F8CFF);
    background-color: var(--bg-panel, #151922);
}

.dock-tab-title {
    font-size: 12px;
    color: var(--text-muted, #8C96A8);
}

.dock-tab.active .dock-tab-title {
    color: var(--text, #DCE1EA);
}

.dock-tab-close {
    font-size: 14px;
    color: var(--text-subtle, #5D6779);
}

.dock-tab-close:hover {
    color: var(--text, #DCE1EA);
}

.dock-body {
    flex-grow: 1;
}

.dock-drop-hint {
    background-color: var(--primary-soft, #4F8CFF26);
    border-color: var(--primary, #4F8CFF);
    border-width: 2px;
    border-radius: 6px;
}
