"""trade_explore.py OUTDIR STEPS...: drive the Trading Post on two linked emulators (both booted with run 2's save) and
save both screens after each step - for finding the screens' keys. A step: KA,KB,N (keys of A / B held N frames, '-'
none, 'UP+A' combos) or 'shot' (screenshots now)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dojo_record import L, ROOT, mkemu, soft_reset, sbemu, keyof, step  # noqa: E402

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
a, b = mkemu('buu_menus.ss'), mkemu('buu_menus.ss')
for e in (a, b):
    sbemu.sd_write(e, sav)
    e.run(3)
    soft_reset(e)
L.emu_link(a.h, 0)
L.emu_link(b.h, 1)
k = 0
for s in sys.argv[2:]:
    if s == 'shot':
        a.screenshot(out / f'{k:03d}_a.png', 1)
        b.screenshot(out / f'{k:03d}_b.png', 1)
        print('shot', k, 'frame', L.emu_frame(a.h))
        k += 1
        continue
    ka, kb, n = s.split(',')
    for t in range(int(n)):
        a.set_keys(keyof(ka))
        b.set_keys(keyof(kb))
        step(a, b)
