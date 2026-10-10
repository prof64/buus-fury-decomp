"""music.py [--chain run1|run2|run3] [OUT.json.gz] [--frames N]: the game's music state along a whole real run - the
same chain as record.py / sounds.py, read from the sound object after every video frame: the player's playing byte
(0x030038A4 + 5: the scripts' stop_music clears only this), its song (+0xC, the header's index in the song table) and
the Options block's volumes (0x03001914: [1] sound effects, [2] music). Writes the tracked, compact
data/realrun*/music.json.gz: {"rows": N, "ev": [[row, playing, song, music volume, sfx volume], ...]} - only the rows
where the state changed (row 0 always). The whole-run comparison (examples/wholerun.rs `--music`) checks the app's
state after each logic iteration against the game's at the iteration's last row. Headless, muted."""
import gzip
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
from route import start_emu, status  # noqa: E402
from common_field import SONG_TABLE  # noqa: E402
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
import chains  # noqa: E402

PLAYER = 0x030038A4
OPTIONS = 0x03001914

a = sys.argv[1:]
chain = chains.chain_arg(a)
pos = [x for i, x in enumerate(a) if not x.startswith('--') and (i == 0 or a[i - 1] not in ('--chain', '--frames'))]
out = Path(pos[0]) if pos else chains.paths(chain)['sounds'].with_name('music.json.gz')
limit = int(a[a.index('--frames') + 1]) if '--frames' in a else None

e = start_emu('reset')


def state():
    sp = e.r32(PLAYER + 0xC)
    song = (sp - SONG_TABLE) // 20 if SONG_TABLE <= sp < SONG_TABLE + 53 * 20 else -1
    return [e.r8(PLAYER + 5), song, e.r8(OPTIONS + 2), e.r8(OPTIONS + 1)]


rows, evs, last, t0 = 0, [], None, time.time()
for name, keys, end in chains.parts(chain):
    for k in keys:
        if limit is not None and rows >= limit:
            break
        e.set_keys(k)
        e.run(1)
        st = state()
        if st != last:
            evs.append([rows] + st)
            last = st
        rows += 1
    if limit is not None and rows >= limit:
        break
    if end is not None and status(e) != end:
        raise SystemExit(f'{name}: end state {status(e)} != manifest {end}')
    print(f'{name}: to row {rows}, {len(evs)} changes ({time.time() - t0:.0f}s)', flush=True)
out.parent.mkdir(parents=True, exist_ok=True)
with gzip.open(out, 'wt') as f:
    json.dump({'rows': rows, 'ev': evs}, f, separators=(',', ':'))
print('rows', rows, 'changes', len(evs))
