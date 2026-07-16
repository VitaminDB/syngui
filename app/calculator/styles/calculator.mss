/* ── Calculator MSS Theme ─────────────────────────────────────────── */

:root {
    --bg: #1a1a2e;
    --display-bg: #16213e;
    --display-text: #e8e8e8;
    --display-expr: #6a7a9a;
    --btn-number: #2a2a4a;
    --btn-number-hover: #3a3a5a;
    --btn-number-active: #4a4a6a;
    --btn-operator: #e94560;
    --btn-operator-hover: #ff6b81;
    --btn-operator-active: #c73e54;
    --btn-func: #0f3460;
    --btn-func-hover: #1a4a7a;
    --btn-func-active: #0a2a50;
    --btn-equals: #e94560;
    --btn-equals-hover: #ff6b81;
    --btn-text: #ffffff;
    --btn-func-text: #a8b8d8;
}

.calculator-root {
    background: var(--bg);
}

/* ── Display ───────────────────────────────────────────────────── */

.display-container {
    background: var(--display-bg);
    border-radius: 16px;
    padding: 16px;
}

.display-expression {
    color: var(--display-expr);
    font-size: 16px;
    text-align: right;
}

.display-value {
    color: var(--display-text);
    font-size: 44px;
    font-weight: 700;
    text-align: right;
}

/* ── Button styles ──────────────────────────────────────────────── */

Button {
    border-radius: 16px;
    font-size: 22px;
    font-weight: 600;
    padding: 18px;
    border: none;
    cursor: pointer;
    transition: background 150ms ease, transform 100ms ease;
}

Button.btn-number {
    background: var(--btn-number);
    color: var(--btn-text);
}

Button.btn-number:hover {
    background: var(--btn-number-hover);
}

Button.btn-number:pressed {
    background: var(--btn-number-active);
}

Button.btn-operator {
    background: var(--btn-operator);
    color: var(--btn-text);
}

Button.btn-operator:hover {
    background: var(--btn-operator-hover);
}

Button.btn-operator:pressed {
    background: var(--btn-operator-active);
}

Button.btn-func {
    background: var(--btn-func);
    color: var(--btn-func-text);
}

Button.btn-func:hover {
    background: var(--btn-func-hover);
}

Button.btn-func:pressed {
    background: var(--btn-func-active);
}

Button.btn-equals {
    background: var(--btn-equals);
    color: var(--btn-text);
}

Button.btn-equals:hover {
    background: var(--btn-equals-hover);
}

Button.btn-equals:pressed {
    background: var(--btn-operator-active);
}

/* Flex layout helpers */
.grow   { flex-grow: 1; }
.grow-2 { flex-grow: 2; }
.grow-3 { flex-grow: 3; }
