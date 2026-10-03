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
