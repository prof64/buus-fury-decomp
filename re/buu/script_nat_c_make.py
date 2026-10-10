"""Write engine/annotations/script_natives_c.json (natives 0xA8..0xF7) and
engine/annotations/script_resource_kinds.json. Use counts come from script_nat_c_count.py
(work/buu/script/native_uses_c.json: every graph node of every resource-table entry + supply/equipment scripts).

Conventions: args are in source order (first pushed = first arg). pops/pushes are exact (from the handler asm).
GD = GameData 0x03001068; rec(c) = character record GD+0x4F8+0x68*c; cur = GD+0 (current character).
eqacc = equipment-effect accumulator 0x03002B5C (built while an equipment effect script runs).
Engine singletons: ROM table 0x08054FBC (+0x0c area manager 0x0300197C, +0x10 BG-effect slots 0x03001990,
+0x14 GD, +0x18 world entity list 0x03001C4C, +0x24 camera 0x030019A4).
world_add = 0x0801191A (append entity to world list), broadcast = 0x08011C0C(world, msg, &args)
(calls every entity's vtable[9] message handler), run_mode = 0x0803D8A0 (nested main loop = BLOCKS)."""
import json
from common import *

ROOT_ENGINE = ROOT / 'engine' / 'annotations'
uses = json.loads((WORK / 'script' / 'native_uses_c.json').read_text())
V, I = 'VERIFIED', 'INFERRED'

N = []


def nat(id, pops, pushes, args, name, category, kind, blocking, engine_fn, semantics, status, evidence):
    h = u32(0x08055098 + 4 * id)
    u = uses.get(f'0x{id:02x}', {'uses': 0, 'where': []})
    N.append({'id': id, 'hex': f'0x{id:02X}', 'handler': f'0x{h:08X}', 'pops': pops, 'pushes': pushes, 'args': args,
              'name': name, 'category': category, 'kind': kind, 'blocking': blocking, 'engine_fn': engine_fn,
              'semantics': semantics, 'status': status, 'evidence': evidence, 'uses': u['uses'],
              'used_in': u['where']})


nat(0xA8, 5, 0, ['fused_char', 'fusion_kind', 'member_a', 'member_b', 'seconds'], 'fuse_characters', 'party',
    'immediate', False, '0x0801375E',
    'Build fusion character record rec(fused) from rec(member_a)/rec(member_b): rec+2 = form_char(member_b) | '
    'form_char(member_a)<<4 | fusion_kind<<8 (fusion components); maxHP/HP/maxEP/EP/stats = (a+b)*0x9A>>8 (60% of '
    'the sum); rec(fused)+8 = seconds*60 frames (or -1 if seconds == -1 = permanent); rec(fused)+0 |= 3 (unlocked+'
    'active); rec(member_a/b)+0 &= ~1 (members unavailable while fused).', V,
    'emulator: fuse_characters(5,0,2,3,300) wrote GD+0x700 (rec5): +0 02->03, +2 = 0x23, +8 = 18000, stats = 60% '
    'of sums; used by the fusion-dance script #13 and #476/#501')
nat(0xA9, 3, 0, ['item_slot', 'x', 'y'], 'spawn_map_item_at', 'map', 'immediate', False, '0x080090E6 -> 0x0800DCDC',
    'If item_slot < current map header byte 6: create the map\'s item pickup #item_slot (map +0x1C list; def+4 = '
    'item id, >=0 supply / <0 equipment; pickup vtable 0x0804C508) at pixel (x, y) and add it to the world.', I,
    'asm: 0x080090E6 indexes area->map +0x1C, 0x0800DCDC builds a pickup from the supply (0x083BD2F8) or equipment '
    '(0x086F82EC) table; #398 spawn_map_item_at(0,160,178) after an event')
nat(0xAA, 1, 0, ['item_slot'], 'spawn_map_item', 'map', 'immediate', False, '0x080090E6 -> 0x0800DCDC',
    'Same as spawn_map_item_at but the position/extra args are the globals 0x03001010/0x03001018 (normally 0 = '
    'the pickup\'s own placement).', I, 'asm only; no uses in table scripts')
nat(0xAB, 0, 0, [], 'show_radar', 'screen', 'immediate', False, '0x0802B9A8 (+ ROM header checksum)',
    'Anti-tamper check: sum bytes 0x08000000..0x0800009F (mod 256) != 0x33 -> [0x03001004] = 1 (read by native '
    '0xF6). Then create the radar HUD entity (vtable 0x0804B400, OBJ tiles from 0x08753008; top-left semicircle) '
    'and add it to the world. The radar accepts targets through world message 0x1B (native 0xAC) and resets on '
    'message 0x0B.', V,
    'emulator: semicircle radar appears top-left; with the genuine ROM 0xF6 still returns 0')
nat(0xAC, 2, 0, ['x', 'y'], 'radar_add_target', 'screen', 'immediate', False, 'broadcast(world, 0x1B, {x, y})',
    'Broadcast world message 0x1B with (x, y); the radar entity appends the point to its target list (+0x1C '
    'count, 8-byte entries) and shows a blip.', I, 'asm of radar handler 0x0802BC21; blip seen in emulator')
nat(0xAD, 0, 0, [], 'respawn_conditional_map_objects', 'map', 'immediate', False, '0x08008AFE',
    'For each conditional object def of the current map (map +0x18 list, count map byte 5): if its condition '
    '(0x08004784, story-flag test) now holds, spawn it (0x0800C7E2). Used after setting story flags so NPCs/'
    'objects appear without reloading the map.', I, 'asm; #413/#418 call it right after set_flag natives')
nat(0xAE, 1, 0, ['seconds'], 'start_countdown', 'screen', 'immediate', False, 'GD+1 |= 4; GD+0x4D4 = seconds*60',
    'Start the countdown timer: GD byte 1 bit 2 = running, GD+0x4D4 = remaining frames (seconds*60), decremented '
    'by the game loop.', V, 'emulator: start_countdown(10) + show_countdown_hud shows 09:93, 09:58 ... counting down')
nat(0xAF, 0, 0, [], 'stop_countdown', 'screen', 'immediate', False, 'GD+1 &= ~4',
    'Stop the countdown timer (clear GD byte 1 bit 2).', I, 'asm')
nat(0xB0, 0, 0, [], 'show_countdown_hud', 'screen', 'immediate', False, '0x0802FCA0(0, 0) + world_add',
    'Create the timer HUD entity (vtable 0x0804BACC) in countdown mode (shows GD+0x4D4 as SS:hh, top-right).', V,
    'emulator (see start_countdown)')
nat(0xB1, 0, 0, [], 'hide_timer_hud', 'screen', 'immediate', False, 'broadcast(world, 0x1C, 0)',
    'Broadcast message 0x1C: the timer HUD entity (handler 0x0802FEEF) removes itself.', V,
    'asm of the timer HUD handler; identical handler body to 0xBD')
nat(0xB2, 3, 0, ['string_id', 'x', 'y'], 'show_title_text', 'screen', 'immediate', False,
    '0x0802BFBC -> 0x0802BECC + world_add',
    'Create a big outlined title-text entity for string table entry string_id, centred at (x, y) (e.g. '
    '"Match 2 Fight!"). Removed by 0xEF.', V, 'emulator: show_title_text(1525,120,65) shows "MATCH 2 FIGHT!"')
nat(0xB3, 2, 1, ['radius', 'angle'], 'sin_scale', 'math', 'query', False, 'sine table 0x087B6EEC',
    'Push (radius * sin_table[angle & 0xFF]) >> 16 (angle in 1/256 turns, table is 16.16).', V,
    'emulator: sin_scale(1000,64) = 1000')
nat(0xB4, 2, 1, ['radius', 'angle'], 'cos_scale', 'math', 'query', False, 'sine table 0x087B6EEC',
    'Push (radius * sin_table[(angle + 64) & 0xFF]) >> 16.', V,
    'emulator: cos_scale(1000,64) = 0, cos_scale(1000,0) = 1000')
for k, nm in ((0xB5, 0), (0xB6, 1), (0xB7, 2), (0xB8, 3), (0xB9, 4)):
    params = [f'p{j}' for j in range(nm)]
    nat(k, 1 + nm, 0, ['graph'] + params, 'run_scene' if nm == 0 else f'run_scene{nm}', 'script', 'modal', True,
        '0x08012588 (thread 0x0801246E, step runner 0x080124F8, kind table 0x080728AC)',
        ('Write ' + ', '.join(f'P{j} = p{j}' for j in range(nm)) + ' into the CURRENT parameter block (visible to the '
         'caller afterwards), then ' if nm else '') +
        'run resource-table entry `graph` as a full event graph (all node kinds: bytecode, dialog, choice, goto) '
        'with the current parameter block, inside a modal game loop; returns when the graph reaches an END step. '
        'The END value is discarded. Broadcasts world message 0x0F at start and 0x10 at end; nesting count at '
        '0x03002A5C, cancel flag 0x03002A58.', V,
        'emulator: run_scene2(516,1462,1463) shows the "Chapter 7 / Majin Buu" title card and returns after 466 '
        'frames; run_scene(37) shows the kind-5 dialog "The game will not run on this hardware."')
nat(0xBA, 2, 1, ['first_flag', 'count'], 'get_flag_bits', 'flag', 'query', False, '0x08004364 per bit',
    'Push the integer whose bit i = story flag (first_flag + i), i < count.', V,
    'emulator: set_flag_bits(293,13,1234) then get_flag_bits(293,13) = 1234; used for record times (#164)')
nat(0xBB, 3, 0, ['first_flag', 'count', 'value'], 'set_flag_bits', 'flag', 'immediate', False,
    '0x08004380 / 0x08004396 per bit',
    'value clamped to 0..2^count-1; story flag (first_flag + i) = bit i of value.', V,
    'emulator: set_flag_bits(293,13,1234) wrote GD+0x79/0x7A')
nat(0xBC, 0, 0, [], 'show_stopwatch_hud', 'screen', 'immediate', False, '0x0802FCA0(0, 1) + world_add',
    'Create the timer HUD entity in stopwatch mode (shows GD+0x4D8 counting up).', V,
    'emulator: with start_stopwatch the HUD counts 00:00 -> 03:01')
nat(0xBD, 0, 0, [], 'hide_timer_hud', 'screen', 'immediate', False, 'broadcast(world, 0x1C, 0)',
    'Same handler body as 0xB1 (separate table entry).', V, 'asm identical to 0xB1')
nat(0xBE, 0, 0, [], 'reset_stopwatch', 'screen', 'immediate', False, 'GD+0x4D8 = 0', 'Stopwatch frames = 0.', V,
    'emulator')
nat(0xBF, 0, 0, [], 'start_stopwatch', 'screen', 'immediate', False, 'GD+1 |= 0x10',
    'Stopwatch running (game loop increments GD+0x4D8 each frame).', V,
    'emulator: counting only starts after this native')
nat(0xC0, 0, 0, [], 'stop_stopwatch', 'screen', 'immediate', False, 'GD+1 &= ~0x10', 'Stop the stopwatch.', I,
    'asm; #164-#167 call it before reading the time')
nat(0xC1, 0, 1, [], 'get_stopwatch', 'screen', 'query', False, 'GD+0x4D8', 'Push stopwatch frames.', V,
    'asm + #164 stores it into record-time flag bits')
nat(0xC2, 1, 0, ['amount'], 'give_xp', 'party', 'immediate', 'conditional', '0x08013914(rec(cur), amount)',
    'Give XP to the current character (same routine as kill XP); level-ups run their popups/levelling inline and '
    'block the script while shown.', V,
    'emulator: give_xp(1000) blocked (level-up), HP/EP/XP of rec(cur) changed')
nat(0xC3, 0, 1, [], 'get_xp', 'party', 'query', False, 'rec(cur)+0x24', 'Push XP of the current character.', V,
    'emulator: 2200 in buu_snakeway_start')
nat(0xC4, 0, 0, [], 'set_game_bit_20', 'screen', 'immediate', False, 'GD+1 |= 0x20',
    'Set GameData byte 1 bit 5 (set together with the stopwatch in the race script #163; cleared by 0xC5).', I, 'asm')
nat(0xC5, 0, 0, [], 'clear_game_bit_20', 'screen', 'immediate', False, 'GD+1 &= ~0x20',
    'Clear GameData byte 1 bit 5.', I, 'asm')
nat(0xC6, 0, 0, [], 'run_power_struggle', 'battle', 'modal', True, 'run_mode(vtable 0x0804B5D8)',
    'Run the button-mash power-struggle gauge mini-game (bar at the bottom of the screen) as a modal mode; used in '
    'the Spirit Bomb vs Kid Buu finale (#463/#464).', V, 'emulator: gauge bar appears, script stays blocked')
nat(0xC7, 1, 1, ['mode'], 'run_punch_minigame', 'battle', 'modal', True, '0x08030384(0, mode) + run_mode',
    'Run the "Press A Button To Punch" gauge mini-game as a modal mode; push (mode result - 1).', V,
    'emulator: run_punch_minigame(1) shows the prompt and returns after 93 frames of mashing A')
nat(0xC8, 0, 1, [], 'get_level', 'party', 'query', False, '0x080135D2(rec(cur))',
    'Push the level (50..200) of the current character from its XP.', V, 'emulator: 60')
nat(0xC9, 0, 0, [], 'broadcast_1f', 'system', 'immediate', False, 'broadcast(world, 0x1F, 0)',
    'Broadcast world message 0x1F (receiver not identified).', I, 'asm; no uses')
nat(0xCA, 0, 0, [], 'heal_party', 'party', 'immediate', False, 'rec(0..8): HP = maxHP, EP = maxEP',
    'For all 9 character records: HP = maxHP, EP = maxEP; if rec+0x13 bit 0, also 0x08013C66 (refresh the live '
    'player).', I, 'asm (no change on an already full party in the emulator)')
nat(0xCB, 3, 1, ['actor', 'w', 'h'], 'screen_rect_x_at_actor', 'camera', 'query', False,
    '0x080001C6, 0x0800FDFA, 0x08009816(camera)',
    'Centre a w x h rectangle on the actor\'s bounding-box centre, clamp it to the camera/map bounds; push its '
    'left x (used to place fusion-dance / cutscene props on screen).', I, 'asm; #13 fusion dance')
nat(0xCC, 3, 1, ['actor', 'w', 'h'], 'screen_rect_y_at_actor', 'camera', 'query', False,
    '0x080001C6, 0x0800FDFA, 0x08009816(camera)', 'As 0xCB but pushes the rectangle\'s top y.', I, 'asm')
nat(0xCD, 4, 0, ['category', 'item', 'price', 'flag'], 'shop_add_item', 'shop', 'immediate', False,
    '0x08031808 (list 0x03003174)',
    'Append a shop entry {u8 category (0 supply, 1 equipment), u8 flag, u16 item, u32 price} to the shop list '
    '(count at 0x03003174, 8-byte entries) consumed by the buy screen 0xCE.', V,
    'shop graphs #28.. build the list (prices scaled by P0 percent) then call 0xCE; buy screen verified')
nat(0xCE, 0, 0, [], 'open_shop_buy', 'shop', 'modal', True,
    'run_mode(0x0805058C fade) ; 0x080315B0 buy screen ; fades',
    'Fade out, run the shop BUY screen over the list from 0xCD (modal), restore the world (0x08011DB4/0x08011E0E, '
    'camera 0x0800938C), fade in. Blocks until the player leaves the shop.', V,
    'emulator: "Buy x1 / Cancel / ZENIE" screen')
nat(0xCF, 1, 0, ['price_percent'], 'open_shop_sell', 'shop', 'modal', True, '0x08031E84(0, price_percent)',
    'As 0xCE but the SELL screen; items sell for price*price_percent/100 (shops pass 50).', V,
    'emulator: "Sell x1 / Cancel" screen with RqLvl column')
nat(0xD0, 1, 0, ['form'], 'set_form', 'party', 'immediate', False, '0x0801712C, 0x08010692, 0x0801711C',
    'Transform the current character: leave the old form (0x0801712C(form def)), rec(cur)+0x14 = form, refresh '
    'the player entity (0x08010692), enter the new form (0x0801711C).', V,
    'emulator: set_form(1) changed GD+0x50C 2 -> 1')
nat(0xD1, 0, 0, [], 'run_credits', 'system', 'modal', True, '0x080335BC(0, 1) + run_mode',
    'Run the staff-credits sequence (Webfoot Technologies, Jeffrey Lim, ...) as a modal mode.', V,
    'emulator: credits roll')
nat(0xD2, 0, 1, [], 'a_pressed', 'system', 'query', False, '[0x03003300]+4 bit 0',
    'Push 1 if the A button was newly pressed this frame (input manager 0x03003300: +4 = keys pressed this '
    'frame, +6 = released).', V, 'emulator: input halfwords observed')
nat(0xD3, 1, 1, ['char'], 'form_name_ptr', 'party', 'query', False, 'string table[form_def(rec(char)+0x14)+2]',
    'Push a POINTER to the UTF-16 name of the character\'s current form (for %s in formatted dialog).', V,
    'emulator: form_name_ptr(0) pushed 0x0805A8F8 (a ROM string pointer)')
nat(0xD4, 0, 0, [], 'sleep_mode', 'system', 'modal', True, '0x0803FA38',
    'GBA sleep: disable interrupts except keypad, SWI 3 (Stop) until the wake key combo, then wait for release.',
    I, 'asm: writes IE/KEYCNT 0xC304 then svc #3')
nat(0xD5, 1, 1, ['item'], 'is_equipped', 'equip', 'query', False, 'rec(cur)+0x3C+4*slot(item)',
    'Push 1 if equipment `item` is in its slot (equipment table 0x086F82EC + 0x2C*item + 0x14 = slot) on the '
    'current character.', V, 'notes §6 (VERIFIED equipment/supply behaviour)')
nat(0xD6, 1, 0, ['script'], 'run_script', 'script', 'immediate', 'conditional', '0x08004098',
    'Synchronously run ONLY node 0 of resource entry `script` (must be kind 0 or 4, else assertion) with the '
    'CURRENT parameter block (shared P0..P3) and shared globals G0..G3; its return value is discarded. Blocks '
    'only if the callee blocks.', V, 'notes §7 loot (VERIFIED: 60/60 kills reproduced)')
nat(0xD7, 2, 0, ['script', 'p0'], 'run_script1', 'script', 'immediate', 'conditional', '0x08004098',
    'P0 = p0 in the CURRENT parameter block, then run_script(script).', V, 'loot #497 -> #496/#499/#500')
nat(0xD8, 3, 0, ['script', 'p0', 'p1'], 'run_script2', 'script', 'immediate', 'conditional', '0x08004098',
    'P0 = p0, P1 = p1 in the CURRENT parameter block, then run_script(script).', V, 'loot (enemy drop entry)')
nat(0xD9, 1, 0, ['set'], 'show_destination_label', 'screen', 'immediate', False, '0x0803643C(0, set) + world_add',
    'Create a destination-label HUD entity (vtable 0x0804BAF4) for label set `set` (table 0x0875A73C+8); its 32 '
    'entries are filled by 0xDA.', I, 'asm; #511 (travel menu: HFIL, Heaven, Earth, Grand Kai\'s Planet, ...)')
nat(0xDA, 2, 0, ['index', 'string_id'], 'set_destination_label', 'screen', 'immediate', False,
    'broadcast(world, 0x23, {index, string_id})',
    'Broadcast message 0x23: the label entity stores string_id at entry index and redraws if it is the current '
    'one.', I, 'asm of handler 0x08036915; #511')
nat(0xDB, 1, 0, ['enable'], 'set_area_flag_c', 'map', 'immediate', False, '[0x0300197C+0xC] = enable != 0',
    'Set the area manager\'s +0xC flag (meaning not identified).', I, 'asm; no uses')
nat(0xDC, 2, 0, ['id', 'value'], 'set_entity_value', 'system', 'immediate', False, '0x08011C70(world, 8, id)',
    'Find the first world entity that answers message 8 with `id`; set its +0x18 = value and +4 = +0x30 + value.',
    I, 'asm; no uses')
nat(0xDD, 0, 1, [], 'party_xp_total', 'party', 'query', False, 'sum rec(0..4)+0x24',
    'Push the summed XP of character records 0..4.', V, 'emulator: 85400; used by the ending/credits #312')
nat(0xDE, 1, 1, ['string_id'], 'string_ptr', 'query', 'query', False, 'string table 0x0806BCE8',
    'Push a POINTER to string table entry string_id (for %s formatting).', V,
    'emulator: string_ptr(541) pushed the pointer of "Earth"')
EQ = [
    (0xDF, 1, ['n'], 'eq_add_hp_regen', 'eqacc+0x10 (u16) += n*0x445 >> 8', 'HP regeneration per frame /256'),
    (0xE0, 1, ['n'], 'eq_add_thorns', 'eqacc+0x08 (u16) += n*0xA4 >> 6', 'thorns damage'),
    (0xE1, 1, ['n'], 'eq_add_end', 'eqacc+0x1C += n; eqacc+0x20 += n', 'END (melee) and END (energy)'),
    (0xE2, 1, ['n'], 'eq_add_pow', 'eqacc+0x18 += n', 'POW'),
    (0xE3, 1, ['n'], 'eq_add_spd', 'eqacc+0x24 += n', 'SPD'),
    (0xE4, 1, ['n'], 'eq_add_str', 'eqacc+0x14 += n', 'STR'),
    (0xE5, 0, [], 'eq_halve_block_damage', 'eqacc+0x0C (u16) /= 2', 'damage taken while blocking'),
    (0xE6, 0, [], 'eq_halve_energy_cost', 'eqacc+0x01 (u8) /= 2', 'EP cost of energy attacks'),
    (0xE7, 0, [], 'eq_halve_energy_block_cost', 'eqacc+0x02 (u8) /= 2', 'EP cost of blocking energy'),
    (0xE8, 0, [], 'eq_scale_ss_cooldown', 'eqacc+0x0E (u16) = x*7 >> 3', 'Super Saiyan drain/cooldown rate'),
]
for id, pops, args, name, op, what in EQ:
    nat(id, pops, 0, args, name, 'equip', 'immediate', False, op,
        f'Equipment-effect accumulator (0x03002B5C, filled while an equipment effect script runs during stat '
        f'recalculation): {op} ({what}).', V,
        'items.json equipment effect scripts decoded and checked in-game (notes §6)')
nat(0xE9, 3, 0, ['char', 'skill', 'level'], 'set_skill_level', 'party', 'immediate', False,
    '0x08013B54(rec(char), skill, level)',
    'Find `skill` in the character\'s known-skill list (rec+0x0D.., count rec+0x0C) and set both level bytes '
    '(rec+0x2E+2i, +0x2F+2i) = level; asserts (0x0803DF14) if the skill is not known.', V,
    'notes §3 ("skill levels are set by story scripts (native -> 0x08013B54)"); #3 graphs print "Goku learned '
    'Kamehameha Level 2!"')
nat(0xEA, 2, 1, ['char', 'skill'], 'get_skill_level', 'party', 'query', False, '0x08013BCC(rec(char), skill)',
    'Push the level byte (rec+0x2F+2i) of a known skill; asserts and pushes 0 if unknown.', I, 'asm; #3')
nat(0xEB, 0, 0, [], 'set_game_bit_40', 'system', 'immediate', False, 'GD+1 |= 0x40',
    'Set GameData byte 1 bit 6.', V, 'emulator: GD+1 0x02 -> 0x42; meaning not identified; no uses')
nat(0xEC, 3, 0, ['x', 'y', 'highlight'], 'add_map_marker', 'map', 'immediate', False,
    '0x08026DE4(0, {x-4, y-4}, highlight != 0) + world_add',
    'Create a small blinking marker entity (vtable 0x0804D634) centred at (x, y); highlight selects the '
    'current-location style. Used by the world-map screen script #15 (one marker per unlocked area).', V,
    'emulator: red/orange blinking dots at the given points')
nat(0xED, 1, 1, ['char'], 'get_stat_points', 'party', 'query', False, 'rec(char)+0x2C',
    'Push unspent stat points.', V, 'emulator: add_stat_points then get_stat_points = 5')
nat(0xEE, 2, 0, ['char', 'n'], 'add_stat_points', 'party', 'immediate', False, 'rec(char)+0x2C += n (u16)',
    'Give unspent stat points.', V, 'emulator: GD+0x524 0 -> 5')
nat(0xEF, 0, 0, [], 'hide_title_text', 'screen', 'modal', True,
    'broadcast(world, 0x27, 0); run_mode(vtable 0x08049978, 1)',
    'Broadcast message 0x27 (title-text entities remove themselves) and run a 1-frame modal wait so the removal '
    'is drawn before the script continues.', V, 'emulator: returned after 1 frame; #289-#298 tournament banners')
nat(0xF0, 2, 0, ['actor', 'value'], 'set_actor_field_1a8', 'actor', 'immediate', False,
    '0x080001C6(actor)+0x1A8 = value << 10', 'Write actor object +0x1A8 = value<<10 (field not identified).', I,
    'asm; no uses')
nat(0xF1, 2, 0, ['char', 'slot'], 'unequip_slot', 'equip', 'immediate', False, '0x08004544(GD, char, slot)',
    'If the slot does not hold its default item (0x080555B8[slot]), move the equipped item back to the inventory '
    '(GD+0xE4 list, entries: top nibble = owner+1, low 28 bits = equipment id) and put the default back.', I,
    'asm; emulator no-op on default slots; #362')
nat(0xF2, 5, 0, ['seconds', 'str', 'pow', 'end', 'spd'], 'boost_stats', 'party', 'immediate', False,
    'seconds == 0: rec(cur)+0x28..0x2B += ; else GD+0x4DC.. ; 0x0801064C',
    'seconds == 0: permanently add to the current character\'s base STR/POW/END/SPD (u8). Otherwise timed buff: '
    'GD+0x4DC = seconds*60 frames, GD+0x4E0/4E4/4E8/4EC = str/pow/end/spd, GD+0x4F0 = 0 (applied by 0x080046AE '
    'during stat recalculation); then refresh the player (0x0801064C).', V,
    'emulator: boost_stats(30,25,25,25,25) = Holy Water (supply #20): GD+0x4DC = 1799 after 1 frame, effective '
    'stats +25; boost_stats(0,1,2,3,4) raised base stats')
nat(0xF3, 2, 0, ['char', 'level'], 'level_up_to', 'party', 'immediate', False,
    '0x080135D2, 0x08013678, 0x080135BC',
    'level clamped to 200; if the character\'s level is lower: apply level-up growth from current to target '
    '(0x08013678) and set XP to the target level threshold (0x080135BC).', I, 'asm; #455')
nat(0xF4, 4, 0, ['x', 'y', 'radius', 'param'], 'show_energy_sphere', 'screen', 'immediate', False,
    '0x0803A84A(0, 0, {x, y}, radius, param, 1) -> BG effect slot 0 (0x0800927E)',
    'Create the large BG-layer sphere effect (vtable 0x080493B4, 0x830 bytes) at (x, y) with radius and a '
    'parameter, replacing BG-effect slot 0. Used by the Spirit Bomb finale (#461-#463, #480).', V,
    'emulator: show_energy_sphere(144,100,48,80) draws a large pale sphere on a BG layer')
nat(0xF5, 1, 0, ['n'], 'eq_add_crit', 'equip', 'immediate', False, 'eqacc+0x00 (u8) += n',
    'Equipment-effect accumulator: critical-hit chance percent += n.', V, 'items.json (Charge Gloves: +5%)')
nat(0xF6, 0, 1, [], 'is_tampered', 'system', 'query', False, '[0x03001004]',
    'Push the anti-tamper flag set by 0xAB (ROM header checksum mismatch). Script #101 then runs the dialog '
    'graph #37 "The game will not run on this hardware."', V, 'emulator: 0 on the genuine ROM')
nat(0xF7, 1, 0, ['seconds'], 'start_timed_special', 'party', 'immediate', False,
    'GD+0x4DC = seconds*60; GD+0x4E0..0x4EC = 0; GD+0x4F0 = 1; 0x0801064C',
    'Timed buff with no stat bonuses but GD+0x4F0 = 1, which stat recalculation (0x080046AE) copies into the '
    'record byte +0x67 (special flag; effect not identified).', V,
    'emulator: start_timed_special(10) -> GD+0x4DC = 599, GD+0x4F0 = 1, rec0+0x67 = 1; used by graph #0')

assert [n['id'] for n in N] == list(range(0xA8, 0xF8)), [hex(n['id']) for n in N]
ROOT_ENGINE.mkdir(parents=True, exist_ok=True)
(ROOT_ENGINE / 'script_natives_c.json').write_text(json.dumps(N, indent=1) + '\n')
print('natives', len(N))

# ---------------------------------------------------------------- resource kinds
from script_nat_c_graph import walk, TABLE, N as NTAB
from collections import Counter
node0 = Counter()
reach = Counter()
listed = Counter()
for i in range(NTAB):
    h = u32(TABLE + 4 * i)
    node0[u8(u32(h) + 2)] += 1
    for s, w in walk(h).items():
        if w >= 0x10000 and 0x08000000 <= w < 0x08800000:
            (reach if s >= 0 else listed)[u8(w + 2)] += 1
H = {k: f'0x{u32(0x080728AC + 4 * k) & ~1:08X}' for k in range(8)}
kinds = {
    'table': '0x0879B704 (522 u32 pointers to graph headers; the only code references are natives 0xB5..0xB9 and '
             '0xD6..0xD8)',
    'model': 'Each entry is an EVENT GRAPH, not a single script. Header = array of u32 step words, indexed by step '
             'number (step 0 = entry). A step word < 0x10000 is END: the graph finishes with that value (written '
             'to the runner\'s result halfword unless cancelled via 0x03002A58). Otherwise it points to a node blob '
             '{u16 next_step (LE, bytes 0-1), u8 kind (byte 2), u8 arg (byte 3), payload}. The step runner '
             '0x080124F8 (thread struct 0x2C bytes from 0x0801246E: +0 header, +4 &result, +8 u16 current step, '
             '+0x14 params, +0x18 own params, +0x28 active node object) sets step = next_step, builds the node '
             'object via handler table 0x080728AC[kind], calls its start (vtable+4) and then polls vtable+8 every '
             'frame until done. VERIFIED: emulator trace of run_scene natives hits handler kind 5 for #37 and '
             'kind 0 for #516 from 0x08012563.',
    'branching': 'Bytecode nodes (kind 0/4) override next_step with their return value: if the interpreter stack '
                 'holds exactly one value at `end`, current step = that value (0x08011F90..0x08011F9A). '
                 'Choice nodes (kind 3) store the selected option index in 0x03002A60 (read by native 0x18); the '
                 'following bytecode node branches on it. VERIFIED by reading graph #28 (Z-Mart shop): choice -> '
                 'native 0x18 == 0 -> return 4.',
    'synchronous_runner': '0x08004098 (natives 0xD6..0xD8, item/equipment/loot callers) runs ONLY node 0, which '
                          'must be kind 0 or 4 (byte 2 checked; anything else = assertion 0x0803DF14).',
    'node_stats': {'entries_by_node0_kind': {str(k): v for k, v in sorted(node0.items())},
                   'nodes_reached_via_next_by_kind': {str(k): v for k, v in sorted(reach.items())},
                   'other_header_nodes_by_kind (reached only through bytecode return values / choices; '
                   'heuristic header scan)': {str(k): v for k, v in sorted(listed.items())}},
    'kinds': [
        {'kind': 0, 'name': 'bytecode', 'handler': H[0], 'payload': 'raw bytecode at blob+3 (byte 3 is the first '
         'opcode, not an arg)', 'consumer': '0x08011F5A: runs the interpreter 0x08004038 inside a world entity '
         '(vtable 0x08049E84) with the thread\'s params; natives may block (nested loops); return value -> next '
         'step', 'status': V},
        {'kind': 1, 'name': 'dialog_inline', 'handler': H[1], 'payload': 'UTF-16 text inline at blob+4; arg = '
         'speaker portrait', 'consumer': '0x08012012 text box (vtable 0x08049E90)', 'status': I,
         'note': 'no reachable node in the table'},
        {'kind': 2, 'name': 'dialog_inline_format', 'handler': H[2], 'payload': 'u32 pointer to a UTF-16 printf '
         'format at blob+4, bytecode at blob+8 computing the arguments (interpreter stack = argument list, %s '
         'takes a string POINTER, %d an int), formatted by 0x0803DB7C; arg = portrait', 'consumer': '0x080121D8 '
         '-> 0x08012012', 'status': I, 'note': 'no reachable node in the table'},
        {'kind': 3, 'name': 'choice', 'handler': H[3], 'payload': 'UTF-16 text inline at blob+4: prompt, blank '
         'line, then "-Option" lines', 'consumer': '0x0801236E -> 0x08030C16 choice-box entity (vtable '
         '0x0804A7C4, added to the world); selection index -> 0x03002A60', 'status': V,
         'example': 'graph #28 step 1: "Welcome to the Hercule City Z-Mart! How can I help you?\\n\\n-Buy\\n-Sell'
                    '\\n\\n-End"'},
        {'kind': 4, 'name': 'bytecode_lz', 'handler': H[4], 'payload': 'Webfoot-LZ bitstream at blob+4 WITHOUT the '
         '8-byte {type,size} container (byte 3 = 0); inflated by 0x08049120 into a 0x800-byte buffer. Decodes '
         'with engine/crates/wf-rom/src/lz.rs as if it were a type-1 container stream (initial literal width 1; '
         'stops at the end code) - VERIFIED equal to the game\'s output for graphs #2 #3 #6 #13 #450. NOTE: '
         'Rom::lz(blob+4) is wrong here (it would parse the first 8 stream bytes as a container header).',
         'consumer':
         '0x08011FD6 -> 0x08011F5A (as kind 0)', 'status': V},
        {'kind': 5, 'name': 'dialog', 'handler': H[5], 'payload': 'u32 text ref at blob+4: bank = ref & 0x3F '
         '(container pointer table 0x087B5B64; 7 LZ banks), start = ((ref >> 6) & 0x7FFF) * 2 bytes, length = '
         'ref >> 21 UTF-16 units (no terminator in the bank); arg = speaker portrait id (0x08011F20: 0 or 7 = none, '
         '1..6 = expression of the current character\'s form (form def +0x0F+id), >= 8 = portrait table '
         '0x086F393C index)', 'consumer': '0x0801227E -> 0x08012230 (inflate bank, copy) -> 0x08012012 text box',
         'status': V, 'example': 'graph #37: ref 0x04EECA00 -> "The game will not run on this hardware." '
                                 '(shown in the emulator)'},
        {'kind': 6, 'name': 'dialog_format', 'handler': H[6], 'payload': 'u32 text ref at blob+4 (as kind 5) = '
         'printf format; bytecode at blob+8 computes the arguments', 'consumer': '0x08012334 -> 0x080122B4 -> '
         '0x08012012', 'status': V, 'example': 'graph #3 step 1: "%s has reached level %d!" with args '
                                                '(form_name native 0x16, get_level native 0xC8)'},
        {'kind': 7, 'name': 'goto_graph', 'handler': H[7], 'payload': 'u32 pointer to another graph header at '
         'blob+4', 'consumer': 'thread.header = that header, step = 0 (tail call); trivial node object vtable '
         '0x0804AEB0', 'status': I},
    ],
    'text_format': 'UTF-16LE; "\\n" = line break; {xxxx} = control codes (see strtab.py); printf %s/%d in kinds '
                   '2/6.',
}
(ROOT_ENGINE / 'script_resource_kinds.json').write_text(json.dumps(kinds, indent=1) + '\n')
print('kinds written', dict(node0), dict(reach), dict(listed))
