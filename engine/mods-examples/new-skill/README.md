# new-skill — Rapid Blast, a brand-new skill for Goku

A new skill id (24) built on the Energy Blast's state machine, with its own cost, damage and frame data, a HUD
icon derived from the Energy Blast icon, and assigned to Goku.

| file | what |
|---|---|
| `skills/24_rapid_blast.json` | the skill: `"skill": 24`, name, description, and `engine`: `base: 7` (run the Energy Blast behaviour), `cost`/`check` 12 EP, `damage` 640/1024, `attack_sub` 4, `fire_frame` 6 (the shot leaves 6 frames after the press instead of 16), `recovery` 18 (refire window, 40), `speed` 1280 (1/256 px per frame, 768), `hit_box` 12x12 (16x16), `anim` `["kiblast"]` |
| `icons/skills/skill_24.png.derive.json`, `skill_24_alt.png.derive.json` | the HUD icon (and its greyed variant): the Energy Blast icon with the blast recoloured blue. A *derive* file names a base file by path and a palette remap; no pixels are shipped |
| `characters/goku.patch.json` | Goku's new-game skill list becomes `[24, 7, 20, 15, 14]` (Rapid Blast selected first, in battle and in a new story game) |

Behaviours a new skill can reuse (`engine.base`): any built-in skill id 1..23 (`skills/*.json` in the base pack:
2 Big Bang, 4 Burning Attack, 7 Energy Blast, 8 Energy Punch, 9 Energy Sword, 10 Final Flash, 15 Kamehameha, 17
Masenko, ...). Frame data (`fire_frame`, `recovery`, `speed`, `hit_box`) applies to the Energy Blast family;
`drain` to the Kamehameha family; `cost`, `check`, `damage`, `attack_sub`, `anim` to every skill.

No Rhai is needed: a battle skill is simulation data. (Skills cannot run scripts: the battle core has no script
host, see `docs/modding.md` §8.) Story scripts can teach it: `learn_skill(0, 24)`.

## Try it

```
buus-fury --mod engine/mods-examples/new-skill                    # Goku vs a Majin Fighter, B fires Rapid Blast
buus-fury --mod engine/mods-examples/new-skill --story 8-3        # also in story mode (new game skill list)
```

## Test

`cargo test -p wf-app --test mods new_skill` — skill 24 is selected first, `SkillStart { id: 24 }`, 12 EP spent,
its shot spawns exactly 10 frames earlier than the Energy Blast's in the same battle, it hits, the HUD shows its
icon and the frame renders without missing content.
