# Buu's Fury map collision

How Buu's Fury decides where actors may stand and where shots explode, and how the battle simulator uses it.
All addresses are GBA bus addresses (Thumb functions without the +1). Verified in the emulator unless marked.

Tools: `tools/battle/buu_collision.py` (extract every map, `--check` vs RAM, `--origins` composite offsets),
`tools/battle/gen_buu_collision_traces.py` (player walks into walls), `tools/battle/gen_buu_path_traces.py`
(enemy path follower). Data: `data/battle/buu_collision.json`, `buu_collision_traces.json`, `buu_path_traces.json`.
Simulator: `site/src/battle/rulesets/buu/collision.ts`, `path.ts`; test `site/tests/battle/buu_collision.test.ts`.

## 1. The three grids

The current map object lives at `0x030019A4` (camera rect at +0x18 = `0x030019BC`). It holds three bit grids of 8x8 px
tiles, each 256 x 256 tiles = 32 bytes per row (maps up to 2048 px), bit `x & 7` of byte `row*32 + (x >> 3)`,
**1 = passable**. Each is a heap block (0x2008 bytes with the allocator header).

| map object | use | filled from |
|---|---|---|
| +0xA8 (`[0x03001A4C]`) | **walk** grid: the player | map header +0x34 |
| +0xAC (`[0x03001A50]`) | **projectile** grid: shots explode on it | map header +0x38 |
| +0xB0 (`[0x03001A54]`) | **enemy** grid: a copy of the walk grid (memcpy at `0x080095C4`) on which every enemy marks its own footprint blocked | walk grid |

Map header (e.g. Southwest Forest 8-3 at `0x08240044`): +0x14 + 4*bg = BG field resources (the ones in
`data/buu/sprites.json` maps[].layers[].rom_field_resource, which is how the extractor finds a map's header),
**+0x34 walk blob, +0x38 projectile blob**, both webfoot-LZ (`gba.wf_decompress`, inflated by the IWRAM decoder straight
into the grid) of exactly 0x2000 bytes. Some maps share a blob (`0x0817A7B4` is an all-open projectile grid, e.g.
Rocky Canyon 8-70, Nataday Village 15-1). The projectile grid is often the walk grid shifted about one tile up (walls are
drawn taller than their footprint). 418 of the 450 rendered maps have a header found this way (the other 32 have no
rendered layers).

After loading, objects mark their own rectangles with `0x08009930(box, grid, value)` (sets / clears every tile under
the box, clipped to 0..2048): NPCs on the walk grid, enemies on the enemy grid. `buu_collision.py --check`: in
`buu_fight_swforest.ss` the RAM walk grid differs from the ROM blob in 16 tiles (NPCs), the enemy grid in 51
(NPCs + the four Majin Fighters), the projectile grid in none. The JSON holds the bare terrain.

## 2. The box test `0x08009324(box, grid)`

`box` = s32 x0, y0, x1, y1 in px. Returns 0 if x0 < 0, y0 < 0, x1 > 2048 or y1 > 2048; else 1 if every tile
`tx in [x0 >> 3, (x1 + 7) >> 3)`, `ty in [y0 >> 3, (y1 + 7) >> 3)` is passable (x1 / y1 exclusive: a 16 px box at
x = 8 covers tiles 1 and 2; at x = 9 tiles 1..3).

Wrappers on the map object: `0x080097C2(map, int pos, u8 size[2])` and `0x080097EA(map, 24.8 pos, size)` test
pos .. pos + size on the walk grid; `0x08009904(map, 24.8 pos, size)` the same on the enemy grid.

## 3. Actors

**Footprint.** Every actor tests the box (x, y) .. (x + w, y + h) where (x, y) = its position (obj+0x1A0 / +0x1A4,
24.8, `>> 8`) and (w, h) = the two size bytes at obj+0x19C = sprite header shadow bytes 0, 1 (Goku / Vegeta 16 x 8).
Only the feet matter: bodies overlap walls drawn above them freely.

**Player** (walk grid):
* Walk state `0x08013230` moves one axis at a time: left (else right) with the try-move flags 2, then up (else down)
  with flags 1 - diagonals slide along walls.
* Try-move `0x08012960(player, new 24.8 pos, flags)`: if the footprint at the new position fits, take it. Otherwise
  corner snapping: flags & 1 (a vertical step) and x & 7 in {0, 1, 6, 7} -> try x = (x + 4) & ~7 (and the new y);
  then flags & 2 (a horizontal step) and y & 7 in {0, 1, 6, 7} -> try y = (y + 4) & ~7 (with the x of the previous
  test). The player slides around a corner when within 2 px of the tile edge.
* Dash `0x08012DEE`: one test of pos + facing x dash speed (`0x080097EA`); blocked -> the bump state (sound 8 / 0x64,
  hurt animation, pushed back 0.75 px/frame for 23 frames). The perpendicular walk component while dashing is tested
  separately (no snapping). Knock-back / push-back: plain tests of the new position (INFERRED: other callers of
  `0x080097C2` such as `0x0801AC5A`; the bump timing is emulator-verified).
* Enemies are not on the walk grid: the player walks through enemies.

**Enemies** (enemy grid). obj+0x291 = "detached" flag:
* `0x0800F740(enemy, v)` (vfunc +0x48): if attached, mark (v = 0) / free (v = 1) the footprint's tiles on the enemy
  grid. Every enemy step frees its own tiles, tests, and marks them again at the final position, so enemies block
  each other (tile-rounded footprints), not themselves. Freeing sets the tiles passable even if they were terrain
  (two enemies sharing a tile: one leaving frees it for both).
* `0x0800F9EE(enemy, 24.8 pos)` (vfunc +0x54) / `0x0800F9C2(enemy, box)`: the enemy-grid test; a **detached enemy
  always fits** (no terrain at all).
* Detach `0x0800F924` / attach `0x0800F954` (attach also reports whether the spot was taken): called by the worm body
  `0x0802D76C` (its segments replay the head's steps without a test), the anim 0x11 grab / hug controller
  (`0x0804E53C`), the anim 0x13 controller (`0x0804E938`) and their exits (`0x0804F5CC`, `0x0804F620`).
* The steps fall back per axis: ChasePlayer's step `0x0803B934` tries the full move, else only its x part, else
  only its y part; `0x08018C3A` (the node enemy_ai.json calls StepTowardPlayer) tries the full move, else
  +-speed on x, else +-speed on y.
  `0x08018C3A` moves **away** from the player (x = x - d*speed/max(|dx|,|dy|), positive param) while facing it; the
  simulator calls it BackOffFacingPlayer. `0x0801CBA1` (StepAwayFromPlayer) is the same step facing away. Both return
  1 (node done) when none of the three tries moved, 0 when d = 0.
* Wander `0x0802B708` (+4 speed, +8 turn chance): chance -> facing = rand(4); one full step in the facing (table
  `0x08752EB4`); blocked -> facing = rand(4), no fallback. WalkToPlayer `0x0802CDE8` (+8 detour counter): no step on
  walk-animation frames 1 and 4 (obj+0x11); full step d*speed/max; blocked -> n = rand(32)-16 and sidestep `speed`
  on the minor axis |n| frames (blocked sidestep -> n = 0).

**Knockback** (KnockbackSlide `0x08018BCA`, run by the shared on-hit reaction states 2, 3, 4 (K 16 / 4 / 16) and
0xE / 0xF (K 36, the Majin Fighters') - the enemy_ai.json parameter `frames` is this K). The hit handler `0x0800FAFA`
first stores obj+0x290 = hit dir, obj+0x294 = the hit was critical (hit struct +0x10, set by ComputeDamage),
obj+0x298 = attack sub id 3.
* enter `0x08018A08`: K x4 after a critical hit ("launch"); d = hit dir ^ 1 (the enemy turns to face the attacker);
  s = (rand(64) + 96) * K >>> 7; velocity along the hit = table `0x086FDA20`[d] (-8, 8, 8, -8) * s, across it
  (rand(0x8000) - 0x4000) * s >> 12 (1/256 px per frame); hurt anim 9 unless blocking (obj+0x1C8).
* update `0x08018A92`: blocking -> guard anim 7 for 16 frames; otherwise until the hurt animation ends. Each frame free
  the footprint, move, test the footprint rect on the enemy grid (vfunc +0x50); blocked -> move back and re-mark;
  after a critical / sub-3 hit that ends the slide with a **wall splat**: a hit message (kind 1 = from the player's
  side, attack sub 3 `0x086B80A0`, mult int(409.6f) = 409, attacker = the enemy, rect = its blocked footprint) to
  everyone, the enemy itself unhittable meanwhile (obj+0x1CC = 0). A fighter hit by a splat gets obj+0x298 = 1 and can
  splat into the next wall itself (chain). The player's hurt state is separate code (the node never runs on Goku).

**Projectiles** (projectile grid): each shot's update tests its box with `0x08009324(box, [0x03001A50])` (bullet
`0x08027562` and the other classes) and explodes when blocked or outside 0..2048; beams extend to the first wall.

No slopes, ledges, water or flying layer: the grids are binary. Jumping / airborne bosses use a height offset and
detach; vampires' FlyToPlayer ignores walls.

## 4. Enemy path following

ChasePlayer (`0x0802A09F`), MoveToPoint (`0x0802E105`) and WanderToRandomPoint (`0x0803BD65`) own a path object
(0x24 bytes): +4 speed, +8 line budget (50), +0x0C/+0x10 waypoint (24.8), +0x14 valid, +0x18 re-plan counter,
+0x1C/+0x20 goal (24.8).

* enter `0x0803BB00`: counter = byte `0x030032E5`, which then becomes +1 & 7 (enemies re-plan on different frames);
  free own tiles, plan, mark.
* update `0x0803BB48`: counter 0 -> free / plan / mark; counter = (counter + 7) & 7 (a re-plan every 8 frames);
  step `0x0803B934`.
* plan `0x0803B6EA(path, enemy, goal)`: Bresenham line from the enemy position toward the goal in 1 px steps
  (+-256 on the 24.8 coordinates), at most min(major axis px, 50) - 1 steps. All free -> waypoint = goal. A blocked
  straight step -> wall-follow from the last free point in that direction; a blocked diagonal step -> try the
  straight part, then the minor-axis part, and wall-follow in the direction that failed.
* wall-follow `0x0803B4FC(path, enemy, x, y, dir, goal)`: two probes start at (x, y), side vectors (-dy, dx) and
  (dy, -dx), forward = dir. Each of 256 steps: move sideways; blocked -> undo and turn (side = -forward,
  forward = old side); free -> if pos + forward is free, stop (found). Waypoint = the probe that found a way (both or
  neither: the one nearer to the goal, integer px distance squared); returns whether one was found.
* step `0x0803B934`: d = waypoint px - position px; facing on the dominant axis (|dx| > |dy| -> right if dx > 0 else
  left, otherwise down if dy > 0 else up); v = trunc(d * speed / max(|dx|, |dy|, 1)); free own tiles, try full /
  x part / y part, mark.
* ChasePlayer: goal = the player's position (24.8, top-left of the footprint) every frame; within 2 px of it
  ((dx^2 >> 8) + (dy^2 >> 8) <= 0x400 in 24.8) it stands still (idle anim) and never finishes. MoveToPoint: goal =
  the node's map point, finished when |dx| and |dy| < 8 px after the step. WanderToRandomPoint (speed 0xB3): enter
  `0x0803BC2C` draws x = 96 + rand(294), y = 112 + rand(240) until the spot is passable and >= sqrt(80) px away.

## 5. Map composites vs the world grid

The map PNGs (`site/public/assets/buu/maps/*/composite.png`) are aligned to the world grid on every battle arena but
two: the World Tournament stadium 7-6 (composite x = world x - 16) and HFIL 18-14 (composite x = world x + 5).
`buu_collision.py --origins` measures this per map: warp in, read the camera at `0x030019BC` (world px of screen
0,0), find the screen in the composite; the JSON stores `origin` = world position of the composite's top-left.

## 6. Verification

* `buu_collision.test.ts`: 7 emulator walks (Giant Crater left / top / right / bottom cliffs incl. diagonals and
  corner snapping, a dash into the right cliff, Rocky Canyon rocks, Demon World stone ring; 70..212 frames each):
  the simulator's 24.8 position equals the game's on every frame. 3 emulator runs of the path follower in Southwest
  Forest (goals behind trees, 300..420 frames, 12-13 waypoints each): same waypoint and position every frame.
* Data: header found for 418 maps; RAM grids vs ROM blobs as in §1.

## 7. Simulator model and gaps

* `collision.ts` keeps the walk + projectile grids of the arena map and a private enemy grid; `Actor.fits` = arena
  bounds (the bounding box of the walkable region around the arena's spawn point) and the grid test. Player: walk
  grid with `0x08012960`; enemies: enemy grid with marks (freeing sets the tiles passable like the game, holes in
  walls under an enemy that stood on them included); projectiles: projectile grid.
* Detached (INFERRED mapping of the controllers): Janemba body parts, worm segments, the enemy holding the player,
  airborne enemies (height offset), defeated enemies.
* Objects that mark tiles at load in the game (NPCs, Broly's rocks are separate entities here, breakables) are not
  placed; the grids are bare terrain.
* Also verified (`gen_buu_step_traces.py`, 5 runs, 240..600 frames, node update called per frame, RNG state per
  frame): back-off (2 runs), step-away, wander, walk-to: position, facing, return value and detour counter.
* Knockback (`gen_buu_knockback_traces.py`, 11 cases caught call by call: punches in the 4 directions, an Energy
  Blast, punches next to / into the tree trunk (guarding fighters), critical punches in the open and into the trunk,
  a critical punch whose wall splat hits a second fighter): velocity, every position, slide length, splat damage.
* Not traced against the emulator: projectile explosions on the projectile grid, AlignWithPlayer, CirclePlayer, the
  player's own hurt knock-back.
