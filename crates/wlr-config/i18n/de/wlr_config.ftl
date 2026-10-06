# wlr-config: Warnungen zur Konfiguration und die Migration der alten Dateien.

config-shadowed = { $file } wird ignoriert: stattdessen wird { $used } gelesen
config-legacy-ignored = { $file } wird ignoriert, da { $used } existiert: löschen Sie sie
config-legacy = { $file } ist eine alte Konfigurationsdatei, die bis wlr-utils 2.0 gelesen wird: `{ $command }` verschiebt sie nach { $target }
config-unreadable = { $file } kann nicht gelesen werden: { $error }
config-invalid = { $file }, Zeile { $line }: { $error }
config-unknown-key = { $file }: unbekannter Schlüssel `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" ist keine Farbe (#rgb, #rrggbb oder #rrggbbaa)
config-bad-theme-name = { $file }: `theme.name` erwartet einen Theme-Namen oder einen Pfad, in Anführungszeichen
config-theme-not-found = { $file }: kein Theme `{ $name }` in { $dirs }
config-not-a-section = { $file }: `{ $key }` ist ein Abschnitt: schreiben Sie [{ $key }] in eine eigene Zeile, darunter seine Schlüssel
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = keine alte Konfigurationsdatei zu migrieren
migrate-exists = { $file } existiert bereits: nichts wurde geändert
migrate-invalid = { $file } ist kein gültiges TOML, nichts wurde geändert: { $error }
migrate-section = { $file } enthält einen Abschnitt, [{ $key }], den das alte Format nie hatte: nichts wurde geändert
migrate-broken-link = { $file } verweist auf { $target }, das nicht mehr existiert, und kein installiertes Theme trägt diesen Namen: nichts wurde geändert
migrate-write-failed = { $file } kann nicht geschrieben werden: { $error }
migrate-wrote = { $file } geschrieben
migrate-removed = { $file } gelöscht
migrate-kept = { $dir } behalten: es enthält weitere Dateien
migrate-remove-failed = { $file } kann nicht gelöscht werden: { $error }
