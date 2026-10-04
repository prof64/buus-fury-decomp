"""The whole-run chains: real playthroughs from the soft reset, each one continuous key log.

  run1  the first real run: data/story_playthrough/earth_route_keys.json (title, New Game, Other World, Earth) then the
        73 data/realrun/*.keys.gz stretches (ship -> credits)
  run2  the second, played differently (tools/run2/route2.py: game overs, a second New Game, saves / Continues, shops,
        other stat points, ...): data/realrun2/*.keys.gz

`--chain NAME` on record.py / sounds.py / resync.py / check.py picks one (default run1); per chain: the recording
(`master`), its sound requests (`sounds`), its known stops (`known`) and the cut directory (`cuts`)."""
import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
W = ROOT / 'work' / 'wholerun'
NAMES = ('run1', 'run2')


def chain_arg(argv):
    c = argv[argv.index('--chain') + 1] if '--chain' in argv else 'run1'
    if c not in NAMES:
        raise SystemExit(f'unknown chain {c} ({", ".join(NAMES)})')
    return c


def paths(chain):
    if chain == 'run1':
        return {'master': W / 'master.jsonl', 'sounds': ROOT / 'data' / 'realrun' / 'sounds.json.gz',
                'known': ROOT / 'data' / 'realrun' / 'known_stops.json', 'cuts': W / 'cuts', 'dir': W,
                'manifest': ROOT / 'data' / 'realrun' / 'manifest.json',
                # run 1 is strict: no ratchet file
                'expected': ROOT / 'data' / 'realrun' / 'no_expected.json'}
    d = ROOT / 'data' / 'realrun2'
    return {'master': W / 'run2' / 'master.jsonl', 'sounds': d / 'sounds.json.gz', 'known': d / 'known_stops.json',
            'cuts': W / 'run2' / 'cuts', 'dir': W / 'run2', 'manifest': d / 'manifest.json',
            # check.py's ratchet: fail only if the first difference comes earlier than this
            'expected': d / 'expected.json'}


def parts(chain):
    """[(stretch name, per-frame keys, expected end status or None)] in order"""
    import realrun
    if chain == 'run1':
        out = [(name, realrun.pre_keys()[part], None) for _, name, part in realrun.PRE]
        src = ROOT / 'data' / 'realrun'
    else:
        out = []
        src = ROOT / 'data' / 'realrun2'
    for m in json.loads((src / 'manifest.json').read_text()):
        keys = realrun.unrle(json.loads(gzip.open(src / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
        out.append((f'{m["n"]:03d}_{m["name"]}', keys, m['end']))
    return out


def chapter_of(chain, stretch):
    """a stretch's chapter (the routing owner key)"""
    try:
        n = int(stretch[:3])
    except ValueError:
        return 'opening' if 'opening' in stretch else 'earth'
    for m in json.loads(paths(chain)['manifest'].read_text()):
        if m['n'] == n:
            return m['chapter']
    return '?'
