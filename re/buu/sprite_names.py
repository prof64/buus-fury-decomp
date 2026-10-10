"""sprite_names.py [--apply] -- game names for the Buu sprite definitions (sprite index = gfx id - 9).

Sources, in order: playable forms (characters.json; Dojo/character-select text style 'SS Goku', 'SS3 Gotenks'), the
Capsule Corp database rows (gfx id -> name; rows named 'Human' are the game's own placeholder and are not used),
enemy types spawned with the gfx (enemies.json). Sprites sharing a name get the qualifier in QUALIFIER (form number,
halo, colour, ball form ...). Sprites with no game name keep their descriptive name (confidence 'low', notes say so).

Prints index, current name, new name, source. --apply rewrites only the `entities` section of
tools/extract/data/names_buu.json (name / category / confidence / notes), leaving every other section byte-identical.
"""
import json
import sys
from collections import defaultdict
from common import DATA, ROOT
from tables import db_entries

NAMES = ROOT / 'tools' / 'extract' / 'data' / 'names_buu.json'
chars = json.loads((DATA / 'characters.json').read_text())
enemies = json.loads((DATA / 'enemies.json').read_text())['enemies']

HALO = {49, 74, 93, 108, 110, 112, 152, 173, 271, 274}
# final names where the plain game name is shared by several sprites (or a form needs its variant)
QUALIFIER = {
    93: 'SS Gogeta (halo)', 94: 'SS Gogeta', 95: 'Gogeta', 96: 'Gogeta (2)',
    97: 'Gohan (form 9)', 98: 'Gohan (form 10)', 99: 'Gohan (form 11)', 100: 'Gohan (form 12)', 101: 'Gohan (form 13)',
    102: 'Great Saiyaman', 103: 'Gohan (form 15)', 104: 'SS Gohan (forms 16, 17)', 105: 'SS Gohan (form 18)',
    107: 'Goku (Other World)',
    118: 'Gotenks (form 32)', 119: 'Gotenks (form 33)', 262: 'Trunks (form 23)',
    40: 'Super Buu', 42: 'Super Buu (Gotenks absorbed)', 43: 'Super Buu (Gohan absorbed)',
    69: 'Destroyer', 70: 'Destroyer (2)', 77: 'Elite Shield Soldier', 163: 'Elite Shield Soldier (2)',
    124: 'Grand Mechanoid', 125: 'Grand Mechanoid (ball)', 178: 'Mechanoid', 179: 'Mechanoid (ball)',
    221: 'Power Mechandoid', 222: 'Power Mechandoid (ball)',
    140: 'Janemba (back)', 141: 'Janemba Hand (portal)', 142: 'Janemba Hand (portal, 2)',
    166: 'Majin Shield Soldier', 167: 'Majin Shield Soldier (2)', 168: 'Majin Soldier',
    193: 'Nosferatu', 194: 'Nosferatu (bat)', 265: 'Vampire', 267: 'Vampire (bat)', 279: 'Vlad', 280: 'Vlad (bat)',
    200: 'Fighter (blue)', 202: 'Fighter (red)', 204: 'Fighter (pink)', 206: 'Fighter (green)',
    61: 'Criminal', 226: 'Rapscallion', 22: 'Bio Mech', 76: 'Elite Majin Soldier', 155: 'Knight Destroyer',
    212: 'Pharoah', 64: 'Dabura', 89: 'Dabura (2)', 169: 'Super Shield Soldier (2)', 248: 'Super Shield Soldier',
    247: 'Super Majin Soldier', 277: 'Videl (2)', 278: 'Videl', 295: "Yemma's Assistant", 296: "Yemma's Assistant (2)",
    143: 'Mini-Janemba', 144: 'Mini-Janemba (2)',
}
# no game name, but the old descriptive name named the wrong thing (same look as a named sprite)
DESCRIPTIVE_FIX = {
    201: 'Fighter (blue, 2)', 203: 'Fighter (blue, 3)', 205: 'Fighter (blue, 4)', 207: 'Fighter (blue, 5)',
    138: 'Janemba arm', 139: 'Janemba arm (2)', 187: 'Mummy (2)', 191: 'Ninja (2)', 35: 'Kamikaze ghost',
    33: 'Evil Buu', 10: 'Old man in suit', 11: 'Old man in suit (2)', 12: 'Old man in suit (3)', 13: 'Old man in suit (4)',
    51: 'Bald thug', 52: 'Bald thug (2)', 53: 'Bald thug (3)', 54: 'Bald thug (4)', 55: 'Bald thug (5)',
}
# categories the visual guess got wrong (the game name says what it is)
CATEGORY = {22: 'enemy', 64: 'boss', 89: 'npc', 243: 'boss', 225: 'boss', 33: 'boss', 113: 'enemy', 254: 'enemy',
            157: 'player', 7: 'player', 255: 'player', 293: 'player', 215: 'player', 213: 'player'}


def candidates():
    by = defaultdict(list)
    for f in chars['forms']:
        if not f['gfx_id']:
            continue
        cname = f['name'] or chars['characters'][f['char_index']]['name']
        pre = 'SS3 ' if f['flags'] & 2 else 'SS ' if f['flags'] & 1 else ''
        by[f['gfx_id'] - 9].append((pre + cname, f'playable form {f["index"]}', 'player'))
    db = defaultdict(list)
    for d in db_entries():
        if d['name'] != 'Human':
            db[d['gfx_id'] - 9].append((d['name'], f'Capsule Corp database row {d["index"]}',
                                        'npc' if d['link'] < 0 else None))
    en = defaultdict(list)
    for e in enemies:
        if e['name'] and e['name'] != 'Human' and e.get('display_name_source') != 'descriptive':
            for g in e['gfx_ids']:
                en[g - 9].append((e['name'], f'enemy type {e["type"]:#x}', None))
    return by, db, en


def main():
    text = NAMES.read_text()
    doc = json.loads(text)
    ents = doc['entities']
    by, db, en = candidates()
    enemy_gfx = {g - 9: e for e in enemies for g in e['gfx_ids']}
    changes = []
    for k, cur in ents.items():
        i = int(k)
        cand = by.get(i) or db.get(i) or en.get(i)
        new = dict(cur)
        if i in QUALIFIER or cand:
            nm = QUALIFIER.get(i) or sorted({c[0] for c in cand}, key=lambda s: (len(s), s))[0]
            if i in HALO and 'halo' not in nm:
                nm += ' (halo)'
            src = ', '.join(sorted({c[1] for c in cand})) if cand else 'qualified game name'
            new['name'] = nm
            new['confidence'] = 'high'
            cat = next((c[2] for c in cand if c[2]), None) if cand else None
            if cat == 'player' or (cat == 'npc' and cur.get('category') not in ('player',)):
                new['category'] = cat
            elif i in enemy_gfx and cur.get('category') in ('npc', 'effect', 'item'):
                new['category'] = 'enemy'
            note = f'name: game text ({src})'
        elif i in DESCRIPTIVE_FIX:
            new['name'] = DESCRIPTIVE_FIX[i]
            new['confidence'] = 'low'
            note = 'name: descriptive (no game name for this sprite)'
        else:
            new['confidence'] = 'low'
            note = 'name: descriptive (no game name for this sprite)'
            if i in enemy_gfx and enemy_gfx[i]['name'] == 'Human':
                note = 'name: descriptive; the scouter calls this character "Human"'
        if i in CATEGORY:
            new['category'] = CATEGORY[i]
        old_notes = [n for n in (cur.get('notes') or '').split('; ') if n and not n.startswith('name:')]
        new['notes'] = '; '.join([note] + old_notes)
        if new.get('name') != cur.get('name'):
            changes.append((i, cur.get('name'), new['name'], note))
        ents[k] = new
    for c in changes:
        print(*c, sep=' | ')
    seen = defaultdict(list)
    for k, v in ents.items():
        seen[v['name']].append(k)
    print('DUPLICATES', {n: v for n, v in seen.items() if len(v) > 1})
    print(len(changes), 'renamed')
    if '--apply' in sys.argv:
        # splice the entities value only (other sections keep their exact text)
        key = '\n "entities": '
        a = text.index(key) + len(key)
        _, end = json.JSONDecoder().raw_decode(text, a)
        body = json.dumps(ents, indent=1, ensure_ascii=False).replace('\n', '\n ')
        NAMES.write_text(text[:a] + body + text[end:])
        print('wrote', NAMES)


if __name__ == '__main__':
    main()
