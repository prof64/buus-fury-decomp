"""set_gate.py SEGMENT.json N [SRC.json] -- set a gated segment's `expect.matched` to N (rewrites the first line in
place); with SRC, first take SRC's header (e.g. a copy that got `init.snd_phase` from snd_phase.py) - the frames
stay the file's own."""
import re
import sys
from pathlib import Path

dst, n = Path(sys.argv[1]), int(sys.argv[2])
s = dst.read_text()
nl = s.index('\n')
head = s[:nl]
if len(sys.argv) > 3:
    t = Path(sys.argv[3]).read_text()
    head = t[:t.index('\n')]
head = re.sub(r'"expect":\{"matched":\d+\},?', '', head)
head = head.replace('{', '{"expect":{"matched":%d},' % n, 1)
dst.write_text(head + s[nl:])
print(dst.name, 'gate', n, 'snd_phase' if '"snd_phase"' in head else '')
