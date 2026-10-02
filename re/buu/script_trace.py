"""script_trace.py -- record every bytecode-VM event of the real game (ground truth for wf-script).

Library:   tr = Tracer(e); tr.run(frames, keyfn); tr.save(name, meta)
Hooks (single-stepping only the frames in which the VM runs; other frames run at full speed):
  RUNNER 0x08004098  r0 = interp (0 = allocate), r1 = script header, r2 = param block   -> 'run'
  ENTER  0x08004038  r0 = interp, r1 = code start, r2 = param block (0 = interp+0x5C)   -> 'enter'
  FETCH  0x08004068  r0 = opcode byte, r1 = address of the opcode, r4 = interp           -> per-op
  END    0x08004074  r4 = interp (opcode 0x11 reached: handler table entry is NULL)       -> 'exit'
Per op we record nothing but a counter, except ops that call natives / setters:
  0x02 native n, 0x03 setter n, 0x15..0x1A var ops (native n then setter n)
for which an event carries the VM stack before the op and after it (read at the next fetch by the same
interp, or at its exit), so a re-implementation fed the same native results can be compared call by call.
Nested scripts (native 0xD6..0xD8) appear between a native's 'before' and 'after'.
Blocking natives (that run the nested main loop 0x0803D8A0) show up as a frame gap between before/after.
"""
import json
import sys
from common import *
from emutools import cur_pc

RUNNER, ENTER, FETCH, END = 0x08004098, 0x08004038, 0x08004068, 0x08004074
SCRIPTS = 0x0879B704
DEPTH = 0x03001034
PARAMS = 0x03001030
GLOBALS = 0x03001058
SCENE_RUN, SCENE_STEP = 0x08012588, 0x08012538   # run_scene(header, params); tick: r1 = header[state], r4 = scene
HOOKS = (RUNNER, ENTER, FETCH, END, SCENE_RUN, SCENE_STEP)
NATIVE_OPS = {0x02, 0x03, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A}
OPNAME = {0x02: 'native', 0x03: 'set', 0x15: 'inc', 0x16: 'preinc', 0x17: 'postinc', 0x18: 'dec',
          0x19: 'predec', 0x1A: 'postdec'}

_hdr_index = None


def header_index():
    global _hdr_index
    if _hdr_index is None:
        _hdr_index = {}
        for i in range(522):
            _hdr_index.setdefault(u32(SCRIPTS + 4 * i), i)
    return _hdr_index


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


class Tracer:
    def __init__(self, e, max_events=200000):
        self.e = e
        self.events = []
        self.code = {}       # interp -> code base
        self.pending = {}    # interp -> event awaiting 'after'
        self.ops = {}        # interp -> op count since enter
        self.max_events = max_events
        self.vm_frames = 0

    # --- reads
    def stack(self, interp):
        e = self.e
        sp = e.r32(interp)
        if sp > 64:
            return None
        return [s32v(e.r32(interp + 4 + 4 * i)) for i in range(sp)]

    def loops(self, interp):
        e = self.e
        n = e.r32(interp + 0x44)
        if n > 16:
            return None
        return [s32v(e.r32(interp + 0x48 + 4 * i)) for i in range(n)]

    def gp(self):
        e = self.e
        p = e.r32(PARAMS)
        return ([s32v(e.r32(GLOBALS + 4 * i)) for i in range(4)],
                [s32v(e.r32(p + 4 * i)) for i in range(4)] if p else None)

    # --- hook handling
    def finish(self, interp):
        ev = self.pending.pop(interp, None)
        if ev is not None:
            ev['after'] = self.stack(interp)
            ev['frame_after'] = self.e.frame
            g, p = self.gp()
            ev['G_after'], ev['P_after'] = g, p

    def on(self, pc):
        e = self.e
        fr = e.frame
        if pc == RUNNER:
            hdr = e.reg(1)
            self.events.append({'ev': 'run', 'frame': fr, 'header': hex(hdr),
                                'script': header_index().get(hdr), 'lr': hex(e.reg(14)),
                                'interp': hex(e.reg(0)), 'params': hex(e.reg(2))})
        elif pc == ENTER:
            it, code, par = e.reg(0), e.reg(1), e.reg(2)
            self.code[it] = code
            self.ops[it] = 0
            self.pending.pop(it, None)
            g, p = self.gp()
            pblk = par if par else it + 0x5C
            self.events.append({'ev': 'enter', 'frame': fr, 'interp': hex(it), 'code': hex(code),
                                'params': hex(par), 'lr': hex(e.reg(14)), 'depth': e.r32(DEPTH) + 1,
                                'G': g, 'P': [s32v(e.r32(pblk + 4 * i)) for i in range(4)],
                                'code_head': e.read(code, 16).hex()})
        elif pc == FETCH:
            op, addr, it = e.reg(0), e.reg(1), e.reg(4)
            self.finish(it)
            self.ops[it] = self.ops.get(it, 0) + 1
            if op in NATIVE_OPS:
                n = e.r8(addr + 1)
                g, p = self.gp()
                ev = {'ev': OPNAME[op], 'frame': fr, 'interp': hex(it), 'off': addr - self.code.get(it, 0),
                      'id': n, 'depth': e.r32(DEPTH), 'before': self.stack(it), 'G': g, 'P': p}
                lp = self.loops(it)
                if lp:
                    ev['loops'] = lp
                self.events.append(ev)
                self.pending[it] = ev
        elif pc == SCENE_RUN:
            hdr = e.reg(0)
            self.events.append({'ev': 'scene', 'frame': fr, 'header': hex(hdr), 'scene': header_index().get(hdr),
                                'lr': hex(e.reg(14)), 'params': hex(e.reg(1)),
                                'P': [s32v(e.r32(e.reg(1) + 4 * i)) for i in range(4)] if e.reg(1) else None})
        elif pc == SCENE_STEP:
            sc, w = e.reg(4), e.reg(1)
            hdr = e.r32(sc)
            ev = {'ev': 'step', 'frame': fr, 'scene_obj': hex(sc), 'header': hex(hdr),
                  'scene': header_index().get(hdr), 'state': e.r16(sc + 8), 'entry': hex(w)}
            if w >= 0x10000:
                ev['kind'] = e.r8(w + 2)
                ev['next'] = e.r16(w)
                ev['arg'] = e.r8(w + 3)
                if ev['kind'] in (5, 6):
                    ev['text_ref'] = hex(e.r32(w + 4))
            else:
                ev['exit'] = w
            self.events.append(ev)
        elif pc == END:
            it = e.reg(4)
            self.finish(it)
            self.events.append({'ev': 'exit', 'frame': fr, 'interp': hex(it), 'stack': self.stack(it),
                                'ops': self.ops.get(it, 0), 'depth': e.r32(DEPTH)})

    def step_frame(self):
        e = self.e
        start = e.frame
        hooks = set(HOOKS)
        while e.frame == start:
            p = cur_pc(e)
            if p in hooks:
                self.on(p)
            e.step()

    def run(self, frames, keyfn=lambda f: 0, always_step=False, on_frame=None):
        """Advance `frames` frames. Frames that never reach a hook run at full speed (breakpoint probe)."""
        e = self.e
        for f in range(frames):
            e.set_keys(keyfn(f))
            if on_frame:
                on_frame(f)
            if always_step or self.pending:
                self.vm_frames += 1
                self.step_frame()
                continue
            st = e.save()
            bids = [e.breakpoint(a) for a in HOOKS]
            e.run(1)
            hs = e.hits()
            for b in bids:
                e.clear(b)
            if hs:
                e.load(st)
                e.set_keys(keyfn(f))
                self.vm_frames += 1
                self.step_frame()
            if len(self.events) > self.max_events:
                break

    def summary(self):
        from collections import Counter
        c = Counter(ev['ev'] for ev in self.events)
        scripts = [ev.get('script') for ev in self.events if ev['ev'] == 'run']
        return {'counts': dict(c), 'scripts_run': scripts, 'vm_frames': self.vm_frames}

    def save(self, name, meta=None):
        out = WORK / 'script' / 'traces'
        out.mkdir(parents=True, exist_ok=True)
        doc = {'name': name, 'meta': meta or {}, 'summary': self.summary(), 'events': self.events}
        p = out / f'{name}.json'
        p.write_text(json.dumps(doc, indent=0))
        return p


def traced_call(tr, func, r=(0, 0, 0, 0), keyfn=lambda f: 0, max_frames=6000):
    """emutools.call, but the injected frame (and, if the callee blocks in a nested main loop, every following
    frame until it returns) is traced. The stub / hook stay installed until the mailbox reports the return.
    Returns (r0, frames)."""
    from emutools import STUB, ARGS, HOOK_SITE, HOOK_TARGET, MAILBOX, _bl
    e = tr.e
    blob = (ROOT / 'work' / 'buu' / 'stub.bin').read_bytes()
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, blob[i] | (blob[i + 1] << 8))
    vals = list(r) + [0, 0, 0, 0] + [func | 1, HOOK_TARGET | 1]
    for i, v in enumerate(vals):
        e.w32(ARGS + 4 * i, v)
    hi, lo = _bl(HOOK_SITE, STUB)
    o1, o2 = e.r16(HOOK_SITE), e.r16(HOOK_SITE + 2)
    e.w32(MAILBOX, 0xDEADBEEF)
    e.w16(HOOK_SITE, hi); e.w16(HOOK_SITE + 2, lo)
    n = 0
    restored = False
    while n < max_frames:
        e.set_keys(keyfn(n))
        tr.step_frame()
        tr.vm_frames += 1
        n += 1
        if not restored:
            # the hook fired once the frame ran; restore the call site so nested main loops do not re-enter
            e.w16(HOOK_SITE, o1); e.w16(HOOK_SITE + 2, o2)
            restored = True
        if e.r32(MAILBOX) != 0xDEADBEEF:
            break
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, u16(STUB + i))
    return e.r32(MAILBOX), n


if __name__ == '__main__':
    # script_trace.py state frames [name] -- trace a save state with no input
    st = sys.argv[1]
    frames = int(sys.argv[2])
    name = sys.argv[3] if len(sys.argv) > 3 else 'adhoc'
    e = emu(st)
    tr = Tracer(e)
    tr.run(frames)
    print(tr.save(name, {'state': st, 'how': 'natural, no input'}), tr.summary())
