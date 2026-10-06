# wlr-config: предупреждения о конфигурации и перенос старых файлов.

config-shadowed = { $file } не учитывается: вместо него читается { $used }
config-legacy-ignored = { $file } не учитывается, так как существует { $used }: удалите его
config-legacy = { $file } — старый файл конфигурации, он читается до wlr-utils 2.0: `{ $command }` переносит его в { $target }
config-unreadable = не удаётся прочитать { $file }: { $error }
config-invalid = { $file }, строка { $line }: { $error }
config-unknown-key = { $file }: неизвестный ключ `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" — не цвет (#rgb, #rrggbb или #rrggbbaa)
config-bad-theme-name = { $file }: `theme.name` принимает имя темы или путь в кавычках
config-theme-not-found = { $file }: нет темы `{ $name }` в { $dirs }
config-not-a-section = { $file }: `{ $key }` — это раздел: напишите [{ $key }] на отдельной строке, а под ним его ключи
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = нет старого файла конфигурации для переноса
migrate-exists = { $file } уже существует: ничего не изменено
migrate-invalid = { $file } — некорректный TOML, ничего не изменено: { $error }
migrate-section = в { $file } есть раздел [{ $key }], которого в старом формате никогда не было: ничего не изменено
migrate-broken-link = { $file } ссылается на { $target }, которого больше нет, и ни одна установленная тема не носит это имя: ничего не изменено
migrate-write-failed = не удаётся записать { $file }: { $error }
migrate-wrote = Записан { $file }
migrate-removed = Удалён { $file }
migrate-kept = Оставлен { $dir }: в нём есть другие файлы
migrate-remove-failed = не удаётся удалить { $file }: { $error }
