# Русский перевод (строки интерфейса) wlr-chooser.

tab-all = Все
tab-windows = Окна
tab-outputs = Экраны
filter-hint = Фильтр…
screen-label = Экран { $name }
loading = …
preview-unavailable = Предпросмотр недоступен
show-system = Системные окна
error = wlr-chooser: { $error }
capture-no-window = Этот композитор не может захватывать отдельные окна. Захват окон требует ext-image-copy-capture-v1 с источником foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Запустите `wlr-peek doctor`, чтобы узнать, что поддерживает ваш композитор.
focus-unsupported = Этот композитор не может передать фокус выбранному окну. Для этого нужен wlr-foreign-toplevel-management-v1 или cosmic-toplevel-management-v1 в COSMIC. Запустите `wlr-peek doctor`, чтобы узнать, что поддерживает ваш композитор.
mru-unmatched = История фокуса этого композитора не называет ни одно из перечисленных окон, поэтому --window-order mru возвращается к сортировке по имени.
filter-no-match = Ни одно открытое окно не соответствует { $filter }. Показывать нечего.
pid-unsupported = Этот композитор не сообщает, какому процессу принадлежит окно, поэтому --pid неприменим. Для этого нужен IPC sway, Hyprland или niri. Запустите `wlr-peek doctor`, чтобы узнать, обнаружен ли бэкенд фокуса.
hints-need-keyboard = --hints требует режима без поля фильтра: используйте --layout strip или --layout grid.
grid-needs-card = --grid задаёт размер карточки, поэтому требует --layout card (по умолчанию). Экспозе и полоса располагаются сами.
daemon-already-running = Демон wlr-overlayd уже запущен.
daemon-none = Демон wlr-overlayd не запущен.
daemon-gone = Демон wlr-overlayd завершился, не ответив.
daemon-not-running = Демон wlr-overlayd не запущен, поэтому этот оверлей оплачивает полный запуск (около 90 мс). Запускайте `wlr-overlayd` вместе с сеансом, чтобы он появлялся мгновенно, или передайте --no-daemon, если так и задумано — тогда это сообщение исчезнет. См. `wlr-overlayd --help` или README wlr-chooser.
daemon-busy = Демон wlr-overlayd уже показывает оверлей, поэтому этот показан здесь и оплачивает полный запуск.
daemon-bypassed = Этот запуск не идёт через демон — --no-gpu меняет поведение всего процесса — поэтому он оплачивает полный запуск.
daemon-cannot-serve = Демон не может обслужить этот запуск: --no-gpu и --doctor меняют поведение всего процесса. Запустите его с --no-daemon.
