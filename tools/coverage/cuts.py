"""cuts.py [-j N] [--every K] CHAIN:ROW:N:NAME ..: segments cut from the recorded whole runs for the coverage report -
as tools/wholerun/cut.py (the segment recorder: every actor, camera, sounds), but started from the nearest earlier
state the whole-run check already cut (work/wholerun[/runN]/cuts/c<ROW>.ss) instead of the reset, and with the
emulator's screen dumped as the picture board wants it (frame 0, every K-th (default 30), the last).
Writes data/segments/NAME.json (source: the chain and row), work/coverage/states/NAME.ss and
work/superbuu/pix/fb/NAME.fb (git-ignored: ROM-derived). An existing NAME is not overwritten. Headless, muted."""
import json
import sys
import zlib
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 'tools/script_lockstep', 're/buu', 'tools/wholerun'):
    sys.path.insert(0, str(ROOT / d))
SEGS = ROOT / 'data' / 'segments'
STATES = ROOT / 'work' / 'coverage' / 'states'
FB = ROOT / 'work' / 'superbuu' / 'pix' / 'fb'


def one(args):
    spec, every = args
    import chains
    import segment as sg
    from route import start_emu
    chain, row, n, name = spec.split(':')
    row, n = int(row), int(n)
    if (SEGS / f'{name}.json').exists() or (FB / f'{name}.fb').exists():
        return f'{name}: exists - not overwritten'
    keys = []
    for _, ks, _ in chains.parts(chain):
        if len(keys) > row + n:
            break
        keys += ks
    P = chains.paths(chain)
    cuts = sorted(int(p.stem[1:]) for p in P['cuts'].glob('c*.ss') if p.stem[1:].isdigit())
    near = max((c for c in cuts if c <= row), default=0)
    e = start_emu(str(P['cuts'] / f'c{near}.ss') if near else 'reset')
    for k in range(near, row):
        e.set_keys(keys[k])
        e.run(1)
    STATES.mkdir(parents=True, exist_ok=True)
    st = STATES / f'{name}.ss'
    e.save_file(str(st))
    init, objs, rows = sg.record(e, lambda j, _e: keys[row + j] if row + j < len(keys) else None, n)
    c = start_emu(str(st))
    c0, p0 = c.cycles(), c.r32(0x0300389C)
    while c.r32(0x0300389C) == p0:
        c.step()
    init['snd_phase'] = c.cycles() - c0
    sg.OUT = SEGS
    sg.save(name, {'kind': 'segment', 'name': name,
                   'source': {'kind': 'state', 'state': str(st.relative_to(ROOT)), 'chain': chain, 'row': row},
                   'init': init, 'objs': objs, 'frames': list(rows)})
    # the screen at the sampled frames (as work/superbuu/pix/dump.py)
    m = len(rows)
    want = sorted({0, m - 1} | set(range(0, m, every)))
    e = start_emu(str(st))
    blobs, wi = [], 0
    for k, r in enumerate(rows):
        e.set_keys(r['keys'])
        e.run(1)
        if wi < len(want) and want[wi] == k:
            blobs.append(zlib.compress(e.pixels(), 6))
            wi += 1
    with (FB / f'{name}.fb').open('wb') as f:
        f.write((json.dumps({'frames': want[:len(blobs)], 'sizes': [len(b) for b in blobs]}) + '\n').encode())
        for b in blobs:
            f.write(b)
    return f'{name}: {chain} row {row} (from cut {near}), {m} rows, {len(blobs)} pictures'


if __name__ == '__main__':
    a = sys.argv[1:]
    j, every = 6, 30
    if '-j' in a:
        j = int(a[a.index('-j') + 1]); del a[a.index('-j'):a.index('-j') + 2]
    if '--every' in a:
        every = int(a[a.index('--every') + 1]); del a[a.index('--every'):a.index('--every') + 2]
    with ProcessPoolExecutor(j) as ex:
        for line in ex.map(one, [(s, every) for s in a]):
            print(line, flush=True)
