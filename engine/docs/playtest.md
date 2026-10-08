# Playtest log

Hands-on play of the real native app (`buus-fury`) and the web build, headless and muted, driven through the launcher
with scripted button presses like a player, frames dumped and looked at. Not the parity tests: these are problems a
player meets.

* **Round 1:** 8a67646 plus the uncommitted working tree of 2026-10-07 ~19:10 (Classic default, "Change fighter",
  Final Flash affine drawing in progress).
* **Round 2:** a clean build of 6a369e5 (the source of 9c24c25, exported with `git archive`; no work in progress):
  every round-1 item re-checked (status line under each), then deeper play: bosses, the Earth chapter and later saves,
  the title's Multiplayer (Dojo / Trading Post, also over a local link), Options, Select Game, and the web build in
  headless Chrome. New findings are 22-31.
* **Evidence paths** are under `work/playtest/` (local, git-ignored). Each run folder holds `fNNNNNN.png` frames,
  `launcher_KKK.png`, `state.jsonl` (players / enemies / outcome per dumped frame), `input.txt`, `log.txt` (the exact
  app command); `s*.png`, `z*.png`, `crop_*.png` are contact sheets made from them. Round-2 runs start with `r2`.

## How to play it headless

App flags (wf-native, off by default):

* `--dump DIR [--dump-every N]`: a PNG every N frames (default 10) rendered as the window draws it (`Control::render`:
  tool overlays and, once `end_screen` is on, the end screen), `DIR/launcher_KKK.png` after each `--launcher-keys` key,
  and `DIR/state.jsonl` (one line per dumped frame: hp / ep / form / state / selected skill / SS gauge / position of
  each player, enemies' name / hp / position, outcome, camera, field area).
* `--input` scripts take `l` / `r` for the shoulder buttons (upper-case `L` / `R` stay the d-pad). In battle: A punch,
  B the selected skill (hold for charge / beams / Instant Transmission's pick), L cycles the skill, R block, R+B the
  energy shield.
* On 6a369e5 the battle end screen is opt-in (`Control::end_screen`) and only the window / web shells turned it on, so
  headless dumps showed no "You Win!!" (the outcome still changes in `state.jsonl`). The working tree after it turns it
  on in headless too (`main.rs`); items 14 and 19 need a re-check with that.

Local helpers (work/playtest/bin, not in git): `pt.py NAME --battle fighter:form:enemy-or-preset[:count[:arena[:level]]]
--keys ArrowDown,Enter,Enter --input "N30,R30,A2,N8" --frames N --every K [--view gba:0] [--bin head|old]
[--saves SAVE]` (writes `settings.json` in a fresh per-run data dir, so the launcher starts exactly that fight;
`--saves` copies an in-game save so Story -> Continue 1 = `--keys Enter,ArrowDown,Enter` resumes it), `st.py NAME`
(state per dumped frame), `sheet.py OUT.png frames... [--crop x,y,w,h] [--scale N]` (contact sheets), `rerun.py`
(a round-1 run on another build), `chase.py` (closed-loop fighting). Settings written by hand need
`"classic_default": true` or the migration switches the view to Classic.

Later story points without playing hours: `work/playtest/savconv` (a small cargo project using `wf_import::import_saves`)
turns run 2's emulator EEPROM saves (`work/run2/*.sav`) into the app's slot files (`work/playtest/saves/<cut>/saves/`):
`seg_continue` 1-10 Other World (Goku), `seg_e_tournament` 3-1 Hercule City (Gohan Lv 64), `seg_f_fat` 8-41 (2 h 12),
`seg_j_janemba` 15-1 (3 h 56), `seg_b_superbuu` 18-13 (5 h), `end` 4-1 (6 h 33). All six Continue correctly
(`r2s_c_sheet.png`).

Web build: built from the clean export (`package/web-dist.sh` with its own `CARGO_TARGET_DIR`), served on 127.0.0.1,
driven through the DevTools protocol in `--headless=new --mute-audio` Chrome (`work/playtest/web/play.mjs`, extended
from `package/web_check.mjs`); local link play with two headless apps and a 127.0.0.1 signalling server
(`work/playtest/web/link.sh`).

## Findings (most severe first in round 1; new round-2 findings from 22)

Severity: **critical** = blocks play / never ends, **high** = a core feature broken or missing on screen, **medium**
= clearly wrong but playable, **low** = polish. Status after round 2 on 6a369e5 in the line starting **R2**.

1. **High - Story: softlock in Yemma's Office when the door ogre walks into Goku.**
   Did: New Game, prologue, talked to the left-door ogre from his left beside the wall (run `st_p17`, ~11,950 frames;
   `st_p18` continues). Saw: his script walks him onto Goku's spot (87,240); afterwards Goku plays the walk animation
   in every direction for 1,200+ frames but stays at (93,240) (`st_p17/z.png`, `st_p17/f011640.png`,
   `st_p18/f012650.png`, `st_p18/state.jsonl`). This is before the first save point, so the fix is a reset and the whole
   prologue again. Approaching from below works (`st_p23/s.png`). Expected: the player can always walk free. (Not
   checked against the original.)
   **R2: still broken** - stuck at (93,240) for 1,200+ frames (`r2s_p18/s.png`, `r2s_p18/f012600.png`).

2. **High - Battle: "Janemba > Super Janemba" never ends: Janemba takes no damage.**
   Did: `--battle goku:5:janemba:1::200` (SS3 Goku, Lv 200), point-blank punches and ki blasts (`bw_jan3`, `bw_jan4`).
   Saw: Janemba 8880/8880 for 2,000-7,200 frames while the hits land on his body; Mini-Janembas do take damage
   (`bw_jan4/s.png`, `bw_jan3/s.png`, `bw_p_janemba/s.png`). Expected: he loses HP and phase 2 (Super Janemba)
   starts.
   **R2: changed - he is beatable** (Janemba, then Super Janemba, won at ~frame 3000: `r2b_janemba_win2`; the old
   build the same, `r2b_janemba_old`), but only hits from one small spot at his feet count - see 23.

3. **High - Battle: the opponent never comes to you; fights that never end.**
   Did: default fight (Goku vs Kid Buu) standing still for 6,000 frames (`bw_lose_kb`); Majin Fighter / Elite Majin
   Fighter idle runs (`bf_idle_stadium`, `bf_idle_waste21`). Saw: Kid Buu stands ~120 px away for 100 s, Goku sits
   down bored, nothing happens (`bw_lose_kb/s.png`); enemies only react within ~90-100 px. The Majin Buu "survive"
   presets win after 60 s of nothing with no countdown on screen (`bw_s_mb1idle/s.png`). Expected: a boss in a battle
   mode goes for the player; a survive fight shows its timer.
   **R2: still broken** (`r2b_bw_lose_kb/f003000.png`, `r2b_bf_idle_stadium`, `r2b_bw_s_mb1idle/f001600.png`).

4. **High - Battle: Goku and Gotenks cannot transform into Super Saiyan 3.**
   Round 1: no Super Saiyan 3 skill in battle; starting in SS3 left no way down (`bf_gt_tr/s_up.png`,
   `bg_g0_all/crop_756_852.png`, `bg_ss3_goku5/hud.png`).
   **R2: fixed at Lv 100+ (Goku) / 125+ (Gotenks)**: base -> SS -> SS3 by transforming, the SS3 skill again drops to
   base (the form table's way down), an SS3 start form gets the skill at any level (`r2b_goku_ss3/s.png`,
   `r2b_gotenks_ss3/s.png`, `r2b_ss3_down`). Remaining (low-medium): below those levels there is no SS3 - Gotenks at
   the automatic Lv 75 (any regular enemy) has skills [7, 20, 13, 23].

5. **High - Battle: Ultra Volleyball Attack never turns the enemy into a volleyball.**
   Did: SS Gotenks Lv 150 vs Majin Fighter, Volleyball selected, B (`bf_vb_aggro2`), then punched it (`bf_vb_punch2`).
   Saw: the beam hits, a dust puff, then the Majin Fighter is drawn as itself, frozen, never attacking again
   (`bf_vb_aggro2/z.png`); punching it facing right slides it ~120 px straight down (`bf_vb_punch2/s.png`).
   Expected: the ball, kicked in the punch's direction.
   **R2: still broken** - own sprite, no ball, a right punch sends it ~125 px down (`r2b_bf_vb_punch2/z.png`).

6. **High - Battle: the auto "Southwest Forest" arena hides the fight under tree canopy.**
   It is the automatic arena for every regular enemy without a story preset. Did: `--battle goku:0:t2|t9|t78|t149|t46`
   (`bw_f_*`), `goku:0:t68` (`b_win2`, `b_idle_mf`). Saw: Goku and/or the enemy invisible under the trees while the
   fight goes on (`bw_forest_enemies.png`, `bw_c8forest/f000120.png`); the Majin Fighter spawns at its story spot
   (425,433), off-screen, and never comes (`bw_a_forest/f000060.png`). Expected: an open arena, both fighters in view.
   **R2: still broken** (`r2b_auto_arena/s.png`).

7. **Medium-high - Battle: the energy shield (hold R, press B) is invisible.**
   Did: `--battle goku:0:t68:1:wasteland`, input `...,r30,rB120,...` (`bg_shield`); Gotenks vs Elite Majin Fighter
   (`bf_block_gotenks31`). Saw: EP drains and hits do 0 damage, but only the block pose is drawn - block alone and
   shield on look identical (`bg_shield/crop_63_129.png`, `bf_block_gotenks31/sh.png`). The shield state is never read
   by view code. Expected: the shield bubble.
   **R2: still broken** (`r2b_bg_shield/crop_63_129.png`).

8. **Medium - PC wide: the view is not clamped to the map, so black void shows beside it (battle and story).**
   `Renderer::view_origin` centres the wide view on the GBA camera; the GBA camera is clamped, the wide view is not.
   Saw: a 120 px black band at 2x (40 px at 3x) at the start of most arenas (`bw_view_pc/s.png`, `bw_arenas.png`,
   `b_win3/f000800.png`); Yemma's Office with Yemma's head and the door off the top while the bottom third is black
   (`st_p8/f008800.png`, `st_p12/f010800.png`); Snake Way / Grand Kai's bands (`st_p3/f006660.png`,
   `st_f2/f001800.png`); sprites cut at the void (`st_p3/z1.png`). Classic frames the same scenes correctly. Expected:
   clamp to the map; centre only when the map is smaller than the view.
   **R2: still broken** - battle (`r2b_bw_view_pc/f000050.png`; the top third black in Super Buu: Wasteland,
   `r2b_sbuuw_win2/f001510.png`), story (`r2s_p18/s8.png`, `r2s_p18/z.png`), later areas too: Nataday Village
   (`r2s_nv/s.png`), HFIL (`r2s_h1/s.png`), and on the web build. (Classic is now the default view, so fewer players
   meet it.)

9. **Medium - Story: wandering souls walk into Goku, block him and take his A-button talk.**
   Saw: under the office ogre, a soul hidden behind Goku answers every A with "I'm going to Heaven!" (`st_p21/z.png`);
   at 1-4 a soul inside Goku blocks the open stairs and answers A (`st_q3/s.png`). Walking away frees him. Probably
   the same cause as 1 (NPC movement ignores the player's body).
   **R2: still broken** (`r2s_p21/s.png`, `r2s_q3/halo.png`).

10. **Medium - Battle launcher: the opponent list has things that are not fighters.**
    Saw (`bw_opps/s1.png`..`s4.png`): raw catalogue names cut off ("DUMMY TYPE 0X54 (U...", "FLOOR TRIGGER (NOT...",
    "INERT HELPER 0X22", "JANEMBA PART (ARM...", "WORM PART 0X90", ...). Floor trigger / dummy / Janemba arm: instant
    "You Win!!" with no opponent (`bw_o_floor/s.png`); Vegeta 1 HP / 5 HP and the inert helper never fight
    (`bw_s_veg/s.png`); "Janemba Hand (summoned)" is invisible, never acts, empty preview.
    **R2: still broken** (`r2b_bw_o_floor` won at frame 30, `r2b_bw_s_veg`, `r2b_bw_opps/s1.png`).

11. **Medium - Battle: at the automatic level the fusions' signature moves can never be used.**
    Lv follows the opponent (Lv 75 vs a Majin Fighter): Vegito max EP 199 vs Banshee Blast 200 / Energy Sword 350;
    Gogeta 199 vs Dragon Fist 300; Gotenks 227 vs Volleyball 350 - greyed all fight (`bf_skt_vegito39/s_hud.png`).
    **R2: still broken** (`r2b_bf_skt_vegito39/hud.png`).

12. **Medium - Battle: Final Flash fizzles from the start spot in Barren Wasteland** (Kid Buu's default arena).
    `--battle vegeta:26:t21:1:wasteland:150`, hold B (`bf_ff26`): only the half-disc, no beam, ~580 EP spent
    (`bf_ff26/s.png`); the rock pillar beside the start cuts the beam. In the open it fires and kills
    (`bf_ff_stadium/s.png`).
    **R2: still broken** (`r2b_bf_ff26/crop_40_200.png`).

13. **Medium-low - Battle: Unstable Super Saiyan Trunks transforms on frame 1, locking the player ~3 s**
    (`bg_t24/crop_6_186.png`). Expected: first transformation 120-300 frames in.
    **R2: still broken** (`r2b_bg_t24`: frames 6-186).
    **Fixed (in the tree, verified 2026-10-08):** the battle makes the form object's countdown with the player
    (`battle::create`: 24 -> 120 + rand(180), as `player::new_form_object`); `work/pt/t24`: walks from frame 60,
    transforms at 158, back at 585.

14. **Low-medium - Battle end screen: holding / mashing A after the last hit retries instantly.**
    `bw_win`: "You Win!!" at f190, a new fight by f230 (`bw_win/s.png`); the input lock is 40 frames.
    **R2: not visible headless on 6a369e5** (see "How to play"); the 40-frame lock is unchanged in the code; the web
    build shows the same instant retry (the recording id changes from -01 to -02 right after the win, `r2w_battle`).
    **Fixed (in the tree, verified 2026-10-08):** the end screen ignores the buttons for 60 frames and takes a confirm
    only after 12 frames with no button held (`control.rs` END_LOCK / END_QUIET); `work/pt/win`: A mashed every 10
    frames for 1,200 frames after the win picks nothing, one A after a pause retries.

15. **Low-medium - Battle: 2 players are indistinguishable and only player 1 has a HUD** (`bw_pl2/s2.png`).
    **R2: still broken** (`r2b_bw_pl2/s.png`).

16. **Low - Great Saiyaman Gohan: the Super Saiyan icon is lit but B does nothing, no way back to Teen Gohan.**
    **R2: partly fixed** - the Saiyaman toggle works both ways (`r2b_saiyaman/hud.png`, frames 100 / 610); in Great
    Saiyaman the SS icon is still lit and B on it does nothing (frame 310).
    **Fixed (in the tree, verified 2026-10-08):** the Great Saiyaman gets the story's Saiyaman-era list - the Saiyaman
    skill in Super Saiyan's place (`launcher::story_skills`; the form has no Super Saiyan above it); `work/pt/gs`:
    skills 7 / 18 / 6 / 17, no SS icon, Saiyaman -> Teen Gohan at frame 585.

17. **Low - Dragon Fist: tapping B spends 300 EP and does nothing; holding works** (`bf_dfist3/z.png`). Possibly
    faithful. **R2: unchanged** (`r2b_bf_dfist3`).

18. **Low - Launcher Battle page polish:** Arena "Auto" shows no "Auto:"; Goku defaults to the "Halo (Other World)"
    outfit; "SPD 121" runs over the right border (`bw_arena_row/launcher_007.png`, `bw_chars/launcher_001.png`,
    `bw_spd.png`). **R2: unchanged** (`r2b_s18.png`).

19. **Low - Classic: "You Lose!!" overlaps the opponent's head** (`bw_gba2.png`, f700).
    **R2: not visible headless on 6a369e5**; the code moved the title up (y 56 -> 44).

20. **Low - PC wide auto zoom jumps between 2x and 3x from area to area**; title / Select Game / pause menu are a
    240x160 panel in a mostly black window (`st_t4/f001320.png`, `st_p8/f008800.png`, `st_title/s1.png`).
    **R2: unchanged** (PC wide only now; prologue 3x vs Snake Way 2x, HFIL 3x vs Nataday 2x: `r2s_p18/z.png`).

21. **Low - Launcher Story page** is a plain dark text panel; Continue slots show an area code ("Goku Lv60 1-10
    0:04") instead of the area name (`st_cont/launcher_000.png`).
    **R2: unchanged** ("Goten Lv110 15-1 3:56", `r2s_storypage/s.png`).

Round 1's unconfirmed "Kamehameha fired up stops ~25 px above Goku": **not a bug** - it stops at the cliff edge right
above him (`r2b_bg_g0_left/crop_540_620.png`).

### New in round 2

22. **High - Battle: the Kid Buu fight is declared won while Kid Buu is still alive.**
    Did: `chase.py r2b_kidbuu_win2 --battle goku:5:kid_buu:1::200 --view pc:0` (closed-loop fighting). Saw: at frame
    1900 his two halves recombine into the "Kid Buu (split copy)" (49,663 + 44,529 -> 94,192 HP); the main Kid Buu is
    gone, the outcome becomes Won, the copy keeps firing beams (`r2b_kidbuu_win2/f001930.png`, `f001990.png`), kills
    Goku around frame 3,750 and the outcome stays Won (`r2b_kidbuu_win2/state.jsonl`). Round 1's build does the same
    (`r2b_kidbuu_old`); in another run the halves merged into the main Kid Buu and the fight went on normally
    (`r2b_kidbuu_win`). Expected: the fight ends when the last Kid Buu is down, whichever object carries the HP.

23. **Medium - Battle: Janemba's hittable spot is tiny.** Only punches from right at his left foot (player at about
    (107-122, 106-114)) count, and only every 180-350 frames; punching from below at (122,135) does nothing for 2,400
    frames (`r2b_jan_below/s.png`, HP 8880 throughout). Only "Janemba" is in the battle's enemy list - possibly the
    arm / upper-body hit parts the story version has are not spawned in battle (not checked).

24. **Medium - Story: the player walks behind the bottom wall of Babidi's Ship save room (8-41) and disappears.** **Matches the game (not a bug):** the original also lets the player walk there and draws the wall over him; ours matches it pixel for pixel over 910 frames (segments.md entry 20).
    `pt.py r2s_f_walk --view gba:0 --saves seg_f_fat --keys Enter,ArrowDown,Enter --input "N60,R10,D300,N30,D200,N30,R200,N30,L300,N30" --frames 1200 --every 20`.
    Gohan walks to y=351 past the drawn wall and along it (x 40-183), only his hair tips showing
    (`r2s_f_walk/z.png`, `r2s_f_walk/f000740.png`); the walkable area reaches ~2 tiles below the wall
    (`r2s_f_walk/grid808.png`). Not checked against the original.

25. **Low-medium - Battle: in "Super Buu: Wasteland" Super Buu starts on the top ledge off-screen** (266,76), ~160 px
    above the player, and engages only when you climb up; in PC wide the top third is black
    (`r2b_sbuuw_win2/f000010.png`, `f003260.png`).

26. **Low-medium - Dojo: the In-Game roster shows the raw fighter id: "goku Lv90"** (`r2l_dojo/host_frames/f000040.png`;
    two headless apps over a local link). `link.rs` `char_name` matches only the game's text table, so a lower-case id
    falls through as is.

27. **Low - Title -> Multiplayer -> Dojo / Trading Post without a link waits forever** ("Waiting for a friend...", A /
    Start do nothing, B backs out correctly) with nothing telling a player how to set up link play (`r2_dojo/s.png`,
    `r2_trade/s.png`).

28. **Low - PC wide: text boxes are placed relative to the 240x160 game screen**, so in Yemma's Office the box sits
    half over the black void (`r2s_p21/s.png`; follows from 8).

29. **Low - Battle: Pui Pui stands frozen upright ~150 frames after his defeat, then fades, no knock-down**
    (`r2b_ship_pui.png`, frames 200-360). Possibly faithful.
    **Fixed 2026-10-08 (soak agent):** not faithful - Pui Pui's story defeat (8-27, inline 0x0807CDAC) is
    `remove_actor(234)` in the KO frame: he vanishes, no knock-down (his sprite has none; the battle's Idle-death
    "hurt" cel froze him upright). Battles now remove him at the KO (`enemy::IDLE_DEATH_REMOVED`, TS the same);
    `work/pt/pui`: gone at the KO punch (frame 70), Yakon next.

30. **Low - Battle: after the KO, Kid Buu keeps beaming the downed Goku** (`r2b_kidbuu_lose/f000400.png`). Possibly
    faithful.

31. **Low - Web: the F1 view toast ("Classic 240x160 (F1)" / "PC wide (F1)") is drawn over the end screen's "Retry"
    line** (`r2w_battle/13_after_F1.png`); **the HTML "Replays" / "Settings" buttons sit over the game picture** top
    right in PC wide during play (`r2w_battle/04_battle_start.png`, `r2w_quick/01_quick_1.png`).

Not covered yet: the World Tournament itself - from the Hercule City save it is ~10 story segments away (school,
Bulma, Great Saiyaman, circus, Goten's training, Vegeta vs Trunks); `--story 7-1` starts a fresh game there, so the
desk says "Sorry, there's no tournament today" (`r2s_t9/s.png`), which is expected without story progress.

### Round 3 (story): World Tournament chapter -> end of Earth (Earth agent, 2026-10-08)

Played from the `seg_e_tournament` save (Hercule City 3-1, Gohan Lv 64, 31 / 155 HP) to Goku arriving on Papaya
Island 7-2 after Spopovich and Yamu (the Babidi chapter's start): the Hercule City world-map circle, East District 2-8,
Goku's House (Chi-Chi, the "Chapter 3 World Tournament" card), the 2-9 .. 2-11 fights (level 65), the level-65 gate in
2-90, Goten's rock drill (dodged), the fight with Super Saiyan Goten, Trunks vs Vegeta in the gravity chamber 4-25,
Bulma at Capsule Corp ("ready to go to the World Tournament? Yes"), Papaya Island: the drinks stall bought down to 30
zenie, registering Trunks and Gohan at the desk, the punching machine (weak / qualify / "the machine is broken" by
timing), the Junior division in the ring (four fighters, Goten, Hercule's exhibition), the adult division, Videl vs
Spopovich, the infirmary. 179,326 frames; pause pages (Status, Inventory, Equip, Options, Journal) at the end.

How: closed-loop like a player through the agent interface (`wf-mcp`: new `new_game {title: true, saves: DIR}` loads
the app's slot files so the title's Continue resumes them, and world-map observations now carry `field.flight`
heading / altitude to steer by). Its replay converted to an app `--input-file` and played through the **real app**
(`buus-fury --launcher --launcher-keys Enter,Enter`, same slot files, Classic view): **the same end-state hash
(2638bd2e5194d548)**; frames every 30 in `work/earth/r3b/` (contact sheets `/tmp/earthkb/sh/c_*.png` while they last).

**Nothing broke**: no softlock, no missing graphics, menus / shop / minigame / dialogue all worked; the chapter can be
finished. Things a player may report, each checked against the game and left as they are:

32. **Info - 2-10 -> 2-11's left exit leads into a dead-end pocket under the tree canopy** where the player is hidden
    (2-11 (60 .. 88, 365 .. 454)). Emulator (run 1 at 2-10, row 127,700, up through the same exit): the game has the
    same pocket and the same canopy over (87, 368) - faithful. (The agent's `take_exit` picks this nearer exit; a
    player goes up the right one.)
33. **Info - the punching machine reads "That was weak" when A is mashed**: it is a timing gauge (A stops a sweeping
    marker; ~8 frames after it opens gives "702 points ... the machine is broken", ~22 "Not bad! You qualify!").
    Matches the game's minigame model (`menu/minigame.rs`, gated by the recorded runs).
34. **Info - the HUD is hidden while walking after some scenes** (West City / Capsule Corp after the gravity chamber,
    Papaya Island) until a later script shows it: the scripts' `hide_hud` / `show_hud` (pixel-gated slide).
35. **Info - at a stall with too little zenie, A on a row does nothing visible** (30 zenie, milk 50): the refusal at
    the stalls is a silent no-op in the recorded runs too (run 3's ship part bought every stall row down to a refusal;
    exact in logic and sound).

Tooling notes (wf-mcp, not the game): `talk` at the 7-2 registration desk walked to the trigger's edge and reported
"no reaction" (standing in the middle, facing up, A works); `attack` leaves level-up popups open until the next A.

## Checked and looked right

Round 1:

* Battle: punches / combos in four directions for every fighter and form tried (`bf_cmb3_sheetA.png`); dash; block
  (4 damage); Energy Blast; Kamehameha (charge, four directions), Electric Kamehameha, Masenko, Kamekameha, Energy
  Punch, Burning Attack, Flame Kamehameha, Kamikaze Ghost, Banshee Blast, Big Bang (Kamehameha)
  (`bf_h150_vegito39/s_bb.png`), Final Flash in the open, Energy Sword, Dragon Fist (held); Instant Transmission
  (`bg_it2/s1.png`).
* Super Saiyan up / down for Goku, Gohan, Goten, Trunks, Gotenks, Vegeta (halo kept), Vegito: aura centred in both
  views (`bf_tr_sheet.png`, `bg_classic_tf/crop_28_300.png`); gauge drain; no-limit forms do not drain; a hit cancels
  the transformation (`bg_tf_hit/crop_90_132.png`).
* Win / loss screens, Retry, multi-phase presets (Pui Pui > Yakon > Dabura, Time Chamber, `bw_p_htc/s.png`), 4 / 8
  enemies, Uub and Hercule beatable.
* Launcher Battle / Settings / Controls / Story pages. Story: title -> New Game, the prologue (`st_t4/s1.png`), Snake
  Way to Grand Kai's Planet, talking, pause menu pages, Options with text speed taking effect (`st_opt/s.png`), save ->
  Continue (`st_cont/s.png`), Quick start, first fights with zenie / XP / level-up (`st_fight1/s.png`), game over ->
  continue (`st_die/s.png`). Classic and PC wide reach the same state hash on the same inputs.

Round 2 (6a369e5):

* A fresh data folder starts in Classic; the default Goku vs Kid Buu fight shows the SS3 skill (`r2_first/s.png`).
* Bosses: Broly (jump-and-slam with a shadow tracking the player, death, loss: `r2b_broly_win/s1.png`,
  `r2b_lose.png`), Dabura (`r2b_dabura_win/s1.png`), the Babidi ship chain (`r2b_ship.png`), Super Buu win / loss
  (`r2b_sbuu.png`), Time Chamber, Kid Buu's split / recombine / beams drawn (`r2b_kidbuu_win/s1.png`); losses set Lost
  ~200 frames after the KO.
* Story from every run-2 save (`r2s_c_sheet.png`); a new save at the school lists "Gohan Lv64 3-10 0:31" and resumes
  (`r2s_e5/s.png`, `r2s_cont/s.png`); Hercule City -> Orange Star High School (`r2s_e2/s.png`, `r2s_e6/s.png`); the
  Papaya Island drinks stand's shop screen (`r2s_t5/s.png`); later-save pause menu: party tabs, stat points, Equip,
  Options, Journal, Skills popups, Select = dungeon map (`r2s_f_menu/s.png`, `r2s_f_sel/s.png`); Switch Characters at a
  save point (Goku -> Vegeta, `r2s_h2/s.png`); field Super Saiyan (`r2s_f_ss/s.png`); ship soldiers' lasers and the red
  crystal drawn see-through (`r2s_f_fight/s.png`); HFIL Janemba Hand, Instant Transmission's charge (`r2s_h3/s.png`);
  Other World: Grand Kai / Olibu scenes, Battlefield Royale, two Fighters beaten (`r2s_ow3/s.png`, `r2s_ow11/s.png`).
* Title flow: Single Player / Multiplayer -> Dojo / Trading Post and Cancel (`r2_mp/s.png`); Options sliders persist
  across restarts, Credits (`r2_opt/s.png`, `r2_credits/s.png`); Select Game Continue (`r2_cont/s.png`) and Delete Game
  ("Are you sure?", the slot file removed, `r2_del/s.png`).
* Local link play: Dojo (choose character -> Round 1 -> "You Win!!" / "You Lose!!" -> back to the title,
  `r2l_dojo/s.png`, `r2l_dojo/s2.png`) and Trading Post (offers, accept, TRADE COMPLETE!, `r2l_trade/s.png`).
* Web build: launcher, Battle (punches, ki blast, SS aura, "You Win!!" with Retry / Change fighter:
  `r2w_battle/s1.png`), a fresh profile in Classic with Goku vs Kid Buu Lv 150 and the SS3 skill (`r2w_fresh/s.png`),
  Story title -> New Game -> the prologue (`r2w_story/s.png`), Quick start with the pause menu and the F1 switch
  (`r2w_quick/s.png`), Settings view cycling (`r2w_views/s.png`); pixel-for-pixel like the native app for the same
  scenes (`r2w_battle/cmp.png`), 60 fps.

## Fixed (ship fix agent, 2026-10-08; frames under `work/playtest/fix/`)

* **6 - arenas:** a fight with no story place of its own goes to the Rocky Plateau (`launcher::DEFAULT_ARENA`), the
  forest presets to open arenas (Ninja Boss / Airship Warlord: plateau, Pilaf's Guardian: canyon, 4 Majin Fighters:
  crater; online Majin Fighters: crater). The launcher's battles start the opponents in view
  (`BattleConfig::spawn_near`: 80 px in front of the player, a story spawn point only when that close; None keeps the
  reference placement the TS mirror checks). Every listed opponent starts on the GBA screen (`examples/opponents.rs`);
  `f6_sheet.png` (Majin Fighter, Assassin, Kid Buu, 4 Majin Fighters, PC wide), `arenas_gba.png`.
* **10 - opponents:** only real opponents with game names (`launcher::NOT_OPPONENTS` / `OPPONENT_NAMES`): no dummy,
  floor trigger, inert helpers, unused fighters, body parts / summons, never-fighting story actors (Babidi, the two
  Vegetas, tag-game Goten, the Majin Buu survive fights - their presets stay), no duplicate "(variant)" / disguised /
  tournament-copy records; 150 entries -> 105, each played headless (`work/shipfix/opponents_after.log`). Online list
  names fixed (it called Totenhotep "Broly", Dabura "Demon World boss").
* **8 / 20 - PC wide:** the view is clamped to the map (`Renderer::view_origin_in`; a map smaller than the view is
  centred, the world map is not clamped), "PC wide (auto)" is one zoom per window size (`steady_zoom`), menus scale
  up by the largest integer: `f8_sheet.png` (Yemma's Office f8,800 / f10,800, Snake Way f6,660, before | after).
* **18 / 21 - launcher:** "Arena Auto: <name>"; Goku starts in his normal outfit; the stat line fits at any level;
  Story / Settings / Mods pages in the game's menu style; Continue slots name the area (`lp_sheet.png`, `lp_sheet2.png`).
