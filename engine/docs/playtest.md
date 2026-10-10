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

9. **Medium - Story: wandering souls walk into Goku, block him and take his A-button talk.** **Matches the game (not a bug):** the soul is never inside Goku - it stands just above him (1-4: soul (120, 400) 16x8, Goku (120, 409)), drawn behind him by depth, blocks the step up and is in the facing-up talk box, so it answers A; under the office ogre Goku was 32 px below him, out of the ogre's talk reach. The original does the same (emulator from run 1 in 1-4: Goku stopped under a soul at (128, 400), souls answering A: `work/earth/pt_soul/emu_soul.png`); ours matches that recording whole (9,238 frames; segments.md entry 19).
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

- **Info - #9 and #24 re-reported, both faithful:** the soul "inside" Goku at the 1-4 stairs (only its outline
    shows behind his head: `r2s_q3/halo.png`) stands just above him and answers A - the emulator shows the same
    picture and talk (`work/earth/pt_soul/emu_soul.png`); the 8-41 bottom wall hides the player in the original too
    (segments.md entries 19 / 20). Both marked in their items above.

Tooling notes (wf-mcp, not the game): `talk` at the 7-2 registration desk walked to the trigger's edge and reported
"no reaction" (standing in the middle, facing up, A works); `attack` leaves level-up popups open until the next A.

### Round 3 (playtester): re-verify on c67e5ec, broad Battle, story from saves (2026-10-08)

The play build of c67e5ec (`work/playtest/bin/buus-fury-c67`), headless + muted, Classic (default) and PC wide; web
rebuilt from a clean export of c67e5ec in headless Chrome. Runs `r3b_*` (battle), `r3s_*` (story / launcher / web).

**Verified fixed:** 1 Yemma's Office softlock (Goku walks free right after the ogre, `r3s_p18`); 2 / 23 Janemba
(dies in < 300 frames, Super Janemba follows, won at f1510: `r3b_bosses1.png`); 3 opponents approach (Kid Buu walks
in and beats an idle Goku: `r3b_sheet1.png`); the survive countdown (top right, ends "You Win!!" at 00:00); 5
volleyball (ball drawn, a right punch sends it right: `r3b_vb/z.png`); 6 / 25 arenas and spawns on screen
(`r3_t1/s.png`, `r3b_sheet4.png`); 7 shield bubble (`r3b_bg_shield/z.png`); 8 PC wide clamped (`r3b_sheet4.png`,
`r3s_gk/s.png`; maps smaller than the view centred: `r3s_wide.png`); 11 fusion moves usable at the auto level
(Gotenks vs Majin Fighter now Lv 125, SS3 skill, 924 EP; Vegito 1167, Gogeta 483); 12 Final Flash from the Wasteland
start (`r3b_sheet2.png`); 13 Unstable Trunks (transforms ~f200); 14 end-screen lock (mashed A picks nothing, one A
after a pause retries: `r3b_bw_win`); 15 two players (two HUDs, 1P / 2P labels: `r3b_sheet3.png`); 16 Great
Saiyaman (skills 7 / 18 / 6 / 17, toggles back); 19 "You Lose!!" placement (`r3b_r2b_kidbuu_lose/zoom_100_900.png`);
20 one zoom per window (all story frames 480x320, Select Game scaled up: `r3s_p18/s0.png`); 22 Kid Buu false win (won
only when the split copy dies, f~4950: `r3b_kb_win/end.png`); 26 Dojo names ("Goku Lv90" / "Vegeta Lv60",
`r3s_ldojo2/s.png`); 28 text boxes on the map (`r3s_gk/s.png`); 29 Pui Pui removed at the KO (`r3b_ship`); 31 web
layout (buttons in a bar above the picture, F1 toast top right clear of Retry, no instant retry: `r3s_web_battle/s.png`).

**Still broken:**
* 9 (medium) - souls still answer A instead of the ogre (`r3s_p21/s.png`, f011020-f011600) and a soul inside Goku
  keeps him at (120,409) through 300 frames of Up at the 1-4 stairs (`r3s_q3/s2.png`, f006480 / f006720).
* 24 (medium) - Babidi's Ship save room: Gohan walks behind the bottom wall at y=351, x 40-183, only hair tips showing
  (`r3s_f_walk/s.png`, frames 400 / 740 / 1000).
* 10 (medium, partly fixed) - the list is clean except **Janemba Hand**: still listed, invisible, 50/50 HP, the fight
  never ends (`r3b_sk_goku2/sk.png`); names cut off ("MAJIN BUU & BABIDI (S..", "TIME CHAMBER > SUPE..", "GOTEN (TIME
  CHAMBE..", "SUPER BUU (TWO-PHA..", "HUMAN (WORLD TOUR..") and repeated labels (BROLY x2, YAKON x2, AIRSHIP WARLORD x2,
  MAJIN VEGETA x2) with nothing telling them apart (`r3b_opps/s1.png`).
* 30 (low) - Kid Buu's beam over the downed Goku stays frozen through "You Lose!!" for 800+ frames
  (`r3b_r2b_kidbuu_lose/zoom_100_900.png`); see also 36.
* 17 (low) - Dragon Fist tap still spends 300 EP for nothing (`r3b_sheet2.png` f40).
* 21 (low, partly fixed) - the Story page is in menu style with area names, but the Continue line is cut off:
  "Gohan Lv64 Hercule City 0:3.." loses the play time, "Goten Lv110 Nataday Village .." loses it entirely
  (`r3s_lp/s.png`, `r3_t2/launcher_000.png`).
* 27 (low, partly fixed) - the waiting box now says "No friend connected yet. Play: --link host:ROOM / join:ROOM, Web:
  ?link=host:ROOM / join:ROOM": command-line flags a player of the packaged app cannot use, and the text covers the
  title's menu item and the copyright line (`r3s_dojo/s.png`, f000300).

**New (round 3):**

36. **Medium-high - Battle: a Kid Buu beam hangs in the arena forever.** `chase.py r3b_kb_win --battle
    goku:5:kid_buu:1::200 --view pc:0 --reach 30 --ytol 8 --frames 5000`: a vertical cyan beam stays at one world spot
    from ~f2800 (right after the main Kid Buu merges into the split copy) to the end, still there behind "You Win!!"
    (`r3b_kb_win/zoom_1600_4600.png`, `r3b_kb_win/end.png`). Expected: the beam ends with its attack / owner.
37. **Medium - Battle: Worm Head never fights.** `r3_opps.py goku:0:t142` (900 frames idle, Lv 145): it wanders
    off-screen to (392,138), never attacks, the player's HP unchanged (`r3b_op_t142/l.png`). Expected: it engages like
    the other opponents now do.
38. **Medium - Battle: the Majin Buu survive fight kills an idle player at its automatic level (100).** Lost at f~600
    (`r3b_op_majin_buu`): a survive fight a player cannot survive by default. (Round 2 had nothing happening at all.)
39. **Low-medium - PC wide: the prologue's small maps shrink to a 240x160 panel** in a mostly black window (Goku's
    house, the Lookout, whose text box floats in the black above the scene): one zoom per window now
    (`r3s_p18/s0.png`, f001000 / f001500; repro `pt.py r3s_p18 --keys Enter,Enter --input "$(cat st_inputs/st_p18.txt)"
    --frames 12700 --every 50`).
40. **Low-medium - Battle: the Janemba arena ends with Goku hidden behind a foreground crystal / jelly bean** - "You
    Win!!" with no player in sight (`r3b_jan_win/zoom_1450_1650.png`).
41. **Low - PC wide shows maps' unpainted black margins**: clamped to the whole map, it reaches past where the game's
    camera goes - Hercule City ~40 px black at the top, ~30 at the bottom (game camera y 75-235, painted ~40-290;
    `r3s_wm/s.png` f000180, repro `pt.py r3s_wm --saves seg_e_tournament --keys Enter,ArrowDown,Enter --input
    "N60,U10,L110,N120" --frames 300 --every 20`); Snake Way a black top-left corner and left column, native and web
    (`r3s_p18/s.png` f006650, `r3s_web_quick/s.png` 01 / 09). Expected: clamp to the area the game can show.
42. **Low - web: the pause menu is not scaled up in PC wide** (960x640 Chrome: picture 480x259, the menu stays 1x, a
    quarter-size panel in black: `r3s_web_quick/s.png` 10_pause_menu); native fills the view.
43. **Low (possibly faithful)** - Super Buu's ki balls burst just in front of a knocked-back player (~115 px) and do no
    damage, so an idle fight stalls (`r3b_sbw2/z.png`); the Ghost turns invisible (eyes + shadow), sits on Goku and
    never attacks in 900 frames (`r3b_op_t32/l.png`); Dabura holds Goku in a hit-stun loop 500+ frames (548 -> 236 HP),
    sprites overlapping (`r3b_ops2.png`, middle row). Not seen: Dabura's sphere attack.

**Looked right:** skill sweeps for 13 fighter / form combos (Goku halo and SS3 no-limit, Mystic Gohan, Great Saiyaman,
Goten, Trunks, Vegeta halo SS, Gotenks SS3 no-limit, Vegito, Gogeta, Hercule): every skill fires, transformations up
and down, Energy Sword and Kamikaze Ghosts drawn, both views (`r3b_sk_*/sk.png`); new enemy shots - Super Buu's
stretch punch / beam / ki ball (`r3b_op_super_buu/sh.png`), Mechanoid arm, Totenhotep's bandage (`r3b_ops2.png`); ten
varied opponents approach and fight (ninja, Shinobi, Grenadier, Sniper, Tank, Pharoah, Mega Cyborg, Broly, Airship
Warlord, Totenhotep). Story: the new launcher pages take the same keys; Continue from Grand Kai's Mansion, Hercule City,
Nataday, HFIL and West City saves; pause menu pages; save point menu; field Instant Transmission; a save at Hercule City
-> Continue resumes there (`r3s_sv.png`); Grand Kai's Mansion -> Planet with Olibu. Web: launcher, a battle won with SS,
F1 both ways, Quick start, the pause menu.

### Round 4 (playtester): fan-level details on 71105e5 (2026-10-09)

The user's play build (`engine/target-play/release/buus-fury`, commit 71105e5; frozen as
`work/playtest/bin/buus-fury-r4`), headless + muted; the PC menus driven with `--host-keys FRAME:Code` /
`--host-mouse FRAME:x:y:click` (mouse x / y are view pixels, not window pixels). Looked for small animation /
mechanics / sound / UI inconsistencies; compared with the emulator (`tools/emu/emu.py`, headless, no audio out)
where a matching save state existed. Runs `r4b_*` (battle), `r4s_*` (story / menus). No doubled sounds in any round
2-4 audio log apart from 46.

44. **Low-medium - Esc menu: the panels have no fill, the game shows through the text.** In Story the sewer grates,
    NPCs, Gohan and the manhole sit inside the PAUSED / Settings / QUIT? boxes (`r4s_esc3/z.png`, `r4s_quit2/s.png`);
    in Battle the fighters and a damage number (`r4s_esc4/s.png`, `r4_t1/f000150.png`). Repro: `pt.py r4s_esc3 --saves
    seg_e_tournament --keys Enter,ArrowDown,Enter --input N500 --frames 500 --every 10 -- --host-keys 100:Escape
    --host-mouse 130:160:107:0,160:160:107:1`. Expected: a solid panel like the game's menus. (New UI: no emulator
    counterpart.)
45. **Low-medium - Battle, PC wide: "You Win!!" is placed for the 240x160 screen around the player**, while the
    Retry box is centred at the bottom of the wide view: in a corner fight the title lands on the fighters (Trunks vs
    Broly, `r4b_cf_trunks/f001700.png`), in the open it floats to one side (`r4b_cf_goku/f001600.png`; both in
    `r4b_cf_s.png`). Repro: `chase.py r4b_cf_trunks --battle trunks:22:t10:1::150 --view pc:0 --frames 2400 --every 1
    --blast 3` (win at f1591). Expected: centred with the box.
46. **Low (not compared) - a red "0" damage number over the player on a trade, and a doubled whoosh.** Gohan and Super
    Buu hit each other on the same frame: both flash, Buu takes 3533, Gohan shows "0" (`r4b_cf_gohan/z_106_128.png`,
    f116-f128); sound 51 plays at f115 and f116 - the only near-double in a 2,400-frame fight. Repro: `chase.py
    r4b_cf_gohan --battle gohan:16:t116:1::150 --view gba:0 --frames 2400 --every 1 --blast 3`.
47. **Low - the "Esc: Menu" hint is drawn over the game's own text**: the title's copyright line, Select Game's
    Select / Cancel legend, the area-name banner; it stays full-bright through the fade to black at a door
    (`r4s_title2/cmp.png` f190 / f272, `r4s_mart/t.png` f152-f176). Repro: `pt.py r4s_mart --view gba:0 --saves
    seg_e_tournament --keys Enter,ArrowDown,Enter --input "N60,L8,U110,N120" --frames 400 --every 4`.
48. **Low (not compared) - four quick punches land two hits.** Presses at f55 / 63 / 71 / 79 on a Majin Fighter: 1 and 3
    hit (50 / 21, sounds f56 / f72), 2 and 4 visibly reach the body and do nothing (`r4b_punch/z_54_93.png`); likely
    the enemy's post-hit invulnerability (the emulator try was muddled by a crowd: `r4b_emu/punch/s.png`). Repro:
    `pt.py r4b_punch --view gba:0 --battle goku:0:t68:1:plateau:100 --keys ArrowDown,Enter,Enter --input
    "N20,R30,N5,A2,N6,A2,N6,A2,N6,A2,N60" --frames 140 --every 1`.
49. **Low (not compared) - silent casts:** Kamikaze Ghost and Gotenks turning back from Super Saiyan play no sound at
    launch (hits do): `r4b_gt34R.sum.txt` (casts 662 / 774 silent, hits 719 / 907).
50. **Low (not compared) - the energy blast plays sound 20 or 21 by facing / form** (Goku right: 20 then 21; up: 20
    twice; `r4b_gk0R.sum.txt` vs `r4b_gk0U.sum.txt`) - maybe the game's random pick.
51. **Info - the arena always gives the top skill level**, so the Kamehameha is always the big yellow level-3 beam; the
    game's level-2 beam is thin and cyan (`r4b_emu/sb_up/kame.png` vs `r4b_gk5U150/z_770_830.png`). Faithful to level 3,
    but fans may miss the blue one.
52. **Info - "Quit to main menu" asks "progress will be lost?" only after 600 frames since a load / save** (before that
    it quits at once; after, it defaults to Keep playing: `r4s_quit` vs `r4s_quit2/s.png`). Intended, noted.
53. **Info - menus answer 2-6 frames sooner than the emulator** (START 102 vs 108, Cancel 332 vs 338, Select Game's
    figure every 8 frames vs 10; `r4s_gk/cmp2.png`): the GBA's slowdown, dropped on purpose (no-hardware-lag).

**Matched the game (emulator):** the title -> Select Game -> Continue flow frame-aligned (`r4s_title3/cont.png`);
walking and the Start menu pages with cursor sounds (`r4s_gk/cmp1.png`); a 3-press punch combo gives two swings in both
(`r4s_fat1/p.png` vs `r4s_emu/punch_s.png`); the Kamehameha fired up with hair / halo over the beam's base
(`r4b_emu/sb_up/kame.png`). **Looked right:** the Esc menu by keys and mouse (hover moves the cursor, a click on
Restart fight restarts: `r4b_esc3`; Settings, the view toggle mid-story, Controls, the quit confirm); energy blast and
Kamehameha in four facings (from the hands, stopped by walls, layering right); Gotenks' ghosts and volleyball; the new
Dragon Fist (head on the steered end, body trailing to Gogeta: `r4b_gg41R/z_774_900.png`; the game's only dragon in
reach is cut short by its target, so not frame-compared); five of six closed-loop fights won with the end screen.

### Skill audit (2026-10-09): every player skill against the emulator, frame by frame

Method (`work/skillaudit/`): 528 scenarios from the lockstep setup (`gen.py`): every character's forms, the base
form in all 4 facings, every skill tapped then held at every level (other forms: top level, facing right), punches /
combo, dash, block, the steered moves (Dragon Fist, Energy Sword, Masenko, Instant Transmission, Big Bang tap), the
transformations (SS, SS3, Saiyaman) and back. Each is recorded from the real game as a segment (`skillseg.py`, the
Hyperbolic Time Chamber, no enemy), its every video frame dumped, and our picture compared pixel for pixel
(`examples/pixels.rs` with `PIX_ALL` / `PIX_NOPROPS`); state and sound calls through the segment harness. Not covered:
hits taken / knockback poses and skills landing on an enemy (a target dummy's patched AI diverges in the import;
the story pixel gate covers hits).

| Character | Exact frames (after) | Fixed | Left (small / acceptable) |
|---|---|---|---|
| Goku | ~97% | Energy Blast sorted with the player by depth; form 1 (NPC sprite) shows only the shadow for moves it has no animation for; Instant Transmission: camera during the pick, cursor at the footprint centre, ring timing; Kamehameha fired up behind the hair / halo | Kamehameha charge first frame, beam head f130-160, the up beam's tip; 1-5 frame HUD gauge blips |
| Gohan | 32372 / 33722 (was 28512) | Masenko: cursor 12 px too high, ball made on the press, cursor on the press frame, cursor / ball behind Gohan facing up | Electric Kamehameha (shared beam drawing, 2-16 px); Masenko launch ball size; icon grey timing |
| Goten | 12811 / 13774 | Kamekameha charge sparkle a frame late; beam piece variants a frame late; charge behind Goten facing up | the up beam's start piece over the hair |
| Trunks | 17109 / 18997 (was 16325) | Energy Blast / Burning Attack depth sorting; Flame Kamehameha charge on the press, behind Trunks facing up | Flame Kamehameha up f163-236 order; forms 23 / 24 start states broken in the emulator capture |
| Vegeta | 19292 / 19462 | Energy Blast depth; Final Flash affine sampling 1 px; Big Bang hits the wall a frame early, icon greyed until its explosions end | Final Flash down / left 1 px while scaling, its first frame; icon flicker on the press |
| Gotenks | 21744 / 24647 (was 15843) | Kamikaze ghosts: shadow, easing a frame early, cloud 8 px off, ghost / cloud depth sorting; Ultra Volleyball wave 8 px too long, tail / head order | f32 / f33 fusion defuse visuals (script tint, flash, partner walk-off) |
| Vegito | 13965 / 14344 (was 11271) | Energy Sword charge a frame early, slash records follow the player's frame; Banshee Blast body phase (beam tick from the press) | charge tile-upload tears; colour flicker at the screen top |
| Gogeta + extras | Dragon Fist steer 214 / 215 (was 37) | Dragon Fist drawn before the frame's push, fixed head -> root order, depth by Gogeta's footprint, dying head timing, body easing after the launch; Big Bang Kamehameha flare timing / order, ball lifetime; defuse facing | f42 / f43 defuse visuals (not the game's s014 / s503 scripts); equal-depth explosion stacking |

Round 4 items: the Energy Blast sound depends on tap (21) vs hold (20), the same in every facing, as in the game; four
fast punches landing two hits is the game's (presses 1 and 3 damage a dummy); Kamikaze Ghost's cast and the SS
revert are silent in the game too (only the aura loop 54 stops); the red "0" on a trade with a double swing sound was
not reproduced (no same-frame trade scenario yet). Gohan's SS from Great Saiyaman (form 14) softlocks the real game
(invisible in the aura); ours ignores the press - kept.

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

**Round 3 fixes (ship fix agent, 2026-10-08; frames under `work/playtest/`):**
* 39 / 41 - PC wide: the view stays inside the map's **showable area** - its painted part (the camera-locked layers' tiles
  with a pixel) within what the game's camera can show (`MapView::frame`: the field camera limits + 240x160, an arena's
  view; `Renderer::view_area`), and "PC wide (auto)" zooms in where that area is smaller than the window's view
  (`fill_zoom`, capped so the view never gets smaller than the GBA screen; big maps keep the window's zoom).
  `fix3_sheet.png`: Goku's house and the Lookout fill the window, Hercule City and Snake Way without black margins.
* 40 - battle: the players show through foreground scenery (half blended, `RenderView::see_through`; not in the story)
  and the end screen goes to the screen half the player is not in: `fix3_jan_sheet.png`.
* 42 - a menu screen (title, Select Game, pause menu) in PC wide is shown like Classic, scaled by the largest integer
  the window takes: web 960x518 3x (`fix3_web_pause.png`; `engine/web/tests/pause_probe.mjs`).
* 21 - the Continue line puts the play time before the area: `fix3_lp_sheet.png`.
* 27 - the title's waiting box says how to play with a friend in player terms, at the top clear of the menu; the
  launcher's Story page has **Link play** (Off / Host a room / Join a room) and **Room** (0000-9999) - the title then
  starts with link play (native and web): `fix3_link_sheet.png`.

## Opponent audit

Every Battle-list opponent against the emulator, frame by frame from the same state (`tools/battle/lockstep/visual.py` records `oa_tXX_{idle,left,above,below,punch,ki,ko}` in the Southwest Forest 8-3 clearing - the
emulator screen every frame plus the opponent's animation, facing, flash, AI state and owned objects;
`examples/opp_audit.rs` replays each in the faithful core (arena rules off) and reports the first divergence per
category and the pixel difference; usage in `tools/battle/LOCKSTEP.md` "Visual audit"). Fixes are in the shared
core (Story and Battle) with the TS reference mirrored. Harness-only differences (the emulator player levels up
from field XP on a kill, no field item drops, torn single emulator frames, the KO'd boss kept standing for a
story scene) are not divergences. Shared fixes: damage-number digit centring (crit over an enemy rounds the
other way), the HUD bars read at the HUD object's place in the update pass, the Energy Blast burst position.
Pixel-gate fixes found on the way: shadow byte offsets added as the OAM does (dx unsigned 9-bit, dy 8-bit wrap;
no shadow for an off-screen actor - Dabura / Yamu), an Energy Blast fired up drawn in front (earth_2_10_2 f690).

| Group | Type | Opponent | Matches | Fixed | Remaining | Arena-only |
|---|---|---|---|---|---|---|
| G1 | 60 | Kid Buu | position, facing, HP, AI states, damage, animation raw/frame through all approaches; left/ki pictures | charge sparkle for an enemy's charging beam (view.rs, Kame charging); ki burst offset (view.rs); L2: beam-hit clouds sorted with the actors (layer 0x38, behind the beam - shots.rs Kame hit), the enemy beam object's cel phase a frame ahead (view.rs, vt + 1) | L2: after the release only the hit frame (impact a frame early in the game) and the charge ball's cel / position on 2-3 frames remain (oa_t3c_above OBJ-diff 231 -> 61 frames, the rest the top-edge clipping and damage digits); Kid Buu over the player at an equal depth: equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments) | ko scenario: the battle world has no defeat script (special death: we remove him with an explosion; the field keeps him for the story scene) - harness, story path unchanged |
| G1 | 116 | Super Buu | everything in below/ki/left; position/HP/AI everywhere | stretch arm + candy beam sort as actors by their y; candy beam OAM order muzzle, head, body; muzzle/head gfx switch phase; muzzle kept while retracting (view.rs) | above f92: candy beam fired down now drawn over his legs (view.rs; was the 'stretch-arm piece'), f105.. the player's candy cel colour differs (palette, not chased); punch: player's combo raw 5 vs 7 at f174 after an RNG drift (one draw); animation tick counter not modelled during his charge poses (not drawn) | ko: Idle death - battle world shows knockdown + fade, field keeps him (story scene) - harness |
| G1 | 115 | Super Buu (Time Chamber) | ki fully; position/HP/AI/damage everywhere | as 116 | above: player HP 308 vs 307 at f347 (player-side HP rounding of the same damage 218785, not opponent-specific); tick-only anim counters | ko: as 116 |
| G1 | 121 | Super Buu (inside Buu) | ki/above fully; position/HP/AI/damage | as 116 | below f321: the game's blast-hit rand + sfx 21 on the KO'd player - a ki blast ours dropped at the screen edge; ki blasts now leave only past the edge they fly to (0x08019B7C, shots.rs + TS) - f321 still open (blast count matches up to f254); tick-only counters | ko: as 116 |
| G1 | 118 | Super Buu (Gotenks absorbed) | left/ki fully; position/HP/AI/damage | as 116 | idle/below post-KO sfx 48 + rand: fixed (it was his ki blast fired from beyond the screen edge, culled by ours - shots.rs ki-blast past-edge test 0x08019B7C + TS; oa_t76_idle player/anim/ai exact); above f193 one-frame piece | ko: as 116 |
| G1 | 40 | Gotenks | above/left/punch/ki pixel-exact; everything else identical | ki burst (view.rs) | idle f568 the idle blink 1 tick late (his idle tick freezes after a stand-still node where the game's runs on); below f133 one frame; L4: the blink "1 tick late" was the battle sim freezing every enemy from KO + 200 - the game still runs their state tick (world.rs ko_battle_ticks, TS mirrored); idle now exact but a torn frame | ko: Idle death (battle-world knockdown+fade vs field stays) - harness |
| G1 | 35 | Gohan | above/below/left/punch/ki pixel-exact | ki burst | idle f460 blink 1 tick late (as Gotenks); L4: the blink "1 tick late" was the battle sim freezing every enemy from KO + 200 - the game still runs their state tick (world.rs ko_battle_ticks, TS mirrored); idle now exact but a torn frame | ko: as Gotenks |
| G1 | 90 | Piccolo | above/left/punch/ki pixel-exact | ki burst | idle f460 / below f345 blink tick (as Gotenks); L4: the blink "1 tick late" was the battle sim freezing every enemy from KO + 200 - the game still runs their state tick (world.rs ko_battle_ticks, TS mirrored); idle now exact but a torn frame | ko: as Gotenks |
| G1 | 37 | Goten | all mechanics identical | - | harness: the emulator's spawn record uses gfx 124 (SS Goten), our battle world the type's first gfx 123; the field uses the placement's sprite (field.rs) so Story is right | ko: as Gotenks |
| G1 | 39 | Goten (Time Chamber) | above/ki/left pixel-exact; mechanics identical | ki burst | below f283 / punch f352 single frames | ko: as Gotenks |
| G1 | 130 | Trunks | above/below/left/punch pixel-exact | ki burst | ki f92 one frame | ko: as Gotenks |
| G1 | 70 | Majin Vegeta | below/ki pixel-exact; position/HP/AI/damage | ki burst | punch: RNG drift at f417 near the end; above f273 one piece | ko: as Gotenks |
| G1 | 132 | Uub | above/below/left pixel-exact | ki burst | - | ki/punch/ko: the kill goes through the battle-world Idle-death path (knockdown + fade) where the game keeps him (story scene) - harness |
| G1 | 45 | Hercule | above/below/left pixel-exact | ki burst | - | ki/punch/ko: as Uub (kill path) |
| G2a | 51 | Janemba | idle/left/above/below/punch/ki: anim, AI, parts, hand placement, damage match; OBJ diffs 598->16 frames (idle) | view.rs draw_key (arm parts 0x34/0x35 own depth, Hand depth y-1); enemy.rs hand_update (no fade-in); TS enemy.ts HandRise alpha | damage digits (G4); part damage not logged by ours (harness); ascent anim 20 set a frame early in state only (picture matches); fight scenario diverges 342 after player level-up from field XP (harness); KO: field level-up dialog (harness) | - |
| G2a | 18 | Dabura | idle/left/above/below/ki/punch/long all exact (0 OBJ-diff frames, see-through excluded) | nodes.rs PlayAnim12/12b (first-update SetAnimRaw 0x12 under loop3 + height -1/+1 to -20/0), DarkSphere no anim change + ground centre; shots.rs sphere: shows the pre-update radius, circle-vs-body-box test (0x0803AF58) + knockback dir, unclamped growth then +128 expansion; L3: the hit test with the radius in whole pixels squared (oa_t12_fight f805: the expansion's hit a frame later, no extra hits while knocked away), the sphere gone at 127 px (f924: IfSphereDestroyed / GotoState 0x7a and the descent a frame earlier) - fight now exact in every logic category; TS ai.ts + projectiles.ts mirrored | KO: game freezes on hurt cel + level-up dialog vs ours knockdown (Battle has no defeat scene) | see-through players behind scenery (battle-only PC aid, excluded in harness) |
| G2a | 122 | Super Janemba | idle/long/ki exact; left/below only damage digits/HUD | nodes.rs SuperJanembaWave: object position + table 0x08759440 (controller 0x0804DBCC tick 24); TS ai.ts mirrored | (the post-KO diagonal walk, above f251: no longer diverges - above is exact in every logic category after the other KO / AI fixes); wave controller tick not mirrored in the anim tick (representation); isolated raw 18/19 frames in fight (same pose) | - |
| G2a | 56 | Mini-Janemba | position, AI, cels; left exact, idle 156->5 OBJ-diff frames | fixed.rs HOP_ARC8 = ROM table 0x087B6EEC -(-v)>>13; nodes.rs hop_height (counter before +8, height kept on the ground) for MiniHopToward/Flee/Hop + field mini; enemy.rs set_anim_rule: minis height 0 when their hop/roll controller is replaced (exit 0x0804F750/784); TS ai.ts/enemy.ts mirrored | roll counter shown as anim tick in the game (representation); KO/fight: field level-up (harness) | - |
| G2a | 145 | Yakon | punch exact; summon: Yakon pixel-exact through the summon (OBJ-diff 0) | nodes.rs PlayAnim12Yakon (first-update SetAnimRaw 0x12, controller cel=tick>>3, y-=0x80/update, raw 21 loop3 at the end); Hop enter SetAnimRaw(0x0E) + cels 0/1/2 + ROM hop arc; L3: the Mini-Yakon Hop update 0x08035274 (at each hop's start: within 8 px on the minor axis SetAnim(0), face the player, done - else a 1.5 px step along the minor axis while tick > 0x80) - oa_t91_summon exact in every category, 900 frames, 0 OBJ-diff; TS ai.ts Hop mirrored | idle/left/above/below diverge only after emulator lag frames (Yakon scenes lag every ~9 f: harness); OBJ partly above the screen top differs by a few px (renderer edge, also seen with a rock prop); L4: the rock over Yakon's horn at the top (idle f166 ..) is the harness-drawn map prop; not chased (Story draws props itself, the pixel gate passes) | - |
| G2a | 96 | Pui Pui | idle/left/below/long: exact (HUD/top-row tear only); above: 1 HUD pixel | - | one punch / ki blast kills him in this scenario (low HP): the kill frame's knockback + RNG and the field level-up differ (kill-HP remainder: G4; level-up: harness) | Battle KO: ours removes him at the KO (his story defeat script) - the game's field leaves him standing |
| G2a | 10 | Broly | above/ki exact; idle/left/below/punch/ko/long: 1-7 frames (damage digits, top-row tear) | - | fight: diverges after f978 (player hit sequence after the field level-up: harness); damage digits (G4) | - |
| G2a | 142 | Worm Head | head + every segment: position, cel, facing match | - | segment overlap order: equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments); harness duplicate body | - |
| G2a | 6 | Blister | Blister, Enzymes: positions, cels, AI match through all scenarios | - | Enzyme/player equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments); damage digits (G4); KO: field level-up (harness) | - |
| G2a | 100 | Shadow Colossus | idle/above/below/left/ki/punch/long pixel-exact (0 OBJ-diff frames) | - | fight: kill at f931 (G4 kill remainder / field level-up harness) | - |
| G2b | 0 | Airship Warlord | idle/left/below/ki/punch exact (state, anim, AI, pixels) | nodes.rs RunMotionController enter keeps a running walk | ko: kill/death -> G4; HP-bar tip 3px (HUD, all opponents) | none seen |
| G2b | 112 | Spopovich | above/below/left/punch exact after fixes; anim raw 5/6/7 = same ROM anim (harness now canonicalises) | view.rs actor_depth (player/enemy sort by y + foot_h/2); opp_audit canon raw | ko -> G4; damage digits -> G4 (masked with OA_NODN) | none seen |
| G2b | 148 | Yamu | all 7 exact after fixes except ko | content.rs shadow_off dy signed | ko -> G4 | none seen |
| G2b | 91 | Pikkon | idle/left/above/below/ki/punch exact | view.rs draw_key (enemy KiBlast class sorted like the player's) | ko -> G4 | none seen |
| G2b | 49 | Idasa | all exact except ko | (shared fixes) | ko -> G4 | none seen |
| G2b | 85 | Olibu | idle/left/above/below/punch exact; ki exact until the kill | - | kill remainder + ko -> G4 | none seen |
| G2b | 59 | Tournament Fighter | all exact; ki exact until the kill f361 | - | kill remainder + ko -> G4 | none seen |
| G2b | 129 | Pharoah Totenhotep | idle/left/above/below/ki exact; punch exact in state | shots.rs wrap on_message crit=stale-true + TS projectiles.ts; shots.rs wrap stick position next frame + TS; shots.rs wrap_burst explosion_sorted | punch f295-308 / 415-425: shake offsets of props + stuck bandages (tried drawing stuck bandages unshaken: no improvement, reverted; props = harness) | none seen |
| G2b | 80 | Ninja Boss | all 7 scenarios exact in state for every frame (idle through 600: approach, teleport puffs, align, shuriken); above / left / ki / punch pixel-exact; puffs now in the game's order (no puff pixel left) | the f581 "align end" was the harness: in the field Southwest Forest 8-3's stump (prop 0, a solid breakable, hit box 337..367 x 208..224) blocks his align step, the node ends and he throws - the comparer now marks the map's solid props on the grid as the field does (opp_audit.rs); Story already has it | idle: 2-3 px where he stands against the stump at the screen top (the comparer draws props in front, unsorted: harness); below f83 / f223: HP-bar drain pixels (player HUD); ko: level-up (harness); torn top-band frames f107 / f127 (harness) | none |
| G2b | 63 | Pilaf's Guardian | all exact except ko | - | ko -> G4 | none seen |
| G3 | 1 | Annihilator | movement, facing, AI, HP/damage, flame spawn + hit timing, death | - | flame red/late on hurt player (VRAM tile sharing with the player's hit flash, cosmetic 1-2 frames, not modelled); tick-only; damage number sums hits (G4); kill: HP remainder (G4) + level-up (battle sim has no XP: harness) | none seen |
| G3 | 3 | Atomizer | AI cycle, beam timing/hits, HP, facing | - | tick-only; tiny overlap pixels; kill HP remainder (G4); level-up (harness) | none seen |
| G3 | 5 | Bio Mech | everything incl. pixels (idle/left exact) | - | tick-only (cel same) | none seen |
| G3 | 113 | Super Bio Mech | all after fix | sprite = game's default graphics 173 (wf-core content.rs resolve_enemy_gfx; TS data.ts PLACEHOLDER) | kill HP remainder (G4); level-up (harness) | none seen |
| G3 | 131 | Ultra Bio Mech | all until player KO | - | post-loss frames (harness: battle sim ends at Lost; Story uses the field path) | none seen |
| G3 | 8 | Bomber | bombs, blasts, AI, HP, facing | - | item drop (field-only; harness); kill HP remainder (G4) | Battle mode: no drops (intentional) |
| G3 | 67 | Mad Bomber | all | - | level-up (harness) | none seen |
| G3 | 17 | Cyborg | all after fix | CirclePlayer: SetAnimRaw(1)+reversed walk unless already (wf-core enemy.rs circle_player; TS enemy.ts circlePlayer) | punch f354: game shakes from an uninitialised crit byte on the punch's stack message (game bug, not modelled) | none seen |
| G3 | 48 | Hyper Cyborg | all after fix | back-off walk (as Cyborg) | shadow at top screen edge (renderer edge case, 1 row); HUD bar drain pixels (player HUD); L4: enemy shadows wrapped in at the top with < 5 OBJ rows on screen are dropped (wf-render SpriteEffect::TopCull); above f23 HP bar a frame early (open, as Majin Fighter) | none seen |
| G3 | 74 | Mega Cyborg | all after fix | sprite 173 (content.rs / data.ts); back-off walk (as Cyborg) | game draws it invisible + VRAM garbage during its shot (sprite 173 lacks raw 0x12); ours shows the idle pose (game glitch, not replicated) | none seen |
| G3 | 19 | Death Machine | all until player KO | - | as Annihilator; post-loss (harness) | none seen |
| G3 | 20 | Destroyer | all | - | as Annihilator | none seen |
| G3 | 41 | Grand Mechanoid | all until player KO | - | HUD bar (player HUD, not opponent); post-loss (harness); level-up (harness) | none seen |
| G3 | 72 | Mechanoid | all | - | HUD bar (player HUD); kill HP remainder (G4) | none seen |
| G3 | 94 | Power Mechandoid | all until player KO | - | post-loss (harness) | none seen |
| G3 | 50 | Ion Cannon | all | - | as Atomizer; level-up (harness) | none seen |
| G3 | 64 | Laser Turret | all | - | as Atomizer; level-up (harness) | none seen |
| G3 | 62 | Knight Destroyer | all | - | kill HP remainder (G4) | none seen |
| G3 | 76 | Metal Hulk | all | - | kill HP remainder (G4) | none seen |
| G4 | 0x44 | Majin Fighter | left / below pixel-exact all 360 frames; above / idle 1 frame each; AI, anim, damage, positions all frames | digit centring (wf-render menu.rs); HUD bars read at the HUD's place in the update pass (world.rs hud_sample, view.rs) | above f273: one hit kind where the game's bar is the late one (unexplained, 3 px); ki / punch / ko after the kill: field XP level-up + freed object reuse (harness: battle sim has xp off); L4 open: the HP bar is a frame late on hits whose attacker is on another row than the player (above f273, Majin Vegeta idle f113, Hyper Cyborg above f23) and on time on the same row (left f115) - not the HUD object's creation order alone; unresolved | - |
| G4 | 0x15 | Elite Majin Fighter | above / below / left / ki pixel-exact; all logic rows | (shared fixes) | idle f123 / punch f343: torn single frames (harness); ko: level-up (harness) | - |
| G4 | 0x7b | Super Majin Fighter | above / below / left / ki pixel-exact | - | idle anim tick at f437 (tick only, same cel); ko / punch: harness (level-up, torn frame) | - |
| G4 | 0x45 | Majin Soldier | above / left exact; logic all frames | muzzle k = ((t+1)>>2)&1, muzzle in front of the body (view.rs EnergyWave) | below 3 frames / ki 24 frames: equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments); ko: harness | - |
| G4 | 0x16 | Elite Majin Soldier | above / left exact | (as 0x45) | as 0x45 | - |
| G4 | 0x7c | Super Majin Soldier | above / left exact | (as 0x45) | idle anim tick f587 (tick only); else as 0x45 | - |
| G4 | 0x67 | Majin Shield Soldier | left 1 frame, logic all frames | enemy.rs drawn_sprite (unhashed) + nodes.rs shield_gfx + view.rs | anim raw -1 rows = recorder can't map the old set's record (harness); punch / ko: equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments); above f167: wave at the screen's top edge (not chased) | - |
| G4 | 0x17 | Elite Shield Soldier | left pixel-exact | as 0x67 | as 0x67 | - |
| G4 | 0x7d | Super Shield Soldier | left pixel-exact | nodes.rs Raise / Lower / ShieldBreak maps 0x102 -> 0x101 | as 0x67 | - |
| G4 | 0x56 | Fighter | above / below / left exact | - | ki / punch / ko after the kill: item drop + RNG at death (field placements; harness battle world has none), level-up | - |
| G4 | 0x24 | Goon | above / below / left exact | - | idle / ki / punch: one torn frame each (harness); ko: level-up | - |
| G4 | 0x80 | Thug | above / below / left exact | - | as 0x56 (drop / freed slot after the kill) | - |
| G4 | 0x2f | Hooligan | logic all frames | - | equal-depth tie: the game sorts with an unstable quicksort (0x080531A4) over every drawable list object, incl. objects whose sort keys are uninitialised heap (the stat mark: (56, 329) in 13-45, (255, garbage) in 2-11) - not derivable; kept "later list object in front" (all locked gate segments); ki / punch: level-up | - |
| G4 | 0x0f | Criminal | above / below / left exact | - | as 0x56 | - |
| G4 | 0x4b | Mercenary | logic all frames | - | fixed: rifle bullet sorted with the actors and its smoke puff made before the move, sorted (0x0802776E; shots.rs Trail::pre + TS, view.rs) - idle/left/above OBJ-diff 237/140/80 -> 29/1/1 (left: 1-px puff positions) | - |
| G4 | 0x2c | Gunman | below exact; logic all frames | gun / rifle bullet cloud sorted by y (shots.rs straight_end) | above: bullet clipped at the screen top (47..), ki / punch / ko: kill harness; L4: enemy shadows wrapped in at the top with < 5 OBJ rows on screen are dropped (wf-render SpriteEffect::TopCull); above now 3 frames (bullet cloud 2-6 px at f107) | - |
| G4 | 0x6c | Sniper | below exact | as 0x2c | as 0x2c; L4: enemy shadows wrapped in at the top with < 5 OBJ rows on screen are dropped (wf-render SpriteEffect::TopCull) | - |
| G4 | 0x2b | Grenadier | above / below / ki exact | - | grenade chips + blast now sorted with the actors (0x0800C454 / 0x0801952C layer 0x38; view.rs, shots.rs) - idle OBJ-diff 157 -> 5; f122/241/359 the bomb's last frame before the burst still drawn by ours (game shows nothing); ko: level-up | - |
| G4 | 0x02 | Assassin | below / ki exact | as 0x2c | fixed: a punch taken only by a bullet shakes the screen (crit slot left stale, as bandage/breakable; shots.rs bullet on_message + TS) - the 2 shake draws; oa_t02 punch exact in state | - |
| G4 | 0x0c | Bruiser | above / below / left exact | - | idle / punch anim tick only (no picture diff); torn frames | - |
| G4 | 0x61 | Rapscallion | above / below / left / ki exact | digit centring | idle anim tick f556; ko: level-up | - |
| G4 | (shared) | damage numbers | - | wf-render menu.rs: odd widths a pixel left, except a crit over an enemy (pixel gate db6_007 f90 - empirical split); vram.rs heal_number_shown + nodes.rs VampireGrab (green, top - 2, own reuse slot) | - | - |
| G4 | (shared) | kill-HP remainder | - | harness only: our enemy object is gone at death, the game's stays listed at HP 0 (opp_audit.rs reports 0) | - | - |
| G4 | (shared) | HUD HP / EP bars | - | World::hud_sample at the HUD's list place (field: its sprite-memory record; battle: HP after the pass, EP before); pixel gate earth_2_11_3 f150 / dr4_066 f390 now exact too | - | - |
| G5 | 0x20 | Ghost | AI, anim cels, facing, positions, HP/damage, death | harness: anim raw compared by canonical slot (opp_audit.rs) | dmg digits (G4); stump vs player foot priority (prop sort, not opponent) | - |
| G5 | 0x58 | Phantom | as Ghost | as Ghost | as Ghost | - |
| G5 | 0x5d | Poltergeist | as Ghost | as Ghost | as Ghost | - |
| G5 | 0x4d | Mummy | AI, anim, positions, HP/damage, bandage flight + hit timing | record set before the tick step, stuck ones drawn with record 3 (shots.rs MummyWrap.cel, view.rs); OBJ-diff frames idle 550->5, left 308->4, above/below ~230->1-4 | punch: a screen shake after punching a flying bandage comes from uninitialised stack (crit flag never set by the bandage) - not modelled | - |
| G5 | 0x10 | Cursed One | as Mummy after fix | same fix (OBJ-diff idle 371->5) | same shake/stack quirk (punch f402) | - |
| G5 | 0x59 | Pharoah | as Mummy after fix | same fix | same shake/stack quirk | - |
| G5 | 0x52 | Nosferatu | AI, positions, HP drain timing | grab state 0x0804C3CC as rule tail_loop (eanim.rs, eanim.ts, nodes.rs + ai.ts VampireGrab); drain through the apply-damage path: number + 12-frame flash (nodes.rs, ai.ts) | vampire's green heal number (0x0800E97E: style 6, Small font, body centre x / top-2, own reuse slot) not drawn - G4 (number drawing) | - |
| G5 | 0x85 | Vampire | as Nosferatu | as Nosferatu | as Nosferatu | - |
| G5 | 0x8a | Vlad | as Nosferatu | as Nosferatu | as Nosferatu | - |
| G5 | 0x09 | Bones | AI, cels, positions, damage, death | harness: tick ignored under seq rules | none visible | - |
| G5 | 0x6b | Skeleton | as Bones | as Bones | none visible | - |
| G5 | 0x41 | Lich | as Bones | as Bones | none visible | - |
| G5 | 0x62 | Ronin | AI, cels, positions, damage, death, pixels | - | tick bookkeeping only (cel + picture identical) | - |
| G5 | 0x63 | Samurai | as Ronin | - | as Ronin | - |
| G5 | 0x6a | Shogun | as Ronin | - | as Ronin | - |
| G5 | 0x21 | Ghoul | all but shared items | - | dmg digits (G4) | - |
| G5 | 0x42 | Living Dead | all but shared items | - | dmg digits (G4) | - |
| G5 | 0x95 | Zombie | AI, anim, damage, death | - | harness: drops come from field placements (Story has them, the lockstep battle world not); kill-HP remainder (G4) | - |
| G5 | 0x4e | Ninja | AI, cels, positions, star flight + hit | star sorts like the ki blast: layer 0x38, depth its y (view.rs draw_key) | a shadow above the screen top: culled by the game, 1-2 rows drawn by ours (generic); 2-px star spawn detail; L4: enemy shadows wrapped in at the top with < 5 OBJ rows on screen are dropped (wf-render SpriteEffect::TopCull); above pixel-exact | - |
| G5 | 0x68 | Shinobi | as Ninja | as Ninja | harness: battle world has no field-prop collision (Story has) | - |
| G5 | 0x65 | Shadow Warrior | as Shinobi | as Ninja | as Shinobi | - |
| G5 | all | (every opponent) | - | - | harness artifact (battle sim XP off) | - |
| G5 | all | (shared) | - | - | for the number owner (G4) / coordinator | - |
