# Buu's Fury audio: sound driver, rip, and how the simulator uses it

ROM `roms/buu.gba` (USA). Addresses are GBA bus addresses; Thumb functions without the +1.

## 1. The driver

Not Nintendo's MP2K/"Sappy" (no `Smsh` player signature, no m4a tables) and not GAX/Krawall: Webfoot's own
**tracker-style driver with a software mixer**. Found by watching writes to the mixing buffer (`0x02000000`, written
by the IWRAM ARM mixer at `0x03000AD0`) and the sound-register literals at `0x0803F39C..0x0803F3B0`.

| what | where | notes |
|---|---|---|
| sound system object | `0x03003898` (`[0x08072824+4]`) | init `0x0803F1B8`; +0xC music player, +0x5C4 SFX voices |
| output | DMA1 → FIFO A = left (`0x02000000`), DMA2 → FIFO B = right (`0x02000400`) | SOUNDCNT_H `0x9A0E`, timer 0 reload `0xFBE6` → **15 978 Hz** stereo, 8-bit |
| per-frame update | `0x0803F296(sound)` from the main loop `0x0803D7FA` (after the game-state update, not an IRQ) | mixes `0x0803F07C` (music) + `0x0803E40C` (SFX) |
| music player | `0x030038A4` | +1 row, +2 rows in pattern, +5 playing, +6 samples per tick, +8 tick countdown, +0xC song header, +0x10 order index; 15 tracks × 0x60 at +0x18 |
| **PlaySong(player, header)** | `0x0803F110` | samples/tick = 40000 / tempo; order list ends with 0xFF → back to order 0 (pattern-jump effects can loop elsewhere) |
| song table | `0x083BB78C`, **53** × 20 bytes | pattern pointer table, order list, instrument table, sample table (shared by all songs: `0x0836D18C`, `0x0836D540`), tempo u8, channel count u8 |
| stop music | script native 0x23 with 0xFF: player+5 = 0; `0x0803F0E6` silences the 15 tracks |
| SFX voices | `0x03003E5C`, 16 × 0x40 | +0x10 flags (bit0 busy; 0x1F = start), +0x12 volume (0..0x40), +0x13 pan (s8), +0x28 entry, +0x30 pitch (0x4000 = 1.0) |
| **PlaySfx(voices, entry, vol, pan)** | `0x0803E378` | takes the **first free voice of 16; if none is free the request is dropped**. No priority, no per-sound limit |
| StopSfx(voices, entry) / StopAllSfx / stop-matching | `0x0803E302` / `0x0803E2EC` / `0x0803E32C` | looping effects (charge, beam, aura) are stopped explicitly |
| SFX table | `0x083192A8`, **64** × 12 bytes | `{s8 PCM pointer, u8 loop flag + u16 loop start at +6, u16 length at +8, u16 rate at +10}` (rates 2 600..16 000 Hz) |
| script natives | 0x1F PlaySfx(id), 0x20 PlaySfx(id, vol), 0x21 StopSfx(id), 0x22 StopAllSfx, 0x23 PlaySong(id) | table `0x08055098` |

**Who plays songs.** Only four call sites reach PlaySong: the boot/title code (`0x080041CE`, `0x0800540E`, song 45),
script native 0x23 (`0x080008A8`), and the area loader (`0x08008C70`): every area record (`0x0808E2E0`, 452 × 0x38,
key = world/map) has its song id at **+0x24**; 0xFF = silence, a pointer = a bytecode script that picks the song
(Hercule City 3-1..3-3: song 29 once story flags 26+27 are set, else 6). Story scripts that change the music were
found by scanning every script of the script table `0x0879B704` (decompressed) for `push id; native 0x23`.
18 of the 53 songs have no play site in any of these (catalog name "no play site found"; they may be unused).

## 2. Ripping (`tools/audio/rip_buu.py`)

`buu_sound.Harness` boots the reproducible Snakeway state (`tools/extract/emu_states.py`), then patches the main
loop's game-state update call (`0x0803D80A`) to a small Thumb stub in ROM padding (`0x082ED300`) that runs one
queued function call per frame. The game logic is frozen (nothing else can start a song or a sound), while the
main loop keeps running the sound driver. The emulator shim (`tools/emu/shim.c`) gained opt-in audio capture
(`set_volume`, `run_audio`, `audio_rate`); its default stays muted.

- **Music.** Silence everything, call PlaySong, capture the mixer output at 32 768 Hz and log (order, row) at
  every row change. The loop is the first row whose (order, row) reappears with the next 48 rows and their
  durations identical (a speed change in pass 1 makes the row lengths differ). The loop length is refined to the
  sample by correlation. Pass 1 and pass 2 are compared in 20 ms blocks; the loop region starts after the last
  block where they still differ (notes ringing in from before the loop point), then within 8 s at the point where
  the audio before the loop end best matches the audio before the loop start. The file is intro + one loop + 1 s;
  `loop.start` / `loop.end` (seconds) are periodic, so a decoder offset of a few ms keeps the loop seamless.
  Songs that idle on silent rows after a short tune (song 43) are kept as one-shots (`ends: true`).
- **SFX.** PlaySfx(entry, vol 0x40, centre) through the same stub; capture until the voice is free. Looping
  effects render one pass plus a second loop; their loop points come from the table entry. Leading silence is
  trimmed (minus the sample's own leading zeros) so the effect starts on time.
- One uniform gain for every file (keeps the in-game music / SFX balance), MP3 128 kb/s at 32 kHz (plays in every
  browser). Renders are cached as WAV in `work/audio/cache/<rom sha1>_v<version>/`.

Faithfulness: this is the game's own driver output, including its 8-bit mixing and the GBA's sample-and-hold
output; only the encoder (MP3) and the uniform gain are added. The game's master music volume setting is left at
the value of the boot state.

## 3. Naming (`tools/audio/names_buu.py`)

- Songs by the areas whose record selects them (area names from the string table), the story scripts that play
  them (chapter cards, warp targets, enemy types next to the call) and the boot code. Confidence: high = area table
  / native call site; medium = script context; low = script number only; none = no play site.
- SFX by breakpointing PlaySfx while doing each action in the emulator (`work/audio/sfxtrace2.py`, `skillsfx.py`,
  `eventsfx.py`: punches, every skill at tap / hold, dash, menus, low HP, level-up, being hit) plus a static
  scan of all 238 PlaySfx / StopSfx call sites (constant id resolved for 136) and of the scripts (native 0x1F).

| event (simulator) | SFX | game evidence |
|---|---|---|
| punch (A), enemy punch | 50 / 51 random | attack-type callback `0x0800DEE8` → `0x0800DF1A` |
| hit lands (melee, blasts, beam ticks) | 48 / 49 random | `0x08012AA4`, hit spark `0x0802A258`, `0x080199E0`, `0x080209D6` (attackers with gfx 0x60/0xE5/0xDC use 30, 0xEE/0xF4/0xED use 62) |
| Energy Blast | 20 / 21 random | `0x08019846` |
| charge (beams, Big Bang, Masenko, Energy Sword, Final Flash) | 3, loops until release / fire | `0x08020764`, `0x08037502`, `0x080212F6`, `0x0801B046` |
| beam firing | 32 loop, then 33 at the end | `0x08020828`, stop `0x08020790`, end `0x080207A0` |
| Burning Attack / Masenko release / Ultra Volleyball | 34 | `0x0802940A`, `0x080214D8`, `0x08038482` (volleyball hit: 46) |
| Big Bang release | 23 | `0x08037638` |
| Dragon Fist / Energy Punch | 24 / 47 | `0x0801AB94` / `0x0801B278` |
| Super Saiyan | 54 (loop during the 211-frame transform, again when the form is applied / reverted) | `0x0801C414`, `0x0800E3CC` |
| dash | 59 | `0x08012F5E` |
| level up | 36 | bytecode script #3 |
| low HP | 41 at volume 32, every 61 frames while HP ≤ maxHP/5 | HUD `0x0800BA3A` |
| pause / menu confirm / cursor | 40 / 40 / 39 | `0x0800535E`, `0x0801E622`, `0x08006032` |

Not found: no SFX for block, enemy KO or player death (none fired in those traces).

## 4. In the site

- `site/src/battle/audio/engine.ts` — Web Audio: one music source with `loopStart/loopEnd`, SFX limited to 16
  voices with drop-when-full like the driver, looping SFX by key; mute / volumes in localStorage
  (`bt-audio`); the context is resumed on the first gesture.
- `bus.ts` — `audioEvent()` hook calls placed in `rulesets/buu/player.ts` / `enemy.ts` at the points above
  (no-op without a listener, never uses the game RNG).
- `director.ts` — per battle: arena music = the arena map's song (`arena_music` in the catalog), or the scripted
  battle theme (song 44, used by the story's boss-fight scripts) when a phase has a boss; low-HP beep; pause sound.
- Dashboard: `#/g/buu/audio` lists every track and effect with gapless loop playback.
