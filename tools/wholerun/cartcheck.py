"""cartcheck.py SAV.. [--cut CUT.ss] [--out OUT.json]: real cartridge saves -> Continue, on the game (the app side:
examples/savecheck.rs, which imports each .sav as the cartridge's EEPROM and continues; compared with
`savecheck.py --compare`).

For each .sav (EEPROM image, e.g. work/run2/*.sav): the emulator at a whole-run save cut (any - the soft reset
leaves only the cartridge), the save written to its EEPROM, then the soft reset, the title, Single Player, Game 1 (the
game's boot + slot load path) until the leader is controllable in the field. Written per save: game 1's GameData as
on the cartridge, and after the Continue: area, leader position, the GameData, the RNG words - savecheck.py's layout,
with `cut` naming the cut the app starts from (its .json doc next to it)."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import savecheck as sc  # noqa: E402  (its imports set up the savedata emulator)
from savecheck import ROOT, emu, status, title_continue_first, GD  # noqa: E402
import sbemu  # noqa: E402


def one(sav, cut):
    raw = Path(sav).read_bytes()
    b = b''.join(raw[k:k + 8][::-1] for k in range(0, len(raw), 8))
    gd = b[0x10:0x10 + 0x8A8]
    if gd[1] & 1 == 0 or all(v == 0xFF for v in gd):
        return {'state': str(Path(sav).resolve()), 'skip': 'game 1 empty'}
    e = emu(str(cut))
    sbemu.sd_write(e, raw)
    log = []
    title_continue_first(e, log)
    s1 = status(e)
    return {'state': str(Path(sav).resolve()), 'sav': str(Path(sav).resolve()), 'cut': str(Path(cut).resolve()),
            'before': {'p': None, 'area': None}, 'saved_gd': gd.hex(), 'after': s1, 'gd': e.read(GD, 0x8A8).hex(),
            'rng': [e.r32(0x03004A5C), e.r32(0x03004A60)], 'continue_frames': len(log)}


if __name__ == '__main__':
    a = sys.argv[1:]
    cut = Path(a[a.index('--cut') + 1]) if '--cut' in a else sorted(sc.READY.glob('*.ss'))[0]
    out = Path(a[a.index('--out') + 1]) if '--out' in a else ROOT / 'work' / 'wholerun' / 'cartcheck.json'
    res = []
    for p in [x for x in a if x.endswith('.sav')]:
        r = one(p, cut)
        print(Path(p).name, r.get('skip') or (r['after']['area'], r['after']['p']), flush=True)
        res.append(r)
    out.write_text(json.dumps(res))
