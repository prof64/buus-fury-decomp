# Yakon (enemy type 0x91): how he takes damage

All of this is VERIFIED in the emulator, both force-spawned and in his real arena (Yakon's Planet, world 8 map
40). Goku came from `buu_snakeway_start.ss`. This note corrects notes.md §Yakon, whose advice "melee resist 7/128 →
use energy attacks" is wrong.

## The rule

- **Melee always works, but weakly.** His melee resist is 7/128, so a punch does about 5.5% of normal damage
  (2.3–2.5 HP per punch here, out of 1725 HP).
- **Energy attacks feed him in almost every state.**
  - In states 0x31D, 0x31E, 0x321, 0x322 and 0x324 (wander, walk home, the combo, return), the on-hit tree is
    `SubState 0x320`. In 0x31F (guard) it is the same `IfAttack(1)` branch.
  - That branch is `IfAttack(1 = energy) → Sequence{ HealFraction(100%), Succeed }`, so there is no
    ApplyDamage: he gains HP equal to the damage instead of losing it.
- **The window: the 60-frame pause after his 6-punch combo (state 0x323).**
  - The on-hit tree there is `Sequence{ IfAttack(1) → IncCounter ; SubState(5) → ApplyDamage }`.
  - Energy hits during this pause do **full damage** (energy resist 128/128).
  - They also set the counter, so he skips the downward mouth beam (`0x323` → `0x324`, not `0x326`).
  - Measured: a held Kamehameha Lv2 here took about 21 HP per frame and killed him from 800 HP inside the pause.
- **States 0x325 and 0x326 ignore every hit** (on-hit tree `Idle`).
  - 0x325 is summoning the Mini-Yakons and waiting for them to die.
  - 0x326 is the mouth beam.
- **How to beat him:** dodge the 6-punch combo, then beam him (hold Kamehameha) during the roughly 1-second pause
  that follows. Repeat each cycle. Punch to chip away, and never use energy at any other time.

## The absorb quirk: a 32-bit overflow

`HealFraction` onHit is at `0x0803A79C` (node, obj, msg):
```
if msg.mult == 0: return 1
amount = (s32)(msg.final_damage(+0x18) * node.frac(+0xC)) >> 16     ; muls + asrs: a 32-bit product
obj->vfunc[+0x60](&amount)
```
For Yakon, `frac` is 0x10000, so `amount` equals the low 16 bits of the damage, sign-extended (in 1/1024 HP).
The heal virtual function at `0x0800FA80` (generic enemy vtable `0x0804C484`) does three things. First, it shows a
popup (`0x0800E97E`). Second, it does `HP += ((amount + 0x3FF) >> 10) << 10`. Third, it caps HP at max HP. It
never checks for death.

What this means:
- An energy hit of `d` HP heals `d mod 64` HP when that is under 32. When it is 32 or more, it **hurts** by
  `64 − (d mod 64)` HP. So a 33 HP Energy Blast removes 31 HP, and a 31 HP blast adds 31.
- Recorded Energy Blasts (31–34 HP each) took him 1725 → 1694 → 1664 → 1695 → 1665. Kamehameha ticks (about 10 HP
  each) only ever heal, and filled him 1000 → 1725.
- HP can go **negative** through this path, and he stays alive. Recorded: 20 → −11 → −41 → −10 → −40 → −8 HP.
  - The damage virtual function `0x0800FA2A` treats only `HP == 0` as already dead. Otherwise it subtracts and
    kills when the result is ≤ 0.
  - So the next ApplyDamage hit kills him. In the recording, one punch at −8 HP did it.
- This is very likely an accidental overflow, not a designed "overfeed until he bursts" mechanic. His trees have no
  node that reacts to absorbed energy. In every recording, hundreds of absorbed hits, including heals capped at
  max HP, caused no AI state change, no swelling and no spawned object. His death
  is the ordinary one: the defeat script `0x0878EAE4` runs when HP reaches 0 through ApplyDamage.

## Sim changes (site/src/battle/rulesets/buu)

- `ai.ts` HealFraction now computes `Math.imul(dmg, frac) >> 16`.
- `enemy.ts` `heal()` follows `0x0800FA80`: HP rounds up to whole points, is capped at max, can go negative, and
  shows a damage popup when negative.
- `enemy.ts` `takeHit()` now treats only HP == 0 as already dead.
- The preset label now reads "Yakon (he absorbs energy: beam him in the pause after his punch combo)".
- The 0x323 window already worked in the sim through the decoded trees.

## Evidence and tools

- `re/buu/yakon_absorb.py`: one attack type per run, HP per frame, every ComputeDamage, HealFraction and
  ApplyDamage call. Options `hp=` (start HP), `map=40`, `tag=`. Output goes to `work/buu/yakon_absorb*.json`.
- `re/buu/yakon_window.py`: forces state 0x323 or 0x326 under a Kamehameha or Energy Blasts.
- `re/buu/yakon_hpwatch.py`: write-watch on HP (obj+0x28C). The only writer during energy hits is `0x0800FAA0`
  (the heal function).
- `re/buu/yakon_realmap.py`: warps to 8-40. His spawn record `0x0807D914` has story condition 0x1, which is
  patched to the forest Majin Fighter's. Saves `work/buu/yakon_map40.ss`.
- `re/buu/yakon_record.py` writes `data/battle/buu_yakon_absorb.json`, the fixture for
  `site/tests/battle/buu_yakon.test.ts`. That test replays every recorded hit (state, attack, final damage)
  through the sim's on-hit tree and matches HP after each hit exactly, including the negative-HP survival and
  the deaths.
- The AI state stack at obj+0x238 holds the depth at +4 and entries `{runtime, def}` from +8, with
  def = `0x0870F514 + 8·state`.
- Masenko, Energy Sword and Final Flash produced no hits from Goku in these tests (they are other characters'
  skills). Big Bang Lv2 (`sub 13`, ×101, about 3 HP per hit) and Dragon Fist hits only healed.
