# wlr-config : avertissements sur la configuration, et migration des anciens fichiers.

config-shadowed = { $file } est ignoré : { $used } est lu à sa place
config-legacy-ignored = { $file } est ignoré puisque { $used } existe : supprimez-le
config-legacy = { $file } est un ancien fichier de configuration, lu jusqu'à wlr-utils 2.0 : `{ $command }` le déplace dans { $target }
config-unreadable = impossible de lire { $file } : { $error }
config-invalid = { $file }, ligne { $line } : { $error }
config-unknown-key = { $file } : clé inconnue `{ $key }`
config-bad-value = { $file } : { $error }
config-bad-colour = { $file } : `{ $key }` : "{ $value }" n'est pas une couleur (#rgb, #rrggbb ou #rrggbbaa)
config-bad-theme-name = { $file } : `theme.name` attend un nom de thème ou un chemin, entre guillemets
config-theme-not-found = { $file } : aucun thème `{ $name }` dans { $dirs }
config-not-a-section = { $file } : `{ $key }` est une section : écrivez [{ $key }] seul sur une ligne, puis ses clés
config-key-warning = { $file } : `{ $key }` : { $message }
config-file-warning = { $file } : { $message }

migrate-nothing = aucun ancien fichier de configuration à migrer
migrate-exists = { $file } existe déjà : rien n'a été modifié
migrate-invalid = { $file } n'est pas un TOML valide, rien n'a été modifié : { $error }
migrate-section = { $file } contient une section, [{ $key }], ce que l'ancien format n'a jamais eu : rien n'a été modifié
migrate-broken-link = { $file } pointe vers { $target }, qui n'existe plus, et aucun thème installé ne porte ce nom : rien n'a été modifié
migrate-write-failed = impossible d'écrire { $file } : { $error }
migrate-wrote = { $file } écrit
migrate-removed = { $file } supprimé
migrate-kept = { $dir } conservé : il contient d'autres fichiers
migrate-remove-failed = impossible de supprimer { $file } : { $error }
