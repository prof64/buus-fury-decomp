"""script_nat_b_stack.py [lo hi] -- static stack-effect analysis of script native handlers.

Walks each handler's CFG (thumb), tracking which registers hold the interp pointer and the abstract stack
pointer value (sp0 + k). Reports per path: pops (= -min k) and pushes (= final k - min k), BL targets,
vtables written (ldr =0x0804xxxx literals), and literal constants. Output: work/buu/script/nat_b_stack.json
"""
import json
import sys
import capstone
from common import *

NAT = 0x08055098
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.detail = False


def insn_at(a):
    code = rom()[ra(a):ra(a) + 4]
    for i in md.disasm(code, a):
        return i
    return None


def lit(i):
    if i.mnemonic == 'ldr' and 'pc' in i.op_str:
        off = int(i.op_str.split('#')[1].rstrip(']'), 16)
        return u32(((i.address + 4) & ~3) + off)
    return None


def regs(s):
    return [x.strip() for x in s.replace('[', '').replace(']', '').replace('{', '').replace('}', '').split(',')]


def analyze(entry, maxsteps=3000):
    results = []  # (min_k, final_k)
    bls, lits = set(), set()
    seen = set()
    # state: regs dict name -> ('I',) interp | ('K', k) sp value | None ; k current stored
    work = [(entry, {'r0': ('I',)}, 0, 0, 0)]
    steps = 0
    unknown = []
    while work and steps < maxsteps:
        a, R, k, mn, depth = work.pop()
        R = dict(R)
        while steps < maxsteps:
            steps += 1
            key = (a, tuple(sorted(R.items())), k, mn)
            if key in seen:
                break
            seen.add(key)
            i = insn_at(a)
            if i is None:
                break
            m, ops = i.mnemonic, i.op_str
            nxt = a + i.size
            L = lit(i)
            if L is not None:
                lits.add(L)
                R[regs(ops)[0]] = ('L', L)
                a = nxt
                continue
            rr = regs(ops)
            if m == 'ldr' and len(rr) >= 2 and R.get(rr[1]) == ('I',) and (len(rr) == 2 or rr[2] == '#0'):
                R[rr[0]] = ('K', k)
            elif m == 'str' and len(rr) >= 2 and R.get(rr[1]) == ('I',) and (len(rr) == 2 or rr[2] == '#0'):
                v = R.get(rr[0])
                if v and v[0] == 'K':
                    k = v[1]
                    mn = min(mn, k)
                else:
                    unknown.append(hex(a))
            elif m in ('adds', 'subs') and len(rr) == 3 and rr[2].startswith('#') and R.get(rr[1], (None,))[0] == 'K':
                d = int(rr[2][1:], 16) if rr[2].startswith('#0x') else int(rr[2][1:])
                R[rr[0]] = ('K', R[rr[1]][1] + (d if m == 'adds' else -d))
            elif m in ('adds', 'subs') and len(rr) == 2 and rr[1].startswith('#') and R.get(rr[0], (None,))[0] == 'K':
                d = int(rr[1][1:], 16) if rr[1].startswith('#0x') else int(rr[1][1:])
                R[rr[0]] = ('K', R[rr[0]][1] + (d if m == 'adds' else -d))
            elif m == 'adds' and len(rr) == 3 and rr[2] == '#0' and R.get(rr[1]) == ('I',):
                R[rr[0]] = ('I',)
            elif m == 'mov' and len(rr) == 2 and R.get(rr[1]) == ('I',):
                R[rr[0]] = ('I',)
            elif m == 'bl':
                t = int(ops.split('#')[1], 16)
                bls.add(t)
                if any(R.get(r) == ('I',) for r in ('r0',)) and t not in (0x803d5b8,):
                    pass
                for r in ('r0', 'r1', 'r2', 'r3'):
                    R.pop(r, None)
            elif m in ('pop',) and 'pc' in ops or (m == 'bx' and ops in ('lr', 'r3', 'r1', 'r2')):
                results.append((mn, k))
                break
            elif m == 'b':
                a = int(ops.split('#')[1], 16)
                continue
            elif m.startswith('b') and m not in ('bic', 'bics', 'bl', 'blx', 'bx') and '#' in ops:
                t = int(ops.split('#')[1], 16)
                work.append((t, dict(R), k, mn, depth + 1))
            else:
                if rr and rr[0] in R and m not in ('cmp', 'str', 'strb', 'strh', 'push', 'stm', 'tst', 'cmn'):
                    R.pop(rr[0], None)
            a = nxt
    paths = sorted(set((-mn, k - mn) for mn, k in results))
    return paths, sorted(bls), sorted(lits), unknown


if __name__ == '__main__':
    lo = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0x58
    hi = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0xa7
    out = {}
    for n in range(lo, hi + 1):
        h = u32(NAT + 4 * n) & ~1
        paths, bls, lits, unk = analyze(h)
        out[n] = {'handler': hex(h | 1), 'paths': paths, 'bl': [hex(b) for b in bls],
                  'lits': [hex(x) for x in lits], 'unknown_stores': unk}
        print(f'0x{n:02x} {h:08x} pops/pushes={paths} bl={[hex(b) for b in bls]} '
              f'vt={[hex(x) for x in lits if 0x08040000 <= x < 0x08060000 and x != 0x08054fbc]} {"UNK" if unk else ""}')
    (WORK / 'script' / 'nat_b_stack.json').write_text(json.dumps(out, indent=1))
