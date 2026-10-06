# wlr-config: warnings about the configuration, and the migration from the old files.

config-shadowed = { $file } is ignored: { $used } is read instead
config-legacy-ignored = { $file } is ignored since { $used } exists: delete it
config-legacy = { $file } is an old configuration file, read until wlr-utils 2.0: `{ $command }` moves it to { $target }
config-unreadable = cannot read { $file }: { $error }
config-invalid = { $file }, line { $line }: { $error }
config-unknown-key = { $file }: unknown key `{ $key }`
config-bad-value = { $file }: { $error }
config-bad-colour = { $file }: `{ $key }`: "{ $value }" is not a colour (#rgb, #rrggbb or #rrggbbaa)
config-bad-theme-name = { $file }: `theme.name` takes a theme name or a path, in quotes
config-theme-not-found = { $file }: no theme `{ $name }` in { $dirs }
config-not-a-section = { $file }: `{ $key }` is a section: write [{ $key }] on a line of its own, then its keys
config-key-warning = { $file }: `{ $key }`: { $message }
config-file-warning = { $file }: { $message }

migrate-nothing = there is no old configuration file to migrate
migrate-exists = { $file } already exists: nothing was changed
migrate-invalid = { $file } is not valid TOML, nothing was changed: { $error }
migrate-section = { $file } has a section, [{ $key }], which the old format never had: nothing was changed
migrate-broken-link = { $file } links to { $target }, which is gone, and no installed theme has that name: nothing was changed
migrate-write-failed = cannot write { $file }: { $error }
migrate-wrote = Wrote { $file }
migrate-removed = Removed { $file }
migrate-kept = Kept { $dir }: it holds other files
migrate-remove-failed = cannot delete { $file }: { $error }
