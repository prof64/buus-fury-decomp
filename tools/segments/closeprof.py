"""closeprof.py SEG.json STATE.ss FRAME [DEPTH] [OUT.json]  -- call-tree profile of a long main-loop iteration (a menu
close's world restore, an area load) on the real game, instruction-stepped.

FRAME = a segment row inside the iteration's black / lag frames (the logic counter 0x03000F94 constant). The run of
rows with that counter value is found; the emulator replays the keys up to the row where that counter value began
(the logic frame whose iteration this is) and steps every instruction until the counter changes again. A shadow call
stack follows BL calls (lr = the call's return address) and their returns (pc back at a frame's return address);
user-mode cycles are added inclusively to every function on the stack, interrupt-mode cycles are counted apart. Prints
the tree to DEPTH (default 4) for functions over 3,000 cycles; OUT.json gets the whole tree."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from route import start_emu  # noqa: E402

FC = 0x03000F94


def rows_of(path):
    out = []
    for ln in open(path).read().splitlines()[1:]:
        try:
            out.append(json.loads(ln.strip().rstrip(',')))
        except ValueError:
            pass
    return out


def is_veneer(a):
    # the call stubs: `bx r0` .. `bx r7` at 0x08046AA0, the Thumb -> ARM long-call stubs at 0x08049100..
    return 0x08046AA0 <= a < 0x08046AB0 or 0x08049100 <= a < 0x08049200


def profile(seg, state, frame):
    rows = rows_of(seg)
    lf = rows[frame]['lf']
    s = frame
    while s > 0 and rows[s - 1]['lf'] == lf:
        s -= 1
    e = start_emu(str(Path(state).resolve()))
    for j in range(s):
        e.set_keys(rows[j]['keys'])
        e.run(1)
    # the iteration: from where the counter takes this value (inside row s's video frame) to where it changes
    e.set_keys(rows[s]['keys'])
    f0 = e.frame
    row = s
    while e.r32(FC) != lf:
        e.step()
        if e.frame != f0:
            row += 1
            f0 = e.frame
            e.set_keys(rows[row]['keys'])
    root = {'f': 'root', 'c': 0, 'n': 1, 'k': {}}
    stack = [(root, None)]
    irq = 0
    c0 = last = e.cycles()
    # (r15 reads the prefetch address: the executing instruction + 4 in Thumb, + 8 in ARM)
    real = lambda: e.reg(15) - (4 if e.reg(16) & 0x20 else 8)
    pc, lr0 = real(), e.reg(14)
    while e.r32(FC) == lf:
        mode = e.reg(16) & 0x1F
        e.step()
        c = e.cycles()
        d = c - last
        last = c
        nmode = e.reg(16) & 0x1F
        if mode not in (0x10, 0x1F) or nmode not in (0x10, 0x1F):
            irq += d if mode not in (0x10, 0x1F) else 0
            if nmode in (0x10, 0x1F):
                pc, lr0 = real(), e.reg(14)
            continue
        for fr, _ in stack:
            fr['c'] += d
        npc, lr = real(), e.reg(14)
        seq = npc in (pc + 2, pc + 4)
        if not seq:
            top_ret = stack[-1][1]
            tgt = npc + 2 if e.reg(16) & 0x20 else npc + 4
            # (right after a branch r15 is the target + 2, not + 4: the address read is up to 4 short)
            if top_ret is not None and 0 <= (top_ret & ~1) - npc <= 4:
                stack.pop()
            elif lr != lr0 and 0 <= (lr & ~1) - pc <= 6:
                key = 'veneer' if is_veneer(tgt) else hex(tgt)
                parent = stack[-1][0]
                ch = parent['k'].get(key)
                if ch is None:
                    ch = parent['k'][key] = {'f': key, 'c': 0, 'n': 0, 'k': {}}
                ch['n'] += 1
                stack.append((ch, lr))
            elif stack[-1][0]['f'] == 'veneer' and not is_veneer(tgt):
                # a call stub (`bx rN`, or Thumb -> ARM `bx pc` + a long jump) reached its target: the frame is the
                # target's (its cycles so far stay with the parent)
                v, ret = stack.pop()
                parent = stack[-1][0]
                v['n'] -= 1
                if v['n'] == 0 and not v['k']:
                    parent['k'].pop('veneer', None)
                key = hex(tgt)
                ch = parent['k'].get(key)
                if ch is None:
                    ch = parent['k'][key] = {'f': key, 'c': 0, 'n': 0, 'k': {}}
                ch['n'] += 1
                stack.append((ch, ret))
        pc, lr0 = npc, lr
        if e.frame != f0:
            row += 1
            f0 = e.frame
            e.set_keys(rows[min(row, len(rows) - 1)]['keys'])
    return {'seg': str(seg), 'frame': frame, 'start_row': s, 'end_row': row, 'lf': lf, 'user': root['c'], 'irq': irq,
            'total': last - c0, 'tree': root}


def show(node, depth, ind=0, cut=3000):
    for k, ch in sorted(node['k'].items(), key=lambda x: -x[1]['c']):
        if ch['c'] < cut:
            continue
        print(f"{'  ' * ind}{k} x{ch['n']}: {ch['c']:,}")
        if ind + 1 < depth:
            show(ch, depth, ind + 1, cut)


if __name__ == '__main__':
    a = sys.argv[1:]
    r = profile(a[0], a[1], int(a[2]))
    print(f"rows {r['start_row']}..{r['end_row']} lf {r['lf']}: total {r['total']:,} user {r['user']:,} irq {r['irq']:,}")
    show(r['tree'], int(a[3]) if len(a) > 3 else 4)
    if len(a) > 4:
        Path(a[4]).write_text(json.dumps(r))
