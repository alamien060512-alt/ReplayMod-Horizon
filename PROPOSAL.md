# Proposal: a Replays viewer for Horizon (main menu, self-contained replays)

Status: draft for the Horizon suggestion queue (playhorizon.dev/hub, or `/suggest` on Discord; CONTRIBUTING.md says ideas go there, not to issues). Written against Horizon commit `d0ce8fa`, hooks API 62.

## Goal

What Flashback and Replay Mod give players on Java Edition:

1. A **Replays** button on the main menu that opens a list of recordings (name, length, date, thumbnail).
2. Opening one **plays it in its own temporary world**. The original save is never touched.
3. A **free camera** with pause, speed, scrub and (later) keyframed camera paths.

## What blocks it today (verified in the source)

| # | Gap | Where | Why a mod can't fill it |
|---|---|---|---|
| 1 | No way for a mod to add a main-menu button or screen | `title.rs`: `Mode` enum, `buttons()` (Singleplayer, Multiplayer, a disabled Realms button), `click_action()` | The screen set is a closed enum; the 83 hook points have no title hook. |
| 2 | No way for a mod to open a world | `title::Action::Play { folder }`, `Action::Create` | Only the title screen produces these actions. |
| 3 | No camera control | `native_zoom` only changes FOV; `native_player()` is read-only | Free flight today needs spectator mode and `/tp`, and only on a LAN host. |
| 4 | Recordings aren't self-contained | the `replay` mod stores entity and block-change events only | Playback depends on the original world. |

## Proposed Horizon changes (small, hook-shaped, hooks API 63+)

Following the rule that hook points are only ever added:

- **`native_title_buttons(buttons: &mut Vec<NativeTitleButton>)`** (client, title screen): a mod pushes `{ id, label, enabled }`; Horizon lays it out under the existing buttons with the design system's styling.
- **`native_title_click(id) -> NativeTitleAction`**: a mod answers with `OpenScreen`, `PlayWorld { folder }` or `None`.
- **`native_title_screen_draw / _click / _key`**: the mod draws its own list screen (using the existing `native_fill`, `native_text`, `native_sprite` helpers) and gets mouse and key events. This is the Replays browser.
- **`native_open_world(folder, read_only: bool)`**: lets the mod open a world folder it prepared (the replay's temporary world), with a flag that skips saving on exit.
- **`native_camera(camera: &mut Option<NativeCamera>)`**: per frame, the mod may override position, yaw, pitch and FOV of the view (like `native_zoom` does for FOV). Only honoured in a world the mod opened.

Each is additive and doesn't touch vanilla-parity behaviour: with no mod installed nothing changes.

## Replay format v2 (the mod side)

A `.replay` becomes a folder or archive: `meta.json` (name, date, duration, game version, dimension), `events.log` (today's event stream, extended), `chunks/` (the region data of the chunks near where the player was, snapshotted at start and when a chunk first loads), `thumb.png`. Playback copies `chunks/` into a temp world folder and opens it read-only, then replays the events into it, so the player's real world is never edited and no undo is needed.

Event stream additions needed beyond today's mobs, players and blocks: item entities and projectiles (via the entity join and leave hooks, API 47 and 53), player equipment and animation, time of day and weather, chat. Some need new read helpers (entity pose and equipment); they are the same kind of additive helper.

## Plan

1. **Now (mod only):** finish the format v2 recorder, including chunk snapshots, and the playback into a temp world. Testable with the existing hooks, with the replay opened by hand from a normal world's folder.
2. **Upstream:** submit this proposal to the suggestion queue. The first two hooks (`native_title_buttons`, `native_title_click`) plus `native_open_world` are the minimum that gives a menu entry that opens a replay.
3. **Upstream or fork:** the title screen drawing hooks and `native_camera`.
4. **Mod:** Replays browser screen, then keyframed camera paths and export to video.

## Risks and open questions

- Horizon's maintainers decide whether title-screen hooks fit the vanilla-first rule; the title screen has no vanilla equivalent for mods, so expect design-system review.
- Chunk snapshots are large; they need compression and a size cap per replay.
- Mobs replay as simplified stand-ins unless an entity-state API (pose, equipment, animation) is added.
- Until hooks 63+ land, a fork of Horizon is the only way to ship the main-menu button, and a fork cannot be installed through the registry.
- I could not compile Horizon in my sandbox, so none of this has been built; every hook signature above is a proposal.
