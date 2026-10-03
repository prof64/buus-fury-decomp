"""Segment recorder (headless, muted): the real game from any emulator save state for N frames, with the start
state's work RAM, as ground truth for engine/crates/wf-import (import the start state into wf-core, replay the same
input, report the first divergence). Spec: engine/docs/segments.md.

  cut     a segment cut out of a playthrough recording (data/story_playthrough/*.json): the emulator state at the cut
          is rebuilt by replaying the recording's own keys from its start (soft reset + intro), then the following
          frames are recorded again with the same keys (and checked against the playthrough's rows)
            segment.py cut --from data/story_playthrough/opening.json --at area:1-12 [--frames N] [--name NAME]
            --at area:W-A[#n]   the n-th (1-based, default 1) entry into area W-A, cut at the end of the load's logic frame
            --at K               after video frame K of the recording
  record  a segment from a save state with a key list / key program / route goals
            segment.py record --state work/states/x.ss --keys keys.json | --program 'R:120,0:30:a' |
                              --goals module:ATTR [--frames N] [--name NAME] [--settle 0]
  cuts    list the area-entry cuts of a playthrough recording
            segment.py cuts --from data/story_playthrough/opening.json
  save    write the emulator save state at a playthrough frame / area cut (no recording)
            segment.py save --from ... --at area:1-13#2 --out work/states/x.ss

Output: data/segments/NAME.json (or $SEGMENT_DIR/NAME.json; one frame per line) =
  {kind: 'segment', name, source, init: {lf, keys (input manager held keys at the cut = the previous logic frame's
   input), area, rng, actors, objs, ram: {iwram, ewram} (zlib + base64)}, objs, frames: [rows as the playthrough]}
and work/segments/NAME.ss (the start save state; local only, like every .ss).
The playthrough row format: tools/story_playthrough/record.py."""
import argparse
import base64
import json
import os
import sys
import time
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'script_lockstep'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from capture import snapshot  # noqa: E402
from common_field import player_row  # noqa: E402
from route import start_emu, intro_keys, program_keys, WS, GD, DEPTH  # noqa: E402

OUT = ROOT / 'data' / 'segments'
STATES = ROOT / 'work' / 'segments'
INPUT = 0x03003300          # input manager: +2 held, +4 pressed, +6 released


def load_doc(path):
    lines = Path(path).read_text().split('\n')
    head = json.loads(lines[0][:-len(',"frames":[')] + '}') if lines[0].endswith(',"frames":[') else None
    if head is None:
        doc = json.loads(Path(path).read_text())
        return doc, doc.pop('frames')
    frames = [json.loads(ln.rstrip(',')) for ln in lines[1:] if ln.startswith('{')]
    return head, frames


def area_cuts(frames, limit=None):
    """[(K, [w, a, v], n)]: K = the last video frame of the logic frame in which the area changed (the load's frame
    is complete when the next frame's logic counter differs); n = 1-based entry count of that area"""
    out, seen, prev = [], {}, None
    frames = frames[:limit] if limit else frames
    for j, f in enumerate(frames):
        a = tuple(f['area'])
        if prev is not None and a != prev:
            # the first complete logic frame with the leader in the new area (a blocking on-enter script holds the
            # arrival for some logic frames: the leader appears later)
            k = j
            while k + 1 < len(frames) and (frames[k + 1]['lf'] == frames[k]['lf'] or frames[k]['p'] is None):
                k += 1
            key = (a[0], a[1])
            seen[key] = seen.get(key, 0) + 1
            out.append((k, list(a), seen[key]))
        prev = a
    return out


def parse_at(at, frames):
    if at.startswith('area:'):
        s = at[5:]
        n = 1
        if '#' in s:
            s, n = s.split('#')
            n = int(n)
        w, a = (int(x) for x in s.split('-'))
        for k, ar, m in area_cuts(frames):
            if (ar[0], ar[1]) == (w, a) and m == n:
                return k
        raise SystemExit(f'no entry #{n} into {w}-{a}')
    return int(at)


def replay_to(head, frames, k):
    """the emulator after video frame k of a recording: a playthrough (soft reset + intro + the recorded keys) or a
    segment (its start state work/segments/NAME.ss + the recorded keys)"""
    if head.get('kind') == 'segment':
        e = start_emu(str(STATES / f"{head['name']}.ss"))
    else:
        e = start_emu('reset')
        for f in range(head['skip']):
            e.set_keys(intro_keys(f))
            e.run(1)
    for j in range(k + 1):
        e.set_keys(frames[j]['keys'])
        e.run(1)
    return e


def pack(b):
    return base64.b64encode(zlib.compress(bytes(b), 9)).decode()


def init_of(e):
    init, rec = snapshot(e)
    init['keys'] = e.r16(INPUT + 2)
    init['ram'] = {'iwram': pack(e.iwram()), 'ewram': pack(e.ewram())}
    return init, rec


def row_extras(e, row, flags_prev):
    row['area'] = [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)]
    row['p'] = player_row(e)
    row['depth'] = e.r32(DEPTH)
    rc = 0x03001560 + 0x68 * e.r8(GD)
    row['zenie'] = e.r32(GD + 0x4F4)
    row['hp'] = e.r16(rc + 0x16)
    fl = e.read(GD + 0x55, 0x80).hex()
    if fl != flags_prev:
        row['flags'] = fl
    return fl


VRAM = 0x03001C4C


def vram_state(e):
    """the object tile allocator (wf-core vram.rs): free list head, spare head, then per entry
    [used, start, size, hi, lo, fnext, fprev] (spare entries: [255, hi, lo])"""
    b = e.read(VRAM + 0x694, 138 * 10)
    out = [e.r8(VRAM), e.r8(VRAM + 1)]
    for i in range(138):
        x = b[10 * i:10 * i + 10]
        if x[0] == 0xFF:
            out.append([255, x[6], x[7]])
        else:
            out.append([x[0], x[2] | x[3] << 8, x[4] | x[5] << 8, x[6], x[7], x[8], x[9]])
    return out


def row_vram(e, row, prev):
    v = vram_state(e)
    if v != prev:
        row['vram'] = v
    return v


def record(e, keys_of, n, stop=None):
    """record n video frames; keys_of(j, e) -> keys of segment frame j (None = stop)"""
    init, rec = init_of(e)
    rec.arm()
    rows = []
    fl = e.read(GD + 0x55, 0x80).hex()
    vr = vram_state(e)
    init['vram'] = vr
    for j in range(n):
        k = keys_of(j, e)
        if k is None:
            break
        e.set_keys(k)
        e.run(1)
        row = rec.row(k)
        fl = row_extras(e, row, fl)
        vr = row_vram(e, row, vr)
        rows.append(row)
        if stop and stop(e):
            break
    rec.disarm()
    return init, rec.objs, rows


def save(name, doc):
    out = Path(os.environ.get('SEGMENT_DIR', OUT))
    out.mkdir(parents=True, exist_ok=True)
    p = out / f'{name}.json'
    frames = doc.pop('frames')
    head = json.dumps(doc, separators=(',', ':'))
    body = ',\n'.join(json.dumps(f, separators=(',', ':')) for f in frames)
    p.write_text(head[:-1] + ',"frames":[\n' + body + '\n]}\n')
    return p


def check_against(frames, rows, k0):
    """the re-recorded rows vs the playthrough's (keys, logic counter, RNG, area, player, zenie)"""
    for j, r in enumerate(rows):
        f = frames[k0 + 1 + j]
        for key in ('keys', 'lf', 'rng', 'area', 'p', 'zenie', 'cam'):
            if f.get(key) != r.get(key):
                return f'frame {j} ({k0 + 1 + j}): {key} {r.get(key)} vs playthrough {f.get(key)}'
    return None


def cmd_cut(a):
    head, frames = load_doc(a.src)
    k = parse_at(a.at, frames)
    n = min(a.frames or len(frames), len(frames) - k - 1)
    name = a.name or f'cut_{k}'
    t = time.time()
    e = replay_to(head, frames, k)
    STATES.mkdir(parents=True, exist_ok=True)
    e.save_file(STATES / f'{name}.ss')
    init, objs, rows = record(e, lambda j, _e: frames[k + 1 + j]['keys'], n)
    bad = check_against(frames, rows, k)
    src = {'kind': 'cut', 'from': str(Path(a.src).resolve().relative_to(ROOT)), 'at': k, 'spec': a.at,
           'state': str((STATES / f'{name}.ss').relative_to(ROOT))}
    doc = {'kind': 'segment', 'name': name, 'source': src, 'init': init, 'objs': objs, 'frames': rows}
    p = save(name, doc)
    print(p, len(rows), 'frames from', k, 'area', rows[0]['area'] if rows else None, f'{time.time() - t:.1f}s',
          'MISMATCH vs playthrough: ' + bad if bad else '(identical to the playthrough rows)')


def cmd_record(a):
    e = start_emu(a.state)
    if a.settle:
        e.run(a.settle)
    n = a.frames or 3000
    if a.keys:
        ks = json.loads(Path(a.keys).read_text())
        if isinstance(ks, dict):
            ks = [f['keys'] for f in ks['frames']] if 'frames' in ks else ks['keys']
        keys_of = lambda j, _e: ks[j] if j < len(ks) else None  # noqa: E731
    elif a.program:
        ks = program_keys(a.program)
        keys_of = lambda j, _e: ks[j] if j < len(ks) else None  # noqa: E731
    elif a.goals:
        from driver import Driver
        mod, attr = a.goals.split(':')
        sys.path.insert(0, str(Path(mod).resolve().parent))
        goals = getattr(__import__(Path(mod).stem), attr)
        drv = Driver(goals)
        keys_of = lambda j, ee: None if drv.done() else drv.keys(ee)  # noqa: E731
    else:
        keys_of = lambda j, _e: 0  # noqa: E731
    name = a.name or Path(a.state).stem
    STATES.mkdir(parents=True, exist_ok=True)
    e.save_file(STATES / f'{name}.ss')
    init, objs, rows = record(e, keys_of, n)
    src = {'kind': 'state', 'state': a.state, 'settle': a.settle, 'keys': a.keys, 'program': a.program, 'goals': a.goals}
    doc = {'kind': 'segment', 'name': name, 'source': src, 'init': init, 'objs': objs, 'frames': rows}
    p = save(name, doc)
    print(p, len(rows), 'frames; start area', init['area'], 'ends in', rows[-1]['area'] if rows else None)


def cmd_cuts(a):
    head, frames = load_doc(a.src)
    for k, ar, n in area_cuts(frames, a.limit):
        # a load inside a running event (a script's warp / scene load) is not an importable area entry
        # an importable entry: an exit's arrival (the leader autowalks into the fade-in), not a script's / save load
        p = frames[k].get('p')
        note = '  (inside an event: wf-import continues the event)' if frames[k].get('depth') else \
            '' if p and p[3] == 'autowalk' else f'  (not an exit arrival, leader {p and p[3]}: not importable yet)'
        print(f'area:{ar[0]}-{ar[1]}#{n}', 'frame', k, 'variant', ar[2], 'lf', frames[k]['lf'], note)


def cmd_save(a):
    head, frames = load_doc(a.src)
    k = parse_at(a.at, frames)
    e = replay_to(head, frames, k)
    e.save_file(a.out)
    print(a.out, 'after video frame', k)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest='cmd', required=True)
    c = sub.add_parser('cut')
    c.add_argument('--from', dest='src', required=True)
    c.add_argument('--at', required=True)
    c.add_argument('--frames', type=int)
    c.add_argument('--name')
    r = sub.add_parser('record')
    r.add_argument('--state', required=True)
    r.add_argument('--keys')
    r.add_argument('--program')
    r.add_argument('--goals')
    r.add_argument('--frames', type=int)
    r.add_argument('--settle', type=int, default=0)
    r.add_argument('--name')
    l = sub.add_parser('cuts')
    l.add_argument('--from', dest='src', required=True)
    l.add_argument('--limit', type=int)
    s = sub.add_parser('save')
    s.add_argument('--from', dest='src', required=True)
    s.add_argument('--at', required=True)
    s.add_argument('--out', required=True)
    a = ap.parse_args()
    {'cut': cmd_cut, 'record': cmd_record, 'cuts': cmd_cuts, 'save': cmd_save}[a.cmd](a)


if __name__ == '__main__':
    main()
