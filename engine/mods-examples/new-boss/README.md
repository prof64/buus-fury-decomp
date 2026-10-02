# new-boss — Shadow Brute, a new enemy type with a scripted phase change

Enemy type 150: the Majin Fighter's body recoloured, 900 HP, its own AI (`docs/modding.md` §4c). Phase 1 chases
and punches with the game's node kinds; below half HP a scripted condition starts a roar (invulnerable, screen
shake, an eighth of its HP back), then phase 2 alternates volleys of three homing dark orbs with a faster chase.

| file | what |
|---|---|
| `enemies/shadow_brute.json` | `$extends` the Majin Fighter: `type` 150, stats, `ai_state` 0x2000, `anim_as` 68 (its animation rules), `gfx_ids` [910] |
| `sprites/901_shadow_brute/` | `$extends` the Majin Fighter's sprite + a recoloured sheet (grey skin, crimson clothes) |
| `ai/state_2000.json` | phase 1: `ModCondition` phase check → roar; `IfPlayerWithin` / `Chance` / `IfPlayerInMeleeRange` → punch state; `ChasePlayer`; on hit the game's standard reaction (state 0x00C) |
| `ai/state_2001.json` | phase 2: `ModAction` volley, then `ParallelAny(Wait 70, punch-or-chase)`, again |
| `ai/state_2002.json` | `MeleeAttack`, `Wait`, `ModAction` back to the current phase |
| `ai/state_2003.json` | the roar (`ModAction`, 48 frames) |
| `hooks/ai/brute.rhai` | the scripted nodes: `phase_two_due`, `roar`, `volley` (aims a fan with `isqrt`), `back_to_phase`; the phase lives in `s.vars[0]` |
| `projectiles/brute_orb.json`, `effects/mod_dark_orb/` | the orb: slow, accelerating, homing, explodes (area hit) at walls and at the end of its life; a red recolour of the Energy Blast sheet |

## Try it

```
buus-fury --mod engine/mods-examples/new-boss --enemy t150 --arena forest
```

## Test

`cargo test -p wf-app --test mods new_boss` — type 150 loads with its sprite / AI / animation rules; phase 1 states
only; at half HP the roar runs, sets the phase and heals 112 HP; phase 2 fires volleys that hit the player; draws
without missing art; a byte snapshot in phase 2 restores to an identical continuation.
