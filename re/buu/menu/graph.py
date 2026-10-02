"""graph.py ROOT... [-x EXCL...] -- functions reachable from the roots (bl targets + vtables loaded as literals,
fn = vt + entry), excluding library / engine code ranges. Prints count and list with sizes."""
import sys
sys.path.insert(0, __file__.rsplit('/', 1)[0])
from d import func, NAMES
from common import u32

# shared library code we do not descend into
STOP = set(NAMES) | {0x08046AA2, 0x08046AA4, 0x08046AA6}
LIB = [(0x08046000, 0x08050000)]


def is_vt(v):
    if not (0x08049000 <= v < 0x0804F000) or v & 3:
        return False
    # every entry must resolve into code
    for i in range(4):
        f = (v + u32(v + 4 * i)) & 0xFFFFFFFF
        if not (0x08000000 <= f < 0x08060000):
            return False
    return True


def walk(roots, excl=(), vtn=24):
    seen = {}
    vts = {}
    todo = [r & ~1 for r in roots]
    while todo:
        a = todo.pop()
        if a in seen or a in excl or a in STOP or not (0x08000000 <= a < 0x08060000):
            continue
        if any(lo <= a < hi for lo, hi in LIB):
            continue
        lines = func(a, quiet=True)
        seen[a] = len(lines)
        for l in lines:
            if '; sub_' in l and ' bl ' in l:
                t = int(l.split('; sub_')[1][:8], 16)
                todo.append(t)
            if '; =0x' in l:
                v = int(l.split('; =0x')[1][:8], 16)
                if is_vt(v) and v not in vts:
                    fs = []
                    for i in range(vtn):
                        f = (v + u32(v + 4 * i)) & 0xFFFFFFFF
                        if not (0x08000000 <= f < 0x08060000):
                            break
                        fs.append(f & ~1)
                    vts[v] = fs
                    todo += fs
                if 0x08000000 <= v < 0x08046000 and (v & 1) and not is_vt(v):
                    todo.append(v & ~1)
    return seen, vts


if __name__ == '__main__':
    args = sys.argv[1:]
    excl = set()
    if '-x' in args:
        i = args.index('-x')
        excl = {int(x, 16) for x in args[i + 1:]}
        args = args[:i]
    seen, vts = walk([int(a, 16) for a in args], excl)
    print(len(seen), 'functions', sum(seen.values()), 'insns;', len(vts), 'vtables')
    print(' '.join(f'{v:08x}' for v in sorted(vts)))
