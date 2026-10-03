"""segrun.py DIR [DIR ...]: run the segment test over each cut directory (SEGMENT_DIR) and print one line per cut:
name matched/frames [first divergence]. Env passes through (SEGMENT_VRAM=0 ...). Exact cuts are only counted
unless --all."""
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
dirs = [a for a in sys.argv[1:] if not a.startswith('--')]
tot = ok = 0
for d in dirs:
    env = dict(os.environ, SEGMENT_DIR=str(Path(d).resolve()))
    r = subprocess.run(['cargo', 'test', '--release', '-p', 'wf-import', '--test', 'segments', '--', '--nocapture'],
                       cwd=ROOT / 'engine', env=env, capture_output=True, text=True)
    for line in r.stdout.splitlines():
        m = re.match(r'segment (\S+): (\d+) video frames \(\d+ logic\), matched (\d+) / \d+ \| first divergence (.*?) \| import', line)
        if not m:
            continue
        tot += 1
        if m.group(4) == 'None':
            ok += 1
            if '--all' not in sys.argv:
                continue
        print(Path(d).name, m.group(1), f'{m.group(3)}/{m.group(2)}', m.group(4)[:150])
print(f'{ok} / {tot} exact')
