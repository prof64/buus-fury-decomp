"""Area-load board: every area load of the real run measured on wf-core (a third board, beside scoreboard.py's two).

The main board's segments stop before each area load and the next one starts after it (`rec_segments.py`): the
load's own frames (the black / lag of the map rebuild, 50 - 70 video frames each) are compared nowhere. Here each
segment is recorded again from its start state through the load into the next segment's first frames, and replayed:
a segment that matched to its end on the main board shows here whether its load is exact or off by N frames.

usage: loadboard.py cut [JOBS]   (local: work/real/loads/; needs the main board's cuts and board_rows.json)
       loadboard.py [JOBS]       replays them -> engine/docs/scoreboard-loads.md + data/realrun/scoreboard_loads.json
The existing boards' segments and baseline are not touched."""
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
REAL = ROOT / 'work' / 'real'
LOADS = REAL / 'loads'
DATA = ROOT / 'data' / 'realrun'
AFTER = 60      # frames of the next segment compared after the load
sys.path.insert(0, str(HERE))
import scoreboard as sb  # noqa: E402


def runs():
    return sb.run_dirs()


def plan():
    """[(stretch, segment, global start, rows, global start of the next segment)] for every segment followed by one"""
    rs = runs()
    off, o = {}, 0
    for d in rs:
        off[d.name] = o
        o += len(json.loads((d / 'keys.json').read_text())['keys'])
    segs = []
    for d in rs:
        for p in sorted((REAL / 'segs' / d.name).glob('*.json')):
            f0 = int(re.search(r'"frame":(\d+)', open(p).readline()).group(1))
            n = sum(1 for ln in open(p) if ln.startswith('{"keys"'))
            segs.append((d.name, p.stem, off[d.name] + f0, n))
    segs.sort(key=lambda s: s[2])
    out = []
    for a, b in zip(segs, segs[1:]):
        if b[2] > a[2] + a[3]:
            out.append((a[0], a[1], a[2], a[3], b[2]))
    return out


def all_keys():
    k = []
    for d in runs():
        k += json.loads((d / 'keys.json').read_text())['keys']
    return k


KEYS = None


def cut_one(job):
    global KEYS
    d, seg, g0, n, g1 = job
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    sys.path.insert(0, str(ROOT / 'tools' / 'field'))
    import segment as sg
    from route import start_emu
    if KEYS is None:
        KEYS = all_keys()
    head = json.loads(open(REAL / 'segs' / d / f'{seg}.json').readline()[:-len(',"frames":[\n')] + '}')
    e = start_emu(str(REAL / 'segs' / d / 'states' / f'{seg}.ss'))
    m = min(g1 + AFTER, len(KEYS)) - g0
    init, objs, rows = sg.record(e, lambda j, _e: KEYS[g0 + j], m)
    # (compared as the main board's segments: rec_segments.py records no sprite-allocator rows)
    init.pop('vram', None)
    for r in rows:
        r.pop('vram', None)
    # the load: the rows between this segment's end and the next one's start (kind from the game's rows)
    a, b = rows[n - 1], rows[min(g1 - g0, len(rows) - 1)]
    depth = max(r.get('depth', 0) for r in rows[n:g1 - g0]) if g1 - g0 > n else 0
    ph = head['init'].get('snd_phase')
    if ph is not None:
        init = {'snd_phase': ph, **init}
    name = f'L_{seg}'
    meta = {'rows': n, 'gap': g1 - g0 - n, 'after': m - (g1 - g0), 'from': a['area'], 'to': b['area'], 'depth': depth,
            'logic_gap': (b.get('lf', 0) - a.get('lf', 0))}
    doc = {'kind': 'segment', 'name': name, 'source': {'kind': 'load', 'of': seg, **meta}, 'init': init, 'objs': objs,
           'frames': rows}
    sg.OUT = LOADS / d
    sg.save(name, doc)
    return seg, meta


def do_cut(jobs):
    work = plan()
    LOADS.mkdir(parents=True, exist_ok=True)
    metas = {}
    with ProcessPoolExecutor(jobs) as ex:
        for seg, meta in ex.map(cut_one, work, chunksize=4):
            metas[seg] = meta
    (LOADS / 'meta.json').write_text(json.dumps(metas, indent=0, sort_keys=True))
    print(len(metas), 'loads cut')


VLINE = re.compile(r'^    f(\d+) (\w+): (.*)$')


def replay_dir(d):
    env = dict(os.environ, SEGMENT_DIR=str(d), SEGMENT_STRICT='0', SEGMENT_VERBOSE='400')
    r = subprocess.run([sb.EXE, '--nocapture'], capture_output=True, text=True, cwd=ROOT / 'engine', env=env)
    out, cur, divs = {}, [], []
    for ln in r.stderr.splitlines():
        m = VLINE.match(ln)
        if m:
            divs.append((int(m.group(1)), m.group(2), m.group(3)))
            continue
        m = sb.LINE.match(ln)
        if m:
            out[m.group(1)] = {'frames': int(m.group(2)), 'matched': int(m.group(4)), 'first': m.group(6), 'divs': divs}
            divs = []
    return out


def kind_of(meta):
    w3 = lambda a: a[0] == 0 and a[1] in (50, 55)
    k = 'script warp' if meta['depth'] else 'exit'
    if w3(meta['to']) and not w3(meta['from']):
        k += ' -> 3-D map'
    elif w3(meta['from']) and not w3(meta['to']):
        k += ' 3-D map ->'
    elif w3(meta['from']):
        k += ' 3-D map'
    return k


def board(jobs):
    sb.build()
    metas = json.loads((LOADS / 'meta.json').read_text())
    main = json.loads((REAL / 'board_rows.json').read_text())
    exact_main = {r['segment'] for rows in main.values() for r in rows if r['matched'] >= r['frames']}
    dirs = [d for d in sorted(LOADS.iterdir()) if d.is_dir()]
    res = {}
    with ThreadPoolExecutor(jobs) as ex:
        for out in ex.map(replay_dir, dirs):
            res.update(out)
    rows = []
    for name, r in sorted(res.items()):
        seg = name[2:]
        m = metas.get(seg)
        if not m or seg not in exact_main:
            continue    # (a segment not exact on the main board says nothing about its load)
        n, gap, after = m['rows'], m['gap'], m['after']
        k = r['matched']
        if k >= r['frames']:
            res_k, off, what = 'exact', None, ''
        elif k < n:
            res_k, off, what = 'before the load', None, r['first'][:120]
        else:
            at = k - n
            res_k = 'load' if at < gap else 'after the load'
            # off by N: the run of consecutive frames the first divergence's field stays wrong (a fade / black one:
            # ours longer or shorter by about that many frames)
            fld = r['divs'][0][1] if r['divs'] else ''
            run = 0
            for j, (f, fl, _) in enumerate(d for d in r['divs'] if d[1] == fld):
                if f != k + j:
                    break
                run += 1
            off = run
            what = r['first'][:120]
        rows.append({'segment': seg, 'kind': kind_of(m), 'gap': gap, 'logic_gap': m['logic_gap'], 'from': m['from'],
                     'to': m['to'], 'result': res_k, 'at': None if res_k in ('exact', 'before the load') else k - n,
                     'off': off, 'first': what})
    kinds = {}
    for r in rows:
        c = kinds.setdefault(r['kind'], {'loads': 0, 'exact': 0, 'load': 0, 'after the load': 0, 'before the load': 0})
        c['loads'] += 1
        c['exact' if r['result'] == 'exact' else r['result']] += 1
    head = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
    L = ['# Area-load board: every area load of the real run on wf-core', '',
         f'Tree: `{head}`. Regenerate: `tools/segments/loadboard.py cut` (local, `work/real/loads/`), then',
         '`tools/segments/loadboard.py`. Each main-board segment that is exact, recorded again from its start state through',
         f'the area load after it into the next segment\'s first {AFTER} frames. **load** = the first divergence inside the load\'s',
         'frames (its length / black / lag: the area-load length model, `sndsim::load_frames`), **after the load** = in the next',
         'segment\'s first frames (what the load left: positions, camera, RNG), **before the load** = the segment itself',
         'differs when run on past its usual end (none expected). "off" = consecutive frames the first wrong field stays wrong',
         '(a black / fade row: ours longer or shorter by about that many).', '',
         '| load kind | loads | exact | load | after the load | before the load |', '|---|---|---|---|---|---|']
    for k, c in sorted(kinds.items(), key=lambda x: -x[1]['loads']):
        L.append(f"| {k} | {c['loads']} | {c['exact']} | {c['load']} | {c['after the load']} | {c['before the load']} |")
    t = {x: sum(c[x] for c in kinds.values()) for x in ('loads', 'exact', 'load', 'after the load', 'before the load')}
    L.append(f"| **all** | {t['loads']} | {t['exact']} | {t['load']} | {t['after the load']} | {t['before the load']} |")
    L += ['', '## Misses (for the area-load length model\'s owner)', '',
          'segment: load kind, from -> to, load frames (video / logic), where (frames into the load), off, first divergence', '']
    for r in sorted((r for r in rows if r['result'] != 'exact'), key=lambda r: (r['result'], r['kind'], r['segment'])):
        L.append(f"* `{r['segment']}` ({r['result']}): {r['kind']}, {r['from']} -> {r['to']}, {r['gap']} / {r['logic_gap']}, "
                 f"at {r['at']}, off {r['off']} - {r['first']}")
    (ROOT / 'engine' / 'docs' / 'scoreboard-loads.md').write_text('\n'.join(L) + '\n')
    (DATA / 'scoreboard_loads.json').write_text(json.dumps({'tree': head, 'kinds': kinds, 'total': t, 'loads': rows},
                                                           indent=1, sort_keys=True))
    print('loads:', t)


if __name__ == '__main__':
    a = sys.argv[1:]
    if a and a[0] == 'cut':
        do_cut(int(a[1]) if len(a) > 1 else 8)
    else:
        board(int(a[0]) if a else 8)
