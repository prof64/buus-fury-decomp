# Shop, Save, Title, Switch Character, game over (`wf-core/src/menu/{shop,save,title,charsel,gameover}.rs`)

Scenarios: `scenarios_{shop,save,title,charsel}.py` → `data/menu/*.json`; all strict in `menu_parity.rs`, every
compared frame and every sound exact.

## Shop (0xCE ctor `0x080315B0`, 0xCF)
Buy / sell pages, list, preview stats, LEFT / RIGHT count, sounds. Sell price = price < 0 ? −price :
price · percent / 100. Quick fade with the last level held one frame (`FADE_HOLD`), page on the 3rd black frame.

## Save Game (0x8C, handler `0x08002720`, mode `0x08005460`, vt `0x08049504`)
Slot list (`0x08005632`, rows `0x08005578`), figures (frozen = idle raw 1 facing up + silhouette remap
`0x0805682C`; chosen = walk), corner markers (`0x080561FC`), popup 96x40 at (72, 64). Result 2 saved / 1
cancelled. Slots: EEPROM cache `[0x03004A64] + 0x10 + 0x8B0·k`: GameData 0x8A0 + location (world, area, variant,
u16 x, u16 y) + Adler-32 (`0x0803FC4E`); a slot exists when byte 1 bit 0 is set and the checksum holds. Chapter label
from event #6.

## Title (vt `0x0804A404`, enter `0x080053CC`, song 45)
Title → Single Player → Select Game (New Game / Continue / L-R Delete Game with the "Are you sure?" confirm).
New Game: prologue hook script #4 under the fading Select Game picture (64 frames). Continue: 16-step fade (the
figures tick until level 16), 29 black frames, fade-in step 4.

## Switch Character (0x8D, native `0x080027DE`, ctor `0x0802E83C`, vt `0x0804A5E0`)
Party (char flag bit 0) in a row at y 88: spacing min(0x90 / n, 0x30), x0 = 0x70 − (n − 1)·sp / 2. Flag bit 1
clear = frozen silhouette (turned away), skipped by LEFT / RIGHT. Idle enter tick = rand(255) per figure (ctor order);
the current walks (anim 1). Selection change `0x0802EC9C`: sound 39, old → idle (rand), new → walk; both new
controllers tick one frame late. The OAM shows the figures as the object update left them (before the container
step), so the frame of a change still shows the old ones. Redraw = 2 frames. A (sound 40): `GD[0]` = char,
GD+0x4DC (timed buff) = 0, end 2; B (sound 40) end 1; the closing frame is already black. After 2 the native
rebuilds the leader: form → sprite (`0x0801713C` / `0x0800EDF8`), `0x0801064C`, vt +0x28, HUD (`0x0800938C`).

## Game over
No screen. KO `0x0801070E`: death animation, 200-frame timer → `0x08010588`: form flag 0x40 → base form, sprite,
run hook #0 (`0x08012588`, header `0x0877C5A4`): flag 281 / Lazarus Crystal (supply 23: revive at 25 % HP, timed
special 5 s) / flags 282, 283 / else song 8, 120 frames, fade, death scene (0, 2, 1), "You have died!",
`return_to_title` (0x8E).

## Multiplayer (link cable) - title -> Multiplayer
Modelled up to the link: the page Dojo / Trading Post (vt `0x0804B5AC`, enter `0x0801E7B4`: strings 0x5F / 0x4F,
done check `0x0801E970`: B (sound 40) back to Single / Multiplayer) and the link box "Waiting for a friend..."
(vt `0x0804AEBC`, enter `0x08024120`: string 90 + B Cancel, 136x56 BG0 box at (56, 64); update `0x080240B8`: once
the link is up ([`0x03002F54`]) it counts 10 updates, `0x08040182`, and the mode returns 2). Scenarios
`title_multi` / `title_dojo` / `title_trade` (no cable: the box only ends with B, no sound). Not modelled (no
cable in the emulator either; INFERRED from the code and the strings 79..115):
* **Connected** (`0x0801E84A`, run_mode returns 2): `0x0804019A`, then a Select Game list (`0x08005460(0x5E "Select
  Saved Game", 0xD, arrows)`, vt `0x08049B2C`, +0xB38 = 0 Dojo / 1 Trading Post): each player picks one of their
  saves ("No Saved Games" 91 / "Please select a saved game" 92). Failure / cable pulled: "Disconnected" (93).
* **Dojo** - "Battle with your friends in the Dojo!" (104): character choice ("Choose your character!" 105, the
  save's party: strings 116.. Goku / SS Goku / SS3 Goku / Teen Gohan ... Vegeta), "Character Mode:" Balanced Mode /
  In-Game Mode (97 / 98: fixed stats vs the save's), "Rounds:" 1 / 3 / 5, "Start Fight" (96); a versus fight in
  rounds ("Round 1".."Round 5", "You Win!!" / "You Lose!!" / "Draw!!"), then "Grab your prize!" (114) / "Go back
  to training!" (115). Mode tables of the flow (enter / update / events / redraw / done): `0x08049B5C`,
  `0x0804B604`, `0x0804BC04`, `0x0804CAD4` (built by `0x0801FF04`), `0x0804A60C`, `0x0804C0F0`.
  **Decoded rules** (static disassembly; table with every address in engine/docs/netcode.md "Link play: Dojo"):
  fight `0x0802193A` (state `0x03002BFC`: +0 / +1 wins, +4 timer seconds) zeroes the RNG once (`0x0802193E`);
  area 0-90 (`0x08008E8C`); round = 180 s (`0xB4` at `0x08021C4A`), ends the update a fighter's HP is 0 (one down:
  the other wins; both: Draw!!) or at time-out with both standing = Draw!! (`0x08021D48`, no HP comparison); result
  banner 300 updates (`0x08021DC4`); "Round N" = 106 + both wins (draws repeat the number); the match loops until a
  player has more than rounds / 2 wins (`0x08021B98`: no round cap, never a drawn match); "Grab your prize!" is text
  only (no save write in the Dojo code). Mode byte 1 = Balanced: a fixed roster of 24 (strings 116..139, list
  `0x08023120`, so the strings above are the Balanced list, not the save's party) with records from `0x0872BFCC`
  (HP 15000, EP 100, STR / POW 67, END 25, SPD 50; Hercule 60 / 74; skill 7 only; `0x08022904` / `0x0801339C`);
  0 = In-Game: the save's party with each record packed whole (`0x08022934` -> `0x080229B0`). INFERRED: the
  "Round N" banner's length, that the host's settings apply to both, which side is the host.
* **Trading Post** - "Your Trade:" / "Friend's Trade:" (80 / 81), items and "Zenie" (82), "How many to trade?" (83),
  "Propose Trade" / "Accept" / "Decline" (84..86), "You have accepted trade" (87), "TRADE" "COMPLETE!" (88 / 89).
  Mode tables: `0x08049450`, `0x0804A3BC`, `0x0804AFA0`, `0x0804B550`, `0x0804CAA8` (built by `0x080232A6`,
  `0x080237CC`, `0x08023C22`), `0x08049424`, `0x0804A8AC`. Item picker `0x0801F44E`: supplies whose record bit 0
  (`0x083BD2F8` + 0x34 x id, `story_item`) is clear (ids 0..24), every equipment entry, zenie if not 0; apply
  `0x080231AE` (supplies saturate at 255, equipment add `0x08004448` / remove `0x08004468`).
* **Mapping to the remake (plan, nothing built):** the link maps onto online play (engine/docs/netcode.md: wf-net's
  lobby + rollback). "Waiting for a friend..." = the lobby's matchmaking (Dojo / Trading Post as two room kinds);
  the save pick = each peer's local save slot, exchanged at connect. Dojo = a rollback versus match on the battle
  `Session` (the Buu's Fury battle engine) with the pick / mode / rounds as its config; the prize as a local story
  write after the result. Trading Post = no realtime need: a two-phase commit over the data channel (both propose,
  both accept, then each side applies the other's offer to its own save) - needs a confirmation that both sides
  wrote, or a trade can duplicate / vanish on a disconnect between the two writes. Recording the real flows needs
  two linked emulator instances (mGBA's link support) - not available in the headless harness today.
