# wlr-config: 설정에 관한 경고와 이전 파일의 이전.

config-shadowed = { $file } 파일은 무시됩니다: 대신 { $used } 파일을 읽습니다
config-legacy-ignored = { $used } 파일이 있으므로 { $file } 파일은 무시됩니다: 삭제하세요
config-legacy = { $file } 파일은 이전 설정 파일로, wlr-utils 2.0까지 읽습니다: `{ $command }` 명령으로 { $target } 파일로 옮길 수 있습니다
config-unreadable = { $file } 파일을 읽을 수 없습니다: { $error }
config-invalid = { $file }, { $line }번째 줄: { $error }
config-unknown-key = { $file }: 알 수 없는 키 `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }"은(는) 색이 아닙니다 (#rgb, #rrggbb 또는 #rrggbbaa)
config-negative = { $file }: `{ $key }`에는 0 이상의 숫자를 적습니다
config-bad-theme-name = { $file }: `theme.name`에는 테마 이름이나 경로를 따옴표로 감싸서 적습니다
config-theme-not-found = { $file }: { $dirs }에 `{ $name }` 테마가 없습니다
config-not-a-section = { $file }: `{ $key }`은(는) 섹션입니다: [{ $key }]를 한 줄에 단독으로 쓰고 그 아래에 키를 쓰세요
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = 이전할 예전 설정 파일이 없습니다
migrate-exists = { $file } 파일이 이미 있습니다: 아무것도 바꾸지 않았습니다
migrate-invalid = { $file } 파일은 올바른 TOML이 아니므로 아무것도 바꾸지 않았습니다: { $error }
migrate-section = { $file } 파일에 예전 형식에는 없던 섹션 [{ $key }]이(가) 있습니다: 아무것도 바꾸지 않았습니다
migrate-broken-link = { $file } 파일이 가리키는 { $target }이(가) 없고, 그 이름의 테마도 설치되어 있지 않습니다: 아무것도 바꾸지 않았습니다
migrate-write-failed = { $file } 파일을 쓸 수 없습니다: { $error }
migrate-wrote = { $file } 파일을 썼습니다
migrate-removed = { $file }을(를) 삭제했습니다
migrate-kept = { $dir }은(는) 남겨 두었습니다: 다른 파일이 있습니다
migrate-remove-failed = { $file } 파일을 삭제할 수 없습니다: { $error }
