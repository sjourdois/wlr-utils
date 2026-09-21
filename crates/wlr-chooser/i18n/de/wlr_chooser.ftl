# Deutsche Übersetzung (UI-Strings) für wlr-chooser.

tab-all = Alle
tab-windows = Fenster
tab-outputs = Bildschirme
filter-hint = Filtern…
screen-label = Bildschirm { $name }
loading = …
preview-unavailable = Vorschau nicht verfügbar
show-system = Systemfenster
error = wlr-chooser: { $error }
capture-no-window = Dieser Compositor kann keine einzelnen Fenster aufnehmen. Fensteraufnahme benötigt ext-image-copy-capture-v1 mit der Foreign-Toplevel-Quelle (wlroots >= 0.20 / Sway >= 1.12). Führe `wlr-peek doctor` aus, um zu sehen, was dein Compositor unterstützt.
focus-unsupported = Dieser Compositor kann das gewählte Fenster nicht fokussieren. Das Fokussieren eines Fensters benötigt wlr-foreign-toplevel-management-v1 oder cosmic-toplevel-management-v1 unter COSMIC. Führe `wlr-peek doctor` aus, um zu sehen, was dein Compositor unterstützt.
mru-unmatched = Der Fokusverlauf dieses Compositors benennt keines der aufgeführten Fenster; --window-order mru fällt auf die Sortierung nach Namen zurück.
filter-no-match = Kein offenes Fenster entspricht { $filter }. Nichts anzuzeigen.
pid-unsupported = Dieser Compositor nennt den Prozess hinter einem Fenster nicht, daher kann --pid nicht angewendet werden. Dafür wird die IPC von sway, Hyprland oder niri benötigt. Führe `wlr-peek doctor` aus, um zu sehen, ob ein Fokus-Backend erkannt wurde.
hints-need-keyboard = --hints benötigt eine Darstellung ohne Filterfeld: Verwenden Sie --layout strip oder --layout grid.
grid-needs-card = --grid bemisst die Karte und benötigt daher --layout card (die Voreinstellung). Exposé und Leiste ordnen sich selbst an.
daemon-already-running = Ein wlr-switcher-Daemon läuft bereits.
daemon-none = Es läuft kein wlr-switcher-Daemon.
daemon-gone = Der wlr-switcher-Daemon hat sich ohne Antwort beendet; es wurde nichts umgeschaltet.
daemon-cannot-serve = Ein Daemon kann diesen Lauf nicht übernehmen: --no-gpu und --doctor ändern das Verhalten des gesamten Prozesses. Führe ihn mit --no-daemon aus.
daemon-not-running = Es läuft kein wlr-switcher-Daemon, daher zahlt dieses Overlay den vollen Start (etwa 90 ms). Starte einen aus dem Autostart deiner Sitzung: `wlr-switcher --daemon`.
daemon-bypassed = Dieser Lauf geht nicht über den Daemon — --no-gpu ändert das Verhalten des gesamten Prozesses — und zahlt daher den vollen Start.
