"""sounds.py [OUT.json.gz] [--frames N]: the game's sound requests along the whole real run - the same chain as
record.py (the soft reset, the opening / Earth keys, then the 73 data/realrun stretches), with breakpoints on the
sound driver's request functions (tools/field/common_field.py `SoundLog`: PlaySfx 0x0803E378, StopSfx 0x0803E302,
StopAll 0x0803E2EC, StopMatch 0x0803E32C, PlaySong 0x0803F110). Writes the tracked, compact
data/realrun/sounds.json.gz: {"rows": N, "ev": [[row, [event, ...]], ...]} - only rows with a request; an event is
["play", sfx id, volume] / ["stop", id] / ["stopmatch", id] / ["stopall"] / ["song", id] (the segment rows' `snd`
format). The whole-run comparison (examples/wholerun.rs `--sounds`) groups them by logic iteration: a request made
in an iteration's long CPU time lands on one of its rows (the lag rows included)."""
import gzip
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
from route import start_emu, status  # noqa: E402
from common_field import PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG, SFX_TABLE, SONG_TABLE  # noqa: E402
import realrun  # noqa: E402

a = sys.argv[1:]
out = Path(a[0]) if a and not a[0].startswith('--') else ROOT / 'data' / 'realrun' / 'sounds.json.gz'
limit = int(a[a.index('--frames') + 1]) if '--frames' in a else None

parts = [(name, realrun.pre_keys()[part], None) for _, name, part in realrun.PRE]
man = json.loads((ROOT / 'data' / 'realrun' / 'manifest.json').read_text())
for m in man:
    keys = realrun.unrle(json.loads(gzip.open(ROOT / 'data' / 'realrun' / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
    parts.append((f'{m["n"]:03d}_{m["name"]}', keys, m['end']))

e = start_emu('reset')
for addr in (PLAY_SFX, STOP_SFX, STOP_ALL, STOP_MATCH, PLAY_SONG):
    e.breakpoint(addr)


def collect(hits):
    ev = []
    for h in hits:
        if h['kind'] != 'break':
            continue
        ad, r = h['addr'] & ~1, h['regs']
        if ad == PLAY_SFX:
            ev.append(['play', (r[1] - SFX_TABLE) // 12, r[2] & 0xFF])
        elif ad == STOP_SFX:
            ev.append(['stop', (r[1] - SFX_TABLE) // 12])
        elif ad == STOP_ALL:
            ev.append(['stopall'])
        elif ad == STOP_MATCH:
            ev.append(['stopmatch', (r[1] - SFX_TABLE) // 12])
        elif ad == PLAY_SONG:
            ev.append(['song', (r[1] - SONG_TABLE) // 20 if SONG_TABLE <= r[1] < SONG_TABLE + 53 * 20 else -1])
    return ev


rows, evs, t0 = 0, [], time.time()
for name, keys, end in parts:
    for k in keys:
        if limit is not None and rows >= limit:
            break
        e.set_keys(k)
        e.run(1)
        ev = collect(e.hits())
        if ev:
            evs.append([rows, ev])
        rows += 1
    if limit is not None and rows >= limit:
        break
    if end is not None and status(e) != end:
        raise SystemExit(f'{name}: end state {status(e)} != manifest {end}')
    print(f'{name}: to row {rows}, {len(evs)} rows with requests ({time.time() - t0:.0f}s)', flush=True)
out.parent.mkdir(parents=True, exist_ok=True)
with gzip.open(out, 'wt') as f:
    json.dump({'rows': rows, 'ev': evs}, f, separators=(',', ':'))
print('rows', rows, 'with requests', len(evs))
