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
daemon-already-running = Ya hay un demonio wlr-overlayd en ejecución.
daemon-none = No hay ningún demonio wlr-overlayd en ejecución.
daemon-gone = El demonio wlr-overlayd se detuvo sin responder.
daemon-not-running = No hay ningún demonio wlr-overlayd en ejecución, así que esta superposición paga todo el arranque (unos 90 ms). Inicia `wlr-overlayd` con tu sesión para que sea instantánea, o pasa --no-daemon si eso es lo que quieres y este aviso desaparecerá. Consulta `wlr-overlayd --help` o el README de wlr-chooser.
daemon-busy = El demonio wlr-overlayd ya está mostrando una superposición, así que esta se muestra aquí y paga todo el arranque.
daemon-bypassed = Esta ejecución no pasa por el demonio — --no-gpu cambia el comportamiento de todo el proceso — así que paga todo el arranque.
daemon-cannot-serve = Un demonio no puede encargarse de esta ejecución: --no-gpu y --doctor cambian el comportamiento de todo el proceso. Ejecútala con --no-daemon.
hold-no-modifier = El modo mantener para cambiar está activo: el selector cambia en cuanto no se mantiene ninguna tecla Alt o Super — al instante y sin mostrar nada si no se mantiene ninguna al abrirse. Pasa --no-hold para que la superposición siga abierta.
