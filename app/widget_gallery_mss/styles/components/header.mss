/* === Header === */

TopAppBar {
    background: var(--header-bg);
    elevation: 2px;
    padding-left: 24px;
    padding-right: 16px;
    gap: 12px;

    .title {
        color: var(--header-text);
        font-size: 18px;
        font-weight: 600;
    }
    Badge {
        background: rgba(255, 255, 255, 0.12);
        color: var(--header-text);
        border-color: rgba(255, 255, 255, 0.18);
    }
    Dropdown {
        background: rgba(255, 255, 255, 0.08);
        color: var(--header-text);
        border-color: rgba(255, 255, 255, 0.18);
        accent-color: rgba(255, 255, 255, 0.8);
        height: 32px;
        font-size: 13px;
        --popup-background: var(--bg-surface);
        --popup-color: var(--text);
        --popup-accent: var(--accent);
        --popup-border: var(--border);
    }
}

/* Back-compat: class-based selectors kept for any existing callers. */
.header-title {
    color: var(--header-text);
    font-size: 18px;
    font-weight: 600;
}

.header-badge {
    background: rgba(255, 255, 255, 0.12);
    color: var(--header-text);
    border-color: rgba(255, 255, 255, 0.18);
}

.badge-red { background: var(--error); }
.badge-blue { background: var(--accent); }
.badge-amber { background: var(--warning); }
.badge-green { background: var(--success); }

.header-subtitle {
    color: rgba(255, 255, 255, 0.72);
    font-size: 13px;
}
