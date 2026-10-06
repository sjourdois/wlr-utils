# wlr-config: waarschuwingen over de configuratie, en de migratie van de oude bestanden.

config-shadowed = { $file } wordt genegeerd: in plaats daarvan wordt { $used } gelezen
config-legacy-ignored = { $file } wordt genegeerd omdat { $used } bestaat: verwijder het
config-legacy = { $file } is een oud configuratiebestand, gelezen tot wlr-utils 2.0: `{ $command }` verplaatst het naar { $target }
config-unreadable = kan { $file } niet lezen: { $error }
config-invalid = { $file }, regel { $line }: { $error }
config-unknown-key = { $file }: onbekende sleutel `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" is geen kleur (#rgb, #rrggbb of #rrggbbaa)
config-negative = { $file }: `{ $key }` verwacht een getal, 0 of meer
config-bad-theme-name = { $file }: `theme.name` verwacht een themanaam of een pad, tussen aanhalingstekens
config-theme-not-found = { $file }: geen thema `{ $name }` in { $dirs }
config-not-a-section = { $file }: `{ $key }` is een sectie: schrijf [{ $key }] op een eigen regel, met daaronder de sleutels
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = er is geen oud configuratiebestand om te migreren
migrate-exists = { $file } bestaat al: er is niets gewijzigd
migrate-invalid = { $file } is geen geldige TOML, er is niets gewijzigd: { $error }
migrate-section = { $file } bevat een sectie, [{ $key }], die het oude formaat nooit had: er is niets gewijzigd
migrate-broken-link = { $file } verwijst naar { $target }, dat niet meer bestaat, en geen geïnstalleerd thema heeft die naam: er is niets gewijzigd
migrate-write-failed = kan { $file } niet schrijven: { $error }
migrate-wrote = { $file } geschreven
migrate-removed = { $file } verwijderd
migrate-kept = { $dir } behouden: het bevat andere bestanden
migrate-remove-failed = kan { $file } niet verwijderen: { $error }
