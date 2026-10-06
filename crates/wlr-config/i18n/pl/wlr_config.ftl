# wlr-config: ostrzeżenia o konfiguracji i migracja starych plików.

config-shadowed = { $file } jest pomijany: zamiast niego czytany jest { $used }
config-legacy-ignored = { $file } jest pomijany, bo istnieje { $used }: usuń go
config-legacy = { $file } to stary plik konfiguracji, czytany do wlr-utils 2.0: `{ $command }` przenosi go do { $target }
config-unreadable = nie można odczytać { $file }: { $error }
config-invalid = { $file }, wiersz { $line }: { $error }
config-unknown-key = { $file }: nieznany klucz `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" nie jest kolorem (#rgb, #rrggbb lub #rrggbbaa)
config-negative = { $file }: `{ $key }` przyjmuje liczbę, 0 lub więcej
config-bad-theme-name = { $file }: `theme.name` przyjmuje nazwę motywu lub ścieżkę, w cudzysłowie
config-theme-not-found = { $file }: brak motywu `{ $name }` w { $dirs }
config-not-a-section = { $file }: `{ $key }` to sekcja: napisz [{ $key }] w osobnym wierszu, a pod nim jej klucze
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = brak starego pliku konfiguracji do migracji
migrate-exists = { $file } już istnieje: niczego nie zmieniono
migrate-invalid = { $file } nie jest poprawnym TOML, niczego nie zmieniono: { $error }
migrate-section = { $file } zawiera sekcję [{ $key }], której stary format nigdy nie miał: niczego nie zmieniono
migrate-broken-link = { $file } wskazuje na { $target }, który już nie istnieje, a żaden zainstalowany motyw nie ma tej nazwy: niczego nie zmieniono
migrate-write-failed = nie można zapisać { $file }: { $error }
migrate-wrote = Zapisano { $file }
migrate-removed = Usunięto { $file }
migrate-kept = Zachowano { $dir }: zawiera inne pliki
migrate-remove-failed = nie można usunąć { $file }: { $error }
