"""Record lockstep traces in the emulator -> data/battle/buu_lockstep/fNN.json (one file per form).

usage: .venv/bin/python tools/battle/lockstep/record.py [NAME_GLOB ...] [--print]
  NAME_GLOB  scenario names (fnmatch, e.g. 'f02_*', '*_skill15'); default: all
  --print    also print the recorded rows (state / anim / frame changes and sounds) of each scenario

Rows are run-length encoded: [n, x, y, dx, dy, dir, raw, frame, state, hp, ep, form, rom, rng] covers n frames whose
position advances by (dx, dy) per frame and whose other fields are equal; `states` names the state indices;
`sfx` = [[frame, kind, id, vol?], ...] (frame 1 = the first input frame). See tools/battle/LOCKSTEP.md.
"""
import fnmatch
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from emu_lockstep import record, ROOT  # noqa: E402
from scenarios import SCENARIOS  # noqa: E402

OUT = os.path.join(ROOT, 'data', 'battle', 'buu_lockstep')
FIELDS = ('dir', 'raw', 'frame', 'state', 'hp', 'ep', 'form', 'rom')


def encode(trace):
    states = []
    segs = []
    sfx = []
    for i, r in enumerate(trace['frames']):
        if r['state'] not in states:
            states.append(r['state'])
        key = [r['dir'], r['raw'], r['frame'], states.index(r['state']), r['hp'], r['ep'], r['form'], r['rom'], r['rng']]
        for s in r.get('sfx', []):
            sfx.append([i + 1] + s)
        if segs:
            g = segs[-1]
            if g[5:] == key:
                px = g[1] + g[3] * (g[0] - 1)
                py = g[2] + g[4] * (g[0] - 1)
                if g[0] == 1:
                    g[3], g[4] = r['x'] - g[1], r['y'] - g[2]
                    g[0] += 1
                    continue
                if r['x'] - px == g[3] and r['y'] - py == g[4]:
                    g[0] += 1
                    continue
        segs.append([1, r['x'], r['y'], 0, 0] + key)
    init = trace['init']
    return {'name': trace['name'], 'scenario': trace['scenario'], 'input': trace['input'], 'init': init, 'states': states, 'rows': segs, 'sfx': sfx}


def show(trace):
    prev = None
    for i, r in enumerate(trace['frames']):
        key = (r['raw'], r['frame'], r['state'], r['dir'], r['form'])
        s = r.get('sfx', '')
        if key != prev or s:
            print('  %4d %-14s raw=%2d f=%2d t=%4d dir=%d pos=%.2f,%.2f hp=%d ep=%d form=%d %s' % (
                i + 1, r['state'], r['raw'], r['frame'], r['tick'], r['dir'], r['x'] / 256, r['y'] / 256, r['hp'], r['ep'], r['form'], s))
        prev = key


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    pr = '--print' in sys.argv
    pats = args or ['*']
    todo = [s for s in SCENARIOS if any(fnmatch.fnmatch(s['name'], p) for p in pats)]
    os.makedirs(OUT, exist_ok=True)
    byform = {}
    for scn in todo:
        t = record(scn)
        if pr:
            print(scn['name'])
            show(t)
        byform.setdefault(scn['form'], []).append(encode(t))
        print('recorded', scn['name'], len(t['frames']), 'frames')
    for form, traces in byform.items():
        path = os.path.join(OUT, 'f%02d.json' % form)
        old = []
        if os.path.exists(path):
            old = json.load(open(path))['traces']
        names = {t['name'] for t in traces}
        merged = [t for t in old if t['name'] not in names] + traces
        merged.sort(key=lambda t: t['name'])
        with open(path, 'w') as f:
            json.dump({'source': 'tools/battle/lockstep/record.py (emulator)', 'form': form, 'traces': merged}, f, separators=(',', ':'))
        print('wrote', path)


if __name__ == '__main__':
    main()
