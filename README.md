# Replay (Horizon native mod)

Record a world session and play it back. An original mod; it shares no code with any Java replay mod. MIT licensed.

| Command | Does |
|---|---|
| `/replay start` | Record the dimension you are in (mobs, players, every block change), up to 30 minutes |
| `/replay stop [name]` | Stop and save |
| `/replay list` / `/replay delete <name>` | Manage recordings |
| `/replay play <name>` | Play back in the dimension it was recorded in |
| `/replay pause`, `/replay speed <1-8>`, `/replay skip <seconds>` | Control playback (skip is forward only) |
| `/replay end` | End playback and put every changed block back |

Use `/gamemode spectator` for a free camera during playback.

## How it works

Server-side hooks (API 62): `native_level_tick_post` snapshots mobs (`native_mobs`) and players
(`native_server_players`); `GameLevel::native_block_changed` records block changes with their old state. Only changes
are stored. Playback re-creates mobs with `native_spawn_mob` (no AI, silent, invulnerable), moves them with
`native_teleport_mob`, re-applies blocks with `native_set_block`, and undoes the blocks in reverse on `end`.

## Limits (be aware)

- **Playback edits the live world** (it restores blocks on `end`, but a crash mid-playback leaves them changed). Play in a copy of the world.
- Players are stood in for by named villagers, with no skins or animations. Mob facing and equipment aren't replayed.
- Items, projectiles and other non-mob entities aren't recorded yet.
- Only recording from a world you host (single player or LAN). Joining a server runs only client-side hooks.
- Recordings start from the world as it is: the blocks that existed before recording aren't saved.

## Status

The logic (`mixins/replay/runtime.rs`) has unit tests (`cargo test`). The glue was written against the hook
signatures in Horizon commit `d0ce8fa` and has **not been compiled** into the game:
`python3 native/tools/native.py check replay` from a horizon-mods checkout (copy this folder to `native/replay`).
