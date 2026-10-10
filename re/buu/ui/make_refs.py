"""Write the emulator reference captures used by engine/crates/wf-ui/tests/parity.rs.

Run after tour.py / boot.py / cap.py produced the states in out/ (see engine/docs/ui.md §Parity):
  cap.py buu_snakeway_start.ss snake wait:2 ; cap.py buu_menus.ss menus wait:2 ; cap.py buu_snakeway_start.ss pause START:60
  tour.py mp buu_snakeway_start.ss SELECT:60 R:40 ; boot.py
Outputs (engine/crates/wf-ui/tests/data/):
  boot_<name>.png       emulator screenshots of the boot screens (RGB)
  dialog_surface.bin    the 160x64 8bpp dialog surface (EWRAM) while it shows the page "normal..."
  hud_tiles.bin         OBJ VRAM tiles 880..959 (0x500 bytes, the HUD) + hud_state.json (character record values)
  mapscreen_bg.png      240x160 (grey PNG = indices) palette indices: BG3 then BG0 composited (the map-screen frame), area map BG2 excluded
  worldmap_tiles.bin    sha1 of VRAM tiles 1..600 on the world map page (BG2)
  pause_surface.png     240x160 (grey PNG = indices) BG2 tiles of the Status page (identity-mapped surface)
"""
import sys, struct, json, hashlib, shutil
from pathlib import Path
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
from PIL import Image

HERE = Path(__file__).resolve().parent
OUT = HERE / 'out'
DST = Path('/Users/prof/source/buus-fury-decomp/engine/crates/wf-ui/tests/data')
DST.mkdir(parents=True, exist_ok=True)

for name, f in (('legal', 3), ('atari', 7), ('funimation', 12), ('webfoot', 19), ('title', 30)):
    shutil.copy(OUT / 'boot' / f'f{f:03d}.png', DST / f'boot_{name}.png')

e = emu(str(OUT / 'menus.ss'))
(DST / 'dialog_surface.bin').write_bytes(e.read(0x02032410, 160 * 64))

e = emu(str(OUT / 'snake.ss'))
(DST / 'hud_tiles.bin').write_bytes(e.read(0x06016E00, 0x500))
rec = 0x03001560 + 0x68 * e.r8(0x03001068)
st = {k: struct.unpack('<h', e.read(rec + o, 2))[0] for k, o in
      (('hp', 0x16), ('hp_max', 0x18), ('ep', 0x1A), ('ep_max', 0x1C), ('ss', 0x1E), ('ss3', 0x20))}
(DST / 'hud_state.json').write_text(json.dumps(st))


def bg_layer(e, bg):
    cnt = e.r16(0x04000008 + 2 * bg)
    sb, cb = (cnt >> 8) & 31, (cnt >> 2) & 3
    v = e.vram()
    out = bytearray(240 * 160)
    for ty in range(20):
        for tx in range(30):
            ent = struct.unpack_from('<H', v, sb * 0x800 + (ty * 32 + tx) * 2)[0]
            t, hf, vf = ent & 0x3FF, ent >> 10 & 1, ent >> 11 & 1
            for y in range(8):
                for x in range(8):
                    sx, sy = (7 - x if hf else x), (7 - y if vf else y)
                    out[(ty * 8 + y) * 240 + tx * 8 + x] = v[cb * 0x4000 + t * 64 + sy * 8 + sx]
    return out


e = emu(str(OUT / 'mp_0.ss'))
b3, b0 = bg_layer(e, 3), bg_layer(e, 0)
Image.frombytes('L', (240, 160), bytes(b0[i] or b3[i] for i in range(240 * 160))).save(DST / 'mapscreen_bg.png')

e = emu(str(OUT / 'mp_1.ss'))
(DST / 'worldmap_tiles.sha1').write_text(hashlib.sha1(e.read(0x06000040, 600 * 64)).hexdigest())

e = emu(str(OUT / 'pause.ss'))
Image.frombytes('L', (240, 160), bytes(bg_layer(e, 2))).save(DST / 'pause_surface.png')
print('ok', sorted(p.name for p in DST.iterdir()))
