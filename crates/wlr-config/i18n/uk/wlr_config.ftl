# wlr-config: попередження про конфігурацію та перенесення старих файлів.

config-shadowed = { $file } не враховується: замість нього читається { $used }
config-legacy-ignored = { $file } не враховується, бо існує { $used }: видаліть його
config-legacy = { $file } — старий файл конфігурації, він читається до wlr-utils 2.0: `{ $command }` переносить його до { $target }
config-unreadable = не вдається прочитати { $file }: { $error }
config-invalid = { $file }, рядок { $line }: { $error }
config-unknown-key = { $file }: невідомий ключ `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" — не колір (#rgb, #rrggbb або #rrggbbaa)
config-bad-theme-name = { $file }: `theme.name` приймає назву теми або шлях у лапках
config-theme-not-found = { $file }: немає теми `{ $name }` у { $dirs }
config-not-a-section = { $file }: `{ $key }` — це розділ: напишіть [{ $key }] в окремому рядку, а під ним його ключі
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = немає старого файлу конфігурації для перенесення
migrate-exists = { $file } уже існує: нічого не змінено
migrate-invalid = { $file } — некоректний TOML, нічого не змінено: { $error }
migrate-section = у { $file } є розділ [{ $key }], якого старий формат ніколи не мав: нічого не змінено
migrate-broken-link = { $file } посилається на { $target }, якого більше немає, і жодна встановлена тема не має такої назви: нічого не змінено
migrate-write-failed = не вдається записати { $file }: { $error }
migrate-wrote = Записано { $file }
migrate-removed = Видалено { $file }
migrate-kept = Залишено { $dir }: у ньому є інші файли
migrate-remove-failed = не вдається видалити { $file }: { $error }
