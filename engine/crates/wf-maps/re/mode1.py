import json
from r import R, u32, hexw
hd = json.load(open('out/headers.json'))
for h, keys in hd.items():
    h = int(h, 16)
    if u32(h + 0x3C):
        print(hex(h), keys)
        hexw(h, 20)
        for bg in range(4):
            res = u32(h + 0x14 + 4 * bg)
            print(bg, hex(res))
            hexw(res, 10)
# max chunk index used
mx = 0
n = 0
while R.isptr(u32(0x08179764 + 4 * n)):
    n += 1
print('chunk table entries', n)
