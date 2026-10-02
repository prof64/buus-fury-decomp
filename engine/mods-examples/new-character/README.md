# new-character — Shadow Goku, a new playable fighter

A fighter built from Goku's sprite set with a palette remap, and its own record (stats, growth, skill list),
form and battle-roster entry. Everything references base content by path / id; no sprite pixels or frame data are
shipped.

| file | what |
|---|---|
| `sprites/900_shadow_goku/sprite.json` | `"$extends": "sprites/106_goku/sprite.json"` + a new `id` / `sprite_index` 900: every frame, animation, body box and footprint of Goku's sprite |
| `sprites/900_shadow_goku/sheet.png.derive.json` | the sheet: Goku's with the orange gi -> purple, blue undershirt / wristbands -> crimson (exact colour remap; alpha kept) |
| `forms/54_shadow_goku.json` | form 54: `$extends` Goku's form 0, `char_index` 9, `gfx_id` 909 (= sprite 900 + 9), no transformations (`transform_up/down` 255), its own form bonus |
| `characters/shadow_goku.json` | character 9: new-game level / HP / EP / STR / POW / END / SPD, form, skills `[7, 15, 14]`, growth (`raw_u32`, the game's level-up formula) |
| `battle/fighters/shadow_goku.json` | the battle roster entry: `--fighter shadow_goku` (and the online lobby's fighter list) |

Ids: sprite indices, form and character indices and skill ids are numbers; pick ones the base game does not use
(sprites < 300, forms < 54, characters < 9, skills < 24) — `docs/modding.md` §3.

Story mode: the story's party has the game's nine characters; a new character is a battle fighter (a story
`set_character` to index 9 is not supported).

## Try it

```
buus-fury --mod engine/mods-examples/new-character --fighter shadow_goku --arena crater
```
Web: add the folder as a mod, then `?fighter=shadow_goku`.

## Test

`cargo test -p wf-app --test mods new_character` — the fighter loads with form 54 / sprite 900 / character 9 stats
(different from Goku's), the derived sheet has Goku's size and alpha with > 1000 recoloured pixels, it walks,
fires and hits, and the frame renders `sprites/900_shadow_goku` without errors.
