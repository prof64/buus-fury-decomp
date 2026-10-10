"""Drive the player towards enemies and attack, while recording every entity's animation state each frame.
Aggregates per sprite def / anim struct: observed frame orders and per-frame durations. Output is cached in
tools/extract/data/anim_obs_<game>.json and used by the exporters to fill duration_frames.

    .venv/bin/python tools/extract/autobattle.py log2 [map ...|@file]
    .venv/bin/python tools/extract/autobattle.py buu  [world-area ...|@file|all]

LoG2 keys are sprite-def addresses (hex), Buu keys are anim-struct addresses (hex).
"""
import json
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from animrec import Recorder, segments, KEY  # noqa: E402

CFG = {
    'log2': {'pos': (0x158, 0x15C)},
    'buu': {'pos': (0x1A0, 0x1A4), 'player_ptr': 0x03001C34},
    'log1': {'pos': (0x20, 0x22), 'half': True},
}


def pos(e, game, ent):
    a, b = CFG[game]['pos']
    if CFG[game].get('half'):
        return e.r16(ent + a), e.r16(ent + b)
    return e.r32(ent + a) / 256, e.r32(ent + b) / 256


def battle(game, e, frames=2400):
    r = Recorder(game, emu=e)
    r.run(30)
    t = 0
    while t < frames:
        last = {}
        for f, ent, d, ai, dr, fi in r.log[-60:]:
            last[ent] = d
        if game == 'buu':
            pl = [e.r32(CFG['buu']['player_ptr'])]
        elif game == 'log1':
            pl = [k for k, d in last.items() if d in (0x085C1884, 0x085C1F64, 0x085C18B0)]
        else:
            pl = [k for k, d in last.items() if d == 0x08312648]
        others = [k for k in last if k not in pl]
        keys = 0
        if pl and pl[0] and others:
            px, py = pos(e, game, pl[0])
            best = min(others, key=lambda k: (pos(e, game, k)[0] - px) ** 2 + (pos(e, game, k)[1] - py) ** 2)
            ox, oy = pos(e, game, best)
            dx, dy = ox - px, oy - py
            if abs(dx) + abs(dy) < 28:
                atk = [KEY.A, KEY.B, KEY.A, KEY.B, KEY.R][(t // 120) % 5]
                keys = atk if (t // 6) % 2 == 0 else 0
            elif abs(dx) > abs(dy):
                keys = KEY.RIGHT if dx > 0 else KEY.LEFT
            else:
                keys = KEY.DOWN if dy > 0 else KEY.UP
            if (t // 300) % 4 == 3:          # sometimes just stand still and get hit
                keys = 0
        r.run(6, keys)
        t += 6
    r.clear()
    return r.log


def aggregate(log, agg):
    for sg in segments(log):
        d, ai, dr = sg['key']
        if not d:
            continue
        key = f'{d:08X}'
        sub = str(ai) if ai >= 0 else '-'
        k = agg.setdefault(key, {}).setdefault(sub, {'count': 0, 'seqs': {}})
        k['count'] += 1
        fidx = ','.join(str(i) for i, _ in sg['seq'][:24])
        k['seqs'][fidx] = k['seqs'].get(fidx, 0) + 1
        for fi, n in sg['seq'][1:-1]:
            k.setdefault('dur', {}).setdefault(str(fi), []).append(n)
    return agg


def main():
    game = sys.argv[1]
    args = sys.argv[2:]
    if args and args[0].startswith('@'):
        args = open(args[0][1:]).read().split()
    out = Path(__file__).resolve().parent / 'data' / f'anim_obs_{game}.json'
    agg = json.loads(out.read_text()) if out.exists() else {}
    if game == 'log1':
        from emu_states import log1_warp
        for mi in sorted(set(int(x) for x in args)):
            aggregate(battle(game, log1_warp(mi)), agg)
            print('map', mi, flush=True)
    elif game == 'log2':
        from emu_states import log2_warp
        for mi in sorted(set(int(x) for x in args)):
            aggregate(battle(game, log2_warp(mi)), agg)
            print('map', mi, flush=True)
    else:
        from emu_states import buu_warp
        from gba import Rom
        if args == ['all']:
            R = Rom('buu')
            keys = []
            for i in range(0x1C4):
                b = R.bytes(0x0808E2E0 + 0x38 * i, 2)
                if (b[0], b[1]) not in keys and b[0] > 0:
                    keys.append((b[0], b[1]))
        else:
            keys = [tuple(int(v) for v in a.split('-')) for a in args]
        for k in keys:
            e, ok = buu_warp(*k)
            if ok:
                aggregate(battle(game, e, 1800), agg)
            print('area', k, ok, flush=True)
    for d in agg.values():
        for a in d.values():
            for fi, lst in a.get('dur', {}).items():
                a['dur'][fi] = sorted(lst)[-80:]
    out.write_text(json.dumps(agg, indent=0))


if __name__ == '__main__':
    main()
