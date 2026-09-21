# English (fallback) UI strings for the wlr-chooser tools.

tab-all = All
tab-windows = Windows
tab-outputs = Screens
filter-hint = Filter…
screen-label = Screen { $name }
loading = …
preview-unavailable = Preview unavailable
show-system = System windows
error = wlr-chooser: { $error }
capture-no-window = This compositor cannot capture individual windows. Window capture needs ext-image-copy-capture-v1 with the foreign-toplevel source (wlroots >= 0.20 / Sway >= 1.12). Run `wlr-peek doctor` to see what your compositor supports.
focus-unsupported = This compositor cannot focus the window you picked. Focusing a window needs wlr-foreign-toplevel-management-v1, or cosmic-toplevel-management-v1 on COSMIC. Run `wlr-peek doctor` to see what your compositor supports.
mru-unmatched = This compositor's focus history names none of the windows listed, so --window-order mru falls back to ordering by name.
filter-no-match = No open window matches { $filter }. Nothing to show.
pid-unsupported = This compositor does not report the process behind a window, so --pid cannot be applied. It needs the IPC of sway, Hyprland or niri. Run `wlr-peek doctor` to see whether a focus backend was detected.
