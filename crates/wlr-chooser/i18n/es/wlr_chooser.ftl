# Traducción al español (cadenas de la interfaz) de wlr-chooser.

tab-all = Todo
tab-windows = Ventanas
tab-outputs = Pantallas
filter-hint = Filtrar…
screen-label = Pantalla { $name }
loading = …
preview-unavailable = Vista previa no disponible
show-system = Ventanas del sistema
error = wlr-chooser: { $error }
capture-no-window = Este compositor no puede capturar ventanas individuales. La captura de ventanas requiere ext-image-copy-capture-v1 con la fuente foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Ejecuta `wlr-peek doctor` para ver qué admite tu compositor.
focus-unsupported = Este compositor no puede enfocar la ventana elegida. Enfocar una ventana requiere wlr-foreign-toplevel-management-v1, o cosmic-toplevel-management-v1 en COSMIC. Ejecuta `wlr-peek doctor` para ver qué admite tu compositor.
mru-unmatched = El historial de foco de este compositor no nombra ninguna de las ventanas listadas, así que --window-order mru vuelve al orden por nombre.
filter-no-match = Ninguna ventana abierta coincide con { $filter }. No hay nada que mostrar.
pid-unsupported = Este compositor no indica el proceso al que pertenece una ventana, así que --pid no puede aplicarse. Necesita la IPC de sway, Hyprland o niri. Ejecuta `wlr-peek doctor` para ver si se detectó un backend de foco.
hints-need-keyboard = --hints necesita una presentación sin campo de filtro: use --layout strip o --layout grid.
grid-needs-card = --grid dimensiona la tarjeta, así que necesita --layout card (el valor por defecto). El exposé y la tira se organizan solos.
daemon-already-running = Ya hay un demonio wlr-switcher en ejecución.
daemon-none = No hay ningún demonio wlr-switcher en ejecución.
daemon-gone = El demonio wlr-switcher se detuvo sin responder; no se cambió a ninguna ventana.
daemon-cannot-serve = Un demonio no puede encargarse de esta ejecución: --no-gpu y --doctor cambian el comportamiento de todo el proceso. Ejecútala con --no-daemon.
