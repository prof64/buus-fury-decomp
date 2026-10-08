# Playtest log

Hands-on play of the real native app (`buus-fury`), headless and muted, driven through the launcher with scripted
button presses like a player, frames dumped and looked at. Not the parity tests: these are problems a player meets.

* **Build tested:** 8a67646 plus the uncommitted working tree of 2026-10-07 ~19:10 (other sessions' work in
  progress was in it: the Classic default view, "Change fighter", Final Flash affine drawing). Since then the working
  tree gained `launcher::story_skills` (SS3 / Saiyaman skills by level), which addresses part of finding 4 and 14 -
  not verified here.
* **Evidence paths** are under `work/playtest/` (local, git-ignored). Each run folder holds `fNNNNNN.png` frames,
  `launcher_KKK.png`, `state.jsonl` (players / enemies / outcome per dumped frame), `input.txt`, `log.txt`; `s*.png`,
  `z*.png`, `crop_*.png` are contact sheets made from them.

## How to play it headless

App flags (wf-native, off by default):

* `--dump DIR [--dump-every N]`: a PNG every N frames (default 10) rendered as the window draws it (`Control::render`:
  end screen and tool overlays included), `DIR/launcher_KKK.png` after each `--launcher-keys` key, and
  `DIR/state.jsonl` (one line per dumped frame: hp / ep / form / state / selected skill / SS gauge / position of each
  player, enemies' name / hp / position, outcome, camera, field area).
* `--input` scripts take `l` / `r` for the shoulder buttons (upper-case `L` / `R` stay the d-pad). In battle: A punch,
  B the selected skill (hold for charge / beams / Instant Transmission's pick), L cycles the skill, R block, R+B the
  energy shield.

Local helpers (work/playtest/bin, not in git): `pt.py NAME --battle fighter:form:enemy-or-preset[:count[:arena[:level]]]
--keys ArrowDown,Enter,Enter --input "N30,R30,A2,N8" --frames N --every K [--view gba:0]` (writes `settings.json` in a
fresh per-run data dir, so the launcher starts exactly that fight), `st.py NAME` (state per dumped frame),
`sheet.py OUT.png frames... [--crop x,y,w,h] [--scale N]` (contact sheets). Settings written by hand need
`"classic_default": true` or the new migration switches the view to Classic.

## Findings (most severe first)

Severity: **critical** = blocks play / never ends, **high** = a core feature broken or missing on screen, **medium**
= clearly wrong but playable, **low** = polish.

1. **High - Story: softlock in Yemma's Office when the door ogre walks into Goku.**
   Did: New Game, prologue, talked to the left-door ogre from his left beside the wall (run `st_p17`, ~11,950 frames;
   `st_p18` continues). Saw: his script walks him onto Goku's spot (87,240); afterwards Goku plays the walk animation
   in every direction for 1,200+ frames but stays at (93,240) (`st_p17/z.png`, `st_p17/f011640.png`,
   `st_p18/f012650.png`, `st_p18/state.jsonl`). This is before the first save point, so the fix is a reset and the whole
   prologue again. Approaching from below works (`st_p23/s.png`). Expected: the player can always walk free. (Not
   checked against the original.)

2. **High - Battle: "Janemba > Super Janemba" never ends: Janemba takes no damage.**
   Did: `--battle goku:5:janemba:1::200` (SS3 Goku, Lv 200), point-blank punches and ki blasts (`bw_jan3`, `bw_jan4`).
   Saw: Janemba 8880/8880 for 2,000-7,200 frames while the hits land on his body; Mini-Janembas do take damage
   (`bw_jan4/s.png`, `bw_jan3/s.png`, `bw_p_janemba/s.png`). Expected: he loses HP and phase 2 (Super Janemba)
   starts. Janemba is not in the battle's invulnerable list, so the fight has no way to end.

3. **High - Battle: the opponent never comes to you; fights that never end.**
   Did: default fight (Goku vs Kid Buu) standing still for 6,000 frames (`bw_lose_kb`); Majin Fighter / Elite Majin
   Fighter idle runs (`bf_idle_stadium`, `bf_idle_waste21`). Saw: Kid Buu stands ~120 px away for 100 s, Goku sits
   down bored, nothing happens (`bw_lose_kb/s.png`); enemies only react within ~90-100 px. The Majin Buu "survive"
   presets win after 60 s of nothing with no countdown on screen (`bw_s_mb1idle/s.png`). Expected: a boss in a battle
   mode goes for the player; a survive fight shows its timer.

4. **High - Battle: Goku and Gotenks cannot transform into Super Saiyan 3** (the user's report, still true in the
   tested build). Did: `--battle gotenks:31:t68:1:forest` and `gotenks:36` with B on "Super Saiyan" repeatedly
   (`bf_gt_tr`, `bf_gt_tr36`); Goku base at Lv 100 (`bg_g0_all`). Saw: the skill lists are Energy Blast, Super Saiyan,
   Kamikaze Ghost, Volleyball (Gotenks) / Energy Blast, Super Saiyan, Kamehameha, Instant Transmission (Goku): no
   Super Saiyan 3 skill; the Super Saiyan skill used in SS turns back to base (`bf_gt_tr/s_up.png`,
   `bg_g0_all/crop_756_852.png`). Starting in an SS3 form greys the Super Saiyan skill (`bg_ss3_goku5/hud.png`): no
   way down by hand, and once the gauge runs out (~frame 750 / 820) he drops to base for good. Now in progress:
   `story_skills` gives skill 22 at Lv >= 100 (Goku) / >= 125 (Gotenks) or an SS3 start form - so at an automatic
   level below that (any regular enemy) SS3 still cannot be reached.

5. **High - Battle: Ultra Volleyball Attack never turns the enemy into a volleyball.**
   Did: SS Gotenks Lv 150 vs Majin Fighter, Volleyball selected, B (`bf_vb_aggro2`), then punched it (`bf_vb_punch2`).
   Saw: the beam hits, a dust puff, then the Majin Fighter is drawn as itself, frozen, never attacking again
   (`bf_vb_aggro2/z.png`); punching it facing right slides it ~120 px straight down (`bf_vb_punch2/s.png`). The core
   has the `volleyball` state but no view code draws a ball. Expected: the ball, kicked in the punch's direction.

6. **High - Battle: the auto "Southwest Forest" arena hides the fight under tree canopy.**
   It is the automatic arena for every regular enemy without a story preset. Did: `--battle goku:0:t2|t9|t78|t149|t46`
   (`bw_f_*`), `goku:0:t68` (`b_win2`, `b_idle_mf`). Saw: Goku and/or the enemy invisible under the trees while the
   fight goes on (`bw_forest_enemies.png`, `bw_c8forest/f000120.png`); the Majin Fighter spawns at its story spot
   (425,433), off-screen, and never comes (`bw_a_forest/f000060.png`). Expected: an open arena, both fighters in view.

7. **Medium-high - Battle: the energy shield (hold R, press B) is invisible.**
   Did: `--battle goku:0:t68:1:wasteland`, input `...,r30,rB120,...` (`bg_shield`); Gotenks vs Elite Majin Fighter
   (`bf_block_gotenks31`). Saw: EP drains and hits do 0 damage, but only the block pose is drawn - block alone and
   shield on look identical (`bg_shield/crop_63_129.png`, `bg_shield/crop_66_210.png`, `bf_block_gotenks31/sh.png`).
   The shield state is never read by view code. Expected: the shield bubble.

8. **Medium - PC wide: the view is not clamped to the map, so black void shows beside it (battle and story).**
   `Renderer::view_origin` centres the wide view on the GBA camera; the GBA camera is clamped, the wide view is not.
   Saw: a 120 px black band at 2x (40 px at 3x) at the start of most arenas (`bw_view_pc/s.png`, `bw_arenas.png`,
   `bw_arenas2.png`, `b_win3/f000800.png`); Yemma's Office with Yemma's head and the door off the top while the
   bottom third is black (`st_p8/f008800.png`, `st_p11/f010320.png`), half the screen black (`st_p12/f010800.png`);
   Snake Way / Grand Kai's Planet / Mansion black bands (`st_p3/f006660.png`, `st_p6/f007440.png`,
   `st_f2/f001800.png`, `st_cont/f000300.png`); sprites cut at the void (`st_p3/z1.png`). Classic frames the same
   scenes correctly (`bw_view_gba/s.png`, `st_g8/f008800.png`). Expected: clamp to the map; centre only when the map
   is smaller than the view.

9. **Medium - Story: wandering souls walk into Goku, block him and take his A-button talk.**
   Saw: under the office ogre, a soul hidden behind Goku answers every A with "I'm going to Heaven!" (`st_p21/z.png`);
   at 1-4 a soul inside Goku blocks the open stairs and answers A (`st_q3/s.png`, `st_q2/z.png`). Walking away frees
   him. Probably the same cause as 1 (NPC movement ignores the player's body).

10. **Medium - Battle launcher: the opponent list has things that are not fighters.**
    Saw (`bw_opps/s1.png`..`s4.png`): raw catalogue names cut off ("DUMMY TYPE 0X54 (U...", "FLOOR TRIGGER (NOT...",
    "INERT HELPER 0X22", "JANEMBA PART (ARM...", "WORM PART 0X90", "UNNAMED FIGHTER 0X...", "BUU FRAGMENT (FREE...",
    "KID BUU (SPLIT COPY)", "VEGETA (BARREN WAS..." Lv 1). Floor trigger / dummy / Janemba arm: instant "You Win!!"
    with no opponent (`bw_o_floor/s.png`, Won at frame 30); Vegeta 1 HP / 5 HP and the inert helper wander and never
    fight (`bw_s_veg/s.png`, `bw_odd2.png`); "Janemba Hand (summoned)" is invisible and never acts (fight never ends);
    its preview is empty. Expected: only real opponents, clean names.

11. **Medium - Battle: at the automatic level the fusions' signature moves can never be used.**
    Lv follows the opponent (Lv 75 vs a Majin Fighter): Vegito max EP 199 vs Banshee Blast 200 / Energy Sword 350;
    Gogeta 199 vs Dragon Fist 300; Gotenks 227 vs Volleyball 350 - greyed all fight (`bf_skt_vegito39/s_hud.png`,
    runs `bf_skt_*`). All work at Lv 150.

12. **Medium - Battle: Final Flash fizzles from the start spot in Barren Wasteland** (Kid Buu's default arena).
    `--battle vegeta:26:t21:1:wasteland:150`, hold B (`bf_ff26`). Saw: only the half-disc at Vegeta's hands for ~130
    frames, no beam, no damage on an enemy 120 px away in line, ~580 EP spent (`bf_ff26/s.png`): the rock pillar beside
    the start cuts the beam. In the open (stadium) it fires and kills in all four directions (`bf_ff_stadium/s.png`).
    Faithful beam clipping, but the start spot makes the signature move look broken.

13. **Medium-low - Battle: Unstable Super Saiyan Trunks transforms on frame 1, locking the player ~3 s.**
    `--battle trunks:24:t54:1:wasteland:100` (`bg_t24`): the aura is up at frame 6 until ~180, walking ignored
    (`bg_t24/crop_6_186.png`). Later cycles have the right timing. Expected: first transformation 120-300 frames in
    (the countdown is set on a form change only, not at battle start).

14. **Low-medium - Battle end screen: holding / mashing A after the last hit retries instantly.**
    `bw_win`: "You Win!!" at f190, a new fight by f230 (`bw_win/s.png`); the input lock is 40 frames. Same in
    `b_win3` (`b_win3/f000800.png`, restarted by f900). Expected: a longer lock or a press needed after release.

15. **Low-medium - Battle: 2 players are indistinguishable and only player 1 has a HUD** (`bw_pl2/s2.png`).

16. **Low - Great Saiyaman Gohan: the Super Saiyan icon is lit but B does nothing, no way back to Teen Gohan**
    (`bg_skA_gohan14/crop_84_132.png`, `bg_skA_gohan13/hud.png`; Mystic Gohan's icon is correctly greyed in battle,
    the launcher page still shows it). `story_skills` (in progress) adds the Saiyaman skill for forms 10 / 14.

17. **Low - Dragon Fist: tapping B spends 300 EP and does nothing; holding works** (`bf_dfist3/z.png`, frames 32-100
    vs 186-280). Possibly faithful.

18. **Low - Launcher Battle page polish:** Arena "Auto" looks like a chosen arena ("< Barren Wasteland >", no
    "Auto:"; `bw_arena_row/launcher_007.png` vs `launcher_011.png`); Goku defaults to the "Halo (Other World)" outfit
    (`bw_chars/launcher_001.png`); "SPD 121" runs over the right border at high levels (`bw_spd.png`).

19. **Low - Classic: "You Lose!!" overlaps the opponent's head** (`bw_gba2.png`, f700).

20. **Low - PC wide auto zoom jumps between 2x and 3x from area to area** (3x: prologue `st_t4/f001320.png`, forest
    1-12 `st_f5/f003000.png`; 2x: Snake Way, the office `st_p8/f008800.png`); title / Select Game / pause menu are a
    240x160 panel in a mostly black window (`st_title/s1.png`, `st_menu/s.png`).

21. **Low - Launcher Story page** is a plain dark text panel unlike the Battle page and the game's menus; Continue
    slots show an area code ("Goku Lv60 1-10 0:04") instead of the area name the in-game Select Game shows
    (`st_cont/launcher_000.png`).

Unconfirmed: Kamehameha fired up next to a rock pillar stops ~25 px above Goku while firing down runs full length
(`bg_g0_left/s_up.png`, frames 580 / 784) - may be terrain clipping.

## Checked and looked right

* Battle: punches / combos in four directions for every fighter and form tried (swoosh arcs, hit flash, damage
  numbers; `bf_cmb3_sheetA.png`); dash; block (4 damage); Energy Blast; Kamehameha (charge, four directions),
  Electric Kamehameha, Masenko (aimed, marker), Kamekameha, Energy Punch, Burning Attack, Flame Kamehameha, Kamikaze
  Ghost, Banshee Blast, Big Bang (Kamehameha) (`bf_h150_vegito39/s_bb.png`), Final Flash in the open, Energy Sword,
  Dragon Fist (held); Instant Transmission (hold B: cursor, pick, strike, return; `bg_it2/s1.png`).
* Super Saiyan up / down for Goku (normal + halo), Gohan (teen, suit), Goten, Trunks, Gotenks, Vegeta (halo kept),
  Vegito: aura centred in PC wide and Classic, facing either way (`bf_tr_sheet.png`, `bg_classic_tf/crop_28_300.png`);
  gauge drain and drop back; no-limit forms do not drain; a hit cancels the transformation (`bg_tf_hit/crop_90_132.png`).
* Win: "You Win!!" + Retry / Change fighter, placed right in both views (`b_win3/f000800.png`); Retry restarts the
  same fight. Loss: KO, body stays down, "You Lose!!" ~290 frames later (`bw_lose_broly/s.png`). Multi-phase presets
  advance (Pui Pui > Yakon > Dabura; Time Chamber Goten > Trunks > Super Buu with candy / eat, `bw_p_htc/s.png`);
  4 and 8 enemies spawn in formation; Uub and Hercule can be beaten.
* Launcher: Battle page stepping, previews / portraits for all characters (`bw_chars/s.png`, `bf_lp_sheet.png`,
  `bg_lp.png`); Settings and Controls pages; Story page.
* Story: title -> Single Player -> Select Game -> New Game; the whole prologue (`st_t4/s1.png`); Snake Way intro,
  journal entry, run tutorial; walking / running; area fades and name banners through Snake Way, Yemma's Office, the
  planes, Grand Kai's Mansion and Planet; talking to King Kai, Yemma, ogres, souls; pause menu pages (L / R), Skills
  popup (Select), Options sliders with text speed taking effect (`st_opt/s.png`); save circle "Game Saved" ->
  launcher Continue 1 and the title's Select Game resume there (`st_cont/s.png`); Quick start (`st_quick/s.png`);
  first fights at 1-12 / 1-13 with zenie, XP and a level-up (`st_fight1/s.png`, `st_f7/s.png`); game over -> Select
  Game -> continue (`st_die/s.png`). Classic and PC wide reach the same state hash on the same inputs.
