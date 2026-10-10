"""Static story-flag map: per script (event sN / inline_0x...), flags read / set / cleared, warps, area refs.
usage: python3 flags_scan.py lo hi   -- summary of events lo..hi"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CAT = json.load(open(ROOT / 'engine/annotations/script_catalog.json'))
RH = ROOT / 'work/buu/script/rhai/scripts'
S = {x['id']: x for x in CAT['scripts']}


def rhai(sid):
    p = RH / ('events' if sid.startswith('s') else 'inline') / f'{sid}.rhai'
    return p.read_text() if p.exists() else ''


def ops(sid):
    t = rhai(sid)

    def g(name):
        return sorted({int(m) for m in re.findall(name + r'\((\d+)', t)})
    return {'set': g(r'\bset_flag'), 'clear': g(r'\bclear_flag'),
            'read': sorted(set(g(r'\bget_flag')) | set(g(r'\bnot_flag'))),
            'bits_set': re.findall(r'set_flag_bits\(([^)]*)\)', t), 'bits_get': re.findall(r'get_flag_bits\(([^)]*)\)', t),
            'scenes': re.findall(r'load_scene\(([^)]*)\)', t), 'warps': re.findall(r'\bwarp\(([^)]*)\)', t),
            'run_event': re.findall(r'run_event\((\d+)', t)}


def areas(sid):
    out = []
    for r in S.get(sid, {}).get('referenced_by', []):
        m = re.search(r'world (\d+) map (\d+)', r.get('where', ''))
        out.append((f'{m.group(1)}-{m.group(2)}' if m else '?') + ':' + r['kind'])
    return out


def gotos(sid):
    return [n['goto_event'] for n in S.get(sid, {}).get('nodes', []) if n.get('kind') == 7]


def first_text(sid):
    for n in S.get(sid, {}).get('nodes', []):
        if n.get('text'):
            return n['text'][:70]
    return ''


if __name__ == '__main__':
    lo, hi = int(sys.argv[1]), int(sys.argv[2])
    for i in range(lo, hi + 1):
        sid = f's{i:03d}'
        o = ops(sid)
        print(sid, S[sid].get('purpose'), areas(sid)[:3], 'goto', gotos(sid),
              {k: v for k, v in o.items() if v}, repr(first_text(sid)))
