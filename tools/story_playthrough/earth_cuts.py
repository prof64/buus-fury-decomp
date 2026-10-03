"""Cut a long Earth-chapter recording (data/segments style, made by segment.py record from the chapter start) into
segments at every importable area-entry cut, each running to the next area change (or --max frames).

usage: python3 tools/story_playthrough/earth_cuts.py RECORDING.json PREFIX [--max N] [--out DIR] [--only area:W-A#n,...]
  writes DIR/PREFIX_W_A_n.json (+ work/segments/PREFIX_W_A_n.ss) via tools/segments/segment.py cut"""
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
from segment import load_doc, area_cuts  # noqa: E402


def main():
    rec, prefix = sys.argv[1], sys.argv[2]
    args = sys.argv[3:]
    mx = int(args[args.index('--max') + 1]) if '--max' in args else 6000
    out = args[args.index('--out') + 1] if '--out' in args else str(ROOT / 'work' / 'earth' / 'segs')
    only = set(args[args.index('--only') + 1].split(',')) if '--only' in args else None
    head, frames = load_doc(rec)
    ok = lambda k: not frames[k].get('depth') and frames[k].get('p') and frames[k]['p'][3] == 'autowalk'  # noqa: E731
    cuts = [c for c in area_cuts(frames) if ok(c[0])]
    for i, (k, ar, n) in enumerate(cuts):
        spec = f'area:{ar[0]}-{ar[1]}#{n}'
        if only and spec not in only:
            continue
        end = cuts[i + 1][0] if i + 1 < len(cuts) else len(frames) - 1
        nf = min(mx, end - k)
        name = f'{prefix}_{ar[0]}_{ar[1]}_{n}'
        env = dict(os.environ, SEGMENT_DIR=out)
        r = subprocess.run([sys.executable, str(ROOT / 'tools' / 'segments' / 'segment.py'), 'cut', '--from', rec,
                            '--at', spec, '--frames', str(nf), '--name', name], env=env, capture_output=True, text=True)
        print(spec, k, nf, (r.stdout or r.stderr).strip().split('\n')[-1])


if __name__ == '__main__':
    main()
