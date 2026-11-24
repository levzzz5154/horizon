# Config files and settings

A mod keeps its options in one of two ways:

| | Config files | Settings |
| --- | --- | --- |
| Lives in | The game's `config/` folder (on a server, `serverconfig/` per world) | `horizon-mods/.horizon-mod-data/<mod id>/`, one file per key |
| Edited by | The player, in a text editor | The player, on the mod's page in **Options > Mods** |
| You declare it with | `Hooks::register_configs` | `Registrar::register_setting` |
| You read it in | `config_loading` and `config_reloading` | `Host::setting` and the typed `setting_*` reads |
| NeoForge counterpart | `ModContainer.registerConfig`, `ModConfigEvent` | `ModConfigSpec` entries on the config screen |

## Config files

Config files sit in the game's shared `config/` folder, beside `options.txt` (beside `server.properties` on a
dedicated server), as Fabric and NeoForge mods' do. Your mod picks its own names: a folder `config/<mod id>/`, or
files `config/<mod id>.<ext>` and `config/<mod id>-<rest>`.

1. In `register_configs`, push each file with its kind (`Client`, `Common` or `Server`) and the default text to
   write when the file is missing. Push all of them every time: each game reads only its own kinds.
2. The game reads each file and calls `config_loading` with the mod id, the file and its text, which your mod
   parses (TOML by convention). Every mod hears every config, so check the mod id.
3. When the file changes on disk, `config_reloading` brings the new text. The game checks modification times once a
   second; a save with the same text, a removed file or an unreadable one fires nothing and keeps the last text.
4. A `Server` config is per world: it loads before `server_about_to_start` from the world's `serverconfig/`, and
   `config_unloading` drops it after `server_stopping`. `Client` and `Common` configs load once and stay until the
   game quits.

These hooks run in the Config [load phase](../concepts/load-phases.md): settings reads and config files are
available, world handles and transactions are not. A file outside `config/`, or one already registered, is
skipped. You register only your own configs: one naming another mod's id is dropped, as is a change to a config
another mod pushed. The game reads and writes your config files as the `ConfigFiles` service below does: a symbolic
link on the path, or a file over 4 MiB, is not read.

For files the hooks don't manage, `Host::config_files()` gives your mod its own files in `config/`. Paths retain
the ownership spelling above, such as `<mod id>/extra.toml`. The service is bound to the mod whose hook or Part
is running; you cannot reach another mod's files.

## Bulk data

Keep map tiles, waypoints, seeds and exports out of settings and config files:

- `Host::data_files()` is available on both sides, rooted at `<game directory>/horizon-mod-data/<mod id>/`.
  A client-only mod can key its own subfolders by server or save and dimension.
- `ServerHost::world_data_files()` is server-only, rooted at `<world>/horizon-mod-data/<mod id>/`.
  Its contents are copied, deleted and backed up with the save. Without a current world it refuses calls with
  `ConfigFileError::NoWorld`; a client has no world-folder service.

Both use the same `ConfigFiles`, `ConfigPath`, `ConfigEntry` and `ConfigFileError` types as config files, on native
and wasm. Bulk paths are relative to your implicit mod root: `tiles/overworld/0.bin`, not
`<mod id>/tiles/overworld/0.bin`. The game never scans bulk data as settings or config.

`read`, `write` and `stat` operate on checked `/`-separated paths; traversal (`..`), backslashes and symbolic links
in mod-chosen paths are refused. An operator may link the game's configured `config/` directory itself;
the service opens that directory once and applies the same checks and quotas inside it.
Writes replace the whole file atomically, never exposing partial contents. Horizon's own per-file
limits are `DATA_FILE_MAX` (64 MiB) for both bulk areas and `CONFIG_FILE_MAX` (4 MiB) for config files.
Oversized files and unavailable operations return typed errors, rather than partially writing.

Walk a folder with `list(&folder, after, limit)`: start with `after = None`, then pass the last returned entry's
name as the exclusive cursor. Names are lexicographically ordered; each page has at most `limit` entries.
`limit` must be between 1 and `CONFIG_LIST_MAX` (1024), or the call returns `ConfigFileError::InvalidPage`.
Use `ConfigPath::root()` to list your own root, and stop when a page is empty.

### Storage quotas

`write` also checks the total storage that belongs to the calling mod. The current limits are:

| Scope | Total file bytes | Entries |
| --- | ---: | ---: |
| Owned files in the game's shared `config/` | 32 MiB | 4,096 |
| `Host::data_files()` in this game directory | 1 GiB | 4,096 |
| `ServerHost::world_data_files()` in each world | 1 GiB | 4,096 |

The config allowance is for text and small state; the larger bulk allowances are for tiles and
exports. The entry limit also bounds filesystem metadata, even for empty files. Quotas are
separate for each mod ID, not pooled between mods or worlds. Each owned file and directory
counts as one entry, including the directory named for the mod and leftover temporary files
hidden by `list`. The shared storage directories themselves do not count. File sizes are
logical byte lengths, not disk blocks; atomic replacement still needs space for a temporary copy.

A write is admitted only if the resulting totals fit both quotas. Replacing a file charges the
difference in size, and creating one also charges any missing parent directories. Over-quota
writes return `ConfigFileError::TooLarge`, leave the old file intact, and create no directories.
Files already on disk count toward the next write, even those saved in an earlier game
session. They are not deleted: `read`, `stat`, and `list` keep their existing behavior, including
the individual file limit for `read`. If the scope is already too full, shrink a file enough
to fit or move unneeded files out of the scope while the game is stopped. Horizon refuses a
write rather than guessing if it cannot check usage. For example, it returns `ConfigFileError::Io`
if the shared `config/` directory has more than 65,536 immediate entries or the write would add
an entry beyond that limit.

These quotas apply to writes through the file services. They do not change registered config
loading (including `serverconfig/`) or the separate settings service.

## Settings

Register each setting from `Hooks::register` with `Registrar::register_setting(SettingSpec { … })`: a key
(`a-z`, `0-9`, `_`, `-` and `.`, not starting with `.`), translation keys for its label and tooltip, and its kind
with the default. A setting the spec check refuses, or a key you registered already, comes back as the error.
Settings are client settings and appear on your mod's page in **Options > Mods**, opened from its **Config**
button.

- Read a setting with `host.setting(mod_id, key)` or a typed read. Typed reads are cached per data folder and key.
- `host.set_setting(mod_id, key, value)` keeps a value. A successful write updates the cache; a failed one leaves
  it. This write does not fire `setting_changed`.
- When the player edits a setting, the value is kept first, then `setting_changed` tells your mod alone.

A mod reads and writes only its own settings: another mod's id, or a key its owner never registered, reads
`None` and writes nothing.

The settings folder is settings only, not bulk storage: ordinary files there are read as setting keys and
subfolders or unsafe entries still warn during snapshots. Use the bulk services above instead; their files are
not included in settings snapshots.
