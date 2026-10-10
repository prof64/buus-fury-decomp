"""holy.py [SEG.json ..] [-j N]: the timed boost's count (Holy Water / boost_stats: GameData +0x4DC, frames left)
along segments -> data/segments/timers/holy.json (engine/crates/wf-import/tests/holy_timer.rs).

Each segment's keys are replayed on the emulator from its start state (recs.py's start rules) with the count poked to
20000 and the four bonuses (+0x4E0..) to 0: nothing else of the game changes, so the recording stays in step. The
count after every row is packed as runs [row, value, step] (step -1 counting, 0 held). Default: every segment of
data/segments; the file's "known" notes are kept."""
import json
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import recs  # noqa: E402

GD = 0x03001068
OUT = ROOT / 'data' / 'segments' / 'timers' / 'holy.json'
START = 20000


def one(p):
    p = Path(p)
    try:
        h = recs.header(p)
        if not h or h.get('kind') != 'segment':
            return p.name, None, 'not a segment'
        e = recs.start_of(p, h, recs.state_of(p, h))
        if e is None:
            return p.name, None, 'no start state'
        e.w32(GD + 0x4DC, START)
        for k in range(4):
            e.w32(GD + 0x4E0 + 4 * k, 0)
        rows = [json.loads(ln.strip().rstrip(',')) for ln in p.read_text().splitlines()[1:] if ln.startswith('{"keys"')]
        v = []
        for r in rows:
            e.set_keys(r['keys'])
            e.run(1)
            v.append(e.r32(GD + 0x4DC))
        runs, k = [], 0
        while k < len(v):
            step = v[k + 1] - v[k] if k + 1 < len(v) else 0
            if step not in (0, -1):
                step = 0
            j = k
            while j + 1 < len(v) and v[j + 1] - v[j] == step:
                j += 1
            runs.append([k, v[k], step])
            k = j + 1
        return h['name'], runs, f'{len(rows)} rows, {len(runs)} runs'
    except Exception as ex:  # noqa: BLE001
        return p.name, None, f'error {ex!r}'


if __name__ == '__main__':
    a = sys.argv[1:]
    jobs = int(a[a.index('-j') + 1]) if '-j' in a else 8
    files = [x for x in a if x.endswith('.json')] or sorted(str(q) for q in (ROOT / 'data' / 'segments').glob('*.json'))
    doc = json.loads(OUT.read_text()) if OUT.exists() else {'about': '', 'known': {}, 'segments': {}}
    with ProcessPoolExecutor(jobs) as ex:
        for name, runs, note in ex.map(one, files):
            print(f'{name}: {note}', flush=True)
            if runs is not None:
                doc['segments'][name] = runs
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, separators=(',', ':')))
