"""world.py SEG.json.. [-j N] [--dir D] [--out DIR]: the game's world state along a segment (beside recs.py's
character records) - replays the segment's keys from its start state (recs.py's start rules) and writes the tracked
sidecar data/segments/world/<name>.json, delta-encoded: {"v": 1, "rows": [[row, delta], ...]}; row 0's delta is the
whole state, later deltas hold only what changed.

State (GameData 0x03001068 outside the character records, plus the script words; data/field/gamestate_layout.json):
  bits   GD+1 game bits (bit1 HUD shown, bit2 countdown on, bit4 stopwatch on, bit5 game_bit_20, bit6 game_bit_40)
  flags  GD+0x55, 143 bytes: the story flags (event progress, opened chests, defeated bosses, map-visited from 448)
         - row 0 as hex, later rows "fl": {byte index: new byte}
  cd sw  GD+0x4D4 countdown, GD+0x4D8 stopwatch (frames; while one runs, kept at the other changes and every 300
         rows - each kept row then carries both)
  holy   GD+0x4DC Holy Water frames left + the four bonuses (+0x4E0..+0x4EC)
  spec   GD+0x4F0 timed special
  l g    script words L0..L3 (0x03001020), globals G0..G3 (0x03001058; the scripts' scratch: not a change of its
         own, carried whole by every kept row, compared only with no script running)
  var    the variant byte (0x03002BB0)
The segment harness compares ours at each row where the game's changed (SEGMENT_WORLD=1, report only; wf-import
world.rs); a gate with "world_exact": true fails on any difference."""
import json
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import recs  # noqa: E402

OUT = ROOT / 'data' / 'segments' / 'world'
GD = 0x03001068


def read_state(e):
    gd = e.read(GD, 0x4F4)
    s32 = lambda b, o: int.from_bytes(b[o:o + 4], 'little', signed=True)
    lw = e.read(0x03001020, 16)
    gw = e.read(0x03001058, 16)
    return {'bits': gd[1], 'flags': gd[0x55:0xE4].hex(), 'cd': s32(gd, 0x4D4), 'sw': s32(gd, 0x4D8),
            'holy': [s32(gd, 0x4DC + 4 * k) for k in range(5)], 'spec': s32(gd, 0x4F0),
            'l': [s32(lw, 4 * k) for k in range(4)], 'g': [s32(gw, 4 * k) for k in range(4)],
            'var': e.read(0x03002BB0, 1)[0]}


def delta(prev, s):
    if prev is None:
        return dict(s)
    d = {k: v for k, v in s.items() if k != 'flags' and v != prev[k]}
    if s['flags'] != prev['flags']:
        a, b = bytes.fromhex(prev['flags']), bytes.fromhex(s['flags'])
        d['fl'] = {str(i): b[i] for i in range(len(b)) if a[i] != b[i]}
    return d


def one(p, out=OUT):
    p = Path(p)
    h = recs.header(p)
    if not h or h.get('kind') != 'segment':
        return f'{p.name}: not a segment'
    st = recs.state_of(p, h)
    e = recs.start_of(p, h, st)
    if e is None:
        return f'{p.name}: no start state' if st is None else f'{p.name}: start state is from another moment'
    rows = [json.loads(ln.strip().rstrip(',')) for ln in p.read_text().splitlines()[1:] if ln.startswith('{"keys"')]
    packed, prev, prev_key = [], None, None
    for k, r in enumerate(rows):
        e.set_keys(r['keys'])
        e.run(1)
        s = read_state(e)
        # a running countdown / stopwatch changes every field frame: its value is kept at the other changes and every
        # 300 rows (each kept row carries it whole), not each tick
        key = {f: v for f, v in s.items() if f not in ('cd', 'sw', 'g')}
        if key != prev_key or (s['bits'] & 0x14 and k % 300 == 0) or (s['bits'] & 0x14 == 0 and (s['cd'], s['sw']) != (prev['cd'], prev['sw'])):
            d = delta(prev, s)
            if prev is not None and s['bits'] & 0x14:
                d['cd'], d['sw'] = s['cd'], s['sw']
            if prev is not None:
                d['g'] = s['g']
            packed.append([k, d])
            prev, prev_key = s, key
    out.mkdir(parents=True, exist_ok=True)
    (out / f'{h["name"]}.json').write_text(json.dumps({'v': 1, 'rows': packed}, separators=(',', ':')))
    return f'{p.name}: {len(rows)} rows, {len(packed)} states'


if __name__ == '__main__':
    a = sys.argv[1:]
    jobs = int(a[a.index('-j') + 1]) if '-j' in a else 8
    out = Path(a[a.index('--out') + 1]) if '--out' in a else OUT
    files = [x for x in a if x.endswith('.json')]
    if '--dir' in a:
        files += sorted(str(q) for q in Path(a[a.index('--dir') + 1]).glob('*.json'))
    with ProcessPoolExecutor(jobs) as ex:
        for line in ex.map(one, files, [out] * len(files)):
            print(line, flush=True)
