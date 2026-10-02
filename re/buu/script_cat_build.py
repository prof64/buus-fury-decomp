"""script_cat_build.py -- build engine/annotations/script_catalog.json: every bytecode script in the ROM
(event-table events and their bytecode/text nodes, plus inline raw-bytecode scripts), where each is
referenced, and a purpose guess.

Formats (see SCRIPT.md / the catalog's "formats" section):
  * Event table 0x0879B704 (522 entries) -> event header = array of u32 entries. entry < 0x10000 = terminal
    (finish, result value); else pointer to a node {u16 next, u8 kind, u8 arg, payload}. Node kinds
    (handler table 0x080728AC): 0 bytecode raw @+3, 4 bytecode LZ @+4, 1 text UTF-16 inline @+4,
    2 text @[+4] + bytecode args @+8, 3 choice prompt inline UTF-16 @+4, 5 packed bank text @+4,
    6 packed bank text @+4 + bytecode args @+8, 7 goto event header @+4.
  * Inline raw bytecode: code pointer straight into ROM (map triggers, conditions, callbacks, items...).
"""
import json
import struct
import sys
from collections import defaultdict, Counter
from common import *
from strtab import S
from script_cat_lz import decompress_stream

sys.path.insert(0, str(ROOT / 'tools' / 'extract'))
import gba  # noqa: E402

R = rom()
EVT_TABLE = 0x0879B704
NEV = 522
HDRS = [u32(EVT_TABLE + 4 * i) for i in range(NEV)]
HDR = {}
for i, h in enumerate(HDRS):
    HDR.setdefault(h, i)
RAW_CB, EVT_CB = 0x0800421F, 0x0800424F
AREAS, NAREA = 0x0808E2E0, 0x1C4
TEXT_BANKS = 0x087B5B64

RUN_SCRIPT = {0xd6: 1, 0xd7: 2, 0xd8: 3}            # run_script(idx[,p0[,p1]]) (nested, synchronous)
RUN_EVENT = {0xb5: 1, 0xb6: 2, 0xb7: 3, 0xb8: 4, 0xb9: 5}  # run_event(idx[,p0..p3]) (blocking event)


def isrom(v):
    return 0x08000000 <= v < 0x08800000


def varint(b, i):
    v = 0
    while True:
        c = b[i]
        i += 1
        v = (v << 7) | (c & 0x7f)
        if not c & 0x80:
            break
    return (-(v >> 1) if v & 1 else v >> 1), i


def decode(b, base=0):
    """Control-flow decode of bytecode `b`. Returns dict(size, natives, ends, rets, run_scripts, run_events,
    ok). Offsets of jumps: 0x12/0x13 operand_addr + u16 (unsigned); 0x1c operand_addr - u16."""
    seen = set()
    work = [0]
    maxend = 0
    natives = Counter()
    rets = set()
    dyn_ret = False
    runs, evs = set(), set()
    ok = True
    while work:
        pc = work.pop()
        recent = []
        prev_push = None
        while True:
            if pc in seen:
                break
            if pc < 0 or pc >= len(b):
                ok = False
                break
            seen.add(pc)
            op = b[pc]
            i = pc + 1
            try:
                if op == 0:
                    recent.append(b[i]); i += 1
                elif op == 1:
                    v, i = varint(b, i); recent.append(v)
                elif op == 0x1d:
                    n = b[i]; i += 1
                    for _ in range(n):
                        v, i = varint(b, i); recent.append(v)
                elif op == 2:
                    n = b[i]; i += 1
                    natives[n] += 1
                    if n in RUN_SCRIPT and len(recent) >= RUN_SCRIPT[n] and recent[-RUN_SCRIPT[n]] is not None:
                        runs.add(recent[-RUN_SCRIPT[n]])
                    if n in RUN_EVENT and len(recent) >= RUN_EVENT[n] and recent[-RUN_EVENT[n]] is not None:
                        evs.add(recent[-RUN_EVENT[n]])
                    if n <= 0x18:          # getters: pop 0, push 1 (value unknown)
                        recent.append(None)
                    elif n in (0x19, 0x1a, 0x1b, 0x1c) and recent:   # pop 1, push 1
                        recent[-1] = None
                    else:
                        recent = []
                elif op == 3 or 0x15 <= op <= 0x1a:
                    i += 1; recent = []
                elif op in (0x12, 0x13, 0x1c):
                    off = (b[i] << 8) | b[i + 1]
                    if op == 0x1c:
                        tgt = i - off
                    else:
                        tgt = i + off
                    i += 2
                    work.append(tgt)
                    recent = []
                    if op == 0x12:
                        maxend = max(maxend, i)
                        break
                elif op == 0x11:
                    maxend = max(maxend, i)
                    if recent and recent[-1] is not None:
                        rets.add(recent[-1])
                    else:
                        dyn_ret = True
                    break
                elif op in (4, 5, 7, 8, 9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x10, 0x1e):
                    if len(recent) >= 2:
                        recent = recent[:-2] + [None]
                    else:
                        recent = []
                elif op == 6:
                    if recent:
                        recent[-1] = -recent[-1] if recent[-1] is not None else None
                elif op in (0x14, 0x1b):
                    if recent:
                        recent.pop()
                else:
                    ok = False
                    break
            except IndexError:
                ok = False
                break
            maxend = max(maxend, i)
            pc = i
    return dict(size=maxend, natives=natives, rets=rets, dyn_ret=dyn_ret, runs=runs, evs=evs, ok=ok)


def ustr(a, n=200):
    out = []
    for i in range(n):
        c = u16(a + 2 * i)
        if c == 0:
            break
        out.append(chr(c) if 32 <= c < 127 else ('\n' if c == 10 else '{%04x}' % c))
    return ''.join(out)


_banks = {}


def bank_text(v):
    bank = v & 0x3f
    off = ((v >> 6) & 0x7fff) * 2
    ln = (v >> 21) * 2
    if bank not in _banks:
        p = u32(TEXT_BANKS + 4 * bank)
        _banks[bank] = gba.wf_decompress(R, ra(p))[0]
    s = _banks[bank][off:off + ln]
    return s.decode('utf-16-le', 'replace')


def code_unit(addr, compressed):
    if compressed:
        data, _ = decompress_stream(R, ra(addr))
        return data, 0
    return R[ra(addr):ra(addr) + 0x4000], addr


# ---------------------------------------------------------------- events

def walk_event(ev):
    h = HDRS[ev]
    nodes = {}
    terms = {}
    goto = []
    seen = set()
    work = [0]
    dyn = False
    while work:
        k = work.pop()
        if k in seen or k > 2000:
            continue
        seen.add(k)
        v = u32(h + 4 * k)
        if v < 0x10000:
            terms[k] = v
            continue
        if not isrom(v):
            terms[k] = f'bad:{v:08x}'
            continue
        nxt, kind, arg = u16(v), u8(v + 2), u8(v + 3)
        nd = dict(index=k, addr=f'0x{v:08X}', kind=kind, next=nxt, arg=arg)
        succ = [nxt]
        code = None
        if kind in (0, 4):
            code_addr = v + 3 if kind == 0 else v + 4
            data, base = code_unit(code_addr, kind == 4)
            d = decode(data)
            nd.update(code_addr=f'0x{code_addr:08X}', compressed=kind == 4, size=d['size'],
                      natives=sorted(d['natives']), returns=sorted(x for x in d['rets']), dynamic_return=d['dyn_ret'],
                      decode_ok=d['ok'], runs_script=sorted(d['runs']), runs_event=sorted(d['evs']))
            succ += [x for x in d['rets'] if 0 <= x < 2000]
            dyn = dyn or d['dyn_ret']
            code = d
        elif kind in (2, 6):
            code_addr = v + 8
            d = decode(R[ra(code_addr):ra(code_addr) + 0x1000])
            nd.update(code_addr=f'0x{code_addr:08X}', compressed=False, size=d['size'], natives=sorted(d['natives']),
                      text=(ustr(u32(v + 4)) if kind == 2 else bank_text(u32(v + 4))))
            code = d
        elif kind in (1, 3):
            nd['text'] = ustr(v + 4)
        elif kind == 5:
            nd['text'] = bank_text(u32(v + 4))
        elif kind == 7:
            tgt = u32(v + 4)
            nd['goto_event'] = HDR.get(tgt, f'0x{tgt:08X}')
            goto.append(nd['goto_event'])
        else:
            nd['bad_kind'] = True
        nodes[k] = (nd, code)
        for s in succ:
            work.append(s)
    return nodes, terms, goto, dyn


# ---------------------------------------------------------------- references

refs = defaultdict(list)          # key: ('event', idx) | ('raw', addr) -> list of ref dicts
entry_points = []


def ref_event(idx_or_hdr, kind, where, detail=''):
    idx = idx_or_hdr if isinstance(idx_or_hdr, int) and idx_or_hdr < NEV else HDR.get(idx_or_hdr)
    if idx is None:
        return
    refs[('event', idx)].append(dict(kind=kind, where=where, detail=detail))


def ref_raw(addr, kind, where, detail=''):
    refs[('raw', addr)].append(dict(kind=kind, where=where, detail=detail))


def ref_cb(p, kind, where, detail=''):
    """p points at a callback pair {handler, arg}."""
    w = u32(p)
    if w == RAW_CB:
        ref_raw(u32(p + 4), kind, where, detail + f' (callback 0x0800421F @0x{p:08X})')
        return True
    if w == EVT_CB:
        ref_event(u32(p + 4), kind, where, detail + f' (callback 0x0800424F @0x{p:08X})')
        return True
    return False


def ref_cond(c, kind, where):
    """Condition word: 0 always, 0x0001xxxx flag set, 0x0002xxxx flag clear, 0x0003xxxx test-and-clear,
    ROM pointer = raw bytecode returning bool (evaluator 0x08004784)."""
    if isrom(c):
        ref_raw(c, kind, where, 'condition (bool)')


def ref_vos(v, kind, where):
    """value-or-script word (helpers 0x080089A0/0x080089D8/0x08008A36): < 0x10000 value, else raw bytecode
    whose return value is used."""
    if isrom(v):
        ref_raw(v, kind, where, 'value-or-script (returns value)')


# areas
LIST0_CTORS = Counter()
for ai in range(NAREA):
    a = AREAS + 0x38 * ai
    world, mp = u8(a), u8(a + 1)
    where = f'area {ai} (world {world} map {mp}) @0x{a:08X}'
    ref_vos(u32(a + 0xC), 'area_name', where)
    ref_vos(u32(a + 0x24), 'area_music', where)
    ref_vos(u32(a + 0x28), 'area_variant', where)
    for off, kind in ((0x30, 'area_on_enter'), (0x34, 'area_on_leave')):
        v = u32(a + off)
        if isrom(v):
            ref_raw(v, kind, where, f'area +0x{off:X}')
    # list0: {cond, rec} pairs -> map objects
    n0, l0 = u8(a + 3), u32(a + 0x10)
    if isrom(l0):
        for k in range(n0):
            cond, rec = u32(l0 + 8 * k), u32(l0 + 8 * k + 4)
            w = f'{where} list0[{k}] rec 0x{rec:08X}'
            ref_cond(cond, 'map_object_condition', w)
            if not isrom(rec):
                continue
            ctor = u32(rec)
            LIST0_CTORS[ctor] += 1
            if ctor == 0x08007387:
                if isrom(u32(rec + 0x10)):
                    ref_raw(u32(rec + 0x10), 'map_script_trigger', w, 'ctor 0x08007387 rec+0x10 (run directly on trigger)')
            elif ctor == 0x0800742D:
                ref_event(u32(rec + 0x10), 'map_event_trigger', w, 'ctor 0x0800742D rec+0x10 (blocking event on trigger)')
            elif ctor == 0x08007499:
                ref_event(11, 'map_fixed_event_trigger', w, 'ctor 0x08007499 -> event #11 (0x0877CE00)')
            else:
                for o in range(4, 0x20, 4):
                    x = u32(rec + o)
                    if isrom(x) and not (x & 3) and ref_cb(x, 'map_object_callback', w, f'ctor 0x{ctor:08X} rec+0x{o:X}'):
                        pass
                    elif x in HDR:
                        ref_event(x, 'map_object_event', w, f'ctor 0x{ctor:08X} rec+0x{o:X}')
    # list1: actor records
    n1, l1 = u8(a + 4), u32(a + 0x14)
    if isrom(l1):
        for k in range(n1):
            rec = u32(l1 + 4 * k)
            if not isrom(rec):
                continue
            ctor = u32(rec)
            w = f'{where} list1[{k}] rec 0x{rec:08X}'
            ref_cond(u32(rec + 4), 'actor_spawn_condition', w)
            if ctor == 0x0800FD55:
                p = u32(rec + 0x14)
                if isrom(p) and not p & 3:
                    ref_cb(p, 'enemy_defeat_callback', w, f'enemy type 0x{u32(rec + 8):X} rec+0x14')
            elif ctor == 0x08017DA3:
                ref_event(u32(rec + 0xC), 'npc_talk', w, f'npc actor {u32(rec + 0x10)} rec+0xC')
    # list2 / list3: cond-first records
    for li, cnt_off, loff in ((2, 5, 0x18), (3, 6, 0x1C)):
        n, l = u8(a + cnt_off), u32(a + loff)
        if not isrom(l):
            continue
        for k in range(n):
            rec = u32(l + 4 * k)
            if not isrom(rec):
                continue
            w = f'{where} list{li}[{k}] rec 0x{rec:08X}'
            ref_cond(u32(rec), f'list{li}_condition', w)
            for o in range(4, 0x1C, 4):
                x = u32(rec + o)
                if x in (RAW_CB, EVT_CB):
                    ref_cb(rec + o, f'list{li}_callback', w, f'inline pair rec+0x{o:X}')
                elif isrom(x) and not (x & 3) and ref_cb(x, f'list{li}_callback', w, f'rec+0x{o:X}'):
                    pass
                elif x in HDR:
                    ref_event(x, f'list{li}_event', w, f'rec+0x{o:X}')
    # list4: 8-byte entries (spawned by 0x0800C300)
    n4, l4 = u8(a + 7), u32(a + 0x20)
    if isrom(l4):
        for k in range(n4):
            e = l4 + 8 * k
            for o in (0, 4):
                x = u32(e + o)
                if isrom(x) and not (x & 3):
                    ref_cb(x, 'list4_callback', f'{where} list4[{k}] @0x{e:08X}', f'+{o}')

# callback pairs anywhere in ROM data (catches the ones not reached above)
seen_cb = set()
for d in refs.values():
    for r in d:
        if '(callback' in r['detail']:
            seen_cb.add(r['detail'].split('@')[-1].rstrip(')'))
for h in (RAW_CB, EVT_CB):
    pat = struct.pack('<I', h)
    i = R.find(pat)
    while i >= 0:
        if i % 4 == 0:
            p = 0x08000000 + i
            if f'0x{p:08X}' not in seen_cb:
                ref_cb(p, 'callback_pair_other', f'pair @0x{p:08X}', 'callback pair in ROM data, holder not identified')
        i = R.find(pat, i + 1)

# items / equipment
for k in range(83):
    rec = 0x083BD2F8 + 0x34 * k
    v = u32(rec + 0x30)
    if isrom(v):
        ref_raw(v, 'item_use', f'supply {k} @0x{rec:08X}', f'{S(u16(rec + 4)) if False else ""}supply +0x30 use script'.strip())
for k in range(162):
    rec = 0x086F82EC + 0x2C * k
    v = u32(rec + 0x28)
    if isrom(v):
        ref_raw(v, 'equipment_effect', f'equipment {k} @0x{rec:08X}', 'equipment +0x28 effect script (run on stat recalc)')

# quest log table (40 x {u8, u8, u16 string, cond_hidden?, cond_done, cond_active})
for k in range(40):
    e = 0x08720564 + 16 * k
    sid = u16(e + 2)
    for o, role in ((4, 'skip'), (8, 'list_a'), (12, 'list_b')):
        ref_raw(u32(e + o), 'quest_log_condition', f'quest {k} "{S(sid)}" @0x{e:08X}', f'+0x{o:X} ({role}) bool')

# AI behaviour-tree nodes
ref_raw(0x0870EA28, 'ai_inline_script', 'AI node InlineScript def 0x0870EA1C (Vegeta 0x88)', 'def+0xC inline code')
for d in (0x08702904, 0x0870C2F0, 0x0870C400, 0x0870C528):
    ref_event(u32(d + 0xC), 'ai_run_script', f'AI node RunScript def 0x{d:08X}', 'blocking event, params = actor+0x214')

# fixed engine references (literal pool words in the code region)
ENGINE_HOOKS = {
    0: 'player death / revive (0x080105F4 via run_event)',
    1: 'link-cable disconnect message (0x0801E9D8 run_event; text "You have been disconnected.")',
    2: 'combat hook per melee hit (0x0800FC5E runner; P0/P1 = attacker bytes, P2 = damage, P3 = ...)',
    3: 'level-up (0x08013A6E run_event; "%s has reached level %d!", skill unlocks; result read back)',
    4: 'new game prologue (0x08005C64 run_event; "Previously in DRAGON BALL Z: LEGACY OF GOKU 2...")',
    6: 'save-file chapter label (0x08004C00 runner; return value = string id)',
    8: 'periodic world timer hook (0x080042F4)', 9: 'timer hook (0x08004284)', 10: 'timer hook (0x080042A6)',
    11: 'map object ctor 0x08007499 (fixed event)', 13: 'engine hook 0x0801A1B8 (runner)',
    14: 'actor state hook 0x0800E1A6 / 0x0800E666 (runner, P0 = state byte)',
    15: 'engine hook 0x08026480 (runner)', 498: 'loot drop (0x0801096C runner; P0..P3 = position/loot params)',
}
for ev, desc in ENGINE_HOOKS.items():
    ref_event(ev, 'engine_code', 'code', desc)
for i in range(0, 0x60000, 4):
    v = struct.unpack_from('<I', R, i)[0]
    if v in HDR and HDR[v] not in ENGINE_HOOKS and 0x08000000 + i < 0x08060000:
        ref_event(v, 'engine_code', f'literal @0x{0x08000000 + i:08X}', 'header literal in code')

# ---------------------------------------------------------------- build scripts

scripts = []
event_info = {}
script_runs = defaultdict(set)
event_runs = defaultdict(set)
for ev in range(NEV):
    nodes, terms, goto, dyn = walk_event(ev)
    event_info[ev] = (nodes, terms, goto, dyn)
    for k, (nd, code) in nodes.items():
        for s in nd.get('runs_script', []):
            script_runs[s].add(ev)
        for s in nd.get('runs_event', []):
            event_runs[s].add(ev)
        if 'goto_event' in nd and isinstance(nd['goto_event'], int):
            refs[('event', nd['goto_event'])].append(dict(kind='event_goto', where=f'event #{ev} node {k}', detail='node kind 7'))

for s, evs in script_runs.items():
    for ev in sorted(evs):
        if isinstance(s, int) and 0 <= s < NEV:
            refs[('event', s)].append(dict(kind='run_script', where=f'event #{ev}', detail='native 0xD6..0xD8 (node 0 only, synchronous)'))
for s, evs in event_runs.items():
    for ev in sorted(evs):
        if isinstance(s, int) and 0 <= s < NEV:
            refs[('event', s)].append(dict(kind='run_event', where=f'event #{ev}', detail='native 0xB5..0xB9 (blocking)'))

CUTSCENE_NATIVES = {0x38, 0x3a}
LOOT = {497, 498, 499, 500, 496}


def purpose_for(kinds, natives, texts, has_choice, ev=None):
    if ev in LOOT:
        return 'loot'
    if 'item_use' in kinds:
        return 'item_use'
    if 'equipment_effect' in kinds:
        return 'equipment_effect'
    if 'quest_log_condition' in kinds:
        return 'quest_log'
    if kinds & {'enemy_defeat_callback', 'ai_run_script', 'ai_inline_script'}:
        return 'boss_fight'
    if kinds & {'map_object_condition', 'actor_spawn_condition', 'list2_condition', 'list3_condition'}:
        return 'spawn_condition'
    if kinds & {'area_music', 'area_variant', 'area_name'}:
        return 'area_selector'
    if has_choice and any('Z-Mart' in t or 'buy' in t.lower() for t in texts):
        return 'shop'
    if 'npc_talk' in kinds:
        return 'npc_talk'
    if natives & CUTSCENE_NATIVES:
        return 'cutscene'
    if kinds & {'map_event_trigger', 'map_script_trigger', 'area_on_enter', 'area_on_leave', 'map_object_event',
                'map_fixed_event_trigger', 'list2_event', 'list3_event'}:
        return 'map_event'
    if 'engine_code' in kinds:
        return 'engine_hook'
    if texts:
        return 'dialog'
    if natives:
        return 'quest_flag_logic'
    return 'other'


for ev in range(NEV):
    nodes, terms, goto, dyn = event_info[ev]
    h = HDRS[ev]
    rs = refs.get(('event', ev), [])
    kinds = {r['kind'] for r in rs}
    natives = set()
    texts = []
    has_choice = False
    out_nodes = []
    bc_size = 0
    for k in sorted(nodes):
        nd, code = nodes[k]
        natives |= set(nd.get('natives', []))
        if nd.get('text'):
            texts.append(nd['text'])
        if nd['kind'] == 3:
            has_choice = True
        if nd['kind'] in (0, 4, 2, 6):
            bc_size += nd.get('size', 0)
        out_nodes.append(nd)
    kinds_nodes = Counter(nd['kind'] for nd in out_nodes)
    is_script = any(nd['kind'] in (0, 4) for nd in out_nodes)
    n0 = nodes.get(0, (None,))[0]
    purpose = purpose_for(kinds, natives, texts, has_choice, ev)
    first_text = next((t for t in texts if t.strip()), '')
    summary = (f'{len(out_nodes)} nodes ({", ".join(f"{c}x kind{k}" for k, c in sorted(kinds_nodes.items()))})'
               + (f'; text: "{first_text[:90]}"' if first_text else '')
               + (f'; natives {",".join(f"0x{n:02X}" for n in sorted(natives))}' if natives else ''))
    scripts.append(dict(
        id=f's{ev:03d}', table_index=ev, header=f'0x{h:08X}',
        code_addr=n0['code_addr'] if n0 and 'code_addr' in n0 else (n0['addr'] if n0 else None),
        compressed=bool(n0 and n0.get('compressed')), size=bc_size,
        node0_kind=n0['kind'] if n0 else None,
        runnable_by_run_script=bool(n0 and n0['kind'] in (0, 4)),
        nodes=out_nodes, terminals={str(k): v for k, v in sorted(terms.items())}, dynamic_branch=dyn,
        referenced_by=rs, purpose=purpose, summary=summary,
        strings=[t for t in texts][:40]))

# inline raw scripts
raw_keys = sorted(k[1] for k in refs if k[0] == 'raw')
for addr in raw_keys:
    rs = refs[('raw', addr)]
    d = decode(R[ra(addr):ra(addr) + 0x4000])
    kinds = {r['kind'] for r in rs}
    natives = set(d['natives'])
    for s in d['runs']:
        if isinstance(s, int) and 0 <= s < NEV:
            refs[('event', s)].append(dict(kind='run_script', where=f'inline_0x{addr:08X}', detail='native 0xD6..0xD8'))
    for s in d['evs']:
        if isinstance(s, int) and 0 <= s < NEV:
            refs[('event', s)].append(dict(kind='run_event', where=f'inline_0x{addr:08X}', detail='native 0xB5..0xB9'))
    purpose = purpose_for(kinds, natives, [], False)
    scripts.append(dict(
        id=f'inline_0x{addr:08X}', table_index=None, code_addr=f'0x{addr:08X}', compressed=False, size=d['size'],
        decode_ok=d['ok'], natives=sorted(natives), returns=sorted(d['rets']), dynamic_return=d['dyn_ret'],
        runs_script=sorted(d['runs']), runs_event=sorted(d['evs']),
        referenced_by=rs, purpose=purpose,
        summary=f'{d["size"]} bytes; refs: {", ".join(sorted(kinds))}'
                + (f'; natives {",".join(f"0x{n:02X}" for n in sorted(natives))}' if natives else '')
                + (f'; returns {sorted(d["rets"])[:6]}' if d['rets'] else ''),
        strings=[]))

# referenced_by of events may have grown (inline run_script); refresh
for s in scripts:
    if s['table_index'] is not None:
        s['referenced_by'] = refs.get(('event', s['table_index']), [])

# entry points
EP = [
    ('0x08004098', 'runner', 'event header; runs node 0 only; node byte2 = 0 raw code @+3, 4 LZ (headerless) @+4, else assert',
     'r2 = param block (0 -> own zeroed? block in the 0x6C-byte VM object)', 'caller reads VM stack[0]'),
    ('0x08004038', 'interpreter', 'raw code pointer', 'r2 = param block pointer or 0 (VM object +0x5C)', 'caller-dependent'),
    ('0x08012588', 'run_event (blocking)', 'event header (node graph); runs nested frame loop 0x0803D8A0 until done',
     'r1 = param block (0 -> event object +0x18)', 'no'),
    ('0x0801246E', 'event_new', 'event header; caller steps it (0x080124F8) and frees it (0x080124D2)', 'r3/stack', 'no'),
    ('0x0800421F', 'callback {fn, code}', 'raw code at pair+4', 'zeroed own block', 'no'),
    ('0x0800424F', 'callback {fn, header}', 'event header at pair+4 -> run_event', 'none (0)', 'no'),
    ('0x08004784', 'condition evaluator', '0 true; 0x0001xxxx flag set; 0x0002xxxx flag clear; 0x0003xxxx test+clear; ROM ptr raw code', 'none', 'yes (bool of stack[0])'),
    ('0x080089A0/0x080089D8/0x08008A36', 'value-or-script', 'u32 < 0x10000 = value; else raw code', 'none', 'yes (stack[0])'),
    ('natives 0xD6/0xD7/0xD8', 'run_script(idx[,p0[,p1]])', 'event table idx -> runner (node 0)', 'writes P0/P1 of the CURRENT param block (shared)', 'no'),
    ('natives 0xB5..0xB9', 'run_event(idx[,p0..p3])', 'event table idx -> run_event (blocking)', 'writes P0..P3 of the current param block, passes it', 'no'),
    ('0x08011F5A / 0x08011FD6', 'event node kind 0/4', 'bytecode node', 'event params', 'yes: if stack depth == 1 at end, value = next node index'),
    ('0x080121D8 / 0x080122B4', 'event node kind 2/6', 'bytecode @node+8 computing printf args for the text', 'event params', 'yes (stack -> format args)'),
    ('0x08007368', 'map script trigger (ctor 0x08007387)', 'raw code rec+0x10', 'object +0x14 block', 'no'),
    ('0x080073F0', 'map event trigger (ctor 0x0800742D, vtable 0x08049BD4)', 'event header rec+0x10, blocking', 'object block', 'no'),
    ('0x08017D66', 'NPC talk (ctor 0x08017DA3)', 'event header rec+0xC via event_new', 'npc', 'no'),
    ('0x08008CE8/0x0800908C/0x080091E0', 'area on_leave (+0x34) / on_enter (+0x30)', 'raw code', 'zeroed', 'no'),
    ('0x08013C5E', 'equipment effect (+0x28)', 'raw code', 'zeroed', 'no'),
    ('0x0801E15C/0x0801E182', 'supply item use (+0x30)', 'raw code', 'zeroed', 'no'),
    ('0x080141FA..0x08014232', 'quest log (table 0x08720564, 40 x 3 conditions)', 'raw code', 'zeroed', 'yes (bool)'),
    ('0x0803333D/0x08033355', 'AI InlineScript node', 'inline code def+0xC', 'actor+0x214', 'yes (1 = done)'),
    ('0x080338C5/0x080338DB', 'AI RunScript node', 'event header def+0xC, blocking', 'actor+0x214', 'no'),
    ('0x08033308', 'AI script condition', 'raw code [node+4]', 'actor+0x214', 'yes (bool)'),
    ('0x08035B94', 'deferred event object', 'event header obj+8', 'actor+0x214 if actor alive', 'no'),
    ('0x0800F62C', 'callback dispatcher object (+0x18 raw code, then +0x1C callback)', 'raw code', 'zeroed', 'no'),
    ('0x08025A96', 'scene object script ([obj]+4)', 'raw code', 'zeroed', 'no'),
]
entry_points = [dict(caller=c, kind=k, format=f, params=p, uses_return=(u != 'no'), evidence=u) for c, k, f, p, u in EP]

nonscript = []
for s in scripts:
    if s['table_index'] is not None and not s['runnable_by_run_script']:
        nonscript.append(dict(table_index=s['table_index'], header=s['header'], node0_kind=s['node0_kind'],
                              note='event whose node 0 is not bytecode (text/choice/goto); run via run_event only'))

counts = dict(
    events=NEV,
    events_with_bytecode=sum(1 for s in scripts if s['table_index'] is not None and any(n['kind'] in (0, 2, 4, 6) for n in s['nodes'])),
    event_nodes=sum(len(s['nodes']) for s in scripts if s['table_index'] is not None),
    event_nodes_by_kind=dict(Counter(n['kind'] for s in scripts if s['table_index'] is not None for n in s['nodes'])),
    bytecode_units_in_events=sum(1 for s in scripts if s['table_index'] is not None for n in s['nodes'] if n['kind'] in (0, 2, 4, 6)),
    inline_scripts=sum(1 for s in scripts if s['table_index'] is None),
    unreferenced_events=sum(1 for s in scripts if s['table_index'] is not None and not s['referenced_by']),
    purpose=dict(Counter(s['purpose'] for s in scripts)),
    purpose_events=dict(Counter(s['purpose'] for s in scripts if s['table_index'] is not None)),
    purpose_inline=dict(Counter(s['purpose'] for s in scripts if s['table_index'] is None)),
    ref_kinds=dict(Counter(r['kind'] for s in scripts for r in s['referenced_by'])),
    decode_failures=[s['id'] for s in scripts if s.get('decode_ok') is False or any(n.get('decode_ok') is False for n in s.get('nodes', []))],
)

out = dict(
    source='Dragon Ball Z: Buu\'s Fury (USA) ROM; generated by re/buu/script_cat_build.py',
    formats=dict(
        event_table='0x0879B704: 522 u32 pointers to event headers',
        event_header='array of u32: < 0x10000 = terminal (finish; value stored to the caller\'s state word), else node pointer',
        node='u16 next (LE) | u8 kind | u8 arg | payload; after the node finishes the event continues at `next` unless a bytecode node ends with exactly 1 value on the stack (that value becomes the next index)',
        node_kinds={'0': 'bytecode, raw at +3', '1': 'text box, UTF-16 inline at +4 (arg = box style)',
                    '2': 'text box, UTF-16 at [+4] with printf args computed by bytecode at +8',
                    '3': 'choice prompt, UTF-16 inline at +4 (lines starting with "-" are options)',
                    '4': 'bytecode, headerless Webfoot-LZ stream at +4 (inflated to a 0x800 buffer)',
                    '5': 'text box, packed text-bank ref at +4: bank = v&0x3F (table 0x087B5B64, LZ container), offset = ((v>>6)&0x7FFF)*2, length = (v>>21)*2 bytes',
                    '6': 'kind 5 text + printf args from bytecode at +8', '7': 'goto: continue with event header at +4, node 0'},
        callback_pair='{0x0800421F, raw code ptr} | {0x0800424F, event header}',
        condition_word='0 = true; 0x0001xxxx flag xxxx set; 0x0002xxxx clear; 0x0003xxxx set (and clears it); ROM ptr = bytecode returning bool',
        value_or_script='u32 < 0x10000 = literal; else bytecode pointer whose stack[0] is the value',
        area_entry='0x0808E2E0 + 0x38*i: +0xC name (v-or-s), +0x10 list0 {cond, map-object rec} x count(+3), +0x14 list1 actor rec ptrs x count(+4) (rec+4 cond), +0x18 list2 (+5) / +0x1C list3 (+6) rec ptrs (rec+0 cond), +0x20 list4 (+7), +0x24 music (v-or-s), +0x28 layout variant (v-or-s), +0x2C variant table, +0x30 on_enter code, +0x34 on_leave code',
    ),
    counts=counts, entry_points=entry_points, scripts=scripts, non_script_resources=nonscript,
)
p = ROOT / 'engine' / 'annotations' / 'script_catalog.json'
p.write_text(json.dumps(out, indent=1, ensure_ascii=False))
print(json.dumps(counts, indent=1))
print('list0 ctors', {f'{k:08x}': v for k, v in LIST0_CTORS.items()})
