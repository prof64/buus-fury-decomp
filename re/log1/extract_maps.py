"""Extract map records (0x081CA09C) and their spawn lists -> data/log1/maps.json.

Map record (0x34 bytes, read by load_map 0x08001FA8):
  +0  u16 width_screens, +2 u16 height_screens (informational), +4 u16 width_tiles, +6 u16 height_tiles (8px tiles)
  +8..+0x10  tile/graphics pointers, +0x14 -> map content record, +0x18.. more gfx/collision pointers
Map content record (read by 0x08001FA8):
  +0x00 u8 music index (0xFF = keep), +0x08 actor count, +0x0C -> actors[12 bytes] (0x080058D6)
  +0x10 pickup count, +0x14 -> pickups[8 bytes] (0x0800D1A0)
  +0x18 trigger/script count, +0x1C -> script region list
  +0x20 breakable-obstacle count, +0x24 -> obstacles[8 bytes] (0x0800697E)
  +0x28 default entrance (u16 x, u16 y in pixels)
Actor (12 bytes): +0 u16 x, +2 u16 y (pixels), +4 u8 sprite id, +5 u8 spawn condition
  (0 always, 1 only if story flag CLEAR, 2 only if story flag SET), +6 u16 condition index
  (-> bit number via u8 table 0x08543988, bitfield at 0x03000038), +8 u8 kind (0 talkable NPC,
  1 NPC variant, 2 enemy -> 0x0800573A), +9 u8 dialogue/script id, +0xA u8 extra.
Pickup (8 bytes): +0 u16 x, +2 u16 y, +4 u32 item id.
Obstacle (8 bytes): +0 u16 type (0..3, table 0x0815B948), +2 u16 value (unused by spawn code), +4 x, +6 y.
"""
from romdata import *

FLAG_INDEX_TABLE = 0x08543988
VISUAL = {0: 'Kame House island', 1: 'Kame House interior', 2: "Mt. Paozu (Goku's house)", 3: 'Snake Way head (cutscene)',
          4: 'forest', 5: 'forest/ruins', 6: 'cave (dinosaur skeleton)', 9: 'cliff valley', 10: "King Yemma's office",
          11: 'Snake Way', 12: 'rocky plateau', 13: 'Snake Way (King Kai path)', 16: 'snow forest', 17: 'city outskirts',
          22: 'city interior', 23: 'city/road', 24: 'canyon', 28: 'Namek', 29: 'Namek', 30: 'Namek (lava ruins)',
          31: 'Namek', 34: 'Namek', 35: "Frieza's ship interior?", 40: 'Namek', 41: 'Namek'}


def main():
    names = sprite_names()
    maps = []
    for m in range(N_MAPS):
        rec, c = map_record(m)
        info = dict(index=m, record_addr=h(rec), content_addr=h(c), description=VISUAL.get(m, ''),
                    size_tiles=dict(w=u16(rec + 4), h=u16(rec + 6)), size_screens=dict(w=u16(rec), h=u16(rec + 2)),
                    music=u8(c), default_entrance=dict(x=u16(c + 0x28), y=u16(c + 0x2A)))
        actors = []
        for i in range(u32(c + 8)):
            a = u32(c + 0xC) + 12 * i
            spr = u8(a + 4)
            cond = u8(a + 5)
            ci = u16(a + 6)
            act = dict(x=u16(a), y=u16(a + 2), sprite_id=spr, sprite=names[spr], kind=u8(a + 8),
                       kind_name={0: 'npc', 1: 'npc2', 2: 'enemy'}.get(u8(a + 8), '?'),
                       spawn_condition={0: 'always', 1: 'flag_clear', 2: 'flag_set'}.get(cond, cond),
                       script_id=u8(a + 9), extra=u8(a + 0xA))
            if cond in (1, 2):
                act['flag_bit'] = u8(FLAG_INDEX_TABLE + ci)
            if u8(a + 8) == 2:
                act['enemy_template'] = u8(SPRITE_TO_TEMPLATE + spr)
            actors.append(act)
        info['actors'] = actors
        info['pickups'] = [dict(x=u16(u32(c + 0x14) + 8 * i), y=u16(u32(c + 0x14) + 8 * i + 2),
                                item_id=u32(u32(c + 0x14) + 8 * i + 4)) for i in range(u32(c + 0x10))]
        info['obstacles'] = [dict(type=u16(u32(c + 0x24) + 8 * i), value=u16(u32(c + 0x24) + 8 * i + 2),
                                  x=u16(u32(c + 0x24) + 8 * i + 4), y=u16(u32(c + 0x24) + 8 * i + 6))
                             for i in range(u32(c + 0x20))]
        info['script_regions'] = u32(c + 0x18)
        maps.append(info)
    dump('maps.json', dict(
        source='LoG1 (USA) ROM, map table 0x081CA09C; see re/log1/extract_maps.py docstring for formats',
        coordinates='pixels in map space (map is size_tiles*8 px); entity position = top-left of 16x8 foot box',
        maps=maps))


if __name__ == '__main__':
    main()
