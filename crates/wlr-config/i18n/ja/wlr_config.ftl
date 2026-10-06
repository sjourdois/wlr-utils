# wlr-config: 設定に関する警告と、古いファイルの移行。

config-shadowed = { $file } は無視されます: 代わりに { $used } を読み込みます
config-legacy-ignored = { $used } があるため { $file } は無視されます: 削除してください
config-legacy = { $file } は古い設定ファイルで、wlr-utils 2.0 まで読み込まれます: `{ $command }` で { $target } に移せます
config-unreadable = { $file } を読み込めません: { $error }
config-invalid = { $file } の { $line } 行目: { $error }
config-unknown-key = { $file }: 不明なキー `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" は色ではありません (#rgb、#rrggbb、#rrggbbaa)
config-bad-theme-name = { $file }: `theme.name` にはテーマ名かパスを引用符で囲んで指定します
config-theme-not-found = { $file }: { $dirs } にテーマ `{ $name }` がありません
config-not-a-section = { $file }: `{ $key }` はセクションです: [{ $key }] を単独の行に書き、その下にキーを書いてください
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = 移行する古い設定ファイルはありません
migrate-exists = { $file } はすでに存在します: 何も変更していません
migrate-invalid = { $file } は正しい TOML ではありません。何も変更していません: { $error }
migrate-section = { $file } には古い形式にはなかったセクション [{ $key }] があります: 何も変更していません
migrate-broken-link = { $file } のリンク先 { $target } は存在せず、その名前のテーマもインストールされていません: 何も変更していません
migrate-write-failed = { $file } に書き込めません: { $error }
migrate-wrote = { $file } を書き込みました
migrate-removed = { $file } を削除しました
migrate-kept = { $dir } は残しました: 他のファイルがあります
migrate-remove-failed = { $file } を削除できません: { $error }
