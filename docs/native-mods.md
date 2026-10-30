# Native mods (experimental)

Native mods are **mixin bundles** that change the game's own Rust source before it is compiled, with
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) (MIT, by levzzz5154, made for Horizon). A bundle
is a `horizon-mixin.toml` and a few `.rs` helpers that hook into named functions of the game (HEAD, BEFORE_CALL,
RETURN, MODIFY_ARG, OVERWRITE...; the tool's README lists them). A port such as Veinminer is three HEAD hooks and
one module. Start a mod from [horizon-native-mod-template](https://github.com/horizon-rust/horizon-native-mod-template).

Native mods are native code with the player's rights, not sandboxed. The launcher turns them on only when the player
installs one (after a notice) and builds them only behind a red warning; the registry lists one only after a review; and
they play in a game of their own, built for the instance they are installed in.

## Pinned to a Nightly

A native mod builds on one exact Horizon commit: `[compatibility] horizon_revision` in its `horizon-mixin.toml`,
and `horizon` in the `horizon-native.json` of its registry file (`.hnmod`, `horizon_mods::index::native`). Mods
pin a **Nightly's commit** (or a release's). Every Nightly ships its source and keeps its commit for good as the
tag `nightly-source/<date>-<short>` ([release-flow.md](release-flow.md#nightly-channel)), so a pinned mod still
builds after its Nightly is replaced. All the mods of one build must pin the same commit; when they don't, the
launcher says which mod pins which, and the player installs versions made for the same Nightly. A bundle with no
pin builds on the commit of the installed version the build starts from.

### Mods on the hook points

A mod that hooks only Horizon's native hook points (`crates/horizon-client/src/game/native_hooks.rs`: empty
functions the game calls at fixed places, plus helpers, kept stable across releases) can name the version it
needs instead of pinning: `"hooks": 1` in its `.hnmod`'s `horizon-native.json`, the `NATIVE_HOOKS_API` of that
file when it was made. Hook points are only ever added: each addition raises `NATIVE_HOOKS_API`, and a mod builds
on any Horizon whose version is the one it needs or a later one. Its `horizon` is then only the commit it was tested on. The launcher builds it on the commit the other mods
pin, else on the installed version's commit (the game the player plays), after checking that commit's
`NATIVE_HOOKS_API` (read from GitHub) is at least the mod's; when it isn't and nothing pins, it builds on the commit the
mod was tested on. In its own copy of the mod it sets `horizon_revision` to the commit it builds on. Horizon's
own native mods (Zoom, FPS counter, Keystrokes, Minimap; horizon-rust/horizon-mods, `native/<name>/`) are made this way, so any of them build together
with no re-release for each Nightly. The `mods` feature (WebAssembly mods) is passed to the build only when the source has it.

API 2 adds the item panel beside container screens and its recipe pages (`native_panel_items`,
`native_panel_recipes`, and the hook `native_panel_event` for the player's clicks, searches and keys over items), and
the game's items and recipes (`native_recipes`, `native_known_recipes`, `native_place_recipe`, `native_give`): what a
recipe viewer needs, in builds with or without the `mods` feature. A WebAssembly mod's panel shows only while no native
mod shows one.

API 3 adds authoritative world lifecycle events through `native_world_lifecycle`: open ready, open failed,
and final close. Bundles that use these events need `"hooks": 3`.

API 4 adds what Fabric API gives a Fabric mod: server events, interaction events, networking, command trees, data
kept with the world, and the content registries a mod adds to. Bundles that use any of it need `"hooks": 4`. Server
hooks fire in the `Game`s that simulate the world (single player's server, a LAN host, a dedicated server), never in a
client of a server; client hooks (`native_tooltip`, `native_creative_items`, `native_payload` from the server) fire in
the window. How each part of Fabric maps to these: [fabric-parity.md](fabric-parity.md).

| Hook | When | Fabric API |
|---|---|---|
| `native_level_tick(end, running)` | before and after each level's tick; `running` is false under `/tick freeze` | `ServerTickEvents.START/END_WORLD_TICK` |
| `native_level_loaded(level, loaded)` | a level opens or closes | `ServerWorldEvents.LOAD/UNLOAD` |
| `native_chunk(chunk, loaded)` | a chunk becomes resident or leaves, within half a second | `ServerChunkEvents.CHUNK_LOAD/UNLOAD` |
| `native_block_changed(pos, old, new)` | a write to or from a state of a block a mod watches (`native_watch_block`) | none (Fabric mods mixin into `setBlock`) |
| `native_save()` | each save, the autosave included | none (`PersistentState`) |
| `native_use_block(pos, face, main_hand, result)` | a player uses a block, before it or the item reacts | `UseBlockCallback` |
| `native_use_item(main_hand, result)` | a player uses the held item on no block | `UseItemCallback` |
| `native_use_entity(entity, main_hand, result)` | a player uses an entity | `UseEntityCallback` |
| `native_attack_block(pos, face, result)` | a player starts hitting a block | `AttackBlockCallback` |
| `native_attack_entity(entity, cancel)` | a player attacks an entity | `AttackEntityCallback` |
| `native_before_break(pos, state, cancel)`, `native_after_break(pos, state)` | a player breaks a block | `PlayerBlockBreakEvents.BEFORE/AFTER` |
| `native_block_drops(pos, state, drops)` | a block broken by a player or an explosion drops | `LootTableEvents.MODIFY` (block drops) |
| `native_player_join`, `native_player_leave` | a player joins, leaves, or the server stops | `ServerPlayConnectionEvents.JOIN/DISCONNECT` |
| `native_player_hurt(damage_type, amount)` | the player acting is about to take damage; 0 cancels | `ServerLivingEntityEvents.ALLOW_DAMAGE` |
| `native_player_death`, `native_mob_death` | a player or a mob died | `ServerLivingEntityEvents.AFTER_DEATH` |
| `native_allow_chat(conn, message, allow)` | a player's chat, before anyone gets it (after `native_filter_chat(true)`) | `ServerMessageEvents.ALLOW_CHAT_MESSAGE` |
| `native_chat_message(conn, message)` | a player's chat went out | `ServerMessageEvents.CHAT_MESSAGE` |
| `native_commands(commands)` | each server tick: the mod's command names, for every player's command tree (they run through `native_command`) | `CommandRegistrationCallback` |
| `native_payload(from, channel, data)` | a custom payload arrived, on the server or the client | networking API receivers |
| `native_tooltip(stack, lines)` | an item's tooltip, after its own lines | `ItemTooltipCallback` |
| `native_creative_items(tab, items)` | a creative tab opens the first time | `ItemGroupEvents.modifyEntriesEvent` |

`NativeUse` answers the use and attack hooks as `InteractionResult` does: `Pass`, `Success` (the arm swings), `Consume`,
`Fail`; any but `Pass` ends the click.

The helpers (`crates/horizon-client/src/game/native_server.rs`):
- **The level:** `native_level`, `native_world_dir`, `native_loaded`, `native_block`, `native_set_block`,
  `native_break_block`, `native_powered`, `native_watch_block`, and the watched blocks of a chunk that came
  (`native_watched_blocks`, which skips sections whose palette holds none).
- **Block states:** by name and property (`native_state_name`, `native_state_property`, `native_default_state`,
  `native_state_with`), all of them (`native_state_count`), their facts (`native_state_facts`: air, hardness,
  collision, light, fluid) and tags (`native_block_tagged`).
- **Entities and items:** `native_entity_type`, `native_entity_position`, dropped items (`native_items_in`,
  `native_update_item`, `native_spawn_item`), `native_max_stack`, cooking recipes (`native_cook`).
- **Players:** `native_players`, `native_acting_player`, `native_held`, `native_sneaking`, `native_look`, `native_permission`,
  `native_teleport`, and a command's reply (`native_reply`).
- **Data kept with the world** (Fabric's data attachments, in the world folder's `horizon-native-data.json`):
  `native_world_data`, `native_set_world_data`, and per block `native_block_data`, `native_set_block_data`, which
  last while that block stays.
- **Networking:** `native_send_payload(to, channel, data)`, to a player from the server or to the server from a
  client. A Horizon server and client drop payloads on channels no mod reads.
- **Chat:** `native_filter_chat` holds players' chat for `native_allow_chat`.
- **Content registries**, for every game of the process: `native_set_fuel` (`FuelRegistry`), `native_set_compostable`
  (`CompostingChanceRegistry`).

A native mod can name the other native mods it needs in its `horizon-native.json`: `"depends": {"other_mod": ">=1.2.0"}`
(`*` for any version, an exact version, or `>=` one). The launcher refuses to build a set of mods with a need
missing or a version it doesn't take.

## What the launcher does

In the window: install a native mod from the Mods page. The first one shows a notice once (the first build downloads
Rust and can take 20 to 30 minutes; later builds are faster) and turns on Settings → Game → **Native mods
(experimental)**, which stays off for players who never install one; `mods install` does the same. Then **Build**. The launcher names the mods, warns in red, and builds only from "I trust these mods: build"; the
row shows each step, the error if one fails, and the build the instance plays. From the command line:
`horizon-launcher native-build [FOLDER] [--instance NAME] --trust-native-code`. FOLDER is a bundle, an unpacked
`.hnmod`, or a folder of them; without it, the native mods turned on in the instance. It is not an MCP tool:
trusting native code is the player's decision, never an agent's.

1. **Rust.** The launcher's own toolchain ([`rust_toolchain`](../launcher/src/rust_toolchain.rs)): Rust 1.99.1's
   official `rustc`, `cargo` and `rust-std` (and `rust-mingw` on Windows), each pinned by SHA-256, downloaded once
   from static.rust-lang.org into `<launcher data>/rust/`, with its own Cargo home `<launcher data>/cargo/`. No
   rustup, nothing on the system; the player's own Rust is never used. Windows uses the `windows-gnu` host, which
   links without Visual Studio. `rust-mingw` only links, so on Windows the toolchain also brings MinGW-w64
   (`rust_toolchain::MINGW`: WinLibs' GCC 16.2.0 on MinGW-w64 14.0.0, MSVCRT like `rust-mingw`, one zip pinned by
   SHA-256, its `mingw64/` without the documentation) on the build's `PATH`: `ring`, `hidapi` and wasmtime compile C,
   and `windows-sys`' `raw-dylib` imports need `dlltool` and the assembler it runs. The linker is the system's on
   macOS (the Command Line Tools: the launcher asks for `xcode-select --install`) and Linux (`cc`).
   `horizon-launcher rust status|install|remove`.
2. **horizon-mixins**, at the commit the mods name (their `.hnmod`'s `mixins`), else `native_mods::MIXINS_REV`,
   built once with that cargo (`cargo install --locked --git … --rev …`) into `native/tools/<commit>/`.
3. **The source** of the pinned commit: GitHub's archive of it, once per commit (`native/source/<commit>/`, about
   63 MB unpacked), with the commit in `.horizon-revision`. Never the player's own checkout. Links in the
   archive are left out: the tool refuses them and nothing needs them to build.
4. **Mixins.** The bundles are copied first, so the build and its key use that copy whatever happens to the
   folder meanwhile. `horizon-mixins apply` stages the modded source; the launcher copies only the files that
   changed into `native/stage/`, so Cargo, which goes by file times, recompiles only the crates the mods touch.
5. **Compile**: `cargo build --locked --profile release --features horizon-client/mods -p horizon-client`, the
   release's own command (game-build.yml), into `native/target/`. Cargo downloads the locked crates from
   crates.io (Horizon has no git dependencies). The log is `native/build.log`; a failure shows its last lines (a
   horizon-mixins diagnostic in one line).
6. **Install**: a new version, `native-<commit>-<key>`, a copy of the starting version's folder (resources
   included, cloned on APFS, Btrfs and ReFS) with the built game in place of its own, and a `native-mods.json`
   saying which commit, mods, horizon-mixins and Rust made it. The instance the mods are installed in plays it (a
   folder built from the command line, or mods of the default home, go to the **Native mods** instance). That
   instance gets each mod's settings from the default home the first time it plays it (its
   `horizon-mods/.horizon-mod-data/<id>/` folder and `key.<id>.` lines of `horizon-mods.txt`, where the mod's
   WebAssembly version kept them); nothing it already has is overwritten.

## Cache and disk

The key hashes the commit, the horizon-mixins commit, Rust's version, the profile and features, and every file of
every bundle. The same set on the same commit is never built twice. A change gives a new build, and
`native/target/` stays, so a rebuild compiles only `horizon-client` and what the mods touched.

Measured 2026-10-28 on the Mac mini (M4 Pro, 14 cores) with Veinminer 0.3.0 on Nightly `83f15fa`,
`CARGO_BUILD_JOBS=8` while other agents' builds ran (upper bounds):

| What | Size, time |
|---|---|
| Rust 1.99.1 (`<launcher data>/rust/`) | 453 MB; 58 s to download and unpack |
| horizon-mixins (`native/tools/`) | 7 MB; built once in 19 s |
| Crates in the launcher's Cargo home (`<launcher data>/cargo/`) | 289 MB |
| Source of one commit, and the stage | 64 MB each |
| First build: release profile, the whole game | 9 min 10 s of compiling, 9 min 41 s in all; `native/target/` 1.5 GB |
| Rebuild after a helper's code changed | 2 min 46 s of compiling (only `horizon-client`), 2 min 53 s in all |
| A change horizon-mixins ignores (a comment) | nothing compiled; 31 s, mostly the version folder's copy |

Measured 2026-10-29 on Windows 11 (Ryzen 7 9800X3D, 16 threads), the same mod and Nightly, an empty `native/target/`
(horizon-mixins, the source and the crates were already downloaded):

| What | Size, time |
|---|---|
| Rust 1.99.1 and MinGW-w64 (`<launcher data>/rust/`) | 1.7 GB, MinGW-w64 875 MB of it |
| Crates in the launcher's Cargo home | 657 MB |
| First build: release profile, the whole game | 6 min 57 s of compiling, 8 min 3 s in all; `native/target/` 2.0 GB |

Measured 2026-10-28 on Linux (an x86_64 server with no Rust of its own), Zoom on Horizon `1149c92`, 3 jobs while CI
ran: `rust install` in 11 s (592 MB), the first build in 12 min 9 s. The built game ran Zoom's proof (under `xvfb-run`:
the game needs a display even with `--offscreen`).

Only the native builds an instance plays are kept (`native_mods::prune`); `native/target/` stays for the next one.

## Trust

- **Off until the player installs one.** Settings → Game → Native mods stays off for a player who never installs a
  native mod; the first install shows the notice and turns it on. The command needs `--trust-native-code`; the window
  shows a warning in red before every build: the mods run with full access to the computer, and their ids and
  versions. The Mods page says who made each one, and "Not made or endorsed by the Horizon project" for any mod not
  Horizon's own.
- **The registry** (horizon-rust/horizon-mods) lists them as their own kind, `native-mod`, only after a maintainer
  has read the source: the `.hnmod` contains `.rs` and `.toml` files and may contain data files under
  `mixins/<id>/data/`. The launcher checks the bundle without running anything. Native code is trusted.
  The file is pinned by SHA-256 like every registry file. An update is reviewed again; `revoked.toml` takes one down.
  A WebAssembly mod's entry may turn `native-mod` once (never back): its `.hmod` versions stay published and
  are checked as `.hmod`s, the index lists only its `.hnmod` versions, and the launcher's update keeps the
  player's `.hmod` (the game runs it until a native build has the mod, then skips it); removing the mod removes both.
- **Isolation of data.** Native mods run only in the instance built for them, with its own worlds, options and logs.
- What this does not do: sandbox the compiled game. horizon-mixins' checks stop mistakes, not a malicious helper
  ([security notes](https://github.com/horizon-rust/horizon-mixins/blob/main/docs/security.md)).

## Multiplayer

- **Single player and LAN host:** the mods run in the host's game and its integrated server, so a server-side
  hook applies to every player of the world.
- **Joining a server:** only client-side hooks do anything; the protocol is unchanged.
- **Dedicated servers:** native mods are not built yet. Lifecycle hook call sites are ready for when the pipeline builds them (`--no-default-features`).

## Left to do

- Rebuild after the mods or the Nightly change (asking first, as it compiles for minutes).
- A prebuilt horizon-mixins from its CI instead of building it on the player's computer.

## Named hook catalog (draft)

`native-hooks.json` maps stable hook names and contract versions to the current
source targets. A contract includes the signature, call timing and behavior.
Keep an existing contract or add a new version when it changes incompatibly.
The catalog does not cover helper methods or arbitrary private game code.
Named injection resolution needs a matching horizon-mixins change; existing raw
targets and the `hooks` metadata continue to work as before.
