"""script_trace_show.py name -- print a trace compactly: script runs/enters with depth indent, natives as
id(args...) -> pushed values, with frame and blocking duration."""
import json
import sys
from common import *

d = json.loads((WORK / 'script' / 'traces' / f'{sys.argv[1]}.json').read_text())
print(d['meta'], d['summary'])
for ev in d['events']:
    k = ev['ev']
    ind = '  ' * max(0, ev.get('depth', 1) - 1)
    if k == 'run':
        print(f"{ev['frame']:6d} RUN script #{ev['script']} hdr {ev['header']} lr {ev['lr']}")
    elif k == 'enter':
        print(f"{ev['frame']:6d} {ind}ENTER code {ev['code']} lr {ev['lr']} P {ev['P']} G {ev['G']}")
    elif k == 'scene':
        print(f"{ev['frame']:6d} SCENE #{ev['scene']} hdr {ev['header']} lr {ev['lr']} P {ev['P']}")
    elif k == 'step':
        what = f"exit({ev['exit']})" if 'exit' in ev else \
            f"kind {ev['kind']} arg {ev['arg']} next {ev['next']}" + (f" text {ev['text_ref']}" if 'text_ref' in ev else '')
        print(f"{ev['frame']:6d} STEP scene #{ev['scene']} state {ev['state']}: {what}")
    elif k == 'exit':
        print(f"{ev['frame']:6d} {ind}EXIT stack {ev['stack']} ops {ev['ops']}")
    else:
        b, a = ev.get('before') or [], ev.get('after')
        # common prefix -> args = popped part, pushed = new tail
        if a is None:
            res = '(pending)'
        else:
            i = 0
            while i < min(len(a), len(b)) and a[i] == b[i]:
                i += 1
            res = f'pops {b[i:]} pushes {a[i:]}'
        dur = (ev.get('frame_after', ev['frame']) - ev['frame'])
        blk = f'  BLOCK {dur}f' if dur else ''
        print(f"{ev['frame']:6d} {ind}{k} 0x{ev['id']:02x} @+{ev['off']:#x} stack {b} -> {res}{blk}")
