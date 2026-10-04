"""area.py SEG.json.. [-j N] [--dir D] [--out DIR]: the game's area-local state along a segment (beside recs.py's
character records and world.py's GameData state) - replays the segment's keys from its start state (recs.py's start
rules) and writes the tracked sidecar data/segments/area/<name>.json, delta-encoded: {"v": 1, "rows": [[row,
delta], ...]}; row 0's delta is the whole state, later deltas hold only the fields that changed (each whole).

State (what lives in the field's own lists, not in GameData - a map's state before it is left):
  trig  the trigger list (0x03002A64: count, pointers), in list order, one entry per trigger:
          [vt, x0, y0, x1, y1, v] - vt the class (0x08049BC4 exit / edge, 0x08049BD4 event trigger, 0x0804A0C0 a
          coin's / dropped item's touch trigger, ...), the rect (+4 .. +0xA, s16), v = an event trigger's resume
          node (+0x10, u16: the exit value of the event's last run), else 0; a 0x0804A0C0 entry is [vt, 0, 0, 0, 0, 0]
          (present only: the rect follows a coin in flight)
        - a picked-up coin / item drops its touch trigger, a spent event trigger answers another node
  wg    the walk grid's tiles changed since the segment's start (the map object 0x030019A4 +0xA8: a bitmap, 32 bytes
        a row, 256 x 256, bit = free): sorted tile indices ty * 256 + tx whose bit differs from row 0's (null from the
        first row in another area than the start's: a load rebuilds the grid) - broken
        breakables, opened doors, script-set blocks (the start's grid is the importer's, `import_grids`)
The segment harness side (compare ours at each row the game's changed, lock clean gates) is the Earth agent's."""
import json
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
import recs  # noqa: E402

OUT = ROOT / 'data' / 'segments' / 'area'
TRIGS, CAM = 0x03002A64, 0x030019A4


def trig_list(e):
    n = min(e.r32(TRIGS), 256)
    out = []
    for k in range(n):
        o = e.r32(TRIGS + 4 + 4 * k)
        if not 0x02000000 <= o < 0x03008000:
            continue
        b = e.read(o, 0x12)
        s16 = lambda x: int.from_bytes(b[x:x + 2], 'little', signed=True)
        vt = int.from_bytes(b[0:4], 'little')
        # (a dropped coin's touch trigger follows the coin in flight: its rect changes every frame - kept as present
        # only; its position is the pickup's, compared with the actors)
        if vt == 0x0804A0C0:
            out.append([vt, 0, 0, 0, 0, 0])
            continue
        out.append([vt, s16(4), s16(6), s16(8), s16(10), int.from_bytes(b[0x10:0x12], 'little') if vt == 0x08049BD4 else 0])
    return out


def walk_grid(e):
    b = e.r32(CAM + 0xA8)
    if not 0x02000000 <= b < 0x03008000:
        return None
    return e.read(b, 32 * 256)


def changed_tiles(g0, g):
    if g0 is None or g is None:
        return []
    out = []
    for i, (x, y) in enumerate(zip(g0, g)):
        d = x ^ y
        if d:
            ty, bx = divmod(i, 32)
            out += [ty * 256 + bx * 8 + j for j in range(8) if d >> j & 1]
    return out


def one(p, out=OUT):
    p = Path(p)
    h = recs.header(p)
    if not h or h.get('kind') != 'segment':
        return f'{p.name}: not a segment'
    st = recs.state_of(p, h)
    e = recs.start_of(p, h, st)
    if e is None:
        return f'{p.name}: no start state'
    rows = [json.loads(ln.strip().rstrip(',')) for ln in p.read_text().splitlines()[1:] if ln.startswith('{"keys"')]
    g0 = walk_grid(e)
    area0 = rows[0].get('area') if rows else None
    packed, prev = [], None
    for k, r in enumerate(rows):
        e.set_keys(r['keys'])
        e.run(1)
        # the grid is tracked in the segment's first area only (a load rebuilds it whole): from the first row in
        # another area on, "wg" is null
        if r.get('area') != area0:
            g0 = None
        wg = changed_tiles(g0, walk_grid(e)) if g0 is not None else None
        # (the world maps' and a few scenes' buffer at +0xA8 is not a walk grid - it changes by thousands of bits a
        # frame: not tracked from there on)
        if wg is not None and len(wg) > 512:
            g0, wg = None, None
        s = {'trig': trig_list(e), 'wg': wg}
        if s != prev:
            packed.append([k, s if prev is None else {f: v for f, v in s.items() if v != prev[f]}])
            prev = s
    out.mkdir(parents=True, exist_ok=True)
    (out / f'{h["name"]}.json').write_text(json.dumps({'v': 1, 'rows': packed}, separators=(',', ':')))
    return f'{p.name}: {len(rows)} rows, {len(packed)} states'


def unpack(doc):
    """the delta rows back to whole states: [[row, {trig, wg}], ...]"""
    rows, cur = [], None
    for k, d in doc['rows']:
        cur = dict(d) if cur is None else {**cur, **d}
        rows.append([k, cur])
    return rows


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
