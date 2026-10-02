"""Build engine/annotations/script_natives_a.json (natives 0x00..0x57) and engine/annotations/script_setters.json.

Stack effects were read from the handler asm and cross-checked against emulator measurements
(script_nat_a_arity.py -> work/buu/script/nat_arity_00_58.json: push 1001..1008; native n; end).
`uses` = occurrences in reachable code of the 377 table scripts (blob kind 0 / 4), script_nat_a_ctx.py."""
import json
from common import *

V, I = 'VERIFIED', 'INFERRED'
GD = '0x03001068 (GameData)'
REC = 'active character record GameData+0x4F8+0x68*GameData[0]'
ENQ_NOTE = ('Enqueues a command object on the actor\'s cutscene track (actor+0x18C list, director 0x03002BAC via '
            '0x0801936A). Outside begin_parallel..end_parallel the native blocks (runs the wait mode 0x0804C0C4 '
            'through run_mode 0x0803D8A0) until every track is empty; inside a batch it returns immediately. '
            'If the actor id resolves to no object the command is destroyed without effect.')

N = []


def n(id, handler, pops, pushes, args, name, cat, kind, blocking, fn, sem, status, ev):
    N.append(dict(id=id, handler=handler, pops=pops, pushes=pushes, args=args, name=name, category=cat, kind=kind,
                  blocking=blocking, engine_fn=fn, semantics=sem, status=status, evidence=ev))


for k in range(4):
    n(k, hex(u32(0x08055098 + 4 * k)), 0, 1, [], f'get_g{k}', 'var', 'query', False, f'reads word 0x{0x03001058 + 4 * k:08X}',
      f'Push global G{k} (word at 0x{0x03001058 + 4 * k:08X}). G0..G3 are shared by all scripts, including nested run_script calls; '
      f'write with setter 0x{k:02X}.', V, 'emulator: set 0x00..0x03 then native 0x00..0x03 round-trips; RAM 0x03001058 updated')
for k in range(4):
    n(4 + k, hex(u32(0x08055098 + 4 * (4 + k))), 0, 1, [], f'get_p{k}', 'var', 'query', False, f'reads [[0x03001030]+{4 * k}]',
      f'Push parameter P{k} of the current parameter block ([0x03001030], = interp+0x5C unless the caller passed a block). '
      'Nested run_script natives pass the caller\'s block, so P0..P3 are shared with callees too.', V,
      'emulator: setters 4..7 then natives 4..7 round-trip')
for k in range(4):
    n(8 + k, hex(u32(0x08055098 + 4 * (8 + k))), 0, 1, [], f'get_l{k}', 'var', 'query', False, f'reads word 0x{0x03001020 + 4 * k:08X}',
      f'Push global L{k} (second bank of 4 globals at 0x{0x03001020 + 4 * k:08X}; not reset by the runner).', V,
      'emulator: setters 8..0xB then natives 8..0xB round-trip; RAM 0x03001020 updated')
n(0x0C, '0x08000337', 0, 1, [], 'get_hp', 'party', 'query', False, 'reads s16 rec+0x16',
  f'Push current HP of the {REC}.', V, 'emulator: equals rec+0x16 (135) in buu_snakeway_start')
n(0x0D, '0x0800035D', 0, 1, [], 'get_max_hp', 'party', 'query', False, 'reads s16 rec+0x18', 'Push max HP of the active character.', V,
  'emulator: equals rec+0x18')
n(0x0E, '0x080004BB', 0, 1, [], 'get_energy', 'party', 'query', False, 'form table 0x086B82EC (24 B) flags byte; rec+0x20/+0x1E/+0x1A',
  'Push the active character\'s energy bar: if the current form (rec+0x14) has flag bit1 -> SS3 gauge (u16 rec+0x20), '
  'else bit0 -> SS gauge (u16 rec+0x1E), else EP (s16 rec+0x1A).', V, 'emulator (base form): equals EP rec+0x1A; branches read from asm')
n(0x0F, '0x08000383', 0, 1, [], 'get_max_ep', 'party', 'query', False, 'reads s16 rec+0x1C', 'Push max EP of the active character.', V,
  'emulator: equals rec+0x1C')
n(0x10, '0x080003A9', 0, 1, [], 'get_zenie', 'item', 'query', False, 'reads u32 GameData+0x4F4 (0x0300155C)', 'Push the Zenie count.', V,
  'emulator: equals 0x0300155C')
n(0x11, '0x080003E5', 0, 1, [], 'get_variant_byte', 'system', 'query', False, 'reads u8 0x03002BB0',
  'Push byte 0x03002BB0 (also written by setter 0x11). Read by the player skill code at 0x0801C416 (selects a 0/1/other '
  'branch when firing); purpose not decoded. Unused by table scripts.', I, 'emulator round-trip of setter/getter; meaning INFERRED')
n(0x12, '0x080005DD', 0, 1, [], 'get_world', 'map', 'query', False, 'reads u8 [0x0300197C]+0 (area manager)',
  'Push the current world (area group) index.', V, 'emulator: 8 in Southwest Forest 8-3 states, 1 in Snakeway, 0 in menus state')
n(0x13, '0x080005F1', 0, 1, [], 'get_map', 'map', 'query', False, 'reads u8 [0x0300197C]+1', 'Push the current map index within the world.', V,
  'emulator: 3 in Southwest Forest 8-3, 1 in Snakeway')
n(0x14, '0x08000605', 0, 1, [], 'get_map_variant', 'map', 'query', False, 'reads u8 [0x0300197C]+2',
  'Push the area manager byte +2 (map variant / entrance index; 1 in every captured state). Unused by table scripts.', I,
  'emulator reads 1 everywhere; meaning INFERRED from the area+0x2C variant table')
n(0x15, '0x08000619', 0, 1, [], 'get_character', 'party', 'query', False, 'reads u8 GameData+0',
  'Push the active character index (0 = Goku ... 8; character record order).', V, 'emulator: 0 with Goku active')
n(0x16, '0x0800062D', 0, 1, [], 'get_form_name', 'party', 'query', False, 'form table 0x086B82EC +2 (u16 string id) -> string table 0x0806BCE8',
  'Push a POINTER to the active form\'s name string (e.g. "Goku"); used as a text argument for dialog formatting.', V,
  'emulator: pointer to string #158 "Goku"')
n(0x17, '0x08000679', 0, 1, [], 'get_form_label', 'party', 'query', False, 'form table +4 (u16 string id) -> string table',
  'Push a POINTER to the active form\'s upper-case label (e.g. "GOKU", the dialog speaker label).', V, 'emulator: pointer to string #174 "GOKU"')
n(0x18, '0x080006ED', 0, 1, [], 'get_menu_result', 'menu', 'query', False, 'reads word 0x03002A60',
  'Push the last menu/choice result (word 0x03002A60; menu code at 0x0800AED8/0x0800B342/0x08030B76 writes it, -1 = cancelled).', I,
  'xrefs of 0x03002A60 are all in menu/choice code; not exercised')
n(0x19, '0x08000727', 1, 1, ['n'], 'rand', 'rng', 'query', False, '0x0803FACC rand(n) = ((next()&0xFFFF)*n)>>16',
  'Replace n with a uniform random integer in [0, n) from the game PRNG (state 0x03004A5C).', V,
  'emulator: rand(100) in range; formula VERIFIED earlier (re/buu/notes.md, data/buu/rng.json)')
n(0x1A, '0x08000739', 1, 1, ['percent'], 'chance', 'rng', 'query', False, '0x0803FADE chance(p) = (next()&0xFFFF) < p, p = percent*0x28F5D>>8',
  'Replace percent with 1 with probability percent/100 (else 0). Consumes one PRNG step.', V, 'emulator: chance(0)=0, chance(100)=1')
n(0x1B, '0x080007B7', 1, 1, ['flag'], 'get_flag', 'flag', 'query', False, '0x08004364(GameData, flag): bit (flag&7) of GameData+0x55+(flag>>3)',
  'Replace flag id with 1 if the story flag is set, else 0.', V, 'emulator: set_flag(300) then get_flag(300)=1, RAM bit set')
n(0x1C, '0x080007CD', 1, 1, ['flag'], 'not_flag', 'flag', 'query', False, '1 - 0x08004364(GameData, flag)', 'Replace flag id with 1 if the story flag is CLEAR.', V,
  'emulator: after set_flag(300) not_flag(300)=0')
n(0x1D, '0x080007E7', 1, 0, ['flag'], 'set_flag', 'flag', 'immediate', False, '0x08004380: GameData+0x55+(f>>3) |= 1<<(f&7)', 'Set story flag.', V,
  'emulator: RAM bit set')
n(0x1E, '0x080007FF', 1, 0, ['flag'], 'clear_flag', 'flag', 'immediate', False, '0x08004396: bit clear', 'Clear story flag.', V,
  'emulator: get_flag after clear = 0')
n(0x1F, '0x08000817', 1, 0, ['sfx'], 'play_sfx', 'audio', 'immediate', False, 'PlaySfx 0x0803E378(voices 0x03003898+0x5C4, SFX table 0x083192A8+12*sfx, vol 0x40, pan 0)',
  'Play sound effect sfx (0..63) at full volume on the first free of 16 SFX voices (dropped if none free).', V,
  'emulator: voice entry = 0x083192A8+12*20, volume 64')
n(0x20, '0x08000841', 2, 0, ['sfx', 'volume'], 'play_sfx_volume', 'audio', 'immediate', False, 'PlaySfx 0x0803E378(..., vol=volume, pan 0)',
  'Play sound effect at volume (0..64).', V, 'emulator: voice volume 33 for play_sfx_volume(20,33)')
n(0x21, '0x08000873', 1, 0, ['sfx'], 'stop_sfx', 'audio', 'immediate', False, '0x0803E302: free the voice playing that SFX entry',
  'Stop the (most recently allocated) voice playing sfx.', V, 'emulator: voice freed')
n(0x22, '0x08000899', 0, 0, [], 'stop_all_sfx', 'audio', 'immediate', False, 'StopAllSfx 0x0803E2EC', 'Stop all 16 SFX voices.', I, 'asm (unused by scripts)')
n(0x23, '0x080008A9', 1, 0, ['song'], 'play_music', 'audio', 'immediate', False, 'PlaySong 0x0803F110(player 0x030038A4, song table 0x083BB78C+20*song)',
  'Start song (0..52) unless that song is already playing (player+5 == 1 and player+0xC == header).', V,
  'emulator: player+0xC = 0x083BB78C+20*7, playing flag 1')
n(0x24, '0x080008FD', 0, 0, [], 'stop_music', 'audio', 'immediate', False, 'player 0x030038A4 +5 = 0', 'Stop the music sequencer.', V,
  'emulator: player+5 = 0')
n(0x25, '0x08000907', 1, 0, ['layer'], 'show_bg', 'screen', 'immediate', False, '0x080096AC: DISPCNT |= 0x100<<layer (shadow scene 0x030019A4+0xA2)',
  'Show background layer 0..3.', V, 'emulator: DISPCNT 0x1740 -> 0x1F40 for layer 3')
n(0x26, '0x0800091F', 1, 0, ['layer'], 'hide_bg', 'screen', 'immediate', False, '0x08009692: DISPCNT &= ~(0x100<<layer)', 'Hide background layer 0..3.', V,
  'emulator: DISPCNT bit 11 cleared for layer 3')
n(0x27, '0x08000937', 2, 0, ['layer', 'priority'], 'set_bg_priority', 'screen', 'immediate', False,
  '0x080096C4: scene+0xB4+4*layer = priority; BGxCNT = u16 0x080716D8[mode][layer] + priority',
  'Set a background layer\'s priority (BGxCNT low bits).', V, 'emulator: BG1CNT = 0x1D82 for (1,2)')
n(0x28, '0x0800097B', 2, 0, ['layer', 'mask'], 'set_blend_targets', 'screen', 'immediate', False,
  '0x080096EA: BLDCNT bit (1<<layer) = mask&1 (1st target), bit (0x100<<layer) = mask&2 (2nd target)',
  'Set whether a BG layer is a first/second alpha-blend target.', V, 'emulator: BLDCNT changed accordingly')
n(0x29, '0x08003097', 3, 0, ['slot', 'a', 'b'], 'entity_call', 'system', 'immediate', 'conditional',
  'obj = word[[0x08054FBC+0x10]=0x03001990 + 4*slot]; obj->vtable[+0xC](obj, &{a, b})',
  'Call virtual method +0xC of the object in slot `slot` of the table at 0x03001990 with the pair (a, b). Unused by table '
  'scripts; with garbage arguments it did not return within a frame.', I, 'asm only')
n(0x2A, '0x08000959', 2, 0, ['eva_percent', 'evb_percent'], 'set_blend_alpha', 'screen', 'immediate', False,
  '0x08009728: BLDALPHA = (eva*0x290>>12) | (evb*0x290>>12)<<8',
  'Set alpha-blend coefficients from percentages (100 -> 16/16).', V, 'emulator: (50,25) -> BLDALPHA 0x0408')
for nid, nm, grids, val, h in ((0x2B, 'open_area', 'walk (+0xA8) and shot (+0xB0)', 1, '0x0800099D'),
                               (0x2C, 'block_area', 'walk (+0xA8) and shot (+0xB0)', 0, '0x080009F7'),
                               (0x2D, 'open_area_alt', 'third grid (+0xAC)', 1, '0x08000A51'),
                               (0x2E, 'block_area_alt', 'third grid (+0xAC)', 0, '0x08000AA1')):
    n(nid, h, 4, 0, ['x0', 'y0', 'x1', 'y1'], nm, 'map', 'immediate', False,
      f'0x08009930(rect, grid, {val}) on scene 0x030019A4 {grids}',
      f'{"Set" if val else "Clear"} the bits of the 8x8-tile rectangle [x0>>3, (x1+7)>>3) x [y0>>3, (y1+7)>>3) (pixels) in the '
      f'256x256 collision bitmap(s) {grids}; bit 1 = passable, so this {"opens" if val else "blocks"} the area '
      f'(doors, barriers). The third grid\'s purpose is not decoded.', V,
      'emulator: player tile bits before (1,1,1); after block_area (0,1,0), block_area_alt (1,0,1)')
n(0x2F, '0x08000AF1', 0, 0, [], 'show_hud', 'screen', 'immediate', False, '0x080046F4: GameData+1 |= 2; creates HUD object 0x0800B854 (vt 0x08049FEC) in world',
  'Show the status HUD (slides in) if hidden.', V, 'emulator screenshots (nat_a_hud_show.png)')
n(0x30, '0x08000AFD', 0, 0, [], 'hide_hud', 'screen', 'immediate', False, '0x08004764: GameData+1 &= ~2; broadcast message 0x15 (HUD removes itself)',
  'Hide the status HUD (slides out). First instruction of most cutscene scripts.', V, 'emulator screenshots (nat_a_hud_hide.png)')
n(0x31, '0x0800201F', 2, 0, ['actor', 'speed'], 'camera_center_on', 'camera', 'modal', True,
  'bbox 0x0800FDFA -> center - (120,80); camera mover 0x08015B68/0x08007858 (vt 0x0804AB24); run_mode 0x0803D8A0',
  'Scroll the camera until the actor\'s bounding-box centre is mid-screen, at `speed` px/frame on the dominant axis '
  '(frames = max(|dx|,|dy|)/speed + 1). Blocks until done.', V, 'emulator: camera (0x030019BC) moves; returns when reached')
n(0x32, '0x08001FE5', 3, 0, ['x', 'y', 'speed'], 'camera_pan_to', 'camera', 'modal', True,
  'camera mover 0x08015B68 / 0x08007858; run_mode 0x0803D8A0',
  'Scroll the camera\'s top-left corner to (x, y) world pixels at `speed` px/frame on the dominant axis. Blocks.', V,
  'emulator: speed 2 over 100 px took 48 frames; camera x 0x20 -> 0x70')
n(0x33, '0x0800207F', 1, 0, ['actor'], 'camera_follow', 'camera', 'immediate', False,
  'adds follow object (vt 0x0804D300, or 0x0804E324 when scene+0x90 != 0) to camera controller 0x03002BD8 (0x08021094)',
  'Make the camera track the actor.', V, 'emulator: camera followed the walking player')
n(0x34, '0x080020D9', 0, 0, [], 'camera_release', 'camera', 'immediate', False, '0x080210B6(camera controller 0x03002BD8)',
  'Remove the camera follow behaviour (camera stays where it is).', I, 'asm; paired with camera_follow in scripts')
n(0x35, '0x08000BF1', 3, 0, ['sprite', 'x', 'y'], 'spawn_actor', 'actor', 'immediate', False,
  'descriptor = 0x08000158(sprite); actor = 0x0801111C(0, desc, &{s16 x, s16 y}) (vt 0x0804CEF4); world 0x03001C4C add 0x0801191A',
  'Create a cutscene actor (NPC class) of sprite id `sprite` (>= 10: sprite table 0x086B6BD8) at world pixel (x, y), '
  'facing down. Later natives address it by the same sprite id.', V, 'emulator: new world object at (168,152), count_actors(41)=1')
n(0x36, '0x08000C37', 1, 0, ['index'], 'spawn_map_object', 'map', 'immediate', False,
  '0x08009108(area manager 0x0300197C, index): entry = area.obj_table(+0x14)[index]; obj = entry->fn(entry); world add',
  'Instantiate object `index` of the current area\'s object list (enemies, NPCs, triggers placed by the map) now.', V,
  'emulator: indices 0..3 each added one object (vt 0x0804C92C) at fixed map positions')
n(0x37, '0x08000C4F', 1, 1, ['actor'], 'count_actors', 'actor', 'query', False,
  'id >= 0: world 0x08011CCA(kind 5, descriptor) ; id < 0: kind 7, map object -(id+1)',
  'Replace id with the number of live world objects with that sprite (or map-object index for negative ids). Used as "is X present".', V,
  'emulator: 2 after spawning sprite 41 twice, 0 for 42')
n(0x38, '0x08000C9B', 0, 0, [], 'begin_parallel', 'cutscene', 'immediate', False, '0x08019348: director 0x03002BAC +0 = 1',
  'Start a batch: following actor commands are queued on their actors\' tracks and run concurrently instead of blocking one by one.', V,
  'emulator: two walk_to in a batch ran simultaneously (43 frames)')
n(0x39, '0x08000CA7', 1, 0, ['actor'], 'wait_for_actor', 'cutscene', 'immediate', False,
  'appends the actor object to the group list [0x03001004+4] (vt 0x0804DEEC, created on demand)',
  'Restrict the next end_parallel to wait only for the listed actors\' tracks (other tracks keep running in the background).', I,
  'asm of 0x08000CA7 and group mode 0x08006C3F/0x08006C75')
n(0x3A, '0x08000CE9', 0, 0, [], 'end_parallel', 'cutscene', 'modal', True,
  'group list set: run_mode(group); else 0x0801934E: run_mode(wait-all 0x0804C0C4); director +0 = 0',
  'Close the batch and block until all queued tracks (or only the wait_for_actor ones) are empty.', V, 'emulator: parallel walk test')


def cmd(id, h, pops, args, name, cat, vt, sem, status, ev, kind='enqueue', blocking='conditional'):
    n(id, h, pops, 0, args, name, cat, kind, blocking, f'command vtable {vt}; director enqueue 0x0801936A',
      sem + ' ' + ENQ_NOTE, status, ev)


cmd(0x3B, '0x08000D0B', 3, ['actor', 'dir', 'frames'], 'fire_wave_beam', 'battle', '0x0804C7D8',
    'Actor faces dir (sets actor+0x10), plays the firing animation (raw anim 0x12) and after 16 ticks spawns the wavy '
    'pink beam effect 0x080286D4 at actor + offset table 0x0877C430[dir]; done after `frames` more checks, then idle.', I,
    'asm + emulator screenshot nat_a_blast3b.png (pink wave beam appeared; did not finish within 300 frames when run on Goku)')
cmd(0x3C, '0x08000D63', 2, ['actor', 'anim'], 'play_anim', 'actor', '0x0804D818',
    'Play animation `anim` (script anim id, mapped through 0x080725E8+0x88 to the sprite\'s animation list; actor vtable +0x28) '
    'and stay on the track until it finishes (actor vtable +0x34); anim 0 completes immediately.', V,
    'asm; emulator: returns when finished (nat_a_anim3c.png)')
cmd(0x3D, '0x08000DB3', 2, ['actor', 'anim'], 'set_anim', 'actor', '0x0804E0E8',
    'Switch the actor to animation `anim` (actor vtable +0x28) and complete immediately (no wait).', V, 'asm (same start as 0x3C, done = 1)')
cmd(0x3E, '0x08000E1D', 3, ['actor', 'x', 'y'], 'fire_ball_at', 'battle', '0x0804C414',
    'Actor turns toward (x, y) (4-way from the angle), plays the throw animation (raw anim 0x14) and at tick 8 launches a pink '
    'energy ball 0x0802E444 toward the point; done after 24 ticks once the ball is gone.', V, 'emulator screenshot nat_a_blast3e.png')
cmd(0x3F, '0x08000E75', 1, ['actor'], 'puff', 'actor', '0x0804B2B4 (offsets 0,0)',
    'Spawn the dust-cloud "poof" effect (0x0801952C kind 0x37) at the actor\'s bounding-box centre (appear/vanish smoke).', V,
    'same class as 0x40 (emulator screenshot nat_a_fx40.png)')
cmd(0x40, '0x08000EC1', 3, ['actor', 'dx', 'dy'], 'puff_offset', 'actor', '0x0804B2B4',
    'Spawn the dust-cloud effect at the actor\'s bbox centre + (dx, dy).', V, 'emulator screenshot nat_a_fx40.png')
cmd(0x41, '0x08000F19', 3, ['actor', 'target', 'speed'], 'shoot_at_actor', 'battle', '0x0804DCE0',
    'Actor plays the shoot animation (raw anim 3) and fires a ki blast (0x08027CBC, effect 0x0871370C) toward the target actor\'s '
    'bbox centre at `speed` (px per 100 frames, see 0x080192CE); done when the blast has flown its distance (blast removed).', I,
    'asm; same mechanism as 0x42 which was VERIFIED')
cmd(0x42, '0x08000F79', 4, ['actor', 'x', 'y', 'speed'], 'shoot_at', 'battle', '0x0804DCF4',
    'Like shoot_at_actor but toward point (x, y).', V, 'emulator screenshot nat_a_blast42.png')
cmd(0x43, '0x08000FDF', 1, ['actor'], 'hit_flash', 'actor', '0x0804B918',
    'Send message 0x0C (object vt 0x0804A7A0) to the actor: it blinks red like a hit reaction.', V, 'emulator screenshot nat_a_msg43.png')
cmd(0x44, '0x0800101F', 4, ['actor', 'x', 'y', 'speed'], 'walk_to', 'actor', '0x0804D0A4 (built by 0x0805082C, mode 0x0F)',
    'Walk (walk animation, facing the dominant direction) in a straight line to world pixel (x, y) at `speed` px per 100 frames '
    '(frames = dist*100/speed + 1); snaps to the target at the end.', V,
    'emulator: speed 100 over 44.7 px -> 45 frames + 2 overhead; parallel test')
cmd(0x45, '0x08001081', 4, ['actor', 'x', 'y', 'air_frames'], 'jump_to', 'actor', '0x0804AE5C',
    'Jump to (x, y): 8-frame crouch (pose msg 0xE,0), airborne for air_frames with linear motion and a parabolic arc, then '
    '8-frame landing; total air_frames + 16.', V, 'emulator: air_frames 30 -> 48 frames')
cmd(0x46, '0x08001105', 3, ['actor', 'dir', 'frames'], 'fire_beam', 'battle', '0x0804C428',
    'Actor faces dir, charges (24 ticks) and fires a Kamehameha-style beam (0x080206B8, level definition 0x0871DA88, '
    'per-direction table 0x0877B200) for `frames`, then idles and removes the beam.', V, 'emulator screenshot nat_a_beam46.png')
cmd(0x47, '0x0800115D', 2, ['actor', 'target'], 'face_actor', 'actor', '0x0804AE70',
    'Turn the actor toward the target actor (dominant axis of the position difference); instant.', V,
    'emulator: NPC faced left toward player, player faced right toward NPC')
cmd(0x48, '0x080011B1', 3, ['actor', 'x', 'y'], 'puff_at', 'actor', '0x0804C43C',
    'Spawn the dust-cloud effect at world pixel (x, y) (actor only selects the track).', V, 'emulator screenshot nat_a_fx48.png')
cmd(0x49, '0x08001209', 4, ['actor', 'x', 'y', 'speed'], 'glide_to', 'actor', '0x0804AE84',
    'Move linearly to (x, y) at `speed` px per 100 frames WITHOUT changing animation or facing (floating, flying, being carried).', V,
    'emulator: same timing as walk_to, facing unchanged')
cmd(0x4A, '0x0800126D', 2, ['actor', 'frames'], 'wait', 'cutscene', '0x0804AB58',
    'Idle on the actor\'s track for `frames` frames (counter decremented once per frame, done at 0). With actor 0 outside a '
    'batch this is the plain "wait N frames".', V, 'emulator: wait 30 took 32 frames including runner overhead')
cmd(0x4B, '0x080012B9', 2, ['actor', 'sfx'], 'queue_sfx', 'audio', '0x0804C450',
    'Play sound effect sfx (volume 0x40) when this track reaches the command (sequenced sound).', I, 'asm (same PlaySfx call as 0x1F)')
cmd(0x4C, '0x08001305', 5, ['actor', 'r', 'g', 'b', 'frames'], 'screen_flash', 'screen', '0x0804C800',
    'Fade the whole palette toward colour (r, g, b) (0..255) and back over `frames` (min 2; peak at frames/2) via the palette '
    'fade routine 0x08007074.', V, 'emulator screenshot nat_a_flash4c.png (red flash, 30 frames)')
cmd(0x4D, '0x080013B5', 1, ['actor'], 'remove_actor', 'actor', '0x0804AE9C',
    'Delete the actor from the world (broadcast message 0x12 about it first). If it was the current talk target (0x03001BAC), '
    'sets 0x03002A58 = 1.', V, 'emulator: count_actors 1 -> 0')
cmd(0x4E, '0x080013F5', 2, ['actor', 'sprite'], 'change_sprite', 'actor', '0x0804B2C8',
    'Replace the actor with a new cutscene actor of sprite id `sprite` at the same position and facing; the remaining queued '
    'commands move to the new actor (0x080192BA). Used for transformations (e.g. Janemba -> Super Janemba).', V,
    'emulator: object descriptor 0x086A9DFC -> 0x086A9E84 (sprite 41 -> 42)')
cmd(0x4F, '0x08001441', 2, ['actor', 'index'], 'become_map_object', 'actor', '0x0804DEA0',
    'Replace the cutscene actor with map object `index` of the current area (0x08009124) at the actor\'s position/facing; '
    'hands a cutscene NPC over to the real enemy/NPC (boss fight start).', I, 'asm (LoG2 equivalent "replaceActorWithMapObject")')
cmd(0x50, '0x0800148D', 4, ['actor', 'x', 'y', 'speed'], 'move_to', 'actor', '0x0804D0A4 (0x0805082C, mode 2)',
    'Like walk_to but with movement mode 2 instead of 0x0F (identical path and timing; the mode byte cmd+0x24 selects the '
    'animation/state used while moving).', V, 'emulator: identical positions/frames to walk_to')
cmd(0x51, '0x080014EF', 3, ['actor', 'anim', 'frame'], 'set_pose', 'actor', '0x0804DD1C',
    'Hold animation `anim` at sub-frame `frame` (actor vtable +0x30: static pose state 0x03001B00, actor+0x11 = frame).', V,
    'emulator screenshot nat_a_pose51.png')
cmd(0x52, '0x0800154B', 5, ['actor', 'r', 'g', 'b', 'a'], 'tint', 'actor', '0x0804D4A8 (built by 0x0802B8F0)',
    'Recolour the actor with the nearest (squared distance) of 20 preset palettes in table 0x08752EEC (8-byte entries: RGBA key, '
    'palette pointer); (255,255,255,x) = white flash, (0,0,0,0)-like keys restore.', V, 'emulator screenshot nat_a_tint_red.png')
cmd(0x53, '0x080015BB', 2, ['actor', 'dir'], 'face', 'actor', '0x0804DEC8',
    'Set the actor\'s facing (actor+0x10): 0 down, 1 up, 2 left, 3 right.', V, 'emulator: facing byte 3 after face(41,3)')
cmd(0x54, '0x08001607', 2, ['actor', 'value'], 'set_actor_speed', 'actor', '0x0804C814',
    'Write value<<10 to actor+0x1AC (when scene 0x030019A4+0x90 != 0) or actor+0x180; INFERRED to be a movement speed. Unused.', I, 'asm only')
cmd(0x55, '0x08001671', 5, ['actor', 'kind', 'variant', 'dx', 'dy'], 'set_attachment', 'actor', '0x0804BF14',
    'Attach the record matching (kind, variant) from the 4-entry table 0x086B759C (0x08013E38; none -> detach) to the actor '
    '(0x0800ECB0, actor+0x5C) and set its offset actor+0x178 = (dx, dy). Scripts mostly pass zeros (detach).', I,
    'asm; emulator showed no visible change for (1,0,0,0)')
cmd(0x56, '0x080016E7', 2, ['actor', 'value'], 'set_actor_byte8', 'actor', '0x0804DD30',
    'Write byte actor+8 (0x38 for every actor seen; the attack commands also set it to 0x38). Unused by scripts.', I, 'asm only')
cmd(0x57, '0x08001733', 3, ['actor', 'intensity', 'frames'], 'shake_screen', 'camera', '0x0804C828',
    'Shake the camera (object 0x0804A720 via 0x0800797C; amplitude from intensity, period (100-intensity)/16+1) and keep the '
    'track busy for `frames`.', V, 'emulator: 30 frames -> 32 incl. overhead')

uses = json.loads((WORK / 'script' / 'native_uses.json').read_text())
arity = json.loads((WORK / 'script' / 'nat_arity_00_58.json').read_text())
bad = []
for e in N:
    e['uses'] = uses.get(f"{e['id']:02x}", 0)
    a = arity[str(e['id'])]
    if a['sp'] is not None and a['sp'] != 8 - e['pops'] + e['pushes']:
        bad.append((hex(e['id']), a['sp'], e['pops'], e['pushes']))
    assert len(e['args']) == e['pops'], e
assert [e['id'] for e in N] == list(range(0x58)), [hex(e['id']) for e in N]
print('arity mismatches', bad)
out = ROOT / 'engine' / 'annotations' / 'script_natives_a.json'
out.write_text(json.dumps(N, indent=1) + '\n')

S = []
for k in range(4):
    S.append(dict(id=k, handler=hex(u32(0x08055478 + 4 * k)), name=f'G{k}', semantics=f'word 0x{0x03001058 + 4 * k:08X} = value', status=V))
for k in range(4):
    S.append(dict(id=4 + k, handler=hex(u32(0x08055478 + 4 * (4 + k))), name=f'P{k}', semantics=f'[[0x03001030]+{4 * k}] = value (current parameter block)', status=V))
for k in range(4):
    S.append(dict(id=8 + k, handler=hex(u32(0x08055478 + 4 * (8 + k))), name=f'L{k}', semantics=f'word 0x{0x03001020 + 4 * k:08X} = value', status=V))
S += [
    dict(id=0x0C, handler='0x080003FD', name='hp', semantics='Active character HP = clamp(value, 0, maxHP) (s16 rec+0x16). If the player object exists: '
         'value <= 0 calls player->vtable[+0x7C] (knock-out) and the record comes from player->vtable[+0x80]; if rec+0x13 bit0 '
         '(fusion) 0x08013C66 syncs the components.', status=V),
    dict(id=0x0D, handler='0x08000479', name='max_hp', semantics='Sets BOTH HP and max HP (rec+0x16, +0x18) = value; fusion sync as above.', status=V),
    dict(id=0x0E, handler='0x0800051B', name='energy', semantics='clamp(value, 0, maxEP) into the form\'s energy slot (SS3 gauge +0x20 / SS gauge +0x1E / EP +0x1A, same rule as get_energy).', status=V),
    dict(id=0x0F, handler='0x0800058F', name='max_ep', semantics='Sets BOTH EP and max EP (rec+0x1A, +0x1C) = value.', status=V),
    dict(id=0x10, handler='0x080005C5', name='zenie', semantics='Zenie (0x0300155C) = max(value, 0).', status=V),
    dict(id=0x11, handler='0x080003F7', name='variant_byte', semantics='byte 0x03002BB0 = value (see native 0x11).', status=V),
]
for s in S:
    s['uses'] = 0
setter_uses = {0: 109, 1: 56, 2: 46, 3: 174, 4: 60, 5: 106, 6: 21, 7: 9, 11: 1, 12: 7, 14: 4, 16: 1}
for s in S:
    s['uses'] = setter_uses.get(s['id'], 0)
(ROOT / 'engine' / 'annotations' / 'script_setters.json').write_text(json.dumps(S, indent=1) + '\n')
print(len(N), 'natives', len(S), 'setters')
