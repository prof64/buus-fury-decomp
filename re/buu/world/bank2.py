import sys, struct
ram = open('/tmp/ewram.bin', 'rb').read()
refs = [int(x, 16) for x in sys.argv[1:]]
def ok(s):
    return all(0x20 <= ord(c) < 0x7f for c in s)
for name, f in (('chars>>6', lambda v: 2 * ((v >> 6) & 0xFFFF)), ('bytes>>6', lambda v: (v >> 6) & 0xFFFF)):
    for base in range(0, len(ram) - 0x100, 2):
        good = 0
        for v in refs:
            o = base + f(v)
            n = v >> 22
            if o + 2 * n > len(ram):
                break
            s = ram[o:o + 2 * n].decode('utf-16-le', 'replace')
            if ok(s):
                good += 1
        if good == len(refs):
            print(name, hex(0x02000000 + base), [ram[base + f(v):base + f(v) + 2 * (v >> 22)].decode('utf-16-le') for v in refs])
