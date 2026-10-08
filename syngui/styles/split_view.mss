/* Разделитель SplitView (`widgets::containers::split_view`): встроенные стили
 * фреймворка (см. `mss::defaults`), приложение переопределяет их своими
 * правилами (`.my-split { border-color: …; }` специфичнее селектора типа).
 *
 * Полоса в покое — тонкая линия темы (`--divider`, иначе `--border`), при
 * наведении и перетаскивании — акцент (`--accent`, иначе `--primary`). Тема
 * без этих переменных — запасные цвета `theme_fallback` (светлая/тёмная). */

SplitView {
    border-color: var(--divider, var(--border));
    accent-color: var(--accent, var(--primary));
}
