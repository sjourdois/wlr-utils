# wlr-config: avisos sobre la configuración y migración de los archivos antiguos.

config-shadowed = { $file } se ignora: se lee { $used } en su lugar
config-legacy-ignored = { $file } se ignora porque existe { $used }: bórrelo
config-legacy = { $file } es un archivo de configuración antiguo, que se lee hasta wlr-utils 2.0: `{ $command }` lo traslada a { $target }
config-unreadable = no se puede leer { $file }: { $error }
config-invalid = { $file }, línea { $line }: { $error }
config-unknown-key = { $file }: clave desconocida `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" no es un color (#rgb, #rrggbb o #rrggbbaa)
config-negative = { $file }: `{ $key }` espera un número, 0 o más
config-bad-theme-name = { $file }: `theme.name` espera un nombre de tema o una ruta, entre comillas
config-theme-not-found = { $file }: ningún tema `{ $name }` en { $dirs }
config-not-a-section = { $file }: `{ $key }` es una sección: escriba [{ $key }] solo en una línea y después sus claves
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = no hay ningún archivo de configuración antiguo que migrar
migrate-exists = { $file } ya existe: no se ha cambiado nada
migrate-invalid = { $file } no es TOML válido, no se ha cambiado nada: { $error }
migrate-section = { $file } tiene una sección, [{ $key }], algo que el formato antiguo nunca tuvo: no se ha cambiado nada
migrate-broken-link = { $file } apunta a { $target }, que ya no existe, y ningún tema instalado tiene ese nombre: no se ha cambiado nada
migrate-write-failed = no se puede escribir { $file }: { $error }
migrate-wrote = { $file } escrito
migrate-removed = { $file } borrado
migrate-kept = { $dir } se conserva: contiene otros archivos
migrate-remove-failed = no se puede borrar { $file }: { $error }
