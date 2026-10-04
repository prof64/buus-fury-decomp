"""tileram.py SEG.json...: from a cut's init RAM: the main-loop mode object (0x03000F90) vtable + words, the player
pointer 0x03001C34, scene depth 0x03002A5C, the player-class objects (vt 0x0804C9F0) and their state (+0x14)"""
import base64
import json
import struct
import sys
import zlib


def blob(v):
    if isinstance(v, list):
        return bytes(v)
    b = base64.b64decode(v)
    try:
        return zlib.decompress(b)
    except zlib.error:
        return b


for p in sys.argv[1:]:
    doc = json.loads(open(p).read())
    r = doc['init']['ram']
    iw, ew = blob(r['iwram']), blob(r['ewram'])

    def r32(a):
        if a >> 24 == 3:
            o = a & 0x7FFF
            return struct.unpack_from('<I', iw, o)[0] if o + 4 <= len(iw) else 0
        if a >> 24 == 2:
            o = a & 0x3FFFF
            return struct.unpack_from('<I', ew, o)[0] if o + 4 <= len(ew) else 0
        return 0
    m = r32(0x03000F90)
    objs = [r32(0x03001C4C + 8 + 4 * k) for k in range(min(r32(0x03001C4C + 4), 64))]
    pl = [o for o in objs if r32(o) == 0x0804C9F0]
    print(p.split('/')[-1], 'mode', hex(m), hex(r32(m)), [hex(r32(m + 4 * i)) for i in range(1, 8)], 'PLAYER', hex(r32(0x03001C34)), 'depth', r32(0x03002A5C),
          'players', [(hex(o), hex(r32(o + 0x14)), hex(r32(r32(o + 0x14)))) for o in pl])
