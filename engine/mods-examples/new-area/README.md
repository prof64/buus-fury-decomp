# new-area — the Hidden Grove

A new field area with its own Tiled map, collision and an NPC with a Rhai talk script, reached through a new exit
in an existing area.

| file | what |
|---|---|
| `maps/mod_hidden_grove/map.json` | a Tiled map (40x30 tiles of 8 px, two layers `bg3` ground / `bg0` decoration) that uses the base pack's Southwest Forest tileset by reference (`"source": "../tilesets/b39594a26835bd49.json"`): only tile numbers, no pixels. Open / edit it in Tiled (the tileset resolves once the base pack is next to it) or regenerate it with `tools/make_grove.py` |
| `maps/mod_hidden_grove/collision.json` | collision in the mod-friendly row form: one string per tile row, `.` open, `#` wall (`~` = blocks walking, not shots) |
| `maps/mod_hidden_grove/objects.json` | the map header: start position and its exit (the path at the bottom -> back to 8-3) |
| `world/areas/90-2.json` | area 90-2 "Hidden Grove": the map, song 28 (the forest's), `combat: false` (A talks instead of punching), and the hermit NPC (script sprite 15 = "Robed Elder") with talk event `s910` |
| `world/areas/8-3+grove.json` | appends an exit trigger to 8-3: walking up into the forest's top-right alcove leads to 90-2 |
| `scripts/events/s910.rhai` | the hermit: first visit gives a Senzu Bean and sets flag 1101; afterwards `heal_party()` |
| `tools/make_grove.py` | how the map was made (the layout is ASCII art in the script); `--preview <content/base> out.png` renders it with Pillow |

## Try it

```
buus-fury --mod engine/mods-examples/new-area --story 8-3     # walk to the top-right corner of the forest
buus-fury --mod engine/mods-examples/new-area --story 90-2    # start in the grove
```

## Test

`cargo test -p wf-app --test mods new_area` — the area and map load; walking up in the alcove of 8-3 changes area
to 90-2 on `mod_hidden_grove`, the frame renders the new map without errors, the row collision stops the player at
the wall, talking to the hermit gives the Senzu Bean and sets the flag, and the bottom path leads back to 8-3.
