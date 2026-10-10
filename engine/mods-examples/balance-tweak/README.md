# balance-tweak — change stats with merge patches

The smallest kind of mod: two `*.patch.json` files that change a few numbers of base records without copying
them (RFC 7396 JSON Merge Patch, see [`docs/modding.md`](../../docs/modding.md) §2).

| file | effect |
|---|---|
| `enemies/majin_fighter.patch.json` | the Majin Fighter (enemy type 68, `t68`) gets 1500 HP (base 500), STR 30 (18) and 2000 XP (1225) |
| `skills/07_energy_blast.patch.json` | Energy Blast costs a flat 20 EP per shot (base: 5% of max EP) and hits with 2048/1024 damage (1024): twice as hard |

The skill numbers live in the skill's `engine` object, which every skill file may carry (`engine/docs/modding.md`
§4): `cost` / `check` (EP spent / needed per level), `damage` (multiplier in 1/1024 per level), `attack_sub`,
`drain` (beams) and frame data. Anything not given keeps the game's value.

## Try it

```
buus-fury --mod engine/mods-examples/balance-tweak --enemy t68          # native, headless with --headless --frames N --shot x.ppm
```
Web: Settings → Mods → *Add mod folder…* → pick this folder.

## Test

`cargo test -p wf-app --test mods balance_tweak` — the patched HP / STR reach the spawned enemy, and one Energy
Blast in the same battle (same seed, same inputs) spends 20 EP and deals twice the damage of the unmodded game.
