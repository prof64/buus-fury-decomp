# scripted-skills — new skill behaviours in Rhai

Two skills that no built-in state machine does, written as data + hooks (`docs/modding.md` §4b), given to Goku:

| skill | how it plays |
|---|---|
| 25 Homing Volley | hold B: one ki orb at once, one more every 12 frames (6 EP each, up to 3); release: they leave in a fan and home in on the nearest enemies. 10 EP to start, 30-frame cooldown |
| 26 Teleport Strike | tap B: vanish (not hittable), reappear behind the nearest enemy facing it, one heavy blow (x2.0). 25 EP, 45-frame cooldown |

| file | what |
|---|---|
| `skills/25_homing_volley.json`, `skills/26_teleport_strike.json` | the skills: `engine.script` (the behaviour), `check` / `cost` (EP), `cooldown`, `damage` / `attack_sub` (for its projectiles / strikes), start `anim` |
| `hooks/skills/homing_volley.rhai` | `on_start` / `on_frame` / `on_end`: gather while held (`spend`, `set_var`), `fire_dir` the fan on release, end after the recovery |
| `hooks/skills/teleport_strike.rhai` | `teleport` behind `s.target`, `face`, `strike(26, 26, 2048)`, `hittable` |
| `projectiles/homing_orb.json` | the orb: speed / acceleration, homing turn rate and range, life, hit box, damage, wall = explode |
| `effects/mod_orb/` | its sheet: `$extends` the Energy Blast effect + a recoloured `sheet.png.derive.json` |
| `icons/skills/skill_25*.png.derive.json`, `skill_26*` | HUD icons recoloured from the Energy Blast / Instant Transmission icons |
| `characters/goku.patch.json` | Goku's skill list starts with 25, 26 |

## Try it

```
buus-fury --mod engine/mods-examples/scripted-skills --fighter goku --enemy t68
```
(SELECT / L / R pick the skill; hold B for the volley.)

## Test

`cargo test -p wf-app --test mods scripted_skills` — three orbs gathered and fired, they turn to hit an enemy off the
firing line, EP 10 + 2 x 6, the cooldown; the strike lands behind the enemy facing it; a byte snapshot with orbs in
flight restores to an identical continuation; the orbs draw from the derived sheet.
