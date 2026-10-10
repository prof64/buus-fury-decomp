"""recs_pack.py [--check]: the gates' game records (work/recs/<name>.json from recs.py) packed into the tracked sidecar
data/segments/recs/<gate name>.json, delta-encoded: {"v": 1, "rows": [[row, delta], ...]} - row 0's delta is
the whole state; later deltas hold only what changed: "cur", "zenie", "eq" (the equipment list), "sup" ({index:
count}), "c" ({char: [field index, value, ...]} over the record array recs.py writes: xp, hp, max_hp, ep, max_ep,
str, pow, end, spd, points, ss_cooldown, form, sel, skills, equip). wf-import `recs.rs` reads either form.
--check: unpack every sidecar and compare with its work/recs source."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SEG = ROOT / 'data' / 'segments'
OUT = SEG / 'recs'


def name_of(p):
    return re.search(r'"name":"([^"]+)"', p.open().read(400)).group(1)


def pack(rows):
    out, prev = [], None
    for k, s in rows:
        if prev is None:
            out.append([k, {'cur': s['cur'], 'zenie': s['zenie'], 'eq': s['equip_list'], 'sup': s['supplies'], 'c': s['chars']}])
        else:
            d = {}
            for f, g in (('cur', 'cur'), ('zenie', 'zenie'), ('eq', 'equip_list')):
                if s[g] != prev[g]:
                    d[f] = s[g]
            if s['supplies'] != prev['supplies']:
                a, b = bytes.fromhex(prev['supplies']), bytes.fromhex(s['supplies'])
                d['sup'] = {str(i): b[i] for i in range(len(b)) if a[i] != b[i]}
            cd = {}
            for c, (x, y) in enumerate(zip(prev['chars'], s['chars'])):
                ch = []
                for i, (u, v) in enumerate(zip(x, y)):
                    if u != v:
                        ch += [i, v]
                if ch:
                    cd[str(c)] = ch
            if cd:
                d['c'] = cd
            out.append([k, d])
        prev = s
    return {'v': 1, 'rows': out}


def unpack(doc):
    rows, cur = [], None
    for k, d in doc['rows']:
        if cur is None:
            cur = {'cur': d['cur'], 'zenie': d['zenie'], 'equip_list': d['eq'], 'supplies': d['sup'], 'chars': d['c']}
        else:
            cur = json.loads(json.dumps(cur))
            if 'cur' in d:
                cur['cur'] = d['cur']
            if 'zenie' in d:
                cur['zenie'] = d['zenie']
            if 'eq' in d:
                cur['equip_list'] = d['eq']
            if 'sup' in d:
                b = bytearray(bytes.fromhex(cur['supplies']))
                for i, v in d['sup'].items():
                    b[int(i)] = v
                cur['supplies'] = b.hex()
            for c, ch in d.get('c', {}).items():
                for j in range(0, len(ch), 2):
                    cur['chars'][int(c)][ch[j]] = ch[j + 1]
        rows.append([k, cur])
    return rows


if __name__ == '__main__':
    OUT.mkdir(exist_ok=True)
    total, n = 0, 0
    for p in sorted(SEG.glob('*.json')):
        src = ROOT / 'work' / 'recs' / f'{name_of(p)}.json'
        if not src.exists():
            continue
        rows = json.loads(src.read_text())
        dst = OUT / f'{name_of(p)}.json'
        if '--check' in sys.argv:
            assert unpack(json.loads(dst.read_text())) == rows, p.stem
            n += 1
            continue
        txt = json.dumps(pack(rows), separators=(',', ':'))
        dst.write_text(txt)
        total += len(txt)
        n += 1
    print(n, 'sidecars', total, 'bytes' if total else 'checked')
