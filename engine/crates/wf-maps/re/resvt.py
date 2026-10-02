import json
from r import R, u32, hexw
hd = json.load(open('out/headers.json'))
seen = {}
for h, keys in hd.items():
    h = int(h, 16)
    for bg in range(4):
        res = u32(h + 0x14 + 4 * bg)
        vt = u32(res)
        if vt != 0x08008087 and vt not in seen:
            seen[vt] = (res, keys, bg, h)
for vt, (res, keys, bg, h) in seen.items():
    print(f'vt {vt:08x} res {res:08x} map {keys} bg{bg} hdr {h:08x}')
    hexw(res, 16)
