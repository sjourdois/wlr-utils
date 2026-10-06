# wlr-config: avvisi sulla configurazione e migrazione dei vecchi file.

config-shadowed = { $file } viene ignorato: al suo posto si legge { $used }
config-legacy-ignored = { $file } viene ignorato perché esiste { $used }: eliminalo
config-legacy = { $file } è un vecchio file di configurazione, letto fino a wlr-utils 2.0: `{ $command }` lo sposta in { $target }
config-unreadable = impossibile leggere { $file }: { $error }
config-invalid = { $file }, riga { $line }: { $error }
config-unknown-key = { $file }: chiave sconosciuta `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" non è un colore (#rgb, #rrggbb o #rrggbbaa)
config-negative = { $file }: `{ $key }` richiede un numero, 0 o più
config-bad-theme-name = { $file }: `theme.name` richiede il nome di un tema o un percorso, tra virgolette
config-theme-not-found = { $file }: nessun tema `{ $name }` in { $dirs }
config-not-a-section = { $file }: `{ $key }` è una sezione: scrivi [{ $key }] da solo su una riga, poi le sue chiavi
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = nessun vecchio file di configurazione da migrare
migrate-exists = { $file } esiste già: non è stato modificato nulla
migrate-invalid = { $file } non è TOML valido, non è stato modificato nulla: { $error }
migrate-section = { $file } contiene una sezione, [{ $key }], che il vecchio formato non ha mai avuto: non è stato modificato nulla
migrate-broken-link = { $file } punta a { $target }, che non esiste più, e nessun tema installato ha quel nome: non è stato modificato nulla
migrate-write-failed = impossibile scrivere { $file }: { $error }
migrate-wrote = Scritto { $file }
migrate-removed = Eliminato { $file }
migrate-kept = Conservato { $dir }: contiene altri file
migrate-remove-failed = impossibile eliminare { $file }: { $error }
