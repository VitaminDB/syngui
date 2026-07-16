/* Icon-color states (TreeView/ListView/Tab/Menu/Breadcrumb/TableView).

   Демонстрирует архитектурный фикс: монолитные виджеты, которые рисуют
   glyph-иконки напрямую через push_text, поддерживают per-state цвета
   иконок через MSS — `icon-color`, `icon-color-selected`,
   `icon-color-hover`, `icon-color-disabled`, `icon-opacity`.

   Без явных значений срабатывает fallback-цепочка:
     selected → accent-color → icon-color → color
     hover    → icon-color   → color
     disabled → icon-color   → color (× 0.38 alpha)
     normal   → icon-color   → color
*/

.demo-icon-tree {
    background-color: #FFFFFF;
    color: #475569;             /* base icon/label color */
    accent-color: #6366F1;      /* selected label + selected icon (через fallback) */
    icon-color-hover: #6366F1;  /* hover-иконка подкрашивается accent'ом */
    border-color: #E2E8F0;
    border-radius: 8px;
    border-width: 1px;
}

.demo-icon-tree-vibrant {
    background-color: #0F172A;
    color: #94A3B8;
    accent-color: #38BDF8;
    icon-color: #F472B6;          /* розовая иконка в norm-состоянии */
    icon-color-selected: #FACC15; /* жёлтая на выделении (override accent fallback) */
    icon-color-hover: #FFFFFF;    /* белая под курсором */
    border-color: #1E293B;
    border-radius: 8px;
    border-width: 1px;
}
