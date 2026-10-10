"""Extract LoG2 item table (0x086ade24, 50 x 0x38 bytes) + enemy drop scripts -> data/log2/items.json.
Effects are keyed by the item's use-function (+0x30), decoded by hand from the disassembly (see notes.md Part D)."""
import json, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from common import *
from maps import rec, N
from it_scr import decode

ITEMS = 0x086ade24
STRTAB = 0x080e2840
INV_RAM = 0x03000e90 + 0x12b   # count byte per item id

def s(i):
    p = u32(STRTAB + 4 * i); o = p & 0x1ffffff
    return ROM[o:ROM.index(b'\0', o)].decode('latin1')

USE_FN = {
    0x0800f6e9: dict(kind='full_restore', text='HP=maxHP and EP=maxEP of active character; refused (not consumed) if both already full'),
    0x0800f73d: dict(kind='world_map', text='runs script 0x083b6c50 (World Map / Golden Capsule ship); returns 2'),
    0x0800f45b: dict(kind='stat_up', stat='STR', record_offset=0xa),
    0x0800f4c3: dict(kind='stat_up', stat='POW', record_offset=0xc),
    0x0800f52b: dict(kind='stat_up', stat='END', record_offset=0xe),
    0x0800f593: dict(kind='heal_hp'),
    0x0800f639: dict(kind='restore_ep'),
    0x0800f72d: dict(kind='open_album', text='opens the Album screen (menu object 0x081d4cd0)'),
}
STAT_AMT = {2: 1, 3: 3, 4: 5, 5: 1, 6: 3, 7: 5, 8: 1, 9: 3, 10: 5}
HEAL = {11: ('pct', 0x199a), 12: ('pct', 0x3334), 13: ('pct', 0x4ccd), 43: ('pct', 0x199a), 21: ('flat', 5)}
EP = {14: 0x199a, 15: 0x3334, 16: 0x4ccd}

def effect(i, use):
    if not use:
        return dict(kind='none', text='not usable from the menu (key/quest/collectible item)')
    d = dict(USE_FN.get(use, dict(kind='unknown')))
    if d['kind'] == 'stat_up':
        n = STAT_AMT[i]
        d.update(amount=n, raw_add=n << 8,
                 text=f"record.{d['stat']} += {n}<<8 (8.8 fixed point, i.e. +{n}.0), clamped to 0x6400 (100.0); "
                      f"refused (not consumed) if stat already at cap ({'>=' if d['stat']=='STR' else '=='} 0x6400); then player entity stats refreshed (0x080086a2)")
    elif d['kind'] == 'heal_hp':
        k, v = HEAL[i]
        if k == 'pct':
            d.update(formula=f'HP = min(maxHP, HP + (maxHP*{v:#x} >> 16))', fraction=round(v / 65536, 4))
        else:
            d.update(formula=f'HP = min(maxHP, HP + {v})')
    elif d['kind'] == 'restore_ep':
        v = EP[i]
        d.update(formula=f'EP = min(maxEP, EP + (maxEP*{v:#x} >> 16))', fraction=round(v / 65536, 4))
    return d

def drop_table(a):
    ins = decode(a)
    # skip an optional prefix (e.g. 'push N; call 0x1b' flag ops) before 'push 100; call rand; store 0'
    for k in range(len(ins) - 2):
        if ins[k][1] == 'push' and ins[k + 1][2] == 'rand' and ins[k + 2][1] == 'store':
            break
    else:
        return None
    pre = [f'{x[1]} {x[2]}' for x in ins[:k]]
    N_ = ins[k][2]; rows = []; i = k + 3
    while i < len(ins) - 4:
        if ins[i][1] == 'call' and ins[i][2] == 'L0' and ins[i + 1][1] == 'push' and ins[i + 2][1] == '<':
            T = ins[i + 1][2]; j = i + 3; cond = None
            if ins[j][1] == 'call' and ins[j + 1][1] == 'call' and ins[j + 2][1] == '!=':
                cond = f'{ins[j][2]} != {ins[j+1][2]}'; j += 4
            if ins[j][1] == 'jz' and ins[j + 1][1] == 'push' and ins[j + 2][2] == 'drop_item_at_entity':
                rows.append(dict(roll_lt=T, item=ins[j + 1][2], item_name=None, condition=cond)); i = j + 3; continue
        i += 1
    return dict(roll=f'L0 = rand({N_})', prefix_ops=pre, rows=rows)

def main():
    items = []
    for i in range(50):
        a = ITEMS + 0x38 * i; o = a & 0x1ffffff
        b = ROM[o:o + 0x38]
        w = struct.unpack_from('<14I', b)
        use, pick = w[12], w[13]
        items.append(dict(
            id=i, rom_addr=hex(a), name=s(b[4] | b[5] << 8), description=s(b[6] | b[7] << 8),
            name_string_id=b[4] | b[5] << 8, desc_string_id=b[6] | b[7] << 8,
            stored_in_inventory=bool(b[0]), flag_b1=b[1], show_pickup_popup=bool(b[2]), max_count=b[3],
            gfx_ptrs=[hex(x) for x in (w[5], w[10], w[11])], sprite_w=w[6], sprite_h=w[7],
            use_fn=hex(use) if use else None, pickup_fn=hex(pick),
            effect=effect(i, use)))
    names = {it['id']: it['name'] for it in items}
    # drop scripts from enemy spawn defs (factory 0x0800e77f; spawn+0x14 -> [fn, script])
    tables = {}; spawns = []
    for m in range(N):
        ids, f = rec(m)
        lst = f[3]; cnt = ids[4]
        if not lst: continue
        for k in range(cnt):
            p = u32(lst + 4 * k)
            w = struct.unpack_from('<8I', ROM, p & 0x1ffffff)
            if w[0] != 0x0800e77f: continue
            fn = u32(w[5]); scr = u32(w[5] + 4)
            entry = dict(map_index=m, map_ids=ids[:2].hex(), spawn_def=hex(p), enemy_class=w[2], gfx_id=w[4],
                         on_death_fn=hex(fn), death_script=hex(scr) if fn == 0x0801069b else None)
            if fn == 0x0801069b:
                dt = drop_table(scr)
                if dt:
                    for r in dt['rows']: r['item_name'] = names[r['item']]
                key = json.dumps(dt, sort_keys=True)
                if key not in tables: tables[key] = dict(id=len(tables), table=dt, example_script=hex(scr), uses=0)
                tables[key]['uses'] += 1
                entry['drop_table_id'] = tables[key]['id']
            spawns.append(entry)
    out = dict(
        source='LoG2 ROM; extract_items.py',
        item_table=dict(addr=hex(ITEMS), count=50, stride=0x38,
                        fields={'+0 u8': 'stored_in_inventory (0 = consumed immediately on pickup: Health/Energy bonus orbs)',
                                '+1 u8': 'flag (1 for Golden Cap. #17, Missing Nameks, Dragon Balls) - INFERRED collectible/quest marker',
                                '+2 u8': 'show "obtained" popup on pickup (0x0800fb74)',
                                '+3 u8': 'max count carried (pickup refused when count >= max)',
                                '+4 u16': 'name string id (table 0x080e2840)', '+6 u16': 'description string id',
                                '+8..+0x2f': 'two sprite descriptors (w,h,flags,gfx ptr) + palette/tiles ptrs',
                                '+0x30 fn': 'use(itemId) -> nonzero if used (count decremented by menu)',
                                '+0x34 fn': 'pickup(itemId): 0x0800f3f1 add-to-inventory / 0x0800f441 popup only'}),
        inventory_ram=dict(addr=hex(INV_RAM), desc='u8 count per item id at party(0x03000e90)+0x12b+id (VERIFIED: Items menu + use test)'),
        pickup_object=dict(ctor='0x08011802', vtable='0x08024c90',
                           physics='spawned with height=0x2000 (1/1024 px units, drawn y -= height>>10), vz=0x800; '
                                   'each frame vz-=0x80, height+=vz; on hitting ground height=-h/2, vz=-vz/2; settles when vz<0x80, then collectible',
                           collect='touching the player calls item.pickup_fn (+0x34)',
                           despawn='temporary pickups (stored_in_inventory==0 && no popup) are registered with a watcher (0x0801177c) that '
                                   'removes them when the area is left (INFERRED); persisted >=3000 frames on screen (VERIFIED)'),
        items=items,
        drop_tables=[dict(id=v['id'], uses=v['uses'], example_script=v['example_script'], **(v['table'] or {'table': None}))
                     for v in tables.values()],
        drop_rules='Each enemy spawn def (+0x14) points at [on_death_fn, script]. on_death_fn 0x0801069b runs the script with the '
                   'script VM (0x080092e4) when the enemy dies: L0=rand(100); then rows are tested in order, first match drops '
                   'the item at the enemy position (sub-op 0x57 -> 0x0800aba5 -> pickup ctor 0x08011802). A row whose condition '
                   'fails falls through to the next row (so e.g. L0<2 with full HP can still yield the EP row). 0x08010699 = no drop; '
                   '0x080106b3 = scripted event instead of drops.',
        spawns=spawns,
    )
    path = os.path.join(ROOT, 'data/log2/items.json')
    json.dump(out, open(path, 'w'), indent=1)
    print('wrote', path, len(items), 'items', len(out['drop_tables']), 'drop tables', len(spawns), 'spawns')

if __name__ == '__main__':
    main()
