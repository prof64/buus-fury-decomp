# new-arena-scenario — the Tournament of Fury

A story-mode scenario written in Rhai: an announcer in the Southwest Forest offers a tournament; accepting warps
you into a new arena area where three champions appear one after the other, each round introduced with the game's
dialogue boxes and portraits; beating the last one gives a prize, sets a story flag and brings you back.

| file | what |
|---|---|
| `world/areas/8-3+tournament.json` | appends one object to the existing area 8-3 (Southwest Forest): the announcer NPC (script sprite 18 = sprite 9 "Announcer") whose talk event is `s900`. The `+tournament` suffix lets several mods append to the same area without replacing each other's file |
| `world/areas/90-1.json` | a new area 90-1 "Tournament of Fury" on the base map `area_08_071` (Giant Crater): song 1, combat on, on-enter script `mod_tof_enter`, and three enemies (types 21, 123, 101) with condition `{"value": 1}` (never placed by the load; scripts spawn them) and on-defeat events `s902`..`s904` |
| `scripts/events/s900.rhai` | the announcer: greeting (champions get a different one: `get_flag(1100)`), a Yes / No choice (`get_menu_result()`), Goku's reply, `warp_fade(90, 1, 1, 96, 216)` |
| `scripts/inline/mod_tof_enter.rhai` | the arena's on-enter: `run_event(901)` |
| `scripts/events/s901.rhai`..`s903.rhai` | each round's announcement, then `spawn_map_object(i)` (the area's actor list entry i) |
| `scripts/events/s904.rhai` | the prize (`give_item(21)` with the item-get pose + `add_item(21, 1)`: two Senzu Beans), `set_flag(1100)`, Goku's line, `warp_fade` back to the forest |

Dialogue speakers: an event node's `box` = 8 + portrait index (`portraits/index.json`: 2 Announcer, 47 Goku); 0 =
no portrait. Event numbers 900+ and flag 1100 are this mod's (the base game uses events 0..521 and flags below 860);
`docs/modding.md` §5 lists the ranges.

Execution model (docs/scripting.md §1): every handler runs to completion within a frame; effects (text, warps,
spawns, item-gets) queue and run in order, queries (`get_flag`, `get_menu_result`) see the world after them. All
script state is plain data in the snapshot, so the scenario is rollback-safe and deterministic.

## Try it

```
buus-fury --mod engine/mods-examples/new-arena-scenario --story 8-3     # walk right to the announcer, press A
buus-fury --mod engine/mods-examples/new-arena-scenario --story 90-1    # straight into the arena
```
Web: add the folder as a mod, then `?story=8-3`.

## Test

`cargo test -p wf-app --test mods new_arena_scenario` — headless, muted: the announcer stands in 8-3; talking to
him and confirming warps to 90-1 on the crater map; each round's champion appears after its announcement (the test
defeats each with the engine's `enemy::die`, which runs the on-defeat event like a real KO); after the third the
player is back in 8-3 with flag 1100 and two more Senzu Beans; talking again shows the champion's greeting. No
script errors, every round's frame renders.
