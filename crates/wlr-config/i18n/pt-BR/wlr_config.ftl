# wlr-config: avisos sobre a configuração e migração dos arquivos antigos.

config-shadowed = { $file } é ignorado: { $used } é lido no lugar dele
config-legacy-ignored = { $file } é ignorado porque { $used } existe: apague-o
config-legacy = { $file } é um arquivo de configuração antigo, lido até o wlr-utils 2.0: `{ $command }` o move para { $target }
config-unreadable = não foi possível ler { $file }: { $error }
config-invalid = { $file }, linha { $line }: { $error }
config-unknown-key = { $file }: chave desconhecida `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" não é uma cor (#rgb, #rrggbb ou #rrggbbaa)
config-negative = { $file }: `{ $key }` espera um número, 0 ou mais
config-bad-theme-name = { $file }: `theme.name` espera um nome de tema ou um caminho, entre aspas
config-theme-not-found = { $file }: nenhum tema `{ $name }` em { $dirs }
config-not-a-section = { $file }: `{ $key }` é uma seção: escreva [{ $key }] sozinho em uma linha e depois as chaves dela
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = não há arquivo de configuração antigo para migrar
migrate-exists = { $file } já existe: nada foi alterado
migrate-invalid = { $file } não é um TOML válido, nada foi alterado: { $error }
migrate-section = { $file } tem uma seção, [{ $key }], que o formato antigo nunca teve: nada foi alterado
migrate-broken-link = { $file } aponta para { $target }, que não existe mais, e nenhum tema instalado tem esse nome: nada foi alterado
migrate-write-failed = não foi possível gravar { $file }: { $error }
migrate-wrote = { $file } gravado
migrate-removed = { $file } apagado
migrate-kept = { $dir } mantido: contém outros arquivos
migrate-remove-failed = não foi possível apagar { $file }: { $error }
