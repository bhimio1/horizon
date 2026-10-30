# Fabric parity

What a Fabric mod gets from Fabric Loader and Fabric API, and what a Horizon mod uses instead. Horizon has two kinds
of mods ([mods.md](mods.md)): content mods (`.hmod`, data and assets, no code) and native mods (`.hnmod`, Rust
bundles the launcher compiles into the game with horizon-mixins, [native-mods.md](native-mods.md)). A native mod
that only uses the hook points names the API it needs (`"hooks": 4`) and builds on any later Horizon; one that hooks
other game functions pins a commit.

Status: **yes** (there, with its Horizon name), **partly** (the common case, with the gap said), **not yet**, or
**n/a** (Java machinery Horizon has no need for). The gaps left are at the end.

## Fabric Loader

| Loader feature | Status | In Horizon |
|---|---|---|
| Finding mods in the game folder | yes | `.hmod` and `.hnmod` files, turned on and off in the launcher and in Options > Mods |
| `fabric.mod.json` metadata | yes | `horizon-mod.toml` (content), `horizon-mixin.toml` and `horizon-native.json` (native) |
| Entrypoints (`main`, `client`, `server`) | yes | no entry function: hook points instead. `native_world_lifecycle` (`WorldOpenReady`) is the server's start; `native_window` tells the window's `Game` from the server's |
| Environments (`client`, `server`, `@Environment`) | yes | a content mod's `side`; server hooks fire only in the `Game`s that simulate the world, client hooks in the window |
| Mixin | yes | horizon-mixins: HEAD injections into named Rust functions, applied to the source before the build |
| Dependencies (`depends`, version ranges) | yes | `depends` in `horizon-native.json` (`*`, an exact version, `>=`); the launcher refuses a build with a need unmet. Horizon itself: the `horizon` pin or `hooks` |
| `breaks`, `conflicts`, `recommends`, `suggests` | not yet | |
| Config folder | yes | `native_data_dir`, `native_setting`, `native_set_setting`; per world, `native_world_dir` |
| Access wideners | n/a | a bundle's modules are children of `game.rs` and see its private items |
| Mappings and remapping | n/a | the source is Horizon's own: there are no obfuscated names |
| Language adapters | n/a | mods are Rust |
| Jar-in-jar | n/a | a bundle builds against Horizon's locked crates |

## Fabric API

| Module | Status | In Horizon |
|---|---|---|
| Lifecycle events: server start and stop | yes | `native_world_lifecycle` |
| Lifecycle events: server tick | yes | `native_tick` |
| Lifecycle events: world tick start and end | yes | `native_level_tick` |
| Lifecycle events: world load and unload | yes | `native_level_loaded` |
| Lifecycle events: chunk load and unload | yes | `native_chunk`, after the fact for unloads |
| Lifecycle events: block entity load and unload | not yet | `native_chunk` and `native_watch_block` cover a mod's own blocks |
| Client tick events | yes | `native_tick` and `native_frame` in the window's `Game` |
| Interaction events: use block, use item, use entity | yes | `native_use_block`, `native_use_item`, `native_use_entity` with `NativeUse` |
| Interaction events: attack block, attack entity | yes | `native_attack_block`, `native_attack_entity` |
| Interaction events: block break before and after | yes | `native_before_break` (cancels), `native_after_break`; `CANCELED` is the mod's own cancel |
| Interaction events: pick block | not yet | |
| Connection events: join, disconnect | yes | `native_player_join`, `native_player_leave` (the server's stop included); `native_players` lists them |
| Connection events: init, configuration phase | not yet | |
| Entity events: damage | partly | `native_player_hurt` changes or cancels a player's damage; mobs' damage not yet |
| Entity events: death | yes | `native_player_death`, `native_mob_death` |
| Entity events: sleeping, elytra, world change | not yet | |
| Message API (chat) | yes | `native_allow_chat` (after `native_filter_chat`), `native_chat_message` |
| Message API: game and command messages | not yet | |
| Command API | yes | `native_commands` puts a mod's commands in every player's tree (any text after the name); `native_command` runs them; `native_permission`, `native_acting_player`, `native_reply` |
| Command API: typed arguments and suggestions | not yet | a mod's command takes free text |
| Networking API: play payloads | yes | `native_send_payload` and `native_payload`, both ways, on vanilla's custom payload packets |
| Networking API: configuration phase, channel registration | not yet | a side without the mod drops the payload |
| Data attachments | yes | `native_world_data`, `native_block_data` and their setters, saved with the world; `native_save` |
| Key binding API | yes | `native_key`, `native_mapping`, `native_mod_key` |
| Rendering: HUD | yes | `native_hud`, `native_fill`, `native_text`, `native_zoom`, `native_minimap` |
| Rendering: world, block entity and entity renderers, color providers | not yet | |
| Block render layers | partly | a content block draws as its vanilla template does |
| Item tooltips | yes | `native_tooltip` |
| Item groups (creative tabs) | yes | `native_creative_items` adds to a tab and to Search; new tabs not yet |
| Screen API | partly | the item panel beside container screens (`native_panel_*`) and settings pages (`native_settings`) |
| Screen handler API (new menus) | not yet | |
| Object builders and registries (blocks, items) | partly | content mods add blocks and items that copy a vanilla template, with their own components; their behaviour comes from native mods |
| Registry sync | not yet | joining a server keeps the vanilla protocol |
| Resource loader | yes | a content mod's `assets/` and `data/`; no reload listeners |
| Resource conditions | not yet | |
| Tags and convention tags (`c:`) | yes | content mods add to vanilla tags and to `data/c/tags/` |
| Recipe API | partly | recipes are data in content mods; `native_recipes` and `native_cook` read them; no new recipe types |
| Loot API | partly | block drops: `native_block_drops` (a player's break, explosions); loot tables are data in content mods; entity and chest loot events not yet |
| Item API | partly | item components in content mods |
| Content registries: fuel, composting | yes | `native_set_fuel`, `native_set_compostable` |
| Content registries: flammable, strippable, tillable, villager trades... | not yet | |
| Transfer API (item and fluid storage) | not yet | dropped items: `native_items_in`, `native_update_item`, `native_spawn_item` |
| Biome and dimension APIs | partly | `native_teleport` moves a player between levels; new biomes and dimensions not yet |
| Game rule API | not yet | a mod can answer `/gamerule <mod>:<rule>` with `native_command` and keep it with `native_set_world_data` |
| Sound and particle APIs | partly | a content mod's own sounds; particles not yet |
| Model loading and renderer APIs | partly | vanilla JSON block and item models in content mods |
| Data generation | n/a | content is plain JSON, written or generated by any tool, checked by `cargo horizon-mod check` |
| GameTest API | not yet | |

## Gaps left

The rows marked "not yet". The largest are:
1. **Rendering:** world, entity and block entity renderers. These need the renderer to draw a mod's meshes.
2. **New menus:** screen handlers, so a mod block can open its own container screen.
3. **Mob damage events:** damage reaches mobs from about 25 places, with no single point to hook.
4. **Typed command arguments**, with their suggestions.
5. **Registry sync:** a mod's blocks and items in the protocol, for clients joining a modded server.
