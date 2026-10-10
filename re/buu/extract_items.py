"""Generate data/buu/items.json: supplies/story items (0x083BD2F8) and equipment (0x086F82EC), with
use-scripts / equip-scripts disassembled and their effects summarised.

Effect sources:
  * supply use scripts (table +0x30)                           -> heal amounts, Holy Water, Senzu
  * equipment effect scripts (table +0x28) run by RecalcStats    -> natives 0xDF..0xE8, 0xF5
  * script #2 (on melee damage dealt, from 0x0800FC5E)          -> Armor of Light/Darkness, Vampire Cape,
                                                                   Talisman of Light, Pilaf's Gloves, Kazoo
  * script #497 (enemy loot)                                     -> luck items (owned, not necessarily equipped)
"""
import json
from common import *
from tables import *
from scriptdis import dis_script

NATIVE_EFFECT = {
    0xdf: ('hp_regen_per_frame_256ths', lambda n: n * 0x445 >> 8),
    0xe0: ('thorns_damage', lambda n: n * 0xa4 >> 6),
    0xe1: ('END_and_END_energy', lambda n: n),
    0xe2: ('POW', lambda n: n),
    0xe3: ('SPD', lambda n: n),
    0xe4: ('STR', lambda n: n),
    0xe5: ('block_damage_factor', lambda n: 'x1/2 (0x1000 -> 0x800 of 65536)'),
    0xe6: ('energy_attack_cost', lambda n: 'x1/2'),
    0xe7: ('energy_block_cost', lambda n: 'x1/2'),
    0xe8: ('ss_cooldown_rate', lambda n: 'x7/8'),
    0xf5: ('crit_chance_percent', lambda n: n),
}

# behaviours implemented in shared scripts / code rather than the item's own script (equipment ids)
EXTRA_EFFECTS = {
    28: 'melee hit that deals damage: EP +2 instead of +1 (script #2)',
    15: 'melee hit that deals damage: HP +1 instead of EP +1 (script #2)',
    152: 'melee hit that deals damage: HP +1 instead of EP +1 (script #2)',
    141: 'melee hit that deals damage: extra EP +1 (script #2)',
    56: 'melee hit that deals damage: Zenie +1 (script #2)',
    144: 'melee hit: plays a random kazoo sound (7/10/11/59/61) (script #2)',
    121: 'enemy loot: +1 drop roll, tested twice in script #497 -> effectively +2 (owning one is enough)',
    136: 'enemy loot: +2 drop rolls (script #497, owning one is enough)',
    151: 'enemy loot: +3 drop rolls (script #497)',
    138: 'enemy loot: -1 drop roll (script #497)',
    143: 'enemy loot: -2 drop rolls (script #497)',
    131: 'enemy loot: -3 drop rolls (script #497)  [fake Rabbit\'s Foot]',
    119: 'while equipped (native 0xD5): food items restore 25% more',
    118: 'while equipped (native 0xD5): drink items restore 25% more -- BUG: the straw branch writes the result to HP (setter 0x0C) instead of EP',
    40: 'part of Saiyan set (Saiyan Armor script): Armor+Gloves(73)+Boots(107) all equipped -> +50 STR, +50 END/END2, +50 POW',
    73: 'Saiyan set piece (see Saiyan Armor)', 107: 'Saiyan set piece (see Saiyan Armor)',
}


def effect_summary(addr):
    if not (0x08000000 <= addr < 0x08800000):
        return None, []
    lines = dis_script(addr, 200)
    txt = [s for _, s in lines]
    effects = []
    last_push = None
    for s in txt:
        if s.startswith('push '):
            try:
                last_push = int(s.split()[1])
            except ValueError:
                last_push = None
        elif s.startswith('native '):
            n = int(s.split()[1], 16)
            if n in NATIVE_EFFECT:
                k, f = NATIVE_EFFECT[n]
                effects.append({k: f(last_push or 0)})
    conditional = any(s.startswith('jz') for s in txt)
    return txt, effects if not conditional else [dict(conditional=True, natives=effects)]


def supply_effect(i, s):
    addr = int(s['use_script'], 16)
    if not addr:
        return None, None
    lines = [x for _, x in dis_script(addr, 300)]
    eff = {}
    pushes = [int(x.split()[1]) for x in lines if x.startswith('push ') and x.split()[1].lstrip('-').isdigit()]
    if any(l.startswith('native 0x0d') for l in lines) and any(l.startswith('native 0x0f') for l in lines):
        return lines, dict(full_restore_hp_ep=True, note='HP = maxHP (native 0x0D -> setter 0x0C), EP = maxEP (0x0F -> 0x0E)')
    if 'native 0x0c ' in lines or any(l.startswith('native 0x0c') for l in lines):
        if any(l.startswith('set 0x0c') for l in lines) and any(l.startswith('native 0x0d') for l in lines):
            eff = dict(full_restore=True)
        elif len(pushes) >= 3:
            eff = dict(restore_hp=pushes[2], restore_hp_with_chopsticks=pushes[1])
    if any(l.startswith('native 0x0e') for l in lines) and len(pushes) >= 3 and not eff:
        eff = dict(restore_ep=pushes[2], restore_ep_with_straw=pushes[1],
                   bug='with the Primordial Twisty-Straw owned the game sets HP = EP + bonus instead of EP += bonus')
    if any(l.startswith('native 0xf2') for l in lines):
        eff = dict(holy_water=True, duration_seconds=30, STR=25, POW=25, END=25, SPD=25,
                   note='native 0xF2 pushes (30, 25,25,25,25); bonuses live in GameData+0x4C0 and are applied by 0x080046AE')
    return lines, eff


def main():
    sups = []
    for i in range(N_SUPPLIES):
        s = supply(i)
        lines, eff = supply_effect(i, s)
        s['use_script_disasm'] = lines
        s['effect'] = eff
        sups.append(s)
    eqs = []
    for i in range(N_EQUIP):
        q = equipment(i)
        txt, eff = effect_summary(int(q['effect_script'], 16))
        q['effect_script_disasm'] = txt
        q['script_effects'] = eff
        q['other_effects'] = EXTRA_EFFECTS.get(i)
        if q['exp_bonus_256ths']:
            q['exp_multiplier'] = (256 + q['exp_bonus_256ths']) / 256
        q['recalc_flag'] = bool(q['flags_0x24'])
        eqs.append(q)
    out = dict(
        meta=dict(generator='re/buu/extract_items.py',
                  supply_table=hex(SUPPLIES), equipment_table=hex(EQUIPMENT),
                  supply_layout={'+0x00': 'u16 0=consumable 1=story item', '+0x02': 'u16 short name string id',
                                 '+0x04': 'u16 name', '+0x06': 'u16 plural', '+0x08': 'u32 description string id',
                                 '+0x0C..+0x2B': 'icon/graphics descriptors', '+0x2C': 's32 price (Zenie)',
                                 '+0x30': 'ptr use-script (bytecode)'},
                  equipment_layout={'+0x00': 'u16 name', '+0x02': 'u16 description', '+0x04..+0x13': 'icon descriptors',
                                    '+0x14': 'u8 slot 0 body 1 hands 2 feet 3 accessory', '+0x15': 'u8 required level',
                                    '+0x16..+0x1A': 's8 STR, POW, END (melee def), END2 (energy def), SPD',
                                    '+0x1C': 'u32 XP bonus in 1/256 (added to the 256 base multiplier)',
                                    '+0x20': 's32 price (-1 for the "None" placeholders)',
                                    '+0x24': 'u32 flags OR-ed into record+0x13 (bit0: recompute stats when HP changes, Crisis Ring)',
                                    '+0x28': 'ptr effect script run by RecalcStats (0x08013C66 -> 0x08013C02)'},
                  stat_recalc=('RecalcStats 0x08013C66: start from base STR/POW/END/END(=END)/SPD, add each equipped '
                               'item\'s s8 modifiers and run its effect script, add Holy Water bonuses '
                               '(0x080046AE), clamp negatives to 0, store at record+0x4C..'),
                  status='tables VERIFIED (disasm); stat application VERIFIED (emu Status screen); effects from scripts VERIFIED (disasm)'),
        supplies=sups, equipment=eqs)
    p = DATA / 'items.json'
    p.write_text(json.dumps(out, indent=1))
    print('wrote', p)


if __name__ == '__main__':
    main()
