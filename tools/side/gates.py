"""The level gates (s517 = one-time gate: character p0, level p1, prop p2; s518 = per-visit) as the map scripts call
them, and which of them a state has opened. `gates.py STATE.ss` prints the table: area, the gate's caller, char,
level, the flag it sets, opened or not, and the party's levels (record +0x2C... level byte).

The table is read from the content's inline scripts (work/frontend/dialog_portrait/content/base/scripts/inline)."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SC = ROOT / 'work/frontend/dialog_portrait/content/base/scripts/inline'
NAMES = ['Goku', 'Gohan', 'Goten', 'Trunks', 'Vegeta', 'Gotenks', 'Gogeta', 'Vegito']


def table():
    out = []
    for f in sorted(SC.glob('*.rhai')):
        t = f.read_text()
        m = re.search(r'run_event\((51[78]), (\d+), (\d+)', t)
        if not m:
            continue
        a = re.search(r'world (\d+) map (\d+)', t.splitlines()[0])
        fl = re.search(r'set_flag\((\d+)\)', t)
        out.append({'area': f'{a.group(1)}-{a.group(2)}', 'script': f.stem, 'ev': int(m.group(1)),
                    'char': int(m.group(2)), 'level': int(m.group(3)), 'flag': int(fl.group(1)) if fl else None})
    return sorted(out, key=lambda g: (int(g['area'].split('-')[0]), int(g['area'].split('-')[1]), g['level']))


if __name__ == '__main__':
    sys.path[:0] = [str(ROOT / 'tools' / 'story_playthrough'), str(ROOT / 'tools' / 'field')]
    from route import start_emu
    e = start_emu(str(Path(sys.argv[1]).resolve()))
    G = 0x03001068
    flag = lambda f: (e.r8(G + 0x55 + (f >> 3)) >> (f & 7)) & 1  # noqa: E731
    import json
    tab = json.loads((ROOT / 'data/buu/levels.json').read_text())['levels']
    xp = [e.r32(0x03001560 + 0x68 * c + 0x24) for c in range(8)]
    lv = [max(t['level'] for t in tab if t['total_xp'] <= x) for x in xp]
    print('levels', dict(zip(NAMES, lv)), 'flags 128/149', flag(128), flag(149))
    for g in table():
        st = 'open' if g['flag'] and flag(g['flag']) else ('per-visit' if g['ev'] == 518 else 'closed')
        can = 'can' if lv[g['char']] >= g['level'] else ''
        print(f"{g['area']:>6} s{g['ev']} {NAMES[g['char']]:7} L{g['level']:<4} flag {g['flag']} {st:9} {can}  {g['script']}")
