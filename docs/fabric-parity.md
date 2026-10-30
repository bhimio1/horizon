# Fabric parity

What a Fabric mod gets from Fabric Loader and Fabric API, and what a Horizon mod uses instead. Horizon has two kinds
of mods ([mods.md](mods.md)): content mods (`.hmod`, data and assets, no code) and native mods (`.hnmod`, Rust
bundles the launcher compiles into the game with horizon-mixins, [native-mods.md](native-mods.md)). A native mod
that only uses the hook points names the API it needs (`"hooks": N`) and builds on any later Horizon; one that hooks
other game functions pins a commit.

Status: **yes** (there, with its Horizon name), **partly** (the common case, with the gap said), **not yet**, or
**n/a** (Java machinery Horizon has no need for). The next steps are at the end.

## Fabric Loader

| Loader feature | Status | In Horizon |
|---|---|---|
| Finding mods in the game folder | yes | `.hmod` and `.hnmod` files, turned on and off in the launcher and in Options > Mods |
| `fabric.mod.json` metadata | yes | `horizon-mod.toml` (content), `horizon-mixin.toml` and `horizon-native.json` (native) |
| Entrypoints (`main`, `client`, `server`) | yes | no entry function: hook points instead. `native_world_lifecycle` (`WorldOpenReady`) is the server's start; `native_window` tells the window's `Game` from the server's |
| Environments (`client`, `server`, `@Environment`) | yes | a content mod's `side`; native server events fire only in the `Game`s that simulate the world |
| Mixin | yes | horizon-mixins: HEAD injections into named Rust functions, applied to the source before the build |
| Dependencies (`depends`, `breaks`, version ranges) | partly | a native mod names the Horizon it needs (`horizon` pin or `hooks`); mods can't depend on each other yet |
| Config folder | yes | `native_data_dir`, `native_setting`, `native_set_setting`; per world, `native_world_dir` (API 4) |
| Access wideners | n/a | a bundle's modules are children of `game.rs` and see its private items |
| Mappings and remapping | n/a | the source is Horizon's own: there are no obfuscated names |
| Language adapters | n/a | mods are Rust |
| Jar-in-jar | n/a | a bundle builds against Horizon's locked crates |

## Fabric API

| Module | Status | In Horizon |
|---|---|---|
| Lifecycle events: server start and stop | yes | `native_world_lifecycle` (API 3) |
| Lifecycle events: server tick | yes | `native_tick` |
| Lifecycle events: world tick start and end | yes | `native_level_tick` (API 4) |
| Lifecycle events: chunk load and unload | yes | `native_chunk` (API 4), after the fact for unloads |
| Lifecycle events: world load and unload, block entity load | not yet | |
| Client tick events | yes | `native_tick` and `native_frame` in the window's `Game` |
| Interaction events: `UseBlockCallback` | yes | `native_use_block` (API 4) |
| Interaction events: attack block, use item, use and attack entity, block break | not yet | a watched block's removal shows in `native_block_changed` (API 4) |
| Command API | partly | `native_command` sees a command before the game and `native_reply` answers it (API 4); no Brigadier tree, so no suggestions |
| Key binding API | yes | `native_key`, `native_mapping`, `native_mod_key` |
| Rendering: HUD | yes | `native_hud`, `native_fill`, `native_text`, `native_zoom`, `native_minimap` |
| Rendering: world, block entity and entity renderers, color providers | not yet | |
| Block render layers | partly | a content block draws as its vanilla template does |
| Screen API | partly | the item panel beside container screens (`native_panel_*`, API 2) and settings pages (`native_settings`) |
| Screen handler API (new menus) | not yet | |
| Object builders and registries (blocks, items) | partly | content mods add blocks and items that copy a vanilla template, with their own components; their behaviour comes from native mods |
| Registry sync | not yet | joining a server keeps the vanilla protocol |
| Resource loader | yes | a content mod's `assets/` and `data/`; no reload listeners |
| Resource conditions | not yet | |
| Tags and convention tags (`c:`) | yes | content mods add to vanilla tags and to `data/c/tags/` |
| Recipe API | partly | recipes are data in content mods; `native_recipes` and `native_cook` (API 4) read them; no new recipe types |
| Loot API | partly | loot tables are data in content mods; no events to change vanilla's |
| Item API, item groups | partly | item components in content mods; creative tabs not yet |
| Content registries (fuels, compostables, strippables...) | not yet | |
| Transfer API (item and fluid storage) | not yet | API 4 moves dropped items (`native_items_in`, `native_update_item`, `native_spawn_item`) |
| Data attachments | partly | a mod keeps its own state, saved under `native_world_dir` |
| Networking API | not yet | a native mod on the host acts for every player; the protocol is unchanged |
| Message API (chat events) | not yet | |
| Entity events | not yet | |
| Biome and dimension APIs | not yet | |
| Game rule API | not yet | |
| Sound and particle APIs | partly | a content mod's own sounds; particles not yet |
| Model loading and renderer APIs | partly | vanilla JSON block and item models in content mods |
| Data generation | n/a | content is plain JSON, written or generated by any tool, checked by `cargo horizon-mod check` |
| GameTest API | not yet | |

## Next

Each a small PR that raises `NATIVE_HOOKS_API`:

1. The other interaction events: attack block, use item, use and attack entity, and block break before and after,
   each with a result that can cancel.
2. Block entity data for mod blocks: a per-block compound saved with the chunk, read and written by helpers.
3. Command trees: a mod's commands in the Brigadier tree the server sends, for suggestions.
4. Player join and leave, and world load and unload events.
5. Mod dependencies in `horizon-native.json`, checked by the launcher before a build.
6. Networking: custom payloads between a modded host and modded clients.
