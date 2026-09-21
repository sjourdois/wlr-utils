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
