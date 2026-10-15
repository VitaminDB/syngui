/* Палитра команд (`widgets::overlay::command_palette`). */

.palette {
    padding: 8px;
    border-radius: 12px;
    border-width: 1px;
    border-color: var(--border, #252B37);
    background-color: var(--bg-panel, #151922);
    box-shadow: 0px 12px 32px #00000066;
}

.palette-field {
    width: 100%;
    font-size: 14px;
    padding: 9px 12px;
}

.palette-list {
    padding: 2px 0px;
}

.palette-item {
    height: 30px;
    padding: 0px 10px;
    border-radius: 6px;
    align-items: center;
}

.palette-item:hover {
    background-color: var(--surface-hover, #FFFFFF0F);
}

.palette-item.selected {
    background-color: var(--primary-soft, #4F8CFF26);
}

.palette-item-title-wrap {
    flex-grow: 1;
}

.palette-item-title {
    font-size: 13px;
    color: var(--text, #DCE1EA);
}

.palette-item-category {
    font-size: 13px;
    color: var(--text-muted, #8C96A8);
}

.palette-item-shortcut {
    font-size: 11px;
    color: var(--text-subtle, #5D6779);
}

.palette-empty {
    padding: 10px;
    font-size: 13px;
    color: var(--text-muted, #8C96A8);
}

/* Строка меню (`widgets::navigation::menu_bar`). */

.menubar-item {
    height: 28px;
    padding: 0px 10px;
    border-radius: 6px;
    align-items: center;
}

.menubar-item:hover,
.menubar-item.open {
    background-color: var(--surface-hover, #FFFFFF0F);
}

.menubar-title {
    font-size: 13px;
    color: var(--text, #DCE1EA);
}

.palette-item-title.match {
    color: var(--primary, #4F8CFF);
    font-weight: 600;
}
