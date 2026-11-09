# Coverage: what never appears in a frame compared with the game

Made by `tools/coverage/run.sh` + `tools/coverage/report.py` (do not edit by hand; the numbers are in `data/coverage.json`). Every gated comparison is replayed with a recorder that only counts (`WF_COVERAGE`, `engine/crates/wf-view/src/coverage.rs`), and what it saw is set against the game's content.

An item's level is the strongest comparison it appears in:

* **picture** - in a picture a gate compares with the game's: the pixel lock's frames, the menu scenarios, the link recordings;
* **state** - in a replay a gate compares with the game's state every frame, without a picture of it: the segment gate's matched frames, the three whole runs (`tools/wholerun/check.sh`), the battle lockstep traces;
* **audit** - only in the by-hand opponent / skill audits in `work/` (every frame against the game, not gated here);
* **board** - only on the picture board, in a sampled frame that is not locked (it differs, or its segment has another differing frame);
* **never** - in no frame compared with the game.

"Met" = present in at least one logic iteration of the three recorded whole playthroughs; the lists are ordered by that count (iterations present), the measure of how often a player meets a thing.

Levels in this report: audit, board, link, locked, lockstep, menu, run, state.

## Summary

Share with a **picture** comparison, before -> after this pass (of everything in the content, and of what the whole runs meet):

| Category | In the content | Picture before | Picture now | Picture or state now | Met in the runs | Picture, of met: before | now | Never compared |
|---|---|---|---|---|---|---|---|---|
| native | 183 | 146/183 (80%) | 150/183 (82%) | 152/183 (83%) | 152 | 146/152 (96%) | 150/152 (99%) | 31 |
| effect sheet | 312 | 30/312 (10%) | 31/312 (10%) | 59/312 (19%) | 32 | 29/32 (91%) | 30/32 (94%) | 251 |
| prop sheet | 209 | 144/209 (69%) | 197/209 (94%) | 201/209 (96%) | 163 | 136/163 (83%) | 160/163 (98%) | 8 |
| sprite sheet | 297 | 209/297 (70%) | 244/297 (82%) | 257/297 (87%) | 240 | 208/240 (87%) | 239/240 (100%) | 34 |
| enemy | 150 | 91/150 (61%) | 112/150 (75%) | 119/150 (79%) | 110 | 89/110 (81%) | 109/110 (99%) | 26 |
| enemy AI state | 2319 | 613/2319 (26%) | 796/2319 (34%) | 957/2319 (41%) | 857 | 597/857 (70%) | 760/857 (89%) | 1269 |
| form | 54 | 27/54 (50%) | 34/54 (63%) | 54/54 (100%) | 37 | 26/37 (70%) | 33/37 (89%) | 0 |
| skill (per form) | 123 | 11/123 (9%) | 16/123 (13%) | 122/123 (99%) | 13 | 8/13 (62%) | 13/13 (100%) | 0 |
| skill | 23 | 2/23 (9%) | 3/23 (13%) | 16/23 (70%) | 2 | 1/2 (50%) | 2/2 (100%) | 7 |
| transformation | 38 | 0/38 (0%) | 1/38 (3%) | 15/38 (39%) | 1 | 0/1 (0%) | 1/1 (100%) | 23 |
| area | 452 | 295/452 (65%) | 438/452 (97%) | 441/452 (98%) | 347 | 281/347 (81%) | 344/347 (99%) | 11 |
| screen | 27 | 26/27 (96%) | 26/27 (96%) | 26/27 (96%) | 18 | 18/18 (100%) | 18/18 (100%) | 1 |
| player state | 10 | 9/10 (90%) | 10/10 (100%) | 10/10 (100%) | 10 | 9/10 (90%) | 10/10 (100%) | 0 |
| screen part | 15 | 15/15 (100%) | 15/15 (100%) | 15/15 (100%) | 15 | 15/15 (100%) | 15/15 (100%) | 0 |
| object (shots, effects objects) | 39 | 18/39 (46%) | 19/39 (49%) | 37/39 (95%) | 24 | 18/24 (75%) | 19/24 (79%) | 0 |
| held-by-enemy state | 2 | 0/2 (0%) | 1/2 (50%) | 2/2 (100%) | 1 | 0/1 (0%) | 1/1 (100%) | 0 |
| script visual | 7 | 6/7 (86%) | 7/7 (100%) | 7/7 (100%) | 7 | 6/7 (86%) | 7/7 (100%) | 0 |

## New on the picture board in this pass

201 scenes, 25,561 of 30,101 sampled frames exact; the exact frames are locked in `data/segments/pixels_coverage.json` (25,561 frames of 198 scenes, read by the pixel gate next to the main lock) - **to add to `data/segments/pixels.json` when it is re-made** (their screen dumps are in `work/superbuu/pix/fb/`).

* `cov_*` (38) and `cov2_*` (43): cut from the three whole runs at the row where an uncovered thing first occurs (`tools/coverage/cuts.py`, a picture every 10th frame; `source` names the run and row); each is also on the state gate (`expect.matched`).
* `tour_*` (120 of 121): the field tours, on the state gate before but never dumped (`tools/coverage/dump.py`, every 10th frame).

### Known differences in the new scenes

Every scene with a differing sampled frame: its first one (video frame, the board's class, pixels), how many differ, and what it is where it was looked at.

| Scene | Exact / sampled | First differing frame | Differing frames by class | What |
|---|---|---|---|---|
| cov_spopovich | 24 / 136 | f0 background (BG layers) (41 px) | HUD 69, background (BG layers) 32, text box / dialog 8, sprites (OBJ) 3 | World Tournament, Spopovich: the boss bar on most frames, a BG row on alternate frames (the HUD / boss-bar class) |
| cov2_spopovich_r2 | 20 / 81 | f80 sprites (OBJ) (5 px) | HUD 58, sprites (OBJ) 3 | World Tournament, Spopovich (run 2): the boss bar's end (the HUD helper's class) |
| cov2_gunman2 | 23 / 81 | f0 whole screen (palette / fade / BG scroll) (28446 px) | whole screen (palette / fade / BG scroll) 54, text box / dialog 4 | HFIL (Van Zant and Smitty): the portrait text box stands at the top of the screen in the game, ours draws it lower and the two men above it (the text-box helper's class) |
| cov2_lich_states | 26 / 81 | f260 HUD (76 px) | HUD 52, sprites (OBJ) 3 | HFIL, Liches: a few HUD pixels at the stat mark on 52 sampled frames (the HUD helper's class) |
| cov_enzyme | 9 / 61 | f0 HUD (10 px) | sprites (OBJ) 42, HUD 10 | Inside Buu 21-x, Enzymes (the most met enemy state): their walk's cel phase (the import re-starts the walk, the game's runs on: harness cel check f1), a latched Enzyme drawn over the player where the game hides it (f100), a hit flash / damage digit a frame apart (f190) |
| cov2_broly_r2 | 22 / 63 | f30 background (BG layers) (1598 px) | sprites (OBJ) 39, background (BG layers) 1, HUD 1 | Broly (run 2): a small piece under the player on 39 sampled frames (not looked into further) |
| cov2_mechandoid_ball | 42 / 81 | f50 sprites (OBJ) (4 px) | sprites (OBJ) 27, HUD 6, background (BG layers) 6 | a Power Mechandoid's ball: its rolling cel differs on single frames; HUD frames in a fade |
| cov2_ship50 | 43 / 81 | f0 sprites (OBJ) (14 px) | sprites (OBJ) 28, HUD 7, background (BG layers) 3 |  |
| cov2_cyborg | 44 / 81 | f0 HUD (868 px) | HUD 37 |  |
| cov_tourn_human | 55 / 91 | f0 HUD (1279 px) | HUD 36 |  |
| cov_grapple | 60 / 94 | f0 HUD (680 px) | HUD 34 | a Cyborg's grapple (effect fx_0876dff8): the state comparison stops at f932, the grab's first frame - the game takes a 28-tile block of object tiles a frame before ours and the 4-tile object first (tile allocator order); pictures before it differ in the HUD only |
| cov_ship49 | 33 / 61 | f140 sprites (OBJ) (28 px) | sprites (OBJ) 26, HUD 2 | Babidi's ship: a death cloud's cel / place for single frames (f140 ..) |
| cov_hfil2_living_dead | 64 / 91 | f470 sprites (OBJ) (36 px) | sprites (OBJ) 21, text box / dialog 5, HUD 1 | HFIL 18-2, Living Dead: a dying enemy's cloud drawn over a coin (f470; the sprite-tail helper's class) |
| cov_hfil7_annihilator | 64 / 91 | f160 HUD (63 px) | HUD 14, sprites (OBJ) 10, background (BG layers) 3 |  |
| cov_videl | 34 / 61 | f0 text box / dialog (544 px) | text box / dialog 24, sprites (OBJ) 3 | Videl (form 52) at the tournament: text box frames while it types |
| cov2_props_11_20 | 56 / 81 | f0 background (BG layers) (95 px) | background (BG layers) 23, sprites (OBJ) 2 |  |
| cov2_ship33 | 56 / 81 | f0 text box / dialog (182 px) | sprites (OBJ) 23, text box / dialog 1, HUD 1 |  |
| cov2_stadium74 | 58 / 81 | f0 text box / dialog (88 px) | background (BG layers) 16, text box / dialog 3, sprites (OBJ) 2, HUD 1, flat screen (fade / black / load) 1 |  |
| cov2_broly | 59 / 81 | f140 HUD (65 px) | sprites (OBJ) 16, HUD 4, background (BG layers) 2 |  |
| cov2_spopovich_yamu | 59 / 81 | f280 HUD (2 px) | sprites (OBJ) 13, HUD 9 | World Tournament, Spopovich / Yamu: Yamu's shadow (fixed here, as Dabura's); left: a piece of Spopovich's grab pose and HUD frames |
| cov2_ship46 | 61 / 81 | f0 sprites (OBJ) (32 px) | sprites (OBJ) 18, HUD 2 |  |
| cov2_blood_cell | 62 / 81 | f0 background (BG layers) (1487 px) | background (BG layers) 12, HUD 6, sprites (OBJ) 1 |  |
| cov2_ghost_fighter | 63 / 81 | f0 sprites (OBJ) (26 px) | sprites (OBJ) 18 |  |
| cov2_scene_0_22 | 63 / 81 | f0 HUD (480 px) | HUD 12, background (BG layers) 3, sprites (OBJ) 3 |  |
| cov_superball | 44 / 61 | f0 sprites (OBJ) (20 px) | sprites (OBJ) 17 | Super Buu's Super Ball: f20 - f30 ours shows the ball where the game shows him standing, f40 - f160 the stars over his head a few pixels off |
| cov2_ship69 | 64 / 81 | f0 HUD (480 px) | HUD 17 |  |
| cov2_ship30 | 66 / 81 | f0 sprites (OBJ) (8 px) | sprites (OBJ) 15 |  |
| cov_ship62 | 47 / 61 | f60 sprites (OBJ) (77 px) | sprites (OBJ) 13, HUD 1 |  |
| cov_zmart | 47 / 61 | f130 whole screen (palette / fade / BG scroll) (24172 px) | whole screen (palette / fade / BG scroll) 14 | Z-Mart 0-92: the shopkeeper's Buy / Sell / Exit choice box (f130 .. f260) - ours shows an empty text box in another place and hides the area banner |
| cov2_namek_elder | 67 / 81 | f380 sprites (OBJ) (8 px) | sprites (OBJ) 14 |  |
| cov2_pikkon_state | 67 / 81 | f40 background (BG layers) (618 px) | HUD 12, background (BG layers) 2 |  |
| cov_dabura | 78 / 91 | f0 sprites (OBJ) (120 px) | sprites (OBJ) 9, HUD 4 | Dabura in Babidi's ship: the HUD under a text box / the boss bar (the HUD helper's class) |
| cov_vegeta29_blast | 18 / 31 | f100 sprites (OBJ) (10 px) | sprites (OBJ) 8, HUD 5 | Super Saiyan 2 Vegeta's Energy Blast in HFIL 18-2: HUD frames |
| cov2_kid_buu | 50 / 63 | f0 sprites (OBJ) (435 px) | sprites (OBJ) 12, HUD 1 |  |
| cov_vegeta27_blast | 26 / 38 | f0 HUD (15 px) | sprites (OBJ) 11, HUD 1 |  |
| cov_west_kai | 64 / 76 | f420 sprites (OBJ) (6 px) | sprites (OBJ) 12 |  |
| cov2_kamikaze_ghost | 69 / 81 | f160 sprites (OBJ) (1154 px) | sprites (OBJ) 12 |  |
| cov2_ship27_puipui | 70 / 81 | f0 sprites (OBJ) (2862 px) | sprites (OBJ) 8, HUD 3 |  |
| cov_hfil4_nosferatu | 105 / 114 | f560 sprites (OBJ) (144 px) | sprites (OBJ) 7, HUD 2 | HFIL 18-4, Nosferatu: exact pictures to f550; the state comparison stops at f1162 on the order of two small tile blocks (a 2-tile and a 1-tile object) |
| cov2_shell_brute | 54 / 63 | f0 HUD (3 px) | sprites (OBJ) 8, HUD 1 |  |
| cov_vegeta26_blast | 33 / 41 | f0 HUD (149 px) | sprites (OBJ) 7, HUD 1 | Vegeta's Energy Blast (form 26): a blast's spark for single frames |
| cov2_hfil3_poltergeist | 54 / 62 | f0 background (BG layers) (134 px) | sprites (OBJ) 6, background (BG layers) 2 | HFIL 18-3, Poltergeists: the recording's last rows stand inside an area load (matched 612 of 620) |
| cov_ow_stadium | 54 / 61 | f230 sprites (OBJ) (6 px) | sprites (OBJ) 6, HUD 1 |  |
| cov2_janemba | 10 / 17 | f0 sprites (OBJ) (4369 px) | HUD 4, sprites (OBJ) 2, background (BG layers) 1 | Janemba (run 2): the state comparison stops at f168 - one of his parts (sprite 147) 2 px lower in the game; pictures before it: the boss bar's class |
| cov2_scene_0_10 | 74 / 81 | f620 sprites (OBJ) (2 px) | sprites (OBJ) 6, background (BG layers) 1 |  |
| cov_gohan_form18 | 55 / 61 | f0 sprites (OBJ) (350 px) | sprites (OBJ) 6 |  |
| cov_grandkai13 | 55 / 61 | f0 HUD (6 px) | sprites (OBJ) 5, HUD 1 |  |
| cov_trunks25 | 55 / 61 | f180 sprites (OBJ) (71 px) | sprites (OBJ) 5, background (BG layers) 1 |  |
| cov2_forest11 | 75 / 81 | f150 HUD (3 px) | HUD 5, sprites (OBJ) 1 |  |
| cov2_scene_0_12 | 75 / 81 | f130 sprites (OBJ) (8 px) | sprites (OBJ) 5, background (BG layers) 1 |  |
| cov2_majin_soldier | 77 / 81 | f270 sprites (OBJ) (30 px) | HUD 3, sprites (OBJ) 1 |  |
| cov2_ship54 | 77 / 81 | f50 sprites (OBJ) (53 px) | sprites (OBJ) 4 |  |
| cov_majin_vegeta | 193 / 196 | f1510 sprites (OBJ) (30 px) | sprites (OBJ) 2, HUD 1 |  |
| cov_tourn_hercule | 58 / 61 | f200 HUD (38 px) | HUD 3 |  |
| cov_tourn_idasa | 88 / 91 | f80 HUD (3499 px) | HUD 3 |  |
| cov_unequip | 28 / 31 | f20 sprites (OBJ) (6 px) | sprites (OBJ) 3 | the unequip_slot scene (9-15): the three frames left are the text box's |
| cov2_hfil91 | 78 / 81 | f440 sprites (OBJ) (6 px) | sprites (OBJ) 3 |  |
| cov_grandkai14 | 59 / 61 | f370 sprites (OBJ) (7 px) | sprites (OBJ) 2 |  |
| cov_ship67 | 59 / 61 | f430 background (BG layers) (253 px) | background (BG layers) 1, sprites (OBJ) 1 |  |
| cov_vegeta28 | 74 / 76 | f700 sprites (OBJ) (6 px) | sprites (OBJ) 2 | Rocky Canyon 8-70, Majin Vegeta's scene: Dabura's shadow (fixed here: a script-spawned actor's shadow offset is signed); the two frames left are single-frame sprite pieces |
| cov2_puipui_fight | 26 / 28 | f170 sprites (OBJ) (13 px) | sprites (OBJ) 2 | PuiPui in Babidi's ship: the state comparison stops at f273 on an 8-tile block of object tiles the game has taken and ours has not yet |
| cov2_pyramid12 | 79 / 81 | f180 HUD (46 px) | HUD 1, sprites (OBJ) 1 |  |
| cov2_pyramid23 | 79 / 81 | f0 sprites (OBJ) (85 px) | sprites (OBJ) 1, HUD 1 |  |
| cov2_ship35 | 61 / 63 | f0 sprites (OBJ) (22 px) | sprites (OBJ) 2 |  |
| cov_gotenks_blast | 30 / 31 | f0 sprites (OBJ) (243 px) | sprites (OBJ) 1 |  |
| cov_hfil_vampire_hold | 60 / 61 | f130 sprites (OBJ) (23 px) | sprites (OBJ) 1 |  |
| cov_hfil13 | 60 / 61 | f70 HUD (87 px) | HUD 1 |  |
| cov_sleep_after | 30 / 31 | f299 HUD (18 px) | HUD 1 | the scene after the sleep; last frame's HUD under the text box |
| cov_vegeta_ss2 | 60 / 61 | f530 HUD (2 px) | HUD 1 |  |
| cov_sleep_mode | 92 / 92 | - |  | the Options' sleep (native sleep_mode): exact to f914; from there the console sleeps inside one logic iteration (the logic counter stays at 409,066 for 31 video frames) - lag, not recreated - and the text box opens only after it |
| tour_desert_py11 | 606 / 965 | f200 sprites (OBJ) (5 px) | HUD 285, sprites (OBJ) 70, background (BG layers) 4 |  |
| tour_desert_exp24 | 21 / 237 | f0 sprites (OBJ) (123 px) | background (BG layers) 160, sprites (OBJ) 55, text box / dialog 1 | the Diablo Desert Express (the countdown helper's scene) |
| tour_world_sub_rng3 | 62 / 202 | f0 background (BG layers) (70 px) | background (BG layers) 95, sprites (OBJ) 35, HUD 10 | the world map (the world-map helper's scene) |
| tour_kami_papaya2 | 532 / 654 | f230 HUD (21 px) | text box / dialog 59, HUD 45, sprites (OBJ) 18 |  |
| tour_desert_d | 428 / 544 | f160 HUD (3 px) | sprites (OBJ) 108, HUD 8 |  |
| tour_world_sub_fight | 88 / 171 | f0 background (BG layers) (70 px) | background (BG layers) 58, sprites (OBJ) 22, HUD 3 | the world map (the world-map helper's scene) |
| tour_castle_forest_mid | 205 / 284 | f0 HUD (1401 px) | sprites (OBJ) 62, HUD 15, background (BG layers) 1, text box / dialog 1 |  |
| tour_desert_a | 481 / 560 | f480 sprites (OBJ) (177 px) | sprites (OBJ) 68, HUD 9, text box / dialog 2 |  |
| tour_kami_roshi | 179 / 258 | f0 background (BG layers) (252 px) | background (BG layers) 61, HUD 14, sprites (OBJ) 4 | the world map (the world-map helper's scene) |
| tour_desert_nj36 | 383 / 455 | f410 sprites (OBJ) (36 px) | sprites (OBJ) 54, HUD 18 |  |
| tour_castle_forest_16 | 642 / 701 | f470 sprites (OBJ) (40 px) | sprites (OBJ) 42, text box / dialog 16, HUD 1 |  |
| tour_earth_east_a | 283 / 342 | f0 sprites (OBJ) (21 px) | sprites (OBJ) 46, HUD 10, background (BG layers) 2, text box / dialog 1 |  |
| tour_castle_forest_n | 240 / 298 | f0 sprites (OBJ) (278 px) | sprites (OBJ) 42, HUD 16 |  |
| tour_kais_summit | 49 / 107 | f0 sprites (OBJ) (356 px) | sprites (OBJ) 37, flat screen (fade / black / load) 13, HUD 8 |  |
| tour_castle_forest_16b | 6 / 58 | f0 background (BG layers) (338 px) | HUD 45, sprites (OBJ) 6, background (BG layers) 1 |  |
| tour_desert_e | 334 / 386 | f90 sprites (OBJ) (70 px) | sprites (OBJ) 50, HUD 2 |  |
| tour_kais_nataday | 355 / 403 | f0 background (BG layers) (184 px) | sprites (OBJ) 22, HUD 19, background (BG layers) 7 |  |
| tour_castle_forest_w | 293 / 340 | f470 sprites (OBJ) (3 px) | sprites (OBJ) 45, HUD 1, background (BG layers) 1 |  |
| tour_castle_ship_26 | 0 / 47 | f0 sprites (OBJ) (2862 px) | sprites (OBJ) 28, HUD 18, text box / dialog 1 | Babidi's ship 8-26: a door (map prop 7) the game has opened is drawn shut - its prop object is gone from the game's list; hiding every prop without an object was tried and lost 580 board frames |
| tour_castle_pw_10_6 | 705 / 751 | f0 HUD (422 px) | sprites (OBJ) 39, HUD 7 |  |
| tour_desert_air27 | 0 / 46 | f0 HUD (757 px) | HUD 31, background (BG layers) 14, sprites (OBJ) 1 |  |
| tour_desert_c | 307 / 353 | f190 sprites (OBJ) (34 px) | sprites (OBJ) 40, HUD 6 |  |
| tour_castle_pilaf_w | 738 / 783 | f0 sprites (OBJ) (305 px) | sprites (OBJ) 40, HUD 5 |  |
| tour_castle_ship_28 | 34 / 78 | f80 HUD (143 px) | sprites (OBJ) 41, HUD 2, text box / dialog 1 |  |
| tour_castle_ship_17 | 81 / 123 | f20 sprites (OBJ) (10 px) | sprites (OBJ) 39, HUD 3 |  |
| tour_castle_ship_42 | 117 / 157 | f0 sprites (OBJ) (396 px) | sprites (OBJ) 40 |  |
| tour_desert_nj53 | 151 / 190 | f340 sprites (OBJ) (1 px) | sprites (OBJ) 31, background (BG layers) 6, HUD 2 |  |
| tour_desert_a7 | 215 / 253 | f340 HUD (5 px) | sprites (OBJ) 35, HUD 3 |  |
| tour_castle_pw_10_7 | 658 / 695 | f0 background (BG layers) (783 px) | sprites (OBJ) 23, background (BG layers) 10, HUD 4 |  |
| tour_castle_pw_10_8 | 643 / 680 | f20 sprites (OBJ) (722 px) | sprites (OBJ) 33, HUD 4 |  |
| tour_kais_lookheaven | 158 / 195 | f40 background (BG layers) (3283 px) | background (BG layers) 18, HUD 11, sprites (OBJ) 8 | as tour_kais_pad191 (f40 ..) |
| tour_castle_ship_38 | 12 / 48 | f0 sprites (OBJ) (2862 px) | sprites (OBJ) 19, HUD 16, text box / dialog 1 | as tour_castle_ship_26 |
| tour_castle_ship_22 | 143 / 174 | f230 sprites (OBJ) (12 px) | sprites (OBJ) 31 |  |
| tour_desert_nj45 | 40 / 71 | f260 sprites (OBJ) (2 px) | sprites (OBJ) 29, HUD 2 |  |
| tour_castle_pw_10_10 | 590 / 620 | f20 sprites (OBJ) (465 px) | sprites (OBJ) 28, HUD 2 |  |
| tour_kais_hfil11 | 97 / 127 | f260 HUD (8 px) | sprites (OBJ) 16, flat screen (fade / black / load) 12, HUD 2 |  |
| tour_castle_pw_10_9 | 632 / 661 | f360 HUD (1964 px) | sprites (OBJ) 25, HUD 4 |  |
| tour_desert_air1b | 0 / 28 | f0 HUD (650 px) | HUD 28 |  |
| tour_castle_pw_10_16 | 567 / 594 | f0 HUD (288 px) | sprites (OBJ) 24, HUD 3 |  |
| tour_desert_py25 | 73 / 99 | f160 sprites (OBJ) (168 px) | sprites (OBJ) 25, HUD 1 |  |
| tour_kais_pad191 | 17 / 43 | f60 background (BG layers) (3418 px) | background (BG layers) 26 | the planet-select screen of a teleport pad: the destination's name bar at the bottom is missing in ours (f60 ..) |
| tour_desert_a8 | 189 / 214 | f130 HUD (3 px) | sprites (OBJ) 24, HUD 1 |  |
| tour_castle_ship_18 | 57 / 81 | f80 HUD (36 px) | sprites (OBJ) 15, background (BG layers) 5, HUD 4 |  |
| tour_earth_zmart_thieves | 185 / 208 | f310 HUD (5 px) | whole screen (palette / fade / BG scroll) 22, HUD 1 | as cov_zmart: the shop's choice box (f310 ..) |
| tour_kami_lookout20 | 220 / 243 | f870 flat screen (fade / black / load) (20258 px) | flat screen (fade / black / load) 12, sprites (OBJ) 11 | a save point: the Save Game screen with its 'Saving...' box stays up for the save's long iteration (12 sampled frames from f870), ours is already black - lag, not recreated |
| tour_castle_den_zmart95 | 82 / 104 | f400 whole screen (palette / fade / BG scroll) (24116 px) | whole screen (palette / fade / BG scroll) 22 |  |
| tour_desert_e5 | 181 / 203 | f140 HUD (6 px) | sprites (OBJ) 18, HUD 4 |  |
| tour_kais_waste | 48 / 69 | f470 background (BG layers) (379 px) | background (BG layers) 19, HUD 2 |  |
| tour_desert_b | 303 / 323 | f1070 sprites (OBJ) (16 px) | sprites (OBJ) 17, HUD 3 |  |
| tour_desert_nj39 | 63 / 83 | f310 HUD (3 px) | sprites (OBJ) 19, HUD 1 |  |
| tour_kais_stomach | 36 / 55 | f100 sprites (OBJ) (8 px) | flat screen (fade / black / load) 12, HUD 5, sprites (OBJ) 2 |  |
| tour_castle_pilaf_e | 495 / 513 | f670 HUD (3 px) | HUD 13, sprites (OBJ) 5 |  |
| tour_castle_ship_66 | 27 / 45 | f90 HUD (113 px) | HUD 10, sprites (OBJ) 8 |  |
| tour_earth_city | 225 / 243 | f100 HUD (30 px) | sprites (OBJ) 11, HUD 7 |  |
| tour_kais_hfil5 | 43 / 61 | f0 background (BG layers) (17716 px) | HUD 10, sprites (OBJ) 5, background (BG layers) 2, whole screen (palette / fade / BG scroll) 1 |  |
| tour_kais_volc25 | 78 / 96 | f10 sprites (OBJ) (571 px) | sprites (OBJ) 18 |  |
| tour_kami_snakeway | 187 / 205 | f740 text box / dialog (10 px) | text box / dialog 9, sprites (OBJ) 6, HUD 3 |  |
| tour_desert_nj42 | 43 / 60 | f200 sprites (OBJ) (126 px) | sprites (OBJ) 17 |  |
| tour_desert_py5 | 66 / 83 | f440 sprites (OBJ) (266 px) | sprites (OBJ) 17 |  |
| tour_kais_volc25b | 52 / 69 | f0 sprites (OBJ) (157 px) | sprites (OBJ) 17 |  |
| tour_castle_ship_41 | 42 / 58 | f90 sprites (OBJ) (49 px) | sprites (OBJ) 15, text box / dialog 1 |  |
| tour_castle_forest_late | 72 / 87 | f110 sprites (OBJ) (66 px) | sprites (OBJ) 15 |  |
| tour_earth_east_late | 226 / 241 | f70 HUD (10 px) | HUD 15 |  |
| tour_kami_grandkai | 292 / 307 | f50 flat screen (fade / black / load) (20240 px) | flat screen (fade / black / load) 12, HUD 2, text box / dialog 1 |  |
| tour_castle_ship_21 | 13 / 27 | f110 sprites (OBJ) (69 px) | sprites (OBJ) 14 |  |
| tour_castle_ship_53 | 24 / 38 | f70 sprites (OBJ) (2 px) | sprites (OBJ) 14 |  |
| tour_castle_ship_19 | 73 / 86 | f40 HUD (12 px) | sprites (OBJ) 12, HUD 1 |  |
| tour_kais_intest | 31 / 44 | f210 flat screen (fade / black / load) (20304 px) | flat screen (fade / black / load) 13 |  |
| tour_castle_pw_10_17 | 513 / 525 | f0 HUD (121 px) | sprites (OBJ) 10, HUD 2 |  |
| tour_earth_house | 153 / 165 | f910 sprites (OBJ) (460 px) | sprites (OBJ) 12 |  |
| tour_kais_hfil9 | 63 / 75 | f160 sprites (OBJ) (8 px) | sprites (OBJ) 11, HUD 1 |  |
| tour_kais_north | 103 / 115 | f410 sprites (OBJ) (1 px) | sprites (OBJ) 9, HUD 3 |  |
| tour_kais_volc19 | 34 / 46 | f210 flat screen (fade / black / load) (20227 px) | flat screen (fade / black / load) 12 |  |
| tour_kami_grandkai11 | 242 / 254 | f80 sprites (OBJ) (3 px) | HUD 7, text box / dialog 3, sprites (OBJ) 2 |  |
| tour_kami_htc | 167 / 179 | f0 background (BG layers) (1151 px) | background (BG layers) 8, HUD 4 |  |
| tour_kami_lookout | 368 / 378 | f910 sprites (OBJ) (640 px) | sprites (OBJ) 7, HUD 3 |  |
| tour_castle_ship_52 | 15 / 24 | f80 sprites (OBJ) (108 px) | sprites (OBJ) 9 |  |
| tour_castle_ship_61 | 15 / 24 | f80 sprites (OBJ) (107 px) | sprites (OBJ) 9 |  |
| tour_desert_nj33 | 19 / 28 | f70 sprites (OBJ) (106 px) | sprites (OBJ) 8, background (BG layers) 1 |  |
| tour_desert_nj37 | 43 / 52 | f90 sprites (OBJ) (10 px) | sprites (OBJ) 9 |  |
| tour_earth_mansion | 89 / 98 | f180 HUD (48 px) | sprites (OBJ) 6, HUD 3 |  |
| tour_kami_palace | 270 / 279 | f470 HUD (6 px) | HUD 8, sprites (OBJ) 1 |  |
| tour_earth_2_20 | 78 / 86 | f330 HUD (2 px) | sprites (OBJ) 7, HUD 1 |  |
| tour_capsule | 467 / 474 | f1730 HUD (214 px) | HUD 3, background (BG layers) 3, sprites (OBJ) 1 |  |
| tour_castle_ship_48 | 18 / 25 | f0 HUD (10 px) | sprites (OBJ) 6, HUD 1 |  |
| tour_kais_volc26 | 40 / 47 | f190 HUD (10 px) | sprites (OBJ) 6, HUD 1 |  |
| tour_castle_pilaf_gate | 10 / 16 | f70 sprites (OBJ) (116 px) | sprites (OBJ) 6 |  |
| tour_castle_ship_57 | 18 / 24 | f80 sprites (OBJ) (108 px) | sprites (OBJ) 6 |  |
| tour_kais_planet | 106 / 112 | f540 HUD (66 px) | sprites (OBJ) 4, HUD 2 |  |
| tour_kais_volc21 | 56 / 62 | f460 sprites (OBJ) (176 px) | sprites (OBJ) 6 |  |
| tour_castle_den_town_b | 291 / 296 | f320 background (BG layers) (618 px) | HUD 2, sprites (OBJ) 2, background (BG layers) 1 |  |
| tour_desert_nj38 | 81 / 86 | f0 HUD (10 px) | sprites (OBJ) 4, HUD 1 |  |
| tour_kami_cave | 63 / 68 | f430 sprites (OBJ) (1001 px) | sprites (OBJ) 4, HUD 1 |  |
| tour_earth_zmart_city | 48 / 52 | f60 background (BG layers) (107 px) | background (BG layers) 3, HUD 1 |  |
| tour_kami_lookout_b | 165 / 169 | f230 HUD (16 px) | HUD 3, sprites (OBJ) 1 |  |
| tour_castle_pilaf_11 | 58 / 61 | f0 sprites (OBJ) (305 px) | HUD 2, sprites (OBJ) 1 |  |
| tour_castle_pilaf_save14 | 22 / 25 | f160 text box / dialog (76 px) | sprites (OBJ) 2, text box / dialog 1 |  |
| tour_desert_nj40 | 35 / 38 | f20 sprites (OBJ) (10 px) | sprites (OBJ) 2, HUD 1 |  |
| tour_kais_waste4 | 170 / 173 | f250 HUD (2850 px) | HUD 3 |  |
| tour_castle_den_east | 178 / 180 | f10 sprites (OBJ) (45 px) | sprites (OBJ) 2 |  |
| tour_castle_den_town | 79 / 81 | f690 HUD (18 px) | HUD 1, text box / dialog 1 |  |
| tour_desert_air1 | 44 / 46 | f50 sprites (OBJ) (16 px) | sprites (OBJ) 2 |  |
| tour_earth_school_early | 254 / 256 | f1600 HUD (8 px) | HUD 2 |  |
| tour_kami_yemma | 270 / 272 | f280 HUD (36 px) | HUD 1, sprites (OBJ) 1 |  |
| tour_castle_den_8 | 130 / 131 | f480 HUD (20 px) | HUD 1 |  |
| tour_castle_pw_10_25 | 183 / 184 | f90 sprites (OBJ) (49 px) | sprites (OBJ) 1 |  |
| tour_earth_school | 226 / 227 | f1500 HUD (9 px) | HUD 1 |  |

### Tried and not on the board

| What | Why |
|---|---|
| open_shop_sell (the sell shop opened by a script; run 2 row 140,529) | five cuts tried (rows 139,600 .. 140,590): the import stops at its first frame on the shop scene's object tiles (three blocks ours has taken and the game has not), or 101 frames in on an NPC's animation; the Sell screen itself is covered by the menu scenarios |
| Super Buu, enemy type 120 (run 1 row 1,357,782) | the cut replays in step for 800 frames but no sampled frame is exact (a text box, a BG row or the boss bar on every one): left off the board, the boss-bar helper's class |
| Pilaf's Guardian's later states (run 1 row 755,436) | the cut leaves the game's actors after 80 frames; not looked into |
| props of 11-24 (run 1 row 795,093) | every sampled frame differs in a text box (123 px): left off the board, the text-box helper's class |
| Babidi's ship 8-24 (run 1 row 455,076), the opening's 0-4 / 0-5 (rows 1,070 / 1,280) | cuts inside a scene's load: the import matches 11 - 18 frames; the areas are met for a few seconds only |

## Still without a picture comparison, most met first

Items a player meets in the whole runs that no gated picture shows (level in brackets; where it can be found).

### native (2)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| 0xcf open_shop_sell (6 uses in the scripts) | 5 | state | run run2@0 row 140,529 |
| 0xd4 sleep_mode (1 uses in the scripts) | 1 | state | run run2@1593827 row 1,869,714 |

### effect sheet (2)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| effects/fx_0876dff8  | 1,283 | state | state cov_grapple |
| effects/fx_0875a6a8  | 2 | state | state rj_049_001_18-14 |

### prop sheet (3)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| effects/props_area_00_009  | 2,446 | state | state db7_041_14-1 |
| effects/props_area_00_016  | 696 | state | state cov2_gunman2, rfat_033_002_6-1, sb_004_01_03 (+1) |
| effects/props_area_11_024  | 573 | state | run run1@0 row 795,093 |

### sprite sheet (1)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| sprites/120_ss_gotenks  | 196 | state | state r052_006_6-9, sb_006_06_06, sb_007_06_09 |

### enemy (1)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| 39 Goten | 3,425 | state | state r052_004_1-5 |

### enemy AI state (97)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| 65:529 Lich state 0x211 (?) | 7,378 | state | state cov2_lich_states |
| 116:608 Super Buu state 0x260 (Sequence) | 4,047 | state | state r052_006_6-9, r057_066_14-1 |
| 39:2 Goten state 0x2 (?) | 2,835 | state | state r052_004_1-5 |
| 116:607 Super Buu state 0x25f (Sequence) | 2,627 | state | state r052_006_6-9, r057_066_14-1 |
| 94:364 Power Mechandoid state 0x16c (Sequence) | 2,589 | state | run run1@0 row 1,312,532 |
| 39:208 Goten state 0xd0 (Sequence) | 2,334 | state | state r052_004_1-5 |
| 120:2 Super Buu state 0x2 (?) | 1,978 | state | run run1@1354525 row 1,357,782 |
| 63:311 Pilaf's Guardian state 0x137 (Sequence) | 1,833 | state | run run1@0 row 755,436 |
| 120:689 Super Buu state 0x2b1 (Sequence) | 1,599 | state | run run1@1354525 row 1,358,077 |
| 124:346 Super Majin Soldier state 0x15a (Sequence) | 1,567 | state | run run1@0 row 1,245,240 |
| 51:263 Janemba state 0x107 (Sequence) | 1,565 | state | run run2@0 row 1,317,438 |
| 63:312 Pilaf's Guardian state 0x138 (Sequence) | 1,500 | state | run run1@0 row 755,495 |
| 18:115 Dabura state 0x73 (Sequence) | 1,399 | state | run run3@0 row 1,015,750 |
| 82:776 Nosferatu state 0x308 (Selector) | 1,321 | state | run run1@0 row 1,101,226 |
| 65:531 Lich state 0x213 (?) | 1,158 | state | run run2@0 row 1,237,023 |
| 115:574 Super Buu state 0x23e (Sequence) | 1,089 | state | state r052_006_6-9 |
| 115:573 Super Buu state 0x23d (Selector) | 901 | state | state r052_006_6-9 |
| 51:265 Janemba state 0x109 (Sequence) | 812 | state | run run3@0 row 1,945,686 |
| 105:514 Shinobi state 0x202 (Sequence) | 574 | state | run run2@0 row 1,041,836 |
| 51:264 Janemba state 0x108 (Sequence) | 568 | state | run run3@0 row 1,947,823 |
| 122:731 Super Janemba state 0x2db (Sequence) | 545 | state | run run3@0 row 1,949,548 |
| 9:525 Bones state 0x20d (Sequence) | 505 | state | run run1@0 row 903,184 |
| 104:499 Shinobi state 0x1f3 (Sequence) | 504 | state | run run2@0 row 994,584 |
| 112:565 Spopovich state 0x235 (Sequence) | 474 | state | run run3@0 row 248,617 |
| 82:774 Nosferatu state 0x306 (Selector) | 438 | state | run run1@0 row 1,101,225 |
| 39:206 Goten state 0xce (Sequence) | 399 | state | state r052_004_1-5 |
| 103:483 Majin Shield Soldier state 0x1e3 (Sequence) | 332 | state | run run1@0 row 473,257 |
| 109:540 Spopovich state 0x21c (ParallelAll) | 330 | state | run run2@0 row 213,357 |
| 70:351 Majin Vegeta state 0x15f (Sequence) | 303 | state | run run1@0 row 557,506 |
| 82:768 Nosferatu state 0x300 (Sequence) | 285 | state | run run1@0 row 1,098,031 |
| 39:205 Goten state 0xcd (Sequence) | 278 | state | state r052_004_1-5 |
| 120:694 Super Buu state 0x2b6 (ParallelAll) | 277 | state | run run1@1354525 row 1,357,782 |
| 2:244 Assassin state 0xf4 (Sequence) | 266 | state | run run2@0 row 1,016,024 |
| 64:326 Laser Turret state 0x146 (?) | 261 | state | run run2@0 row 488,045 |
| 39:204 Goten state 0xcc (Selector) | 247 | state | state r052_004_1-5 |
| 111:556 Spopovich state 0x22c (ParallelAll) | 241 | state | run run2@0 row 232,851 |
| 108:243 Sniper state 0xf3 (Sequence) | 226 | state | run run2@0 row 883,376 |
| 119:669 Super Buu state 0x29d (Sequence) | 225 | state | state sb_012_20_06 |
| 78:389 Ninja state 0x185 (Sequence) | 211 | state | run run1@0 row 840,827 |
| 115:577 Super Buu state 0x241 (Sequence) | 208 | state | state r052_006_6-9 |
| 62:314 Knight Destroyer state 0x13a (Sequence) | 201 | state | run run2@0 row 996,903 |
| 1:137 Annihilator state 0x89 (Sequence) | 192 | state | run run1@0 row 1,126,315 |
| 121:708 Super Buu state 0x2c4 (Sequence) | 181 | state | run run2@1593827 row 1,851,038 |
| 104:503 Shinobi state 0x1f7 (Sequence) | 169 | state | run run2@0 row 994,576 |
| 120:693 Super Buu state 0x2b5 (Sequence) | 163 | state | run run1@1354525 row 1,357,855 |
| 115:581 Super Buu state 0x245 (Sequence) | 161 | state | run run2@0 row 1,384,162 |
| 82:764 Nosferatu state 0x2fc (Sequence) | 161 | state | run run1@0 row 1,101,361 |
| 39:209 Goten state 0xd1 (ParallelAll) | 157 | state | state r052_004_1-5 |
| 111:555 Spopovich state 0x22b (Sequence) | 151 | state | run run2@0 row 232,769 |
| 94:370 Power Mechandoid state 0x172 (Sequence) | 150 | state | run run3@0 row 2,120,275 |
| 57:278 Janemba Hand state 0x116 (?) | 147 | state | state r037_084_18-1 |
| 120:684 Super Buu state 0x2ac (Selector) | 131 | state | run run1@1354525 row 1,357,702 |
| 120:687 Super Buu state 0x2af (Sequence) | 125 | state | run run1@1354525 row 1,357,731 |
| 115:578 Super Buu state 0x242 (Sequence) | 115 | state | run run2@0 row 1,373,187 |
| 104:500 Shinobi state 0x1f4 (Sequence) | 108 | state | run run2@0 row 1,012,583 |
| 103:343 Majin Shield Soldier state 0x157 (Sequence) | 107 | state | run run3@0 row 998,731 |
| 115:572 Super Buu state 0x23c (Sequence) | 88 | state | run run3@0 row 2,023,133 |
| 78:394 Ninja state 0x18a (ParallelAll) | 83 | state | run run1@0 row 835,859 |
| 115:579 Super Buu state 0x243 (Sequence) | 82 | state | state r052_006_6-9 |
| 105:515 Shinobi state 0x203 (Sequence) | 79 | state | run run3@0 row 1,536,139 |
| ... 37 more in data/coverage.json | | | |

### form (4)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| 28 Vegeta (form 28, vegeta) | 5,986 | state | state cov_vegeta28 |
| 41 Gogeta (form 41, gogeta) | 806 | state | state rj_049_001_18-14 |
| 5 Goku (form 5, goku) | 203 | state | state rkb_168_14-1, sb_041_21_25 |
| 37 Gotenks (form 37, gotenks) | 196 | state | state r052_006_6-9, sb_006_06_06, sb_007_06_09 |

### area (3)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| 8-43 Inside Babidi's Ship | 3,127 | state | state rr2_544900_8-42 |
| 8-24 Inside Babidi's Ship | 2,641 | state | run run1@0 row 455,076 |
| 0-4  | 792 | state | run run1@0 row 1,070 |

### object (shots, effects objects) (5)

| Item | Met (iterations) | Level | Seen in |
|---|---|---|---|
| rockfx  | 2,310 | state | state earth_2_90_2 |
| grapple  | 312 | state | state cov_grapple |
| stretch  | 193 | state | state shipr_338_8-16 |
| superball  | 34 | state | state cov_superball |
| sjwave  | 2 | state | state rj_049_001_18-14 |

## Not met in the three whole runs and never in a picture

Content the recorded playthroughs do not reach (optional areas, other characters' skills, unused data); counts only, the lists are in `data/coverage.json`.

| Category | Not met, no picture | of them state / audit / board / never | Examples |
|---|---|---|---|
| native | 31 | 0 / 0 / 0 / 31 | 0x12 get_world (13 uses in the scripts), 0xb5 run_event (18 uses in the scripts), 0xb6 run_event (4 uses in the scripts), 0xb7 run_event (12 uses in the scripts), 0xb8 run_event (3 uses in the scripts) |
| effect sheet | 279 | 26 / 2 / 0 / 251 | effects/bbk_flare, effects/beam_banshee_blast, effects/beam_big_bang_kamehameha, effects/beam_electric_kamehameha_2, effects/beam_electric_kamehameha_3 |
| prop sheet | 9 | 1 / 0 / 0 / 8 | effects/props_area_01_010, effects/props_area_02_039, effects/props_area_03_023, effects/props_area_04_026, effects/props_area_07_001_v2 |
| sprite sheet | 52 | 12 / 6 / 0 / 34 | sprites/015_ghost_fighter_blue_2, sprites/017_atomizer, sprites/041_super_buu_2, sprites/044_super_buu_gohan_absorbed_2, sprites/050_shell_brute |
| enemy | 37 | 6 / 5 / 0 / 26 | 101 Shadow Warrior, 102 Shadow Warrior, 106 Shogun, 11 Broly, 113 Super Bio Mech |
| enemy AI state | 1426 | 64 / 92 / 1 / 1269 | 0:21 Airship Warlord state 0x15 (?), 100:16 Shadow Colossus state 0x10 (?), 100:17 Shadow Colossus state 0x11 (?), 100:314 Shadow Colossus state 0x13a (Sequence), 100:315 Shadow Colossus state 0x13b (IfTimeInState(90f)) |
| form | 16 | 16 / 0 / 0 / 0 | 17 Gohan (form 17, gohan), 20 Goten (form 20, goten), 23 Trunks (form 23, trunks), 32 Gotenks (form 32, gotenks), 33 Gotenks (form 33, gotenks) |
| skill (per form) | 107 | 106 / 1 / 0 / 0 | 0:15 Kamehameha as Goku (form 0, goku), 10:17 Masenkoha as Gohan (form 10, gohan), 10:6 Electric Kamehameha as Gohan (form 10, gohan), 11:17 Masenkoha as Gohan (form 11, gohan), 11:6 Electric Kamehameha as Gohan (form 11, gohan) |
| skill | 20 | 13 / 0 / 0 / 7 | 1 Banshee Blast, 10 Final Flash, 11 Flame Kamehameha, 12 Fusion Dance, 13 Super Kamikaze Ghost Attack |
| transformation | 37 | 14 / 0 / 0 / 23 | 0>3 Goku (form 0, goku) -> Goku (form 3, goku), 10>16 Gohan (form 10, gohan) -> Gohan (form 16, gohan), 14>10 Gohan (form 14, gohan) -> Gohan (form 10, gohan), 15>18 Gohan (form 15, gohan) -> Gohan (form 18, gohan), 16>10 Gohan (form 16, gohan) -> Gohan (form 10, gohan) |
| area | 11 | 0 / 0 / 0 / 11 | 0-90, 0-97, 0-99, 19-6 Planet of the Kais, 2-36 East District 439 |
| screen | 1 | 0 / 0 / 0 / 1 | GameOver Game Over |
| object (shots, effects objects) | 15 | 13 / 2 / 0 / 0 | bbkfx, bigbang, bigbangboom, burning, dragonfx |
| held-by-enemy state | 1 | 1 / 0 / 0 / 0 | Freeze |
