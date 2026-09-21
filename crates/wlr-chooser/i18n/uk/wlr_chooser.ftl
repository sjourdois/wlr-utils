# Український переклад (рядки інтерфейсу) wlr-chooser.

tab-all = Усі
tab-windows = Вікна
tab-outputs = Екрани
filter-hint = Фільтр…
screen-label = Екран { $name }
loading = …
preview-unavailable = Попередній перегляд недоступний
show-system = Системні вікна
error = wlr-chooser: { $error }
capture-no-window = Цей композитор не може захоплювати окремі вікна. Захоплення вікон потребує ext-image-copy-capture-v1 із джерелом foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Запустіть `wlr-peek doctor`, щоб побачити, що підтримує ваш композитор.
focus-unsupported = Цей композитор не може передати фокус вибраному вікну. Для цього потрібен wlr-foreign-toplevel-management-v1 або cosmic-toplevel-management-v1 у COSMIC. Запустіть `wlr-peek doctor`, щоб побачити, що підтримує ваш композитор.
mru-unmatched = Історія фокуса цього композитора не називає жодного з перелічених вікон, тому --window-order mru повертається до впорядкування за назвою.
filter-no-match = Жодне відкрите вікно не відповідає { $filter }. Немає чого показати.
pid-unsupported = Цей композитор не повідомляє, якому процесу належить вікно, тому --pid не можна застосувати. Для цього потрібен IPC sway, Hyprland або niri. Запустіть `wlr-peek doctor`, щоб побачити, чи виявлено бекенд фокуса.
