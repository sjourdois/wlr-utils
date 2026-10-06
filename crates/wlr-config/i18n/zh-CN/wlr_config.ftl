# wlr-config：关于配置的警告，以及旧文件的迁移。

config-shadowed = 忽略 { $file }：改为读取 { $used }
config-legacy-ignored = 由于 { $used } 已存在，忽略 { $file }：请删除它
config-legacy = { $file } 是旧的配置文件，在 wlr-utils 2.0 之前仍会读取：`{ $command }` 会把它移到 { $target }
config-unreadable = 无法读取 { $file }：{ $error }
config-invalid = { $file }，第 { $line } 行：{ $error }
config-unknown-key = { $file }：未知的键 `{ $key }`
config-bad-value = { $file }：{ $error }
config-bad-colour = { $file }：`{ $key }`："{ $value }" 不是颜色（#rgb、#rrggbb 或 #rrggbbaa）
config-bad-theme-name = { $file }：`theme.name` 需要用引号括起的主题名或路径
config-theme-not-found = { $file }：在 { $dirs } 中找不到主题 `{ $name }`
config-not-a-section = { $file }：`{ $key }` 是一个节：请把 [{ $key }] 单独写一行，再在下面写它的键
config-key-warning = { $file }：`{ $key }`：{ $message }
config-file-warning = { $file }：{ $message }

migrate-nothing = 没有需要迁移的旧配置文件
migrate-exists = { $file } 已存在：未做任何更改
migrate-invalid = { $file } 不是有效的 TOML，未做任何更改：{ $error }
migrate-section = { $file } 含有旧格式从未有过的节 [{ $key }]：未做任何更改
migrate-broken-link = { $file } 指向的 { $target } 已不存在，也没有已安装的同名主题：未做任何更改
migrate-write-failed = 无法写入 { $file }：{ $error }
migrate-wrote = 已写入 { $file }
migrate-removed = 已删除 { $file }
migrate-kept = 保留 { $dir }：其中还有其他文件
migrate-remove-failed = 无法删除 { $file }：{ $error }
