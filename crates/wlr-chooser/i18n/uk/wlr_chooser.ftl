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
hints-need-keyboard = --hints потребує режиму без поля фільтра: використовуйте --layout strip або --layout grid.
grid-needs-card = --grid задає розмір картки, тож потребує --layout card (типово). Експозе та смуга розташовуються самі.
daemon-already-running = Демон wlr-overlayd уже запущено.
daemon-none = Демон wlr-overlayd не запущено.
daemon-gone = Демон wlr-overlayd завершився, не відповівши.
daemon-not-running = Демон wlr-overlayd не запущено, тож цей оверлей оплачує повний запуск (близько 90 мс). Запустіть його з автозапуску сеансу: `wlr-overlayd`.
daemon-busy = Демон wlr-overlayd уже показує оверлей, тож цей показано тут і він оплачує повний запуск.
daemon-bypassed = Цей запуск не йде через демон — --no-gpu змінює поведінку всього процесу — тож він оплачує повний запуск.
daemon-cannot-serve = Демон не може обслужити цей запуск: --no-gpu і --doctor змінюють поведінку всього процесу. Запустіть його з --no-daemon.
