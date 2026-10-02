# Custom move JSON — `legacy-of-goku-x/custom-move@1`

The Move lab (`#/g/<game>/lab`) exports one move per file (`<name>.<game>.move.json`). The format is
self-contained so the battle simulator can import it without the game's data files. Types live in
`site/src/movelab/schema.ts` (`CustomMove`); `validate()` there checks the required shape.

```jsonc
{
  "$schema": "legacy-of-goku-x/custom-move@1",
  "game": "log2",                 // "log1" | "log2" | "buu" — which damage model applies
  "name": "Big Punch",
  "template": "p0",               // key of the stock move it was cloned from (Moves page ?m=<key>)
  "description": "…",             // optional, copied from the template
  "user": "player",               // "player" | "enemy"

  "attack": {
    "kind": "melee",              // "melee": attacker STR, defender melee multiplier / END
                                  // "energy": attacker POW, defender energy multiplier / END2
    "power": 1536,                // attack-table power, 1024 = 1.0 (LoG2, Buu; 0 for LoG1)
    "attackId": 4,                // LoG2: attack id byte (9 = stun, 15 = zero-damage stun)
                                  // Buu: attack sub id (3 = melee that can never crit)
    "scale": 1024,                // per-hit multiplier: LoG2 hit-message scale, Buu hit mult; 1024 = 1.0
    "canCrit": true,              // LoG2: melee crits ×2 at STR·164/65536; Buu: ×1.5 at (82·STR + bonus)/65536
    "log1": {                     // LoG1 only — LoG1 has no attack stat, damage is an expression:
      "mode": "maxhp_pct",        //   maxhp_pct: (maxHP × (base + rand2(range))) >> 9   (punch: 384, 128)
      "base": 384,                //   maxhp_div: maxHP / divisor                       (ki blast: 3)
      "range": 128,               //   ki_spent: Ki charged, (maxKi+15)>>4 per 8 frames  (Kamehameha)
      "divisor": 3,               //   cur_hp: cur_hp/4 + rand(cur_hp/2)                (enemy melee)
      "fixed": 0                  //   fixed: constant
    }                             // then the LoG1 defense roll (apply_damage 0x08007352) applies
  },

  "cost": { "unit": "EP", "min": 4, "start": 4, "perFrame": 0 },
                                  // unit "Ki" for LoG1. min = needed to start, start = paid on press,
                                  // perFrame = drained while held (LoG2 Kamehameha 86/256)

  "timing": {
    "startupFrames": 0,           // press → first hit / projectile spawn
    "activeFrames": 1,            // frames the hitbox is live
    "recoveryFrames": 16,         // frames until the user can act again
    "hits": 1,                    // hits per use
    "rehitInterval": 0            // frames between hits on the same target (0 = once)
  },

  "hitbox": {                     // [x0, y0, x1, y1] px relative to the user's entity position
    "down":  [0, 0, 16, 16],      // (= sprite anchor; player body ≈ (-2,-24)-(18,10) in LoG2/Buu,
    "up":    [0, -24, 16, 0],     //  (2,-22)-(14,6) in LoG1). Strict overlap test:
    "left":  [-8, -24, 0, 8],     //  x0 < hx1 && hx0 < x1 && y0 < hy1 && hy0 < y1
    "right": [16, -24, 24, 8]
  },

  "projectile": null,             // or:
  // { "speed": 3, "w": 16, "h": 16, "lifetime": 40, "pierce": false,
  //   "spawn": { "down": [8,0], "up": [8,-8], "left": [-2,-8], "right": [18,-8] } }
  //   speed px/frame along the facing; spawn = projectile centre relative to the user;
  //   lifetime frames (0 = until it leaves the screen); pierce = keeps going after a hit;
  //   speed 0 = a beam that stays attached (Buu beam objects).

  "effects": { "stunFrames": 0, "knockback": 16 },
                                  // knockback param p: LoG2/Buu velocity = (rand(64)+96)·p >> 7 · 8 /256 px/f

  "rom": {                        // informative, written on export: the first encoded table entry
    "table": "Attack-table entry (8 bytes)",
    "address": "0x086DB34C",      // GBA bus address; file offset = address − 0x08000000
    "bytes": "00 04 00 00 00 06 00 00"
  }
}
```

## Damage evaluation (what an importer must do)

- **LoG2** — `site/src/formulas/log2.ts damage()`: S = attacker STR/POW byte, DEF = defender END,
  MULT = defender melee/energy taken (128 = 1.0):
  `((((rand(26)+256)·(power·(1+crit)·(128−DEF)>>7)>>8)·f(S)>>18)·scale>>10)·MULT>>7`,
  `f(S) = ((26S−701)S+115618)S>>8`.
- **Buu** — `site/src/formulas/buu.ts damage1024()` (result in 1/1024 HP): melee `P = S·power·0xCCD>>16`
  (crit adds power>>1), energy `P = power`; `D' = D≥200 ? min(D/4+150, 220) : D`;
  `X = (221−D')·P·297>>16`, `A = S + (S²(S+100)>>13)`, `Y = (scale·X+1023)>>10`,
  `raw = ((256+rand(26))·Y>>8)·A`, `dmg = (resist·raw+127)>>7`.
- **LoG1** — raw from `attack.log1`, then `site/src/formulas/log1.ts applyDefense()`; enemy hits on the
  player are first capped at `player maxHP/3 + 1`.

`rand(n)` returns 0..n−1 in LoG2/Buu and 0..n (inclusive, signed-overflow bug for n ≥ 32768) in LoG1.
