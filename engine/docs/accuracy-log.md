# Accuracy log

One line per fix against the original game (emulator / recordings), newest last.

* 2026-10-10 HP bar a frame late on some enemy hits: explained, not a logic difference - the HUD writes the bar into sprite memory mid-frame and on heavy frames the screen has already drawn those rows (scanline 14 vs 2-6); a tear, left unmodelled. Emulator breakpoints now report the scanline.
* 2026-10-10 Kid Buu's Kamehameha: the impact shows a frame before the hit, as in the game (the beam object's tiles run a frame ahead of its position); oa_t3c_above f35 149 -> 1 px.
* 2026-10-10 Kid Buu's Kamehameha charge ball shrinks a frame earlier, as in the game; oa_t3c_above f33 exact.
* 2026-10-10 Player beam charge (Kamehameha, Kamekameha, Electric, Big Bang Kamehameha): no ball on the frame it grows from small to large - the game shows only a torn fragment there (134 -> 37 px on every such segment).
* 2026-10-10 Super Buu's candy beam: the player's candy keeps its own colours (ours drew it through the red hit flash).
* 2026-10-10 Mummy bandages stuck on the player hold still while the screen shakes, as in the game (Pharaoh Totenhotep punch scenario: bandage pixels now exact).
* 2026-10-10 The robots' flames (Annihilator, Death Machine, Destroyer) sort behind the player they burn, so his red hit flash shows over them, as in the game (was the open "flame colour" item).
* 2026-10-10 A damage number that is already on screen shows a new hit's total in that same frame, as in the game (ours lagged a frame on every multi-hit total: flames, beams).
* 2026-10-10 SS gauge edge step a frame late: explained as the same mid-frame tear as the HP bar (the HUD's copy ran at scanline 26-27 on those frames, 215 otherwise); left unmodelled.
* 2026-10-10 Gogeta's Dragon Fist icon 'staying bright' facing up: not a difference - the game greys it the same frame; the skill audit's lasting top-band difference there is a Time Chamber prop the comparer leaves out, and its single icon frame is the HUD tear (scanline 17).
* 2026-10-10 Sound timing in the app: each game frame's sound requests now start at that frame's driver update, as in the game; before, all requests of an audio-device buffer (2-5 frames) started together, a frame or more early.
* 2026-10-10 Coverage: the Rogue Submarine's enemy-group outcome (s509's 25% branch) recorded and gated (tour_world_sub_fight, exact to f1,041; tour_world_sub_rng3 exact) - the one difference it shows (the last kill's coin pickup a frame off) is written up in segments.md, not yet fixed.
* 2026-10-10 Coins dropped while a kill's event holds the field: they move once (not twice) in the frame the event ends, and the trigger check that frame sees them where they were - the Rogue Submarine fight's last coin is picked up on the game's frame (tour_world_sub_fight now 1,696 / 1,696).
* 2026-10-10 Link cable: two emulators linked in one process (shim `emu_link`), the Dojo's versus fight recorded (data/link) and gated against the app's Dojo match (wf-net tests/dojo_link.rs: 2,529 and 5,535 frames exact). Fixed to match: the round's start (spawn places, facing, idle-tick draws, the 125-frame "Round N" with the fighters idling), no EP from landed punches, the result 300 frames after a knockdown with the timer and the fallen fighter's regeneration running on.
