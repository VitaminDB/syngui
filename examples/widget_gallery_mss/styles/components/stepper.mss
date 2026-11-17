/* ── Stepper ────────────────────────────────────────────────────── */

Stepper {
    gap: 16px;
    color: var(--text);
    accent-color: var(--accent);
    border-color: var(--border);
    font-size: 13px;
    icon-size: 32px;
    border-width: 2px;
}

/* 01. Pill variant */
Stepper.pill {
    gap: 4px;
    padding: 8px 16px;
    font-size: 13px;
    border-radius: 6px;
}

/* 02. Radio variant */
Stepper.radio {
    icon-size: 20px;
    gap: 24px;
}

/* 03. Numbered variant */
Stepper.numbered {
    icon-size: 36px;
    gap: 20px;
    font-size: 13px;
}

/* 04. Icon variant */
Stepper.icon {
    icon-size: 40px;
    gap: 16px;
    border-width: 3px;
}

/* 05. Status variant */
Stepper.status {
    icon-size: 14px;
    gap: 32px;
    font-size: 13px;
    border-width: 2px;
}
