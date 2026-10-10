"""Python re-implementation of the Webfoot bytecode VM (enough for loot / item / equipment scripts).

Opcode semantics: see scriptdis.py. Variable natives (VERIFIED from handlers at 0x08000207..):
  native/setter 0x00..0x03 -> G0..G3   global words at 0x03001058 (shared by nested scripts!)
  native/setter 0x04..0x07 -> P0..P3   current parameter block ([0x03001030] + 0..0xC)
  native 0x19 rand(n) -> (rand16*n)>>16 in [0,n)          (0x08000727 -> 0x0803FACC)
  native 0x1a chance(pct) -> rand16 < pct*0x28F5D>>8       (0x08000739 -> 0x0803FADE)
  native 0x84 drop_zenie(size)   size 0..3 = 5/25/50/100 Z (coin defs 0x086FC250)
  native 0x85 drop_supply(id)    (supply item table 0x083BD2F8)
  native 0x86 drop_equipment(id) (equipment table 0x086F82EC)
  native 0x92 equipped(item) (accessory/any slot check)    -- caller supplies the set
  native 0xd7 run_script(idx, p0) / 0xd8 run_script(idx, p0, p1): writes P0(/P1) of the *current*
          parameter block then runs script idx (script table 0x0879B704).
"""
import random
from common import *
from scriptdis import script_bytes, varint


class VM:
    def __init__(self, equipped=(), rng=None):
        self.G = [0, 0, 0, 0]
        self.P = [0, 0, 0, 0]
        self.equipped = set(equipped)
        self.rng = rng or random.Random(1)
        self.drops = []
        self.cache = {}

    def rand16(self):
        return self.rng.getrandbits(16)

    def code(self, idx):
        if idx not in self.cache:
            self.cache[idx] = script_bytes(idx)[0]
        return self.cache[idx]

    def native(self, n, st):
        if n <= 3:
            st.append(self.G[n]); return
        if n <= 7:
            st.append(self.P[n - 4]); return
        if n == 0x19:
            v = st.pop(); st.append((self.rand16() * v) >> 16); return
        if n == 0x1a:
            v = st.pop(); st.append(1 if self.rand16() < ((v * 0x28F5D) >> 8) else 0); return
        if n == 0x84:
            self.drops.append(('zenie', [5, 25, 50, 100][st.pop()])); return
        if n == 0x85:
            self.drops.append(('supply', st.pop())); return
        if n == 0x86:
            self.drops.append(('equip', st.pop())); return
        if n == 0x92:
            st.append(1 if st.pop() in self.equipped else 0); return
        if n == 0xd7:
            p0 = st.pop(); idx = st.pop(); self.P[0] = p0; self.run(idx); return
        if n == 0xd8:
            p1 = st.pop(); p0 = st.pop(); idx = st.pop(); self.P[0] = p0; self.P[1] = p1; self.run(idx); return
        raise NotImplementedError(f'native {n:#x}')

    def setter(self, n, v):
        if n <= 3:
            self.G[n] = v
        elif n <= 7:
            self.P[n - 4] = v
        else:
            raise NotImplementedError(f'setter {n:#x}')

    def run(self, idx, b=None):
        b = self.code(idx) if b is None else b
        i = 0
        st = []
        loop = []
        while True:
            op = b[i]; i += 1
            if op == 0:
                st.append(b[i]); i += 1
            elif op == 1:
                v, i = varint(b, i); st.append(v)
            elif op == 2:
                n = b[i]; i += 1; self.native(n, st)
            elif op == 3:
                n = b[i]; i += 1; self.setter(n, st.pop())
            elif op in (4, 5):
                y, x = st.pop(), st.pop(); st.append(int(bool(x and y)) if op == 4 else int(bool(x or y)))
            elif op == 6:
                st.append(-st.pop())
            elif 7 <= op <= 0x10 or op == 0x1e:
                y, x = st.pop(), st.pop()
                if op == 7: r = x + y
                elif op == 8: r = x - y
                elif op == 9: r = x * y
                elif op == 0xa: r = int(x / y)
                elif op == 0x1e: r = x - int(x / y) * y
                elif op == 0xb: r = int(x == y)
                elif op == 0xc: r = int(x != y)
                elif op == 0xd: r = int(x > y)
                elif op == 0xe: r = int(x >= y)
                elif op == 0xf: r = int(x < y)
                else: r = int(x <= y)
                st.append(r)
            elif op == 0x11:
                return
            elif op == 0x12:
                off = (b[i] << 8) | b[i + 1]; off -= 0x10000 if off & 0x8000 else 0; i = i + off
            elif op == 0x13:
                off = (b[i] << 8) | b[i + 1]; off -= 0x10000 if off & 0x8000 else 0
                if st.pop() == 0:
                    i = i + off
                else:
                    i += 2
            elif op == 0x14:
                st.pop()
            elif op == 0x1b:
                loop.append(st.pop())
            elif op == 0x1c:
                off = (b[i] << 8) | b[i + 1]
                loop[-1] -= 1
                if loop[-1] != 0:
                    i = i - off
                else:
                    loop.pop(); i += 2
            elif op == 0x1d:
                n = b[i]; i += 1
                for _ in range(n):
                    v, i = varint(b, i); st.append(v)
            else:
                raise NotImplementedError(f'op {op:#x} at {i-1:#x}')
