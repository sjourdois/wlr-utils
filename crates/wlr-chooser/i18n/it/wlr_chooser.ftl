# Traduzione italiana (stringhe dell'interfaccia) di wlr-chooser.

tab-all = Tutto
tab-windows = Finestre
tab-outputs = Schermi
filter-hint = Filtra…
screen-label = Schermo { $name }
loading = …
preview-unavailable = Anteprima non disponibile
show-system = Finestre di sistema
error = wlr-chooser: { $error }
capture-no-window = Questo compositor non può catturare singole finestre. La cattura delle finestre richiede ext-image-copy-capture-v1 con la sorgente foreign-toplevel (wlroots >= 0.20 / Sway >= 1.12). Esegui `wlr-peek doctor` per vedere cosa supporta il tuo compositor.
focus-unsupported = Questo compositor non può dare il focus alla finestra scelta. Dare il focus a una finestra richiede wlr-foreign-toplevel-management-v1, o cosmic-toplevel-management-v1 su COSMIC. Esegui `wlr-peek doctor` per vedere cosa supporta il tuo compositor.
mru-unmatched = La cronologia del focus di questo compositor non nomina nessuna delle finestre elencate, quindi --window-order mru ripiega sull'ordinamento per nome.
filter-no-match = Nessuna finestra aperta corrisponde a { $filter }. Niente da mostrare.
pid-unsupported = Questo compositor non indica il processo a cui appartiene una finestra, quindi --pid non può essere applicato. Richiede l’IPC di sway, Hyprland o niri. Esegui `wlr-peek doctor` per vedere se è stato rilevato un backend di focus.
hints-need-keyboard = --hints richiede una presentazione senza campo di filtro: usa --layout strip o --layout grid.
grid-needs-card = --grid dimensiona la scheda, quindi richiede --layout card (il valore predefinito). L’exposé e la striscia si dispongono da soli.
daemon-already-running = Un demone wlr-switcher è già in esecuzione.
daemon-none = Nessun demone wlr-switcher è in esecuzione.
daemon-gone = Il demone wlr-switcher si è fermato senza rispondere; non è stata attivata alcuna finestra.
daemon-cannot-serve = Un demone non può farsi carico di questa esecuzione: --no-gpu e --doctor cambiano il comportamento dell'intero processo. Eseguila con --no-daemon.
daemon-not-running = Nessun demone wlr-switcher è in esecuzione, quindi questa sovrapposizione paga l'intero avvio (circa 90 ms). Avviane uno dall'avvio automatico della sessione: `wlr-switcher --daemon`.
daemon-bypassed = Questa esecuzione non passa dal demone — --no-gpu cambia il comportamento dell'intero processo — quindi paga l'intero avvio.
