/* Окно печати (`syngui::print::PrintDialog`). Цвета — из переменных темы
   приложения (synshell: --surface, --fg, --muted, --border, --accent…),
   без них — тёмные запасные. */

.print-dialog {
    width: 980px;
    max-width: 100%;
    height: 680px;
    max-height: 100%;
    border-radius: 14px;
    border-width: 1px;
    border-color: var(--border, #2A303C);
    background-color: var(--surface, #171B23);
    color: var(--fg, #DCE1EA);
    box-shadow: 0px 18px 48px #00000080;
}

.print-dialog.phone {
    width: 100%;
    height: 100%;
    border-radius: 0px;
    border-width: 0px;
}

.print-header {
    padding: 14px 16px 12px 20px;
    border-bottom: 1px solid var(--divider, #FFFFFF14);
}

.print-title {
    font-size: 16px;
    font-weight: 600;
    color: var(--fg, #DCE1EA);
}

.print-subtitle {
    font-size: 12px;
    color: var(--muted, #8C96A8);
}

.print-close {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    align-items: center;
    justify-content: center;
    &:hover { background-color: var(--hover, #FFFFFF14); }
}

.print-close-text {
    font-size: 15px;
    color: var(--muted, #8C96A8);
}

.print-preview-wrap {
    background-color: var(--content, #0F1218);
}

.print-dialog.phone .print-preview-wrap {
    height: 300px;
}

.print-preview {
    padding: 20px;
}

.print-dialog.phone .print-preview {
    padding: 12px;
}

.print-preview-image {
    width: 100%;
    height: 100%;
}

.print-page-nav {
    padding: 12px 0px 0px 0px;
}

.print-dialog.phone .print-page-nav {
    padding: 6px 0px 0px 0px;
}

.print-nav-btn {
    width: 30px;
    height: 30px;
    border-radius: 15px;
    align-items: center;
    justify-content: center;
    background-color: var(--surface-alt, #FFFFFF0F);
    &:hover { background-color: var(--pressed, #FFFFFF22); }
}

.print-nav-btn.disabled {
    opacity: 0.35;
}

.print-nav-glyph {
    font-size: 18px;
    color: var(--fg, #DCE1EA);
}

.print-nav-text {
    font-size: 13px;
    color: var(--muted, #8C96A8);
}

.print-settings-wrap {
    width: 340px;
    border-left: 1px solid var(--divider, #FFFFFF14);
}

.print-dialog.phone .print-settings-wrap {
    width: 100%;
    border-left: 0px solid transparent;
    border-top: 1px solid var(--divider, #FFFFFF14);
}

.print-settings {
    padding: 16px 18px 20px 18px;
}

.print-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted, #8C96A8);
}

.print-section {
    font-size: 13px;
    font-weight: 600;
    color: var(--fg, #DCE1EA);
}

.print-expander {
    padding: 6px 0px;
}

.print-expander-glyph {
    font-size: 12px;
    color: var(--muted, #8C96A8);
}

.print-note {
    font-size: 12px;
    color: var(--muted, #8C96A8);
}

.print-printer-state {
    font-size: 12px;
    color: var(--muted, #8C96A8);
}

.print-printer-state.warn {
    color: var(--warning, #E0A33A);
}

.print-error {
    font-size: 12.5px;
    color: var(--danger, #E5534B);
}

.print-segmented {
    width: 100%;
}

.print-footer {
    padding: 12px 16px 12px 20px;
    border-top: 1px solid var(--divider, #FFFFFF14);
}

.print-summary {
    font-size: 12.5px;
    color: var(--muted, #8C96A8);
}

.print-go {
    min-width: 110px;
}

.print-grow {
    flex-grow: 1;
}

.print-field {
    width: 100%;
}

/* Поля ввода — в цветах темы (у программ synshell — --field/--input-bg). */
.print-dialog Dropdown, .print-dialog SpinBox, .print-dialog TextField {
    background: var(--input-bg, var(--field, #FFFFFF0D));
    color: var(--fg, #DCE1EA);
    accent-color: var(--accent, #4F8CFF);
    border: 1px solid var(--border, #FFFFFF1F);
    border-radius: 6px;
    font-size: 13px;
}

.print-dialog Dropdown, .print-dialog TextField {
    padding: 7px 10px;
}

.print-dialog Dropdown, .print-dialog SpinBox, .print-dialog TextField, .print-dialog SegmentedButton {
    width: 100%;
}

.print-dialog SegmentedButton {
    background: var(--input-bg, var(--field, #FFFFFF0D));
    color: var(--fg, #DCE1EA);
    accent-color: var(--accent, #4F8CFF);
    border-color: var(--border, #FFFFFF1F);
    border-radius: 6px;
    font-size: 13px;
}

.print-dialog Checkbox {
    color: var(--fg, #DCE1EA);
    accent-color: var(--accent, #4F8CFF);
    font-size: 13px;
}

.print-dialog Button {
    border-radius: 6px;
    padding: 7px 16px;
    font-size: 13px;
}

.print-dialog Button.primary {
    background: var(--accent, #4F8CFF);
    color: var(--accent-fg, #FFFFFF);
}
