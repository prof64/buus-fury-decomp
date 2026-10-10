"""print an area's exits, triggers and NPCs (content objects.json): python3 area_info.py W A [variant]"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from grid import Grid, map_of  # noqa: E402


def main():
    w, a = int(sys.argv[1]), int(sys.argv[2])
    v = int(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3][0] in '0123456789' else 1
    g = Grid(map_of(w, a, v))
    o = g.objects
    print('map', o['map'], 'start', o.get('start_pos'))
    for t in o.get('map_triggers', []):
        print('  map', t['type'], t.get('rect'), t.get('to'), t.get('push'), t.get('condition'))
    for ar in o.get('areas', []):
        print(' area', ar['key'], ar.get('variant'))
        for t in ar['objects']:
            print('   ', t['type'], t.get('rect') or t.get('pos'), t.get('event') or t.get('to') or t.get('talk'),
                  t.get('condition'), t.get('sprite_index', ''), (t.get('activation') or {}).get('mode', ''))
    if '--grid' in sys.argv:
        g.show(0, 0, min(g.w, 130), min(g.h, 70))


if __name__ == '__main__':
    main()
