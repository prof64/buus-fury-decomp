"""Extract item definitions -> data/log1/items.json.

Item table 0x085C224C: 24-byte records {u16 w, u16 h, u32 flags, u32 gfx, u32 on_pickup, u32 on_use, u32 use_msg}.
Pickup entities (map content +0x14 list, 0x0800D1A0) respond to msg 0 (A pressed in front, 0x0800D03C):
  if inventory full (0x08000926) -> message, else sfx 7 and on_pickup(entity, id).
on_pickup handlers: 0x0800D1E6 = count++ at 0x0300004E+id and show message ITEM_MSG_PTRS[id];
  0x0800D1CE = message only (no inventory); 0x0800D20A = immediate flight-energy refill (ids 31/32/33).
on_use handlers (inventory menu): 0x08001708 = not usable (returns 0); 0x0800170C = +maxHP/4 HP (fails at full HP);
  0x08001730 = full heal (fails at full HP). HP set through 0x0800324E (clamps to max).
Limits (0x08000926): herbs (ids 11..13) total count < 6 to pick up more; Senzu (id 19) count < 3.
"""
from romdata import *

ON_PICKUP = {0x0800D1E7: 'add to inventory + message', 0x0800D1CF: 'message only (not kept)',
             0x0800D20B: 'instant flight-energy refill'}
ON_USE = {0x08001709: 'none (key/quest item)', 0x0800170D: 'heal maxHP/4 (not at full HP)',
          0x08001731: 'full heal (not at full HP)'}
FLIGHT_REFILL = {31: 5, 32: 10, 33: 20}
KIND = {range(0, 5): 'flower (quest)', range(5, 11): 'stone (quest)', range(11, 14): 'herbs (healing)',
        range(14, 18): 'sapling (quest)', range(18, 19): 'fruit', range(19, 20): 'Senzu bean (healing)',
        range(20, 21): 'repair kit (quest)', range(21, 23): 'capsule (quest)', range(23, 26): 'magical artifact (quest)',
        range(26, 27): 'Pterodactyl egg (quest)', range(27, 28): 'boat (quest)', range(28, 31): 'magazine (quest)',
        range(31, 34): 'feather (flight refill)'}


def kind(i):
    for r, k in KIND.items():
        if i in r:
            return k
    return '?'


def main():
    items = []
    for i in range(34):
        a = ITEM_TABLE + 24 * i
        pick, use, msg = u32(a + 12), u32(a + 16), u32(a + 20)
        mp = u32(ITEM_MSG_PTRS + 4 * i)
        it = dict(id=i, rom_addr=h(a), kind=kind(i), size_px=dict(w=u16(a), h=u16(a + 2)), flags=h(u32(a + 4)),
                  graphics_ptr=h(u32(a + 8)), on_pickup=h(pick), on_pickup_desc=ON_PICKUP.get(pick, '?'),
                  on_use=h(use), on_use_desc=ON_USE.get(use, '?'),
                  pickup_message=cstr(mp) if 0x08000000 <= mp < 0x08800000 else None,
                  use_message=cstr(msg).strip() if msg else None,
                  inventory_count_addr=h(0x0300004E + i) if pick == 0x0800D1E7 else None)
        if i in FLIGHT_REFILL:
            it['flight_energy_restored'] = FLIGHT_REFILL[i]
        if use == 0x0800170D:
            it['heal'] = 'maxHP // 4'
        if use == 0x08001731:
            it['heal'] = 'full'
        if 11 <= i <= 13:
            it['carry_limit'] = 'herbs 11+12+13 combined: pickup refused when total >= 6'
        if i == 19:
            it['carry_limit'] = 3
        items.append(it)
    dump('items.json', dict(
        source='LoG1 (USA) item table 0x085C224C, messages 0x085C2FB8 (re/log1/extract_items.py)',
        notes=['Enemies never drop items; all items are fixed map pickups (maps.json pickups) or script rewards.',
               'Item names are not stored as text; "kind" is derived from the pickup message.',
               'VERIFIED in emulator: feather (31) refilled flight energy 1 -> 6 (cap = max).'],
        items=items))


if __name__ == '__main__':
    main()
