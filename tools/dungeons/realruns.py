"""Dungeons 1-4 from the REAL route's checkpoint states (the ship agent's fat-Buu route, engine/docs/segments.md §8:
work/ship/fat): joins its key logs into one continuous run per dungeon, in rec_segments.py's layout.

usage: .venv/bin/python tools/dungeons/realruns.py [dr1 dr2 dr3 dr4]   -> work/db/<name>/{start.ss, keys.json, end.ss}

Each piece = (start state, key log, end state | None): the log is replayed from the piece's start; with an end state
the replay stops at the first frame whose logic counter and RNG equal that state's (a failed run's log is longer than
the stretch that reached the checkpoint), and the next piece must start from exactly that state - checked. No RAM
writes: the result is the real game's own input from the real state."""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status  # noqa: E402
from route import start_emu  # noqa: E402

FAT = ROOT / 'work' / 'ship' / 'fat'
OUT = ROOT / 'work' / 'db'
RNG = 0x03000F98


def sig(e):
    s = status(e)
    return (s['lf'], s['area'], s['p'], e.r32(0x030015E0), e.read(0x03001560, 0x68 * 5).hex())


RUNS = {
    # Thieves Den: 9-1 entrance (Goten 95, the Bandana from the Stolen Airship) -> the pad's take-off
    'dr1': [('d06_thieves.ss', 'keys_d06_d10.json', 'd10_map.ss')],
    # Diablo Desert + Express, played by db2_route.py from the real 11-1 state
    'dr2': [('db2/start.ss', 'db2/keys.json', None)],
    # Pilaf's Castle: 10-1 entrance -> Trunks' rock -> the Guardian -> the ball -> the pad's take-off
    'dr3': [('d11_pilaf.ss', 'keys_d11_d11b.json', 'd11b_rock.ss'), ('d11b_rock.ss', 'keys_d11b_d13.json', 'd13_pilaf_won.ss'),
            ('d13_pilaf_won.ss', 'keys_d13_fail.json', 'd15_map.ss')],
    # Nataday / the Volcano / Broly, by db4_route.py from the real 15-1 state, and the way back
    'dr4': [('db4/start.ss', 'db4/keys_a_fail.json', 'db4/j_down.ss'), ('db4b/j_down.ss', 'db4b/keys.json', None)],
}


def build(name):
    out = OUT / name
    out.mkdir(parents=True, exist_ok=True)
    pieces = RUNS[name]
    keys = []
    e = start_emu(str(FAT / pieces[0][0]))
    shutil.copyfile(FAT / pieces[0][0], out / 'start.ss')
    for k, (st, log, end) in enumerate(pieces):
        if k > 0:
            want = sig(start_emu(str(FAT / st)))
            assert sig(e) == want, (name, st, 'the previous piece does not end in this state')
        ks = json.loads((FAT / log).read_text())['keys']
        target = sig(start_emu(str(FAT / end))) if end else None
        n = 0
        for key in ks:
            if target is not None and sig(e) == target:
                break
            e.set_keys(key)
            e.run(1)
            keys.append(key)
            n += 1
        if target is not None:
            assert sig(e) == target, (name, log, 'never reached', end)
        print(name, log, n, 'frames ->', status(e), flush=True)
    e.set_keys(0)
    e.save_file(str(out / 'end.ss'))
    (out / 'keys.json').write_text(json.dumps({'from': None, 'keys': keys}))
    print(name, 'total', len(keys))


if __name__ == '__main__':
    for nm in sys.argv[1:] or list(RUNS):
        build(nm)
