"""dump.py [-j N] [--every K] SEG.json..: the emulator's screen at sampled video frames of segments that have no
screen dump yet (the 121 `tour_*` segments: on the state gate, never on the picture board) - as
work/superbuu/pix/dump.py (frame 0, every K-th (default 30), the last; work/superbuu/pix/fb/<name>.fb, git-ignored),
with the tours' start states (work/states/<source.state>) among the candidates. The replay must end on the
recording's own logic counter (else nothing is written). An existing dump is not overwritten. Headless, muted."""
import json
import sys
import zlib
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
OUT = ROOT / 'work' / 'superbuu' / 'pix' / 'fb'


def one(args):
    p, every = args
    import recs
    from route import start_emu
    p = Path(p)
    h = recs.header(p)
    if not h:
        return f'{p.name}: not a segment'
    out = OUT / f'{h["name"]}.fb'
    if out.exists():
        return f'{p.name}: has a dump'
    rows = [json.loads(ln.strip().rstrip(',')) for ln in p.read_text().splitlines()[1:] if ln.startswith('{"keys"')]
    lf = recs.seg_lf(p)
    cands = list(recs.candidates(p, h))
    s = h.get('source', {}).get('state')
    if s:
        cands += [ROOT / 'work' / 'states' / s, ROOT / 'work' / 'states' / (s + '.ss')]
    st = None
    for c in cands:
        if c.exists() and (lf is None or 0 <= lf - start_emu(str(c)).r32(0x03000F94) <= 2):
            st = c
            break
    if st is None:
        return f'{p.name}: no start state at the recording\'s logic counter'
    n = len(rows)
    want = sorted({0, n - 1} | set(range(0, n, every)))
    e = start_emu(str(st))
    blobs, wi = [], 0
    for k, r in enumerate(rows):
        e.set_keys(r['keys'])
        e.run(1)
        if wi < len(want) and want[wi] == k:
            blobs.append(zlib.compress(e.pixels(), 6))
            wi += 1
    last = rows[-1]
    if e.r32(0x03000F94) != last['lf']:
        return f'{p.name}: the replay ends on logic counter {e.r32(0x03000F94)}, the recording on {last["lf"]} - not written'
    with out.open('wb') as f:
        f.write((json.dumps({'frames': want[:len(blobs)], 'sizes': [len(b) for b in blobs]}) + '\n').encode())
        for b in blobs:
            f.write(b)
    return f'{h["name"]}: {len(blobs)} frames'


if __name__ == '__main__':
    a = sys.argv[1:]
    j, every = 8, 30
    if '-j' in a:
        j = int(a[a.index('-j') + 1]); del a[a.index('-j'):a.index('-j') + 2]
    if '--every' in a:
        every = int(a[a.index('--every') + 1]); del a[a.index('--every'):a.index('--every') + 2]
    OUT.mkdir(parents=True, exist_ok=True)
    with ProcessPoolExecutor(j) as ex:
        for line in ex.map(one, [(p, every) for p in a]):
            print(line, flush=True)
