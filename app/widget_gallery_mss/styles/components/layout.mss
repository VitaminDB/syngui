/* === Layout === */

.gallery-sidebar {
    width: 244px;
    background: var(--sidebar-bg);
    border-right: 1px solid var(--border);
}
.nav-list { padding: 10px 8px; }
.nav-group {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    padding: 12px 10px 4px 10px;
}
.nav-item {
    padding: 7px 10px;
    border-radius: 10px;
    transition: background-color 140ms ease-out, color 140ms ease-out;
}
.nav-item:hover { background: var(--section-hover); }
.nav-item-active { background: var(--accent); }
.nav-icon { icon-size: 18px; icon-color: var(--text-subtle); transition: icon-color 140ms ease-out; }
.nav-item-active .nav-icon { icon-color: #ffffff; }
.nav-text { color: var(--text); font-size: 13px; }
.nav-item-active .nav-text { color: #ffffff; font-weight: 600; }

.content {
    background: var(--bg-base);
    padding: 20px;
}
/* Смена раздела: старая страница уезжает вверх и тает, новая въезжает снизу. */
.content-switch { background: var(--bg-base); }
