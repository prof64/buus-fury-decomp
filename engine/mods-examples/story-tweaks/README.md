# story-tweaks — a file-only story mod

No scripts, no code: JSON files at the base pack's paths (engine/docs/modding.md §1–2).

| file | what it does |
|---|---|
| `characters/goku.patch.json` | merge patch on Goku's record: a new game starts him with STR 40 and SPD 60 (the other stats stay) |
| `skills/15_kamehameha.patch.json` | the Kamehameha needs only 10 EP to start (`engine.check`) and hits harder (`engine.damage`, 1/1024 units) |
| `items/090_senzu_bean.json` | a **new supply** (index 90, above the game's 0..82): name, description, price, icon, a data-only `effect` (restore 300 HP / 100 EP when used from the Inventory) and `sold_at: ["s028"]` (the Hercule City Z-Mart's Buy → Items list) |
| `icons/supply/090.png.derive.json` | its icon: an alias of the Onigiri's (`from`, no recolour) |
| `equipment/200_weighted_gi.json` | a **new equipment** item from the Cotton Gi (`$extends`): own name, STR +6 / END +4, price 800, on the same Z-Mart's Buy → Equipment list |
| `scripts/events/s028.text.json` | **edited dialogue**: the Z-Mart clerk's welcome (node 1, a choice: its option lines stay) and goodbye (node 9) |

Try it: `buus-fury --mod engine/mods-examples/story-tweaks`, New Game, and in Hercule City walk into the Z-Mart
(the door north of the save point in 3-1). Check it: `cargo run -p wf-app --example modcheck -- <content/base>
engine/mods-examples/story-tweaks`. Test: `crates/wf-app/tests/mods.rs` `story_tweaks_file_only_mod`.
