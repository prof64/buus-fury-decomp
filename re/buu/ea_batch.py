"""ea_batch.py [scenarios] [types] -- run ea_rec.record for every enemy type (default all 150) x scenarios.
Each run is a subprocess-free loop; results -> work/buu/eanim/<tt>_<scn>.json."""
import json
import sys
import time
import traceback
from common import *
from ea_rec import record, OUT

scns = sys.argv[1].split(',') if len(sys.argv) > 1 else ['far', 'near', 'punch', 'kill']
types = ([int(x, 0) for x in sys.argv[2].split(',')] if ',' in sys.argv[2] or '-' not in sys.argv[2] else list(range(int(sys.argv[2].split('-')[0]), int(sys.argv[2].split('-')[1]) + 1))) if len(sys.argv) > 2 else list(range(150))
frames = int(sys.argv[3]) if len(sys.argv) > 3 else 600
for t in types:
    for s in scns:
        t0 = time.time()
        try:
            res = record(t, s, frames)
        except Exception as ex:
            res = dict(type=t, scenario=s, error=traceback.format_exc())
        (OUT / ('%02x_%s.json' % (t, s))).write_text(json.dumps(res))
        print('%02x %s %.1fs objs %d ev %d %s' % (t, s, time.time() - t0, len(res.get('objs', {})),
                                                   len(res.get('events', [])), res.get('error', '')[-80:]), flush=True)
