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
scratchpad-unsupported = This compositor has no scratchpad to switch among, so --scratchpad cannot be applied. It needs sway's IPC ($SWAYSOCK). Run `wlr-peek doctor` to see whether a focus backend was detected.
scratchpad-not-put-aside = The compositor would not put the focused window back into the scratchpad.
hints-need-keyboard = --hints needs a layout that has no filter field: use --layout strip or --layout grid.
grid-needs-card = --grid sizes the card, so it needs --layout card (the default). The exposé and the strip lay themselves out.
daemon-already-running = A wlr-overlayd daemon is already running.
daemon-none = No wlr-overlayd daemon is running.
daemon-gone = The wlr-overlayd daemon stopped without answering.
daemon-not-running = No wlr-overlayd daemon is running, so this overlay pays the full startup (about 90 ms). Start `wlr-overlayd` with your session to make it instant, or pass --no-daemon if that is what you want and this notice will stop. `wlr-overlayd --help`, or the wlr-chooser README.
daemon-busy = The wlr-overlayd daemon is already showing an overlay, so this one is shown here and pays the full startup.
daemon-bypassed = This run does not go through the daemon — --no-gpu changes what the whole process does — so it pays the full startup.
daemon-cannot-serve = A daemon cannot take this run on: --no-gpu and --doctor change what the whole process does. Run it with --no-daemon.
hold-no-modifier = Hold-to-switch is on: the switcher switches the moment no Alt or Super key is held — at once, without showing anything, if none is held when it opens. Pass --no-hold to keep the overlay open.
