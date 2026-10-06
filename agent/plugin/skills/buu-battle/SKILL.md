---
name: buu-battle
description: Play Dragon Ball Z Buu's Fury through the buu-battle MCP server (the frame-exact deterministic Rust core of the GBA game) - play the story from the title (New Game, dialogue, NPCs, exits), start battles or field areas, read observations, punch / use skills / block / dodge, explore with save_state / rewind, write and evaluate Rhai controllers, verify replays. Use when asked to play, test, beat, benchmark or script Buu's Fury's story, a fight, boss, enemy group or area.
---

# Playing Buu's Fury through the buu-battle tools

The server runs the reverse-engineered game logic headlessly (no window, no sound). **The game is paused between
your calls.** Every action runs frames (60 frames = 1 second) and returns `{result, observation}`. Everything is
deterministic: same config + same inputs = same fight, bit for bit, so every experiment can be repeated, rewound
and replayed.

## Start

1. `list_presets` (story fights: `dabura`, `yakon`, `janemba`, `majin_fighters`, `kid_buu`, ...), `list_enemies
   {search}`, `list_fighters` (forms, skills, EP costs), `list_arenas`, `list_areas` (overworld).
2. `new_battle {fighter, level, preset | enemy + count, arena?, seed?, skill_level?, players?}` -> `session`, the config
   and a full first observation (skills with EP costs, stats, ASCII map).
   - `level` 50..200 (default 80). Lv60 is hard, Lv80+ is comfortable against regular enemies, bosses want 100+.
     Level matters more than tactics: Janemba at Lv60 does 18 damage per punch against 8 880 HP.
   - `skill_level` 0..2 (default 1) is the upgrade tier: it sets damage *and* EP cost (Kamehameha 15 / 45 / 150 EP).
     A Lv80 Goku has 196 EP, so tier 2 gives one beam per fight; check `player.skills[].usable`.
   - `players: 2` (or a list of fighter specs) = co-op: you act as `slot` (default 0) in every action; the other
     slots run their autopilot (`set_autopilot {slot, mode: idle | attack | controller, code}`).
3. `new_field {area: "8-3"}` starts a bare overworld area instead: walk, take exits (`observation.field.exits`),
   fight the placed enemies. Its scripts, dialogue and NPC events are not run - for those play the story (below).
4. `new_game {seed?}` plays **the story**: the whole game with its script host, from the title through New Game
   into the prologue (`title: true` stops on the title screen - its menu text is in `story.menu`). See "Story".
5. `save_state {name: "start"}` right away - snapshots are instant.

## Controls (GBA buttons)

| button | action |
|---|---|
| D-pad | walk 8-way (about 1.4-2.1 px/frame by SPD). A 1-frame tap only turns you |
| double-tap a direction (from idle) | dash ~2.6 px/frame; dashing into a wall bumps you back and stuns you |
| A | punch: hits on the press frame, ~17-frame recovery, **hits every enemy inside the box**, +1 EP per damaging punch |
| B | use the selected skill (hold to charge / sustain; tap again to refire Energy Blast) |
| R | block: frontal hits do ~1/16 damage. R+B: energy shield (no damage once up, drains EP) |
| L | cycle the selected skill |

A press acts on the frame the button goes **down**: holding A punches once. To punch repeatedly alternate A and
nothing (`press {steps: [{buttons: "A", frames: 1}, {buttons: "", frames: 1}]}`) - or use `attack`.

## Tools you will use most

| tool | what it does |
|---|---|
| `observe {map?, full?}` | the observation (below); drains the events since the last one |
| `attack {target?, hits?, max_frames?, stop_on?}` | path to a punching spot of the target (default: nearest), turn, punch every other frame |
| `use_skill {skill, target?, align?, charge_frames?, taps?, gap?, steer?}` | select with L, line up / face, hold B, wait until you can act |
| `block {frames, shield?}`, `dash {direction, frames}`, `move_to {x, y}`, `face {target or direction}` | |
| `take_exit {to?}` | field / story: walk to an exit and through it (doors sit in walls, edge exits at the border) |
| `talk {npc? or trigger?}` | field / story: walk up to an NPC (or into a "press A" trigger: a desk, a sign), turn, press A |
| `press {buttons, frames}` / `press {steps}` / `wait {frames}` | raw input |
| `save_state`, `load_state`, `rewind {frames or to_frame}`, `fork_session` | time travel (true snapshots, microseconds) |
| `screenshot {view: gba / wide / arena, scale?}` | PNG from the game's own renderer; enemy ids in yellow |
| `run_controller`, `evaluate_controller` | your Rhai policy every frame; win rate over many seeds |
| `get_replay` / `play_replay` | config + input log + state hash; re-simulation must give the same hash |

`stop_on` (every action): `hurt` (you lost HP), `kill`, `hit` (you dealt damage), `no_damage` (a hit of yours did
nothing), `phase` (next wave), `danger` (a hit on you is <= 12 frames away if you stand still), `area` (an exit was
taken), `event` (story: an event, a dialogue box or a menu took the field). `move_to` stops on `hurt` by default;
every macro but `press` / `wait` stops on `area` and `event` unless you pass your own `stop_on`. The `result` says why it stopped (`target_defeated`, `hits_done`, `skill_done`, `arrived`,
`battle_won`, `not_usable (EP 16, needs 150)`, `danger: #2 Yakon hits you for 20 in 6 frames ...`) and sums up
`dealt`, `hp_lost`, `hits_dealt`, `hits_taken`, `kills`, `ep_change`, `hits_without_damage`, `healed_enemy_hp`.

## Reading an observation

```
player:   pos [x, y] (footprint centre, world px, y grows down), facing, hp [cur, max], ep [cur, max],
          state (idle walk dash punch block hurt skill transform held dying), can_act,
          skill {slot, name, ep, usable}, skill_running, ss_gauge, held_by, blocking
enemies:  nearest first: id, name, pos, hp, dist, dir ("up-left"...), facing,
          ai       = the running AI chain ("IfPlayerWithin2>ChasePlayer", "MeleeAttack", "Wait") - its intent
          state    = AI state id ("0x323"), in_state = frames in it (state changes are the boss patterns)
          attacking {kind: melee/ranged/grab/hands, node, t} = it is in an attack node now, t frames in
          melee    = "in_range" (a punch now hits it) | "turn_left" (turn, then punch)
          takes    = what damages it IN ITS CURRENT STATE (only shown when unusual):
                     {"melee": "damage x0.05", "energy": "heals it"} / "no damage" / "damage (conditional)"
          optional (not needed to win), hittable false, airborne, stunned, blocking, targets_slot (co-op)
threats:  hostile shots / hands: kind, from, pos, size, vel px/frame, eta (frames until it touches you, 0 = now),
          Janemba's hands: armed_in / gone_in
danger:   the engine's look-ahead (45 frames): {hit_in, by, hp, if_blocking} = standing still you get hit by `by`
          for `hp` in `hit_in` frames; if_blocking = the same with R held ("no HP lost" = blocking saves you)
walls:    free px in each direction (don't dash toward a small number)
phase, next_phase_in (multi-phase presets), allies (co-op),
field:    area, name, mode, exits (to, to_name, rect, centre, dir), triggers (kind, rect, activation: "walk into it" /
          "press A inside it facing up"), npcs (id, pos, dist, dir, talks)
story:    (story sessions) frame, event_running, menu {screen, text} (title, pause menu, shop, save ...),
          dialog {text of the current page, page [n, of], options + cursor for a choice}, save_slots
events:   structured, since the last observation: hit (by, target, effect damage / healed it / no damage,
          hp_change, target_hp), hurt (by, hp, blocked), defeated, spawn, phase, skill, grabbed, ko, outcome, field
```
`observe {map: true}` adds the ASCII map ('#' wall, 'P' you, letters = enemies in the legend, '*' hostile shot).
Ids are stable, also across rewinds. Use `screenshot` when the JSON is confusing.

## Story (`new_game`)

The story session runs the game's own scripts: cutscenes, dialogue, NPCs, shops, save points, the title's New Game
and Continue, game over. It is the app's story mode (the same script host; one step = one game logic iteration,
no hardware slowdown recreated).

- **The loop**: `observe`; if `story.dialog` or `story.menu` is there, `press {steps: [{buttons: "A", frames: 1},
  {buttons: "", frames: 5}]}` (A must go down to advance; UP / DOWN move a choice's cursor first); if
  `story.event_running` (a cutscene, no box yet) `wait {frames: 20}`; otherwise the field is yours: `take_exit`,
  `talk`, `move_to`, fights as in battles.
- Macros hand control back when an event starts (`stopped: "event: ..."`) or the area changes - read the
  observation, deal with the dialogue, then call the macro again.
- Progress gates are story gates: an NPC in a doorway, a desk to report to. The opening: the prologue's boxes, then
  Snakeway `1-1` -> `1-2` -> `1-5` -> `1-3` (Yemma's Office): `talk {trigger: 0}` at Yemma's desk, then `talk` the
  ogre in the doorway, then `take_exit {to: "1-4"}`.
- `frame` in story sessions counts steps (game logic iterations). `rewind {to_frame}`, `save_state` / `load_state`,
  `fork_session` and replays restore the script host too (a cutscene or a dialogue box resumes where it was).
  `danger` forecasts are not computed in story sessions.
- The in-game save (a save point's menu) writes `story.save_slots`; they live in the session (and its snapshots).

## Mods

To test a mod (engine/docs/modding.md): `modcheck {mods: ["path/to/mod"]}` first (fix every `errors` entry: file,
what, hint), `list_mod_content {mods}` to see what it adds, then pass the same `mods` to `new_battle` / `new_field`
/ `new_game` (or set `WF_MODS`). Edit the mod's files and start a new session to test the change. Replays keep the
mods they were made with.

## Damage basics

- Melee uses your STR against their END, energy your POW; crits x1.5; small random spread. Enemy resistances
  scale it (`takes`: `x0.50` Janemba melee, `x0.05` Yakon melee).
- What an enemy takes depends on **its current AI state** (its on-hit tree): the observation's `takes` is computed
  for the state it is in right now, so it changes during a fight. Events say what each hit really did:
  `effect: "no damage (its current state ignores this attack)"`, `"healed it"`.
- **Yakon** (preset `yakon`, also phase 2 of `babidi_ship`): energy attacks *heal* him in almost every state; melee
  does x0.05. The exception is **state 0x323, the ~60-frame pause right after his 6-punch combo**: there energy
  does full damage (and stops his mouth beam). The observation shows it: `state: "0x323"`, `takes.energy` switches
  from `heals it` to `damage`. Play: block the combo (`danger` / `attacking.kind == "melee"`), stand on his row,
  select Kamehameha in advance, and hold B through the window (`use_skill {skill: "Kamehameha", target, align: true,
  charge_frames: 60}` the moment 0x323 starts). A Lv100 Goku at skill tier 2 kills him in one window. In state 0x325
  (Mini-Yakons summoned) he ignores every hit until they are dead; in 0x326 (mouth beam) too.
- Hits during your hurt animation do 0 HP. Story-invulnerable Buus (`majin_buu`, `majin_buu2`) take nothing: survive
  60 s to win.
- Knocked-back enemies slide; one pinned against a wall can be punched repeatedly (Dabura dies this way untouched).

## Skills and EP

- **Energy Blast** (everyone): 5% of max EP per shot; refires about every 26 frames (closer taps are dropped).
- **Super Saiyan** (10 EP, then a draining gauge, cooldown after reverting): stat boost.
- **Kamehameha / Kamekameha / Electric Kamehameha / Final Flash / Flame Kamehameha**: beams along your facing,
  15 / 45 / 150 EP by tier; hold B to charge and sustain; line up first (`align: true`).
- **Big Bang** (Vegeta), **Masenkoha** (Gohan, cursor), **Burning Attack** (Trunks), **Energy Punch** (Goten),
  **Super Kamikaze Ghost** / **Ultra Volleyball** (Gotenks), **Energy Sword** / **Banshee Blast** (Vegito),
  **Big Bang Kamehameha** / **Dragon Fist** (Gogeta, `steer` the head), **Instant Transmission** (40 EP).
- EP regenerates slowly (about 1-2 points per second); damaging punches give +1 EP. In SS forms skills draw on the gauge.

## Recommended play loop

1. **observe -> decide -> act in chunks of 10-600 frames.** Prefer macros with interrupts:
   `attack {stop_on: ["hurt", "danger", "phase"]}`, then look at what stopped it.
2. React to `danger`: `if_blocking: "no HP lost"` -> `block {frames: hit_in + 8}`; a shot with small `eta` -> step
   across its `vel`; a hand under you (`eta: 0`, `kind: "hand"`) -> step off sideways.
3. **Learn a boss with time travel**: `save_state`, try, `rewind {frames: 60}` and look at `state` / `ai` /
   `attacking` before the hit; `fork_session` to compare two plans side by side.
4. **Encode the pattern in a controller** once you know it, iterate with `save_state` / `load_state` /
   `run_controller`, then measure it with `evaluate_controller` over 16-32 seeds.
5. Done? `get_replay` -> `play_replay` must say `deterministic: true` (story sessions too).

## Controllers in Rhai (`run_controller`, `evaluate_controller`, `set_autopilot`)

Rhai is the game's mod language (Rust-like syntax, integers only). The **script body runs every frame** (or every
`every` frames) and its last value / `return` is the buttons to hold: `"RIGHT+A"`, `""` (nothing), or an int mask.
In scope: `obs` (the observation above as maps / arrays, plus `obs.events` of the last frame and `obs.suggest`),
`mem` (a map kept between frames), `frame`.

- `obs.suggest.attack` = what the `attack` macro would press now for the nearest enemy (path finding on the walk
  grid, turning, punch timing) - delegate approach and punching to it and add your own rules on top.
- Helpers: `toward(from, to)` / `away(from, to)` -> "UP+RIGHT"-style string, `dist(a, b)`, `btn("A+B")` -> mask,
  `names(mask)`, `log(x)` (shown in the result), `stop()` / `stop("why")`.
- Missing map keys are `()`: test with `"danger" in obs` / `mem.x == ()`. `!()` is an error: write
  `mem.a = !(mem.a == true)`.
- Sandbox: no I/O, no `eval`, no imports, no clock, no floats; each decision has an operation budget (`max_ops`,
  default 200 000; a runaway loop ends the run with `controller_error`). Deterministic: a controller gives the same
  fight on the same seed, and what is recorded is the buttons, so replays never need the script.
- `obs.danger` uses a 20-frame look-ahead in controllers (`forecast: 0` turns it off for speed).

A condensed brawler (the full `controllers/melee.rhai` won 32/32 seeds vs 4 Majin Fighters at Lv60, Dabura and
Janemba -> Super Janemba):
```rhai
let p = obs.player;
if obs.enemies.len() == 0 { return ""; }
let e = obs.enemies[0];
if !p.can_act { return if p.state == "block" && "danger" in obs { "R" } else { "" }; }
if "threats" in obs {                       // step across a shot / off a hand about to land
    for t in obs.threats {
        if "eta" in t && t.eta < 16 {
            if t.vel[0] == 0 && t.vel[1] == 0 { return if p.pos[0] > t.pos[0] { "RIGHT" } else { "LEFT" }; }
            if t.vel[0].abs() >= t.vel[1].abs() { return if p.pos[1] > t.pos[1] { "DOWN" } else { "UP" }; }
            return if p.pos[0] > t.pos[0] { "RIGHT" } else { "LEFT" };
        }
    }
}
if "melee" in e {                            // punch on every other frame, or turn first
    if e.melee == "in_range" { mem.a = !(mem.a == true); return if mem.a { "A" } else { "" }; }
    return e.melee.sub_string(5).to_upper();
}
if "danger" in obs && obs.danger.hit_in <= 6 && type_of(obs.danger.if_blocking) == "string" { return "R"; }
obs.suggest.attack                           // path to a punching spot
```
`controllers/yakon.rhai` is the window strategy above as a controller (Lv100 tier 2: 30/32 seeds; Lv80 tier 1:
9/32 - it needs EP from punches between windows). Pass files as `code: "@file:/abs/path.rhai"`.

## Enemy tips

- Groups: a punch hits everyone in its box - fight where they bunch up. Majin Fighters dodge or counter when you
  attack them (`IfPlayerAttackingMe>Chance`); block a neighbour whose `attacking.kind` is `melee`.
- Dabura: knock him into a wall and keep punching. Super Janemba: ~150 per melee hit, teleports, energy wave -
  block when `danger.if_blocking` is small. Janemba (phase 1): hands rise under you (`threats` kind `hand`), parts
  are invulnerable, `ShockStun` states ignore hits (`hits_without_damage`).
- Kid Buu splits when you keep your distance (merges after 1800 frames). Story Majin Buu: survive 60 s.
- Multi-phase presets: the next phase spawns 120 frames after the last enemy falls (`next_phase_in`;
  `attack` without a target waits for it).
