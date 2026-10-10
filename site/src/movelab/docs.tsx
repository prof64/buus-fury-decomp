// Byte-level documentation of how each game builds a move (from re/<game>/notes.md "move_format").
import type { ReactNode } from 'react'

type Row = [string, string, string, string]

function Struct({ title, addr, rows, note }: { title: string; addr?: string; rows: Row[]; note?: ReactNode }) {
  return (
    <div style={{ marginBottom: 14 }}>
      <h4 style={{ margin: '8px 0 4px' }}>
        {title} {addr && <code className="addr">{addr}</code>}
      </h4>
      <div className="plain-table">
        <table className="hex-fields">
          <thead>
            <tr>
              <th>offset</th>
              <th>type</th>
              <th>field</th>
              <th>meaning</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i}>
                <td className="mono">{r[0]}</td>
                <td className="mono">{r[1]}</td>
                <td>{r[2]}</td>
                <td className="small">{r[3]}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {note && <p className="note">{note}</p>}
    </div>
  )
}

function Steps({ items }: { items: ReactNode[] }) {
  return (
    <ol className="small">
      {items.map((x, i) => (
        <li key={i} style={{ marginBottom: 4 }}>
          {x}
        </li>
      ))}
    </ol>
  )
}

export const STRUCT_DOCS: Record<string, ReactNode> = {
  log1: (
    <div>
      <p>
        A LoG1 move is <b>code plus small per-facing tables</b> — there is no generic attack record. To add a projectile move you supply a
        state table, a sprite definition, an animation table and the per-facing parameter tables, then register it in <code>fire_move</code>
        (0x0800B3DC).
      </p>
      <Steps
        items={[
          <>
            <b>State table</b> (one 0x14-byte row per projectile state, e.g. Ki Blast 0x081C7B14): enter, update (logic every frame), anim_tick,
            visible (default 0x0800AF81), draw (default 0x0800B01F). Player moves use 3 states: 0 charge, 1 travel, 2 vanish.
          </>,
          <>
            <b>Hit delivery</b> in <i>update</i>: build the msg-1 record <code>{'{rect, damage, enemy_owned, once_flag, owner_char}'}</code> and
            broadcast with 0x0800283C (first target only, ends the projectile) or 0x0800286A (all targets). Wall test: 0x08004FFA(map, rect, 2).
          </>,
          <>
            <b>Charge</b>: call 0x0800B436(projectile, cost) — takes (maxKi+15)&gt;&gt;4 Ki every 8th frame while B is held.
          </>,
          <>
            <b>Register</b>: <code>fire_move</code> indexes 0x08011E38 (sprite) and 0x08011E44 (state table) by move id; names at 0x08010000; L
            cycles ids 0–2 (0x08003524, <code>cmp #3</code> — raise it to 4 for a new slot); learned bit in 0x0300004D.
          </>,
        ]}
      />
      <Struct
        title="State-table row"
        addr="0x14 bytes"
        rows={[
          ['+0x00', 'ptr|1', 'enter(entity)', 'called by set_state'],
          ['+0x04', 'ptr|1', 'update(entity)', 'logic, every frame'],
          ['+0x08', 'ptr|1', 'anim_tick(entity)', 'advance +0x10/+0x12, end the state'],
          ['+0x0C', 'ptr|1', 'visible(entity)', '1 if on screen (default 0x0800AF81)'],
          ['+0x10', 'ptr|1', 'draw(entity, oam)', 'default 0x0800B01F'],
        ]}
      />
      <Struct
        title="Sprite definition"
        addr="0x2C bytes, e.g. 0x086B3AA8"
        rows={[
          ['+0x00', 'u16', 'w', 'hitbox width (projectiles) / foot box'],
          ['+0x02', 'u16', 'h', 'hitbox height'],
          ['+0x04', 's16×4 ×4', 'hurtboxes', '{x0,y0,x1,y1} for down, up, left, right'],
          ['+0x24', 'ptr', 'palette', '16-colour palette'],
          ['+0x28', 'ptr', 'animation table', '16 bytes per state = 4 strip pointers'],
        ]}
      />
      <Struct
        title="Animation strip (frame descriptor)"
        addr="16 bytes"
        rows={[
          ['+0x00', 's16,s16', 'dx, dy', 'draw offset'],
          ['+0x04', 'u16,u16', 'w, h', 'size'],
          ['+0x08', 'u8', 'tiles/frame', 'frame f uses tiles + f·tiles·32'],
          ['+0x09', 'u8', 'attr1 bits', 'bit3 = h-flip'],
          ['+0x0A', 'u8,u8', 'OAM shape, size', ''],
          ['+0x0C', 'ptr', 'tile data', '4bpp, uncompressed'],
        ]}
      />
      <Struct
        title="Per-facing parameter tables"
        rows={[
          ['0x08011E50', '{s16 dx, s16 dy, u16 flags} ×4', 'spawn', 'projectile placed at owner.pos + spawn[facing]'],
          ['0x08011E68', '{s16 dx, dy, vx, vy} ×4', 'ki-blast launch', 'offset at release and px/frame velocity'],
          ['0x08011E88', '{s16 dx, dy, dw, dh} ×4', 'Kamehameha grow', '+0x20 retract, +0x40 wall retract, +0x60 start offset'],
        ]}
      />
    </div>
  ),
  log2: (
    <div>
      <p>
        A LoG2 move = <b>an attack-table entry</b> (type + power) + <b>a state</b> that, on some frame, broadcasts a <b>hit message</b> with a
        rectangle and a scale — directly (melee) or from a <b>projectile object</b> — + <b>a move-type row</b> with the EP rules, placed in a
        character slot and unlocked by a flag bit.
      </p>
      <Struct
        title="Attack-table entry"
        addr="0x086DB2CC, 8 bytes × 16"
        rows={[
          ['+0x00', 'u8', 'type', '0 melee (attacker STR, can crit, defender mult +0x163) / 1 energy (POW, +0x164)'],
          ['+0x01', 'u8', 'id', 'tag read by receivers: 9 Burning-Attack stun, 15 zero-damage stun'],
          ['+0x02', 'u16', 'pad', '0'],
          ['+0x04', 'u32', 'power', '1024 ≈ a basic punch / blast'],
        ]}
      />
      <Struct
        title="Hit message"
        addr="built on the stack, sent with 0x08006DA6(list, 0, msg)"
        rows={[
          ['+0x00', 'u8', 'team', '0 hostile to player, 1 player’s attack, 2 hits everyone'],
          ['+0x04', 's32×4', 'rect', 'x0,y0,x1,y1 in pixels (strict overlap with the hurtbox)'],
          ['+0x14', 'u8', 'facing / kind', 'direction of the hit (knockback)'],
          ['+0x18', 'u32', 'scale', '1024 = 1.0 (charge level of Big Bang, Masenko, Spirit Bomb…)'],
          ['+0x1C', 'ptr', 'attack entry', 'into the table above'],
          ['+0x20', 'ptr', 'attacker', 'STR/POW read from attacker +0x160/+0x161'],
          ['+0x24..', 'out', 'crit, pre-mult, damage', 'filled by computeDamage 0x0800F910'],
        ]}
      />
      <Struct
        title="Move-type row (B button)"
        addr="0x081D9900, 16 bytes × 14"
        rows={[
          ['+0x00', 'ptr', 'icon', 'HUD icon graphics'],
          ['+0x04', 'ptr', 'icon (alt)', ''],
          ['+0x08', 'fn', 'canUse(&rec.HP, player)', 'EP ≥ min and the global “attacks allowed” flag'],
          ['+0x0C', 'fn', 'execute(&rec.HP, player)', 'EP −= start cost; player.setState(state)'],
        ]}
        note="Then write the type id into a character’s slot list (record +0x18) and set the slot’s unlock bit (record +0x14 bits 3..6) — story scripts do this with builtin 0x53 learn(char, n)."
      />
      <Struct
        title="Projectile object (base ctor 0x08013BC0)"
        addr="0x50–0x5C bytes"
        rows={[
          ['+0x18', 'ptr', 'frame box', '{s8 dx, s8 dy, u8 w, u8 h}: rect = pos − (dx,dy) … + (w,h)'],
          ['+0x1C', 'ptr', 'owner', ''],
          ['+0x20/+0x24', 's32', 'x, y', '24.8 fixed point'],
          ['+0x28..0x34', 's32×4', 'rect', 'current hit rect'],
          ['+0x40..', '', 'class data', 'velocity (24.8; 0x300 = 3 px/f), timers, scale'],
        ]}
        note="Removed when the rect leaves the camera (0x08013C8E). States live in the IWRAM table 0x083B5278: enter / per-frame / exit; most count frames in entity+0x12."
      />
    </div>
  ),
  buu: (
    <div>
      <p>
        A Buu’s Fury skill = <b>an attack type</b> (kind, sub, power) + <b>a skill entry</b> with canUse/activate + <b>a player state</b> whose
        update hits through one of three paths: spawn a ki blast, spawn a beam with a per-level definition, or build a hit struct directly.
      </p>
      <Struct
        title="Attack type"
        addr="0x086B8088, 8 bytes × 18"
        rows={[
          ['+0x00', 'u8', 'kind', '0 melee (S × power / 20, crit ×1.5 unless sub 3, END) / 1 energy (POW, END2)'],
          ['+0x01', 'u8', 'sub', 'id: 2 punch, 3 no-crit melee, 4 Energy Blast, 5 Kamehameha …'],
          ['+0x02', 'u16', 'pad', '0'],
          ['+0x04', 'u32', 'power', '1024 = 1.0 (Volleyball 0)'],
        ]}
      />
      <Struct
        title="Skill entry"
        addr="0x0871DAF8, 24 bytes × 23"
        rows={[
          ['+0x00', 'u32', 'max level', '2 = three levels, 0xFF = fixed'],
          ['+0x04', 'ptr', 'icon', ''],
          ['+0x08', 'ptr', 'icon 2', ''],
          ['+0x0C', 'u16', 'name string', 'UTF-16 string table 0x0806BCE8'],
          ['+0x0E', 'u16', 'description string', ''],
          ['+0x10', 'fn', 'canUse(record)', 'usually EP ≥ cost via 0x080134B8 (cost × rec+0x56/16; SS gauge when transformed)'],
          ['+0x14', 'fn', 'activate(record, player)', 'pushes a player state (5-slot vtable: enter, update, input, destroy, misc)'],
        ]}
      />
      <Struct
        title="Hit struct (passed to ComputeDamage 0x0800E9D8)"
        rows={[
          ['+0x00', 'u32', 'direction', 'for knockback and frontal block'],
          ['+0x04', 'u32', 'multiplier', '0x400 = 1.0, per hit'],
          ['+0x08', 'ptr', 'attack type', 'entry above'],
          ['+0x0C', 'ptr', 'attacker', ''],
          ['+0x10', 'out', 'crit flag', ''],
          ['+0x14', 'out', 'raw damage', ''],
          ['+0x18', 'out', 'final damage', '1/1024 HP'],
        ]}
      />
      <Struct
        title="Beam level definition"
        addr="0x0871DA68 + 16·level"
        rows={[
          ['+0x00', 'u8,u8', 'w, h', 'beam head box'],
          ['+0x02', 'u16', 'mult', 'per-hit multiplier (Kamehameha 41 / 164 / 328)'],
          ['+0x04', 'u16', '?', ''],
          ['+0x06', 'u16', 'attack sub', ''],
          ['+0x08', 'ptr', 'graphics', ''],
          ['+0x0C', 'ptr', 'animation', ''],
        ]}
        note="Costs: byte triples (Lv0/1/2) at 0x0871DA98 for beams, u32 triples for Burning Attack (+0x18), Energy Punch (+0x30), Masenko (+0x48). Spend EP with 0x08013504. Learned by adding the id to record +0x0D.. and bumping +0x0C."
      />
    </div>
  ),
}
