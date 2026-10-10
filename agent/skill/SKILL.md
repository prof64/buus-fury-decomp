---
name: buu-battle
description: Play Dragon Ball Z Buu's Fury battles through the buu-battle MCP server (frame-exact simulator of the GBA game) - start fights, read observations, punch / use skills / dodge, explore with save_state and rewind, or write a per-frame controller. Use when asked to play, test, beat or benchmark a Buu's Fury fight, boss or enemy group.
---

# Playing Buu's Fury through the buu-battle MCP tools

The server runs the reverse-engineered game logic headlessly. **The game is paused between your calls**; every
action runs a number of frames (60 frames = 1 second) and returns `{result, observation}`. Fights are
deterministic: the same config and the same inputs always give the same result, so experiments are repeatable.

## Start

1. `list_presets` (story fights) or `list_enemies {search}` to pick an opponent; `list_fighters` for characters.
2. `new_battle {fighter, level, preset | enemy+count, arena?, seed?, skill_level?}` -> `session` id, a full first
   observation (your skills + EP costs) and an ASCII map of the arena.
   - Level 50..200 (a character never goes below its new-game level, Goku = 60). Lv60 is hard, Lv80+ comfortable
     against regular enemies, bosses want 100+.
   - `skill_level` 0..2 sets the cost tier: at level 2 Kamehameha costs 150 EP, more than a Lv60 Goku has (90).
     Check `player.skills[].usable` and pass `skill_level: 0` for cheap beams at low levels.
3. `save_state {name:"start"}` right away - it costs nothing.

## Controls (GBA buttons)

| button | action |
|---|---|
| D-pad | walk 8-way (~1.4-2.1 px/frame by SPD). Walking starts one frame after the press; a 1-frame tap only turns you |
| double-tap a direction (from idle) | dash ~2.6 px/frame; dashing into a wall bumps you back and stuns ~20 frames |
| A | punch: one instant hit on the press frame, 17-frame recovery, **hits every enemy inside the box**, +1 EP per damaging punch |
| B | use the selected skill (hold to charge / sustain; tap again to refire Energy Blast) |
| R | block: frontal hits do 1/16 damage. R+B: energy shield (all damage 0 once charged, drains EP) |
| L | cycle the selected skill |

A press acts on the frame the button goes **down**: holding A punches once. To punch repeatedly, alternate A and
nothing (`steps: [{"buttons":"A","frames":1},{"buttons":"","frames":1}]`), or just use `attack`.

## Skills (EP cost by skill level 1/2/3)

- **Energy Blast** (everyone): 5% max EP per shot, ~35 damage at Lv60; refires about every 25 frames.
- **Super Saiyan** (EP 10, then a draining gauge; reverts when empty, then a cooldown): stat boost.
- **Kamehameha / Kamekameha / Electric Kamehameha / Final Flash / Flame Kamehameha**: beams, 15/45/150 EP; hold B to
  charge and sustain. A straight line along your facing: line up first (`use_skill {align:true}`).
- **Big Bang** (Vegeta, 10/33/100): charge, release; stronger the longer you hold. **Masenkoha** (Gohan 50/200/800):
  a cursor in front, release throws a ball at it. **Burning Attack** (Trunks 35/105/420): stun ball.
- **Energy Punch** (Goten 25/70/290), **Super Kamikaze Ghost** (Gotenks 160, homing ghosts), **Ultra Volleyball**
  (Gotenks 350), **Banshee Blast** (Vegito 200), **Energy Sword** (Vegito 350, spin + move with the d-pad),
  **Big Bang Kamehameha** (Gogeta 12), **Dragon Fist** (Gogeta 300, steer the head), **Instant Transmission** (40).
- EP regenerates slowly (about 3% of max every 10 s); punches that deal damage give +1 EP.

## Damage basics

- Melee: your STR vs their END, crit chance 82*STR/65536 (x1.5). Energy: POW vs END. Damage has a small random
  spread. Hits during your 24-frame hurt animation do 0 ("during hurt i-frames").
- `enemies[].takes` shows what an enemy shrugs off: `melee: "x0.05"` (Yakon), `melee: "x0.50"` (Janemba),
  `energy: "heals it"` (Yakon absorbs ki - punch him), `all: "no damage"` (story-invulnerable Buus: survive 60 s).
- Knocked-back enemies slide; one pinned against a wall can be punched repeatedly (a strong pattern).

## Reading an observation

```
player:  pos [x,y] (feet, world px; y grows downward), facing, hp [cur,max], ep [cur,max],
         state (idle/walk/dash/punch/block/hurt/skill/...), can_act, skill {name, ep, usable}
enemies: sorted by distance: id, name, pos, hp, dist, dir ("right", "up-left"...), facing,
         ai = what its AI runs now ("ChasePlayer", "MeleeAttack", "IfPlayerAttackingMe>Chance>GotoState"...),
         melee = "in_range" (a punch now hits) | "turn_<dir>" (turn, then punch), takes, stunned, airborne
projectiles: kind, team (hostile / yours), pos, vel px/frame, eta = frames until it hits you (0 = on you),
         Janemba's rising hands: armed_in / gone_in
walls:   free px in each direction before a wall (don't dash into a small number)
events:  hits dealt / taken with enemy ids, defeats, phase changes; sfx: sounds played
```
`observe {map:true}` adds the ASCII map ('#' wall, 'P' you, letters = enemies, '*' hostile shots).
`screenshot` returns the real game picture with enemy ids drawn above them - use it when the JSON is confusing.

## Recommended loop

1. **observe -> plan -> act in chunks of 10-60 frames.** Prefer macros: `attack {target, stop_on:["hurt"]}`,
   `use_skill`, `move_to`, `block`, `dash`, `wait`. Each returns a summary (`dealt`, `hp_lost`, `stopped`).
2. Use `stop_on` to get control back at the moment that matters: `hurt`, `kill`, `hit`, `phase`.
3. **Explore with time travel**: `save_state` before a risky choice, try it, `rewind {frames}` or `load_state` and
   try another. When something hits you, rewind ~30 frames and look at the projectiles / enemy `ai` to learn the
   tell, then dodge (move across the shot's velocity, step off a hand before `armed_in` reaches 0).
4. **Reflex tasks -> controller.** `run_controller {code, frames}` runs your JS `(obs, mem) => buttons` every frame
   (obs = observe() shape; helpers `toward(from, to)`, `dist(a, b)`, `BTN`, `log()`; return null to stop).
   Example that beat 4 Majin Fighters at Lv60 and Super Janemba at Lv110:
   ```js
   (obs, mem) => {
     const p = obs.player, e = obs.enemies[0]
     if (!e) return ''
     const t = (obs.projectiles || []).find(s => s.team !== 'yours' && s.eta != null && s.eta < 20)
     if (t && p.can_act) return Math.abs(t.vel[0]) >= Math.abs(t.vel[1]) && (t.vel[0] || t.vel[1])
       ? (p.pos[1] > t.pos[1] ? 'DOWN' : 'UP') : (p.pos[0] > t.pos[0] ? 'RIGHT' : 'LEFT')
     if (e.melee === 'in_range') { mem.a = !mem.a; return mem.a ? 'A' : '' }
     if (e.melee) return e.melee.slice(5).toUpperCase()
     const side = p.pos[0] < e.pos[0] ? -1 : 1
     return toward(p.pos, [e.pos[0] + side * 14, e.pos[1]]).join('+')
   }
   ```
   Iterate: `save_state`, run, read `result` (dealt vs hp_lost), `load_state`, tweak, run again.
5. Finished? `get_replay` gives the config + input log; `play_replay` re-simulates it and confirms the same hash.

## Enemy tips

- Groups: punch hits everything in its box - fight where enemies bunch up; block (`R`, facing them) when a
  neighbour's `ai` shows `MeleeAttack` and you can't hit first. Majin Fighters dodge/counter when you attack them
  (`IfPlayerAttackingMe>Chance`).
- Dabura: knock him into a wall and keep punching. Yakon: energy heals him, melee does 5% - long fight.
- Janemba: rising hands under you (dodge or tank), arms are parts; Super Janemba hits ~150 in melee, teleports, waves.
- Kid Buu splits when you keep your distance (merges after 1800 frames). Story Majin Buu: survive 60 s.
- Multi-phase presets: the next phase spawns 120 frames after the last enemy falls (`wait {stop_on:["phase"]}`).
