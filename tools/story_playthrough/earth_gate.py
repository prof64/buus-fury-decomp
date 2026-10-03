"""Copy exact Earth segments (work/earth/segs/e_*.json) into data/segments/earth_*.json with an `expect` gate (all
their video frames must match).

usage: python3 tools/story_playthrough/earth_gate.py e_4_20_2 [e_3_3_1 ...]"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent


def main():
    for n in sys.argv[1:]:
        src = (ROOT / 'work' / 'earth' / 'segs' / f'{n}.json').read_text()
        head, rest = src.split('\n', 1)
        assert head.endswith(',"frames":[')
        h = json.loads(head[:-len(',"frames":[')] + '}')
        nframes = sum(1 for ln in rest.split('\n') if ln.startswith('{'))
        new = 'earth' + n[1:]
        h['name'] = new
        h['expect'] = {'matched': nframes}
        out = json.dumps(h, separators=(',', ':'))[:-1] + ',"frames":[\n' + rest
        (ROOT / 'data' / 'segments' / f'{new}.json').write_text(out)
        print(new, nframes)


if __name__ == '__main__':
    main()
