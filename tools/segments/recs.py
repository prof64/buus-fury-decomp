"""recs.py SEG.json.. [-j N] [--dir D]: the game's per-character state along a segment - replays the segment's keys
from its start save state and writes work/recs/<name>.json: [[row, state], ...] for row 0 and every row where the
state changed. state = {cur, zenie, supplies (hex), equip_list (sorted), chars: [[level-relevant xp, hp, max_hp, ep,
max_ep, str, pow, end, spd, points, ss_cooldown, form, sel, skills, equip x4], ...]} read from GameData
(0x03001068: +0 current, +2 supplies, +0xE4 equipment list, +0x4F4 zenie) and the records (0x03001560 + 0x68 * c).
The segment harness compares these rows (SEGMENT_RECS=1, report only: wf-import tests/segments.rs). The start state:
the header's source.state, else the real-run cut with the same (run, frame) under work/real/segs/<run>/states/, else
work/segments/<name>.ss. --dir: every *.json in D (segments only)."""
import json
import re
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
OUT = ROOT / 'work' / 'recs'
GD, REC0 = 0x03001068, 0x03001560


def header(p):
    """kind, name and source from the first line (the header line also holds init / objs: not parsed whole)"""
    with open(p) as f:
        h = f.read(4000)
    if '"kind":"segment"' not in h[:200]:
        return None
    m = re.search(r'"name":"([^"]+)"', h)
    s = re.search(r'"source":(\{[^}]*\})', h)
    try:
        return {'kind': 'segment', 'name': m.group(1), 'source': json.loads(s.group(1)) if s else {}}
    except (ValueError, AttributeError):
        return None


def state_of(p, h):
    src = h.get('source', {})
    if src.get('state') and (ROOT / src['state']).exists():
        return ROOT / src['state']
    if src.get('kind') == 'route':
        d = ROOT / 'work' / 'real' / 'segs' / Path(src['run']).name
        for q in d.glob('*.json'):
            hq = header(q)
            if hq and hq.get('source', {}).get('frame') == src.get('frame'):
                s = d / 'states' / (q.stem + '.ss')
                if s.exists():
                    return s
    s = ROOT / 'work' / 'segments' / (h['name'] + '.ss')
    if s.exists():
        return s
    # the recorders' own state folders (work/db57/segs5/states, work/db/dr1/segs/states, ...): by the cut's name
    return ss_index().get(h['name'] + '.ss')


_SS = None


def ss_index():
    global _SS
    if _SS is None:
        import os
        _SS = {}
        for d, _, fs in os.walk(ROOT / 'work'):
            for f in fs:
                if f.endswith('.ss'):
                    _SS.setdefault(f, Path(d) / f)
    return _SS


def doc_lf(p):
    """the logic counter (0x03000F94) in the segment's own start RAM (init.ram.iwram), or None"""
    import base64
    import zlib
    txt = Path(p).read_text().split('\n', 1)[0]
    i = txt.find('"ram"')
    if i < 0:
        return None
    j = txt.index('{', i)
    d = 0
    for k in range(j, len(txt)):
        if txt[k] == '{':
            d += 1
        elif txt[k] == '}':
            d -= 1
            if d == 0:
                break
    ram = json.loads(txt[j:k + 1])
    iw = ram.get('iwram')
    if not isinstance(iw, str):
        return None
    b = base64.b64decode(iw)
    try:
        b = zlib.decompress(b)
    except zlib.error:
        pass
    return int.from_bytes(b[0xF94:0xF98], 'little')


def start_of(p, h, st):
    """an emulator at the segment's start: its state file when that is the segment's own moment (the logic counter
    matches the segment's start RAM); else, for a cut, the cut rebuilt from its source recording (a state file under
    the cut's name can hold a later save - sb_000_18_12's work/segments state); else None"""
    from route import start_emu
    want = doc_lf(p)
    if st is not None:
        e = start_emu(str(st))
        if want is None or e.r32(0x03000F94) == want:
            return e
    src = h.get('source', {})
    if src.get('kind') == 'cut' and src.get('from') and src.get('at') is not None and (ROOT / src['from']).exists():
        sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
        import segment as sg
        head, frames = sg.load_doc(ROOT / src['from'])
        e = sg.replay_to(head, frames, src['at'])
        if want is None or e.r32(0x03000F94) == want:
            return e
    return None


def read_state(e):
    gd = e.read(GD, 0x500)
    n = min(int.from_bytes(gd[0xE4:0xE8], 'little'), 250)
    eq = sorted(int.from_bytes(gd[0xE8 + 4 * k:0xEC + 4 * k], 'little') & 0x0FFFFFFF for k in range(n)
                if int.from_bytes(gd[0xE8 + 4 * k:0xEC + 4 * k], 'little') >> 28 == 0)
    chars = []
    for c in range(9):
        r = e.read(REC0 + 0x68 * c, 0x68)
        u16 = lambda o: int.from_bytes(r[o:o + 2], 'little')
        s16 = lambda o: int.from_bytes(r[o:o + 2], 'little', signed=True)
        u32 = lambda o: int.from_bytes(r[o:o + 4], 'little')
        k = min(r[0x0C], 16)
        chars.append([u32(0x24), s16(0x16), s16(0x18), s16(0x1A), s16(0x1C), r[0x28], r[0x29], r[0x2A], r[0x2B], u16(0x2C),
                      u32(4), r[0x14], r[0x15], list(r[0x0D:0x0D + k]), [u32(0x3C + 4 * j) & 0x0FFFFFFF for j in range(4)]])
    return {'cur': gd[0], 'zenie': int.from_bytes(gd[0x4F4:0x4F8], 'little'), 'supplies': gd[2:2 + 83].hex(),
            'equip_list': eq, 'chars': chars}


def one(p):
    p = Path(p)
    h = header(p)
    if not h or h.get('kind') != 'segment':
        return f'{p.name}: not a segment'
    st = state_of(p, h)
    e = start_of(p, h, st)
    if e is None:
        return f'{p.name}: no start state' if st is None else f'{p.name}: start state is from another moment (logic counter) and no rebuild'
    rows = [json.loads(ln.strip().rstrip(',')) for ln in p.read_text().splitlines()[1:] if ln.startswith('{"keys"')]
    out, prev = [], None
    for k, r in enumerate(rows):
        e.set_keys(r['keys'])
        e.run(1)
        s = read_state(e)
        if s != prev:
            out.append([k, s])
            prev = s
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f'{h["name"]}.json').write_text(json.dumps(out, separators=(',', ':')))
    return f'{p.name}: {len(rows)} rows, {len(out)} states'


if __name__ == '__main__':
    a = sys.argv[1:]
    jobs = int(a[a.index('-j') + 1]) if '-j' in a else 8
    files = [x for x in a if x.endswith('.json')]
    if '--dir' in a:
        files += sorted(str(q) for q in Path(a[a.index('--dir') + 1]).glob('*.json'))
    with ProcessPoolExecutor(jobs) as ex:
        for line in ex.map(one, files):
            print(line, flush=True)
