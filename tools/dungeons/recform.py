"""recform.py SEG.json STATE.ss F...: the characters' record form bytes (0x03001560 + 0x68 c + 0x14) and the
current character (GD 0x03001068 +0) at the given frames of a segment"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

seg, st = sys.argv[1], sys.argv[2]
want = sorted(int(x) for x in sys.argv[3:])
lines = Path(seg).read_text().split('\n')
keys = [json.loads(x.rstrip(','))['keys'] for x in lines[1:] if x.startswith('{')]
e = start_emu(str(Path(st).resolve()))
for f in range(want[-1] + 1):
    e.set_keys(keys[f])
    e.run(1)
    if f in want:
        print(f, 'current', e.r8(0x03001068), 'forms', [e.r8(0x03001560 + 0x68 * c + 0x14) for c in range(9)])
