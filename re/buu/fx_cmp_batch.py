"""fx_cmp_batch.py emu|compose -- emulator side of the effect comparisons (work/compare/fx/).
emu: runs fx_skill.py for each case (facing down, enemy pinned 80 px below) and writes work/compare/fx/cases.json with
the sim frame (= logic ticks since the press) matching each emulator snapshot frame.
compose: builds work/compare/fx/<case>_<k>.png from the emulator snapshots and work/compare/fx/sim/."""
import sys
import json
import subprocess
from common import *

CASES = [
    # name, skill, level, hold, frames, emulator snapshot frames
    ('energy_blast', 7, 0, 4, 60, [22, 30, 45]),
    ('kamehameha_lv1', 15, 0, 90, 115, [12, 40, 100]),
    ('kamehameha_lv3', 15, 2, 90, 115, [12, 40, 100]),
    ('electric_kamehameha_lv2', 6, 1, 90, 115, [40, 60]),
    ('kamekameha_lv1', 16, 0, 90, 115, [12, 40]),
    ('banshee_blast', 1, 0, 90, 115, [12, 45]),
    ('big_bang_kamehameha', 3, 0, 90, 115, [12, 45, 70]),
    ('flame_kamehameha_lv1', 11, 0, 90, 115, [15, 50, 80]),
    ('big_bang_lv1', 2, 0, 100, 230, [20, 60, 140, 175, 200]),
    ('burning_attack', 4, 0, 4, 70, [10, 30, 45]),
    ('final_flash_lv3', 10, 2, 40, 80, [34, 40, 50]),
    ('masenkoha_lv1', 17, 0, 60, 140, [20, 75, 112, 118]),
    ('dragon_fist', 5, 0, 40, 110, [25, 45, 80, 95], 300),
    ('energy_sword', 9, 0, 130, 175, [30, 90, 150], 300),
    ('ultra_volleyball', 23, 0, 4, 60, [25, 35]),
    ('energy_punch_lv3', 8, 2, 70, 80, [20, 60], 300),
    ('kamikaze_ghost', 13, 0, 110, 150, [30, 80, 125], 300),
]
FACE = 0
DIST = 80
OUT = ROOT / 'work' / 'compare' / 'fx'
OUT.mkdir(parents=True, exist_ok=True)

if sys.argv[1] == 'emu':
    res = {}
    only = sys.argv[2:]
    for name, sk, lv, hold, frames, snaps, *dd in CASES:
        if only and name not in only:
            continue
        subprocess.run([sys.executable, 'fx_skill.py', str(sk), str(hold), str(frames), str(FACE), str(lv), '-1', str(dd[0] if dd else DIST),
                        ','.join(map(str, snaps))], cwd=Path(__file__).parent, stdout=subprocess.DEVNULL, check=True)
        L = json.loads((WORK / 'fx' / f'skill_{sk}.json').read_text())
        t0 = L[0]['gt'] - 1
        for f in snaps:
            (OUT / 'emu').mkdir(exist_ok=True)
            src = WORK / 'fx' / f'skill_{sk}_f{f}.png'
            dst = OUT / 'emu' / f'{name}_f{f}.png'
            dst.write_bytes(src.read_bytes())
        (OUT / 'emu' / f'{name}.json').write_text(json.dumps(L))
        res[name] = {'skill': sk, 'lv': lv, 'hold': hold, 'frames': frames, 'emu': snaps,
                     'sim': [L[f]['gt'] - t0 - 1 for f in snaps]}
        print(name, res[name]['sim'])
    p = OUT / 'cases.json'
    old = json.loads(p.read_text()) if p.exists() else {}
    old.update(res)
    p.write_text(json.dumps(old, indent=1))
else:
    cases = json.loads((OUT / 'cases.json').read_text())
    for name, c in cases.items():
        for k, (ef, sf) in enumerate(zip(c['emu'], c['sim'])):
            sp = OUT / 'sim' / f'{name}_f{sf}.png'
            if not sp.exists():
                print('missing', sp)
                continue
            subprocess.run([sys.executable, 'fx_compare.py', f'{name}_{k}', str(sp), str(sp.with_suffix('.json')),
                            str(OUT / 'emu' / f'{name}_f{ef}.png'), str(OUT / 'emu' / f'{name}.json'), str(ef)],
                           cwd=Path(__file__).parent, check=True)
