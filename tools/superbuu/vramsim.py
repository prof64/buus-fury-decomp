"""vramsim.py: the game's OBJ tile allocator (0x0801124C alloc / 0x0801142A free, allocator 0x03001C4C) in Python,
checked against the emulator: every alloc / free call's effect on the allocator bytes.
usage: vramsim.py SEGMENT.json a b  (replays the segment, verifies frames a..b)"""
import sys
from pathlib import Path

A = 0x03001C4C
NE = 138
FF = 0xFF


class Alloc:
    def __init__(self):
        self.free_head = FF
        self.spare_head = FF
        self.e = [[0] * 8 for _ in range(NE)]  # used, start, size, hi, lo, fnext, fprev, (pad)
        self.idx = {}

    @staticmethod
    def read(emu):
        s = Alloc()
        s.free_head = emu.r8(A)
        s.spare_head = emu.r8(A + 1)
        for i in range(NE):
            b = A + 0x694 + 10 * i
            s.e[i] = [emu.r8(b), emu.r16(b + 2), emu.r16(b + 4), emu.r8(b + 6), emu.r8(b + 7), emu.r8(b + 8), emu.r8(b + 9), 0]
        return s

    @staticmethod
    def fresh():
        """the state an area load leaves (entry 0 = the whole 512-unit free block, entries 1..63 spare in order)"""
        s = Alloc()
        s.free_head = 0
        s.spare_head = 1
        s.e[0] = [0, 0, 512, FF, FF, FF, FF, 0]
        for i in range(1, NE):
            s.e[i] = [FF, 0xFFFF, 0xFFFF, i + 1 if i + 1 < NE else FF, i - 1 if i > 1 else FF, FF, FF, 0]
        return s

    def key(self):
        # a spare entry's start / size are stale (the area load writes 0xFFFF): not state
        return (self.free_head, self.spare_head, tuple((x[0], x[3], x[4]) if x[0] == FF else tuple(x[:7]) for x in self.e))

    # field indices
    U, S, Z, HI, LO, FN, FP = range(7)

    def unlink(self, i):
        e = self.e
        fn, fp = e[i][self.FN], e[i][self.FP]
        if fp == FF:
            self.free_head = fn
        else:
            e[fp][self.FN] = fn
        if fn != FF:
            e[fn][self.FP] = fp

    def push_free(self, i):
        e = self.e
        e[i][self.U] = 0
        e[i][self.FP] = FF
        e[i][self.FN] = self.free_head
        if self.free_head != FF:
            e[self.free_head][self.FP] = i
        self.free_head = i

    def spare(self, i):
        e = self.e
        e[i][self.U] = FF
        e[i][self.LO] = FF
        e[i][self.HI] = self.spare_head
        if self.spare_head != FF:
            e[self.spare_head][self.LO] = i
        self.spare_head = i

    def merge(self, i, j):
        e = self.e
        e[i][self.Z] = (e[i][self.Z] + e[j][self.Z]) & 0xFFFF
        e[i][self.HI] = e[j][self.HI]
        if e[j][self.HI] != FF:
            e[e[j][self.HI]][self.LO] = i
        e[i][self.FP] = e[j][self.FP]
        if e[j][self.FP] != FF:
            e[e[j][self.FP]][self.FN] = i
        else:
            self.free_head = i
        e[i][self.FN] = e[j][self.FN]
        if e[j][self.FN] != FF:
            e[e[j][self.FN]][self.FP] = i
        self.spare(j)

    def largest(self):
        m = 0
        i = self.free_head
        while i != FF:
            m = max(m, self.e[i][self.Z])
            i = self.e[i][self.FN]
        return m << 6

    def alloc(self, nbytes):
        e = self.e
        need = nbytes >> 6
        best, bsz = FF, 0x7FFFFFFF
        i = self.free_head
        while i != FF:
            s = e[i][self.Z]
            if s < bsz:
                if s == need:
                    e[i][self.U] = 1
                    self.unlink(i)
                    self.idx[e[i][self.S]] = i
                    return e[i][self.S] << 1
                if s > need:
                    best, bsz = i, s
            i = e[i][self.FN]
        if best == FF:
            return -1
        if self.spare_head == FF:
            e[best][self.U] = 1
            self.unlink(best)
            self.idx[e[best][self.S]] = best
            return e[best][self.S] << 1
        s = self.spare_head
        self.spare_head = e[s][self.HI]
        if self.spare_head != FF:
            e[self.spare_head][self.LO] = FF
        e[s][self.LO] = best
        e[s][self.HI] = e[best][self.HI]
        if e[best][self.HI] != FF:
            e[e[best][self.HI]][self.LO] = s
        e[best][self.HI] = s
        e[best][self.Z] -= need
        e[s][self.U] = 1
        e[s][self.S] = e[best][self.S] + e[best][self.Z]
        e[s][self.Z] = need
        self.idx[e[s][self.S]] = s
        return e[s][self.S] << 1

    def free(self, handle, emu=None):
        e = self.e
        i = self.idx.get(handle >> 1)
        if i is None and emu is not None:
            i = emu.r8(A + 0xC00 + (handle >> 1))
        e[i][self.U] = 0
        hi, lo = e[i][self.HI], e[i][self.LO]
        if hi == FF:
            if lo == FF:
                self.free_head = i
                e[i][self.FN] = FF
                e[i][self.FP] = FF
                return
            if e[lo][self.U] == 0:
                e[lo][self.Z] += e[i][self.Z]
                e[lo][self.HI] = FF
                self.spare(i)
            else:
                self.push_free(i)
            return
        H = e[hi]
        if lo == FF:
            if H[self.U] == 0:
                self.merge(i, hi)
            else:
                self.push_free(i)
            return
        L = e[lo]
        if H[self.U] != 0:
            if L[self.U] == 0:
                L[self.Z] += e[i][self.Z]
                L[self.HI] = e[i][self.HI]
                H[self.LO] = e[i][self.LO]
                self.spare(i)
            else:
                self.push_free(i)
            return
        if L[self.U] != 0:
            self.merge(i, hi)
            return
        self.unlink(hi)
        L[self.Z] = e[i][self.Z] + H[self.Z] + L[self.Z]
        L[self.HI] = H[self.HI]
        if H[self.HI] != FF:
            e[H[self.HI]][self.LO] = e[i][self.LO]
        self.spare(hi)
        self.spare(i)


if __name__ == '__main__':
    ROOT = Path(__file__).resolve().parent.parent.parent
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
    from route import start_emu
    from segment import load_doc
    head, frames = load_doc(sys.argv[1])
    a, b = int(sys.argv[2]), int(sys.argv[3])
    st = sys.argv[4] if len(sys.argv) > 4 else str(ROOT / 'work' / 'segments' / f"{head['name']}.ss")
    e = start_emu(st)
    for k in range(a):
        e.set_keys(frames[k]['keys'])
        e.run(1)
    e.breakpoint(0x0801124C)
    e.breakpoint(0x0801142A)
    sim = Alloc.read(e)
    ok = bad = 0
    for k in range(a, b):
        e.set_keys(frames[k]['keys'])
        e.run(1)
        hs = e.hits()
        for h in hs:
            r = h['regs']
            if r[15] & ~1 == 0x0801124C or r[15] - 2 == 0x0801124C:
                sim.alloc(r[1])
            else:
                sim.free(r[1], e)
        real = Alloc.read(e)
        if hs or real.key() != sim.key():
            if real.key() != sim.key():
                # an area load resets the allocator after the old objects' frees: fresh, then the frame's later allocs
                for cut in range(len(hs), -1, -1):
                    t = Alloc.fresh()
                    for h in hs[cut:]:
                        r = h['regs']
                        if r[15] - 2 == 0x0801124C:
                            t.alloc(r[1])
                        else:
                            t.free(r[1], e)
                    if t.key() == real.key():
                        sim = t
                        print('reset at', k, 'before call', cut, 'of', len(hs))
                        break
            if real.key() == sim.key():
                ok += 1
            else:
                bad += 1
                print('MISMATCH', k, [(hex(h['regs'][15]), hex(h['regs'][1])) for h in hs])
                for i in range(NE):
                    if real.key()[2][i] != sim.key()[2][i]:
                        print('  e', i, 'real', real.e[i][:7], 'sim', sim.e[i][:7])
                print('  heads real', real.free_head, real.spare_head, 'sim', sim.free_head, sim.spare_head)
                sim = real
                if bad > 5:
                    break
    print('frames with calls ok', ok, 'bad', bad)
