"""pack2.py: run 2's key logs (work/run2/keys.json, play2.py) -> data/realrun2/NNN_<segment>.keys.gz (run-length JSON
[[keys, count], ..], as data/realrun) + data/realrun2/manifest.json ({n, chapter, name, frames, end}), replaying
the whole chain from the soft reset once to take each segment's end status (and to prove the logs replay)."""
import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 're/buu', 'tools/run2'):
    sys.path.insert(0, str(ROOT / d))
from route import start_emu, status  # noqa: E402
from route2 import SEGMENTS, chapter_of  # noqa: E402

OUT = ROOT / 'data' / 'realrun2'


def rle(ks):
    out = []
    for k in ks:
        if out and out[-1][0] == k:
            out[-1][1] += 1
        else:
            out.append([k, 1])
    return out


def main():
    keys = json.loads((ROOT / 'work' / 'run2' / 'keys.json').read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    for p in OUT.glob('*.keys.gz'):
        p.unlink()
    e = start_emu('reset')
    man = []
    for n, (name, _) in enumerate(SEGMENTS):
        if name not in keys:
            print('stop: no keys for', name)
            break
        ks = keys[name]
        start = status(e)
        for k in ks:
            e.set_keys(k)
            e.run(1)
        # mtime 0: the same keys give the same bytes (no churn in git on a repack)
        with open(OUT / f'{n:03d}_{name}.keys.gz', 'wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as fh:
            fh.write(json.dumps(rle(ks), separators=(',', ':')).encode())
        man.append({'n': n, 'chapter': chapter_of(name), 'name': name, 'frames': len(ks), 'start': start, 'end': status(e)})
        print(n, name, len(ks), status(e), flush=True)
    (OUT / 'manifest.json').write_text(json.dumps(man, indent=1))
    print('frames', sum(m['frames'] for m in man))


if __name__ == '__main__':
    main()
