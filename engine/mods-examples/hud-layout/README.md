# hud-layout — HUD position and skill-icon set

`ui/layout.json` (`docs/modding.md` §6b) moves the HUD to the bottom-right corner (8 px from the edges in the GBA
screen and in the zoomed-out PC view), draws the area banner at the top, and swaps skill icons.

| file | what |
|---|---|
| `ui/layout.json` | `hud.pos` [152, 136] + `anchor` bottom_right; `skill_icons.dir` (a set folder; missing icons fall back to the game's) + `files` (one icon per skill id); `banner.y` 8 |
| `icons/hud_mod/energy_blast*.png.derive.json` | the Energy Blast icon (+ `_alt`) recoloured green |
| `icons/skills_night/skill_15.png.derive.json` | the set's Kamehameha icon, recoloured |

## Try it

```
buus-fury --mod engine/mods-examples/hud-layout --fighter goku
```

## Test

`cargo test -p wf-app --test mods hud_layout` — the pixels the HUD changes are at (8, 8) without the mod and in the
bottom-right corner with it (240 x 160 and 480 x 320 views); the selected skill's icon is the mod's; no layout file
= the game's layout.
