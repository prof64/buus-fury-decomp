"""show.py <file.json | micro name> [max_frames] -- print a recorded scenario compactly: per frame only what changed
(actors by obj#: sprite x y dir raw frame, camera, io, hud, sounds, scene steps)."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load(arg):
    p = Path(arg)
    if not p.exists():
        p = ROOT / 'data' / 'script_lockstep' / 'micro' / f'{arg}.json'
    if not p.exists():
        p = ROOT / 'data' / 'script_lockstep' / f'{arg}.json'
    return json.loads(p.read_text())


def fmt_actor(a):
    k, spr, x, y, z, d, raw, fr = a[:8]
    return f'#{k}:{spr} ({x / 256:g},{y / 256:g}) d{d} r{raw}.{fr}'


def main():
    doc = load(sys.argv[1])
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 10 ** 9
    print('done', doc['done'], 'objs', [(i, o['sprite'], o['vt'], o['player']) for i, o in enumerate(doc['objs'])])
    init = doc['init']
    print('init cam', [c / 1024 for c in init['cam']], 'area', init.get('area'))
    for a in init['actors']:
        print('  ', fmt_actor(a))
    prev = {}
    pcam = pio = phud = None
    for i, f in enumerate(doc['frames'][:n]):
        out = []
        cur = {a[0]: a for a in f['actors']}
        for k, a in cur.items():
            if prev.get(k, [None])[:8] != a[:8]:
                out.append(fmt_actor(a))
        for k in prev:
            if k not in cur:
                out.append(f'#{k} gone')
        prev = cur
        if f['cam'] != pcam:
            out.append(f'cam ({f["cam"][0] / 1024:g},{f["cam"][1] / 1024:g})')
            pcam = f['cam']
        if f['io'] != pio:
            out.append('io ' + ' '.join(f'{v:x}' for v in f['io']))
            pio = f['io']
        if f['hud'] != phud:
            out.append(f'hud {f["hud"]:x}')
            phud = f['hud']
        if f['snd']:
            out.append(f'snd {f["snd"]}')
        if f['steps']:
            out.append(f'steps {f["steps"]}')
        if f['keys']:
            out.append(f'keys {f["keys"]:x}')
        if out:
            print(i, ' | '.join(out))


if __name__ == '__main__':
    main()
