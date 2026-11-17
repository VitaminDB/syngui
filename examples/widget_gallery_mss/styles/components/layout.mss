/* === Layout: рамка и карточка === */

/* Шапка и боковая панель — одна поверхность (--header-bg), содержимое —
 * скруглённая карточка внутри неё. */
.gallery-frame { background: var(--header-bg); }
.gallery-sidebar {
    width: 244px;
    background: var(--header-bg);
    /* Область прокрутки начинается ниже скругления карточки: вкладка,
     * доехавшая до верха, перетекает в прямой край карточки, а не в её угол. */
    padding-top: 18px;
    /* Снизу так же: нижний угол карточки (18 px) плюс отступ рамки (12 px). */
    padding-bottom: 30px;
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
/* Пункт навигации в форме вкладки: скруглён слева, плоский справа у
 * карточки; ушки у карточки красятся текущим фоном пункта — при наведении
 * это полупрозрачный «призрак» вкладки, у активного — цвет карточки. */
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
/* Наведение — призрак вкладки: та же форма, плоский край у карточки,
 * без ушек (ушки соседних пунктов спорили бы с ушками активной). */
.nav-item:hover { background: rgba(255, 255, 255, 0.09); }
/* Активная вкладка перетекает в карточку. */
.nav-item-active { background: var(--bg-base); flow-color: var(--bg-base); }
/* Наведение на активную не превращает её в призрак: `.nav-item:hover`
 * специфичнее `.nav-item-active`, и вкладка серела при ушках цвета карточки. */
.nav-item-active:hover { background: var(--bg-base); }
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

