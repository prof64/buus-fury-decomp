//! Field objects with their own physics / rules: breakable props, coins, dropped items, map item pickups, the item-get
//! presentation, bobbing map sprites, deferred calls, and the loot natives' core side (`drop_zenie` / `drop_item` /
//! `drop_equipment` at the loot position). Spec: re/buu/notes.md "Field: triggers, breakables, pickups, game state";
//! ground truth data/field/objects_*.json.

use crate::field::{Cause, FieldEvent};
use crate::fieldcontent::{Callback, ObjKind, ObjRef};
use crate::geom::DIR_VEC;
use crate::world::{Body, EntityId, Event, HitMsg, World};
use serde::{Deserialize, Serialize};

/// what a pickup gives
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PickupKind {
    /// a coin of size 0..3 (5 / 25 / 50 / 100 zenie)
    Zenie(i32),
    Supply(i32),
    Equipment(i32),
    /// a placed map item (area list 3): item (>= 0 supply, < 0 equipment) + the def's on-pickup event
    MapItem {
        obj: ObjRef,
        item: i32,
    },
}

pub const COIN_VALUE: [i32; 4] = [5, 25, 50, 100];

/// coins (`0x08017F5A`), dropped items (`0x0800D30E`) and map items (`0x0800DCDC`)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pickup {
    pub kind: PickupKind,
    /// 24.8: coins = the loot point, items = the 16x16 box's top-left
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    /// height (coins 24.8, items 22.10) and its speed
    pub z: i32,
    pub vz: i32,
    /// frames alive
    pub t: i32,
    /// moves made in frames an event held (the flight's counter is `t + mv`)
    #[serde(default)]
    pub mv: i32,
    pub landed: bool,
    /// coin animation phase (the constructor's raw next())
    pub phase: u32,
    /// the box (px): coins' pickup box, items' 16x16 icon box
    pub bx: [i32; 4],
    /// map items: the box written blocked into the walk + enemy grids
    pub solid: Option<[i32; 4]>,
    /// the coin's touch rect for the trigger check of frame `.0` (`World.frame` during the update): the game's
    /// trigger check runs after the player's update and before the other objects', so it sees a coin where it was
    /// before this frame's step (or where it dropped, for a coin the frame's deferred calls dropped; story
    /// playthrough).
    #[serde(default)]
    pub spawn: Option<(u32, [i32; 4])>,
    /// a held frame's move (`coin_move`, World.frame then): where it was before it (x, y, box, z, vz) - the frame's
    /// completed pass (`Field::completing`) gives the coin its normal update instead
    #[serde(default)]
    pub held: Option<(u32, i32, i32, [i32; 4], i32, i32)>,
    /// what the coin's spin is ahead of its age. The spin's counter (+0x28, from the constructor's random phase)
    /// counts the draws (0x08018284: +1, then the cel = (counter & 0xFF) >> 3, 0 beyond 3), the age (+0x24) the
    /// updates. The draw check (0x0801809E: the flight's move, then no draw on the blink's hidden frames or off the
    /// camera - `coin_drawn`) runs every frame, under an event or an item-get's hold too: there the spin runs on
    /// while the age stands (link recording dojo_px3 lf 3,503 .. 3,508); a frame without the draw leaves it behind.
    /// One ahead from the making (link recording dojo_px1; the pixel board's coins)
    #[serde(default)]
    pub spin: i32,
}

impl Pickup {
    /// picked up with the A-press interact message (items); coins are touch triggers
    pub fn by_a(&self) -> bool {
        !matches!(self.kind, PickupKind::Zenie(_))
    }
    pub fn rect(&self) -> [i32; 4] {
        self.bx
    }
    /// a coin's touch rect (trigger `0x0804A0C0`): (x0 - 19, y0 - 11, x1 + 4, y1 + 4)
    pub fn coin_rect(&self) -> [i32; 4] {
        [
            self.bx[0] - 19,
            self.bx[1] - 11,
            self.bx[2] + 4,
            self.bx[3] + 4,
        ]
    }
}

/// a breakable map prop (`0x0800C65E`): def +5 HP, +6 attack-type mask
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Breakable {
    pub obj: ObjRef,
    pub prop: i32,
    /// the solid / hit box (px)
    pub bx: [i32; 4],
    pub hp: i32,
    pub mask: i32,
    pub last_dir: u8,
    /// shake timer (23 frames per hit) and the display offset
    pub shake: i32,
    pub dx: i32,
    pub dy: i32,
    /// broken: waiting for its deferred callback
    pub broken: bool,
    /// the prop is solid (blocks the grids, can be hit)
    pub solid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MapSprite {
    pub x: i32,
    pub y: i32,
    pub sprite: i32,
    pub params: [i32; 3],
    pub t: i32,
    /// level sign: (label frame, prop it hangs on)
    pub label: Option<(i32, i32)>,
}

/// a call queued for later (world +0x610), run at the start of the world update of frame `at`
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Deferred {
    /// a breakable's break callback (`0x0800C628`)
    Break { at: u32, entity: EntityId },
    /// an event a hit message's acceptor starts (Goten's tag rock, `0x08033F28`: s120 - its run starts with the
    /// next frame's world update, before the objects: the hit leader is held then)
    Run {
        at: u32,
        run: crate::fieldcontent::Callback,
        cause: crate::field::Cause,
        /// the game's nested event loop (the rock's hit): that frame has no object pass; otherwise an event object
        /// (RunScript 0x0803388C) - the newest in the list, it runs first in the pass and the others update held
        #[serde(default)]
        nested: bool,
    },
}

/// the blocking item-get presentation (ItemGet command, 60 frames, SFX 45)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemGet {
    pub left: i32,
    pub slot: u8,
    /// a supply counted when it ends
    pub supply: Option<i32>,
    /// then run (map items' "You found ..." event)
    pub then: Option<(Callback, Cause)>,
    /// the item shown (>= 0 supply, < 0 equipment)
    pub item: Option<i32>,
}

fn ov(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn walk_free(w: &World, r: [i32; 4]) -> bool {
    w.col
        .as_ref()
        .is_none_or(|c| c.test(crate::world::Layer::Walk, r[0], r[1], r[2], r[3]))
}

// ---------------------------------------------------------------------------------------------------------------
// creation from the area lists

/// a breakable: the box is the prop's lower half (the game takes the bounding box of the prop graphic's non-zero
/// pixels, then its lower half; the pixels are not in the content pack: 1 px inset per side approximates the 32-px
/// props), written blocked into both grids
pub fn spawn_breakable(w: &mut World, o: &crate::fieldcontent::ObjDef) -> Option<EntityId> {
    let ObjKind::Breakable {
        prop,
        param_a,
        hits,
        ..
    } = &o.kind
    else {
        return None;
    };
    let r = w
        .field
        .as_ref()
        .and_then(|f| f.props.get(*prop as usize))
        .map(|p| p.rect)?;
    // the prop graphic's opaque bounds, lower half (content `hit_box`; fallback: the rect's lower half)
    let hb = w.field.as_ref().and_then(|f| {
        w.data
            .field
            .as_ref()?
            .maps
            .get(&f.map)?
            .props
            .get(*prop as usize)?
            .hit_box
    });
    let bx = hb.unwrap_or([r.x0 + 1, (r.y0 + r.y1) >> 1, r.x1 - 1, r.y1]);
    // only a solid prop (map prop record +0x17 = the tile word's top byte) blocks the grids
    let solid = w
        .field
        .as_ref()
        .and_then(|f| {
            w.data
                .field
                .as_ref()?
                .maps
                .get(&f.map)?
                .props
                .get(*prop as usize)
                .map(|p| p.tile >> 24 != 0)
        })
        .unwrap_or(true);
    if solid {
        if let Some(c) = w.col.as_mut() {
            c.mark_both(bx, 0);
        }
    }
    Some(w.add(Body::Breakable(Box::new(Breakable {
        obj: o.at,
        prop: *prop,
        bx,
        hp: (*param_a).max(1),
        mask: *hits,
        last_dir: 0,
        shake: 0,
        dx: 0,
        dy: 0,
        broken: false,
        solid,
    }))))
}

/// a map item (16x16 at the def position; (x, y+8)..(x+16, y+16) blocked on both grids)
pub fn spawn_map_item(w: &mut World, o: &crate::fieldcontent::ObjDef) -> Option<EntityId> {
    let ObjKind::Item { item, x, y, .. } = &o.kind else {
        return None;
    };
    let solid = [*x, y + 8, x + 16, y + 16];
    if let Some(c) = w.col.as_mut() {
        c.mark_both(solid, 0);
    }
    let id = w.add(Body::Pickup(Box::new(Pickup {
        kind: PickupKind::MapItem {
            obj: o.at,
            item: *item,
        },
        x: x << 8,
        y: y << 8,
        vx: 0,
        vy: 0,
        z: 0,
        vz: 0,
        t: 0,
        mv: 0,
        landed: true,
        phase: 0,
        bx: [*x, *y, x + 16, y + 16],
        solid: Some(solid),
        spawn: None,
        held: None,
        spin: 0,
    })));
    // its icon's tiles (ctor `0x0800DCDC`, vt 0x0804A314 +0xC: 4 blocks (0x100 bytes), no shadow; exit-fade cuts
    // 8-56 -> 8-57 / 4-24 -> 4-30: the block right after the props', before the player's)
    if w.field.as_ref().is_some_and(|f| f.vram.is_some()) {
        crate::vram::acquire(w, crate::vram::Owner::Ent(id), 0x100);
    }
    Some(id)
}

/// script `spawn_map_item(i)` (0xAA, 0x080090E6 -> 0x0800DCDC with the loot block 0x03001010 / 0x03001018): the area's
/// item `i` built by the dropped-item constructor 0x0800D30E at the loot position with the loot velocity (4 RNG
/// draws, the flight of `drop_item`), then given the map item's vtable 0x0804C508 (A takes it, its event runs).
/// VERIFIED 12-28: the Warlord's ball starts at (loot x - 8, loot y - 16), z 0x2000, and flies (239, 118) / frame.
pub fn throw_map_item(w: &mut World, o: &crate::fieldcontent::ObjDef) -> Option<EntityId> {
    let ObjKind::Item { item, .. } = &o.kind else {
        return None;
    };
    let id = drop_item(w, *item)?;
    let k = w.find(id)?;
    if let Body::Pickup(p) = &mut w.ents[k].body {
        p.kind = PickupKind::MapItem {
            obj: o.at,
            item: *item,
        };
    }
    Some(id)
}

pub fn spawn_map_sprite(w: &mut World, o: &crate::fieldcontent::ObjDef) -> Option<EntityId> {
    let ObjKind::MapSprite {
        x,
        y,
        sprite,
        params,
        label,
    } = &o.kind
    else {
        return None;
    };
    Some(w.add(Body::MapSprite(Box::new(MapSprite {
        x: *x,
        y: *y,
        sprite: *sprite,
        params: *params,
        t: 0,
        label: *label,
    }))))
}

// ---------------------------------------------------------------------------------------------------------------
// breakables

/// message 0 (an attack): needs mult != 0, a strict box overlap and the attack type in the mask
pub fn breakable_message(w: &mut World, k: usize, msg: &mut HitMsg) -> bool {
    // any sender's attack message (a laser turret's wave breaks crates too: Babidi's ship 8-22)
    if msg.mult == 0 {
        return false;
    }
    let Body::Breakable(b) = &mut w.ents[k].body else {
        return false;
    };
    // (a breakable bound to a non-solid prop takes hits too: HFIL 18-10's boulder is prop 13, hidden and not solid,
    // under the solid prop 12 - punched, it shakes the screen and breaks; sc_hfil)
    if b.broken || !ov(b.bx, [msg.rect.x0, msg.rect.y0, msg.rect.x1, msg.rect.y1]) {
        return false;
    }
    if b.mask & ((1 << msg.atk.kind) | (1 << msg.atk.sub)) == 0 {
        return false;
    }
    b.last_dir = msg.dir;
    b.hp -= 1;
    let (cx, cy) = ((b.bx[0] + b.bx[2]) >> 1, (b.bx[1] + b.bx[3]) >> 1);
    let stamp = w.next_id;
    spawn_debris(w.field.as_mut(), cx, cy, stamp);
    for _ in 0..4 {
        crate::vram::keyed_acquire(w, crate::vram::KEY_DEBRIS, 0x40);
    }
    let Body::Breakable(b) = &mut w.ents[k].body else {
        return false;
    };
    // the melee's crit slot is not written for breakables: alone, every recorded hit shook the screen (15, 75) (the
    // stack slot's stale value); with an enemy hit by the same punch the enemy's ComputeDamage wrote it (Babidi's
    // ship 8-34: a killing punch that also hit a crate, no shake)
    if msg.raw == 0 && msg.dmg == 0 {
        msg.crit = true;
    }
    if b.hp > 0 {
        b.shake = 23;
        return true;
    }
    b.broken = true;
    let bx = b.bx;
    let prop = b.prop;
    let id = w.ents[k].id;
    // the breaking hit (0x0800C99A): the bound prop's draw flag (+0x1C) cleared at once, a smoke puff (the generic
    // effect 0x0802A81C: run 0x086FD9D0, layer 0x38 - sorted with the actors by its y) at the box centre, then the
    // 16-frame timer to the callback (whose loot script makes the explosion cloud) - emulator, HFIL 18-10's boulder
    // (rj_back1 f187: the rock's OAM entry gone, the white puff's first cel; the explosion 0x08712FDC at f203)
    hide_prop(w, prop);
    let rate = 0x1000 + w.rng.rand(0x1000);
    let e = crate::fx::fx_anim(w, 0x086f_d9d0, (bx[0] + bx[2]) >> 1, (bx[1] + bx[3]) >> 1, 0, 4, rate);
    crate::fx::set_sorted(w, e);
    let at = w.frame + 16;
    if let Some(f) = w.field.as_mut() {
        f.deferred.push(Deferred::Break { at, entity: id });
    }
    true
}

pub fn breakable_update(w: &mut World, i: usize) {
    // an event holding the field (the level-up popup's nested loop) stops the hit shake, like the pickups
    // (Babidi's ship 8-34: no shake draws during a kill's level-up)
    if w.field.as_ref().is_some_and(|f| f.busy) {
        return;
    }
    let Body::Breakable(b) = &w.ents[i].body else {
        return;
    };
    if b.shake <= 0 {
        return;
    }
    let t = b.shake - 1;
    let (mut dx, mut dy) = (0, 0);
    if t != 0 && t & 2 == 0 {
        dx = w.rng.rand(5) - 2;
        dy = w.rng.rand(3) - 1;
    }
    if let Body::Breakable(b) = &mut w.ents[i].body {
        b.shake = t;
        if t == 0 {
            b.dx = 0;
            b.dy = 0;
        } else if t & 2 == 0 {
            b.dx = dx;
            b.dy = dy;
        }
    }
}

/// the deferred calls due this frame (start of the world update)
pub fn run_deferred(w: &mut World) {
    let now = w.frame + 1;
    let due: Vec<Deferred> = match w.field.as_mut() {
        Some(f) => {
            let (due, keep): (Vec<_>, Vec<_>) = f.deferred.drain(..).partition(|d| match d {
                Deferred::Break { at, .. } | Deferred::Run { at, .. } => *at <= now,
            });
            f.deferred = keep;
            due
        }
        None => return,
    };
    for d in due {
        match d {
            Deferred::Break { entity, .. } => break_done(w, entity),
            Deferred::Run { run, cause, nested, .. } => {
                if let Some(f) = w.field.as_mut() {
                    f.event_started_pre = true;
                    f.event_nested = nested;
                }
                w.events.push(crate::world::Event::Field(crate::field::FieldEvent::Run { run, cause, slot: 0 }))
            }
        }
    }
}

/// a breakable's bound prop no longer drawn (its +0x1C flag): the field's prop, and a script's record of it - a prop
/// a script showed (HFIL 18-10: the boulder swapped in by the area's trigger) would come back with `sync_props`
fn hide_prop(w: &mut World, prop: i32) {
    let loads = w.field.as_ref().map(|f| f.loads);
    if let Some(p) = w.field.as_mut().and_then(|f| f.props.get_mut(prop.max(0) as usize)) {
        p.visible = false;
    }
    for sp in w.scene.props.iter_mut().filter(|sp| sp.id == prop && Some(sp.load) == loads) {
        sp.visible = false;
    }
}

/// `0x0800C628`: loot position (centre x, bottom y), loot velocity = the last hit direction << 8, the grids under the
/// box walkable again, the breakable gone, then the def's callback (usually the loot script s497)
fn break_done(w: &mut World, id: EntityId) {
    let Some(k) = w.find(id) else { return };
    let Body::Breakable(b) = &w.ents[k].body else {
        return;
    };
    let (obj, bx, dir, prop) = (b.obj, b.bx, b.last_dir, b.prop);
    w.ents[k].removed = true;
    // (the bound map prop went with the breaking hit; kept for a break made without one)
    hide_prop(w, prop);
    if let Some(c) = w.col.as_mut() {
        c.mark_both(bx, 1);
    }
    let (x, y) = ((bx[0] + bx[2]) >> 1, bx[3]);
    let v = DIR_VEC[dir as usize & 3];
    set_loot(w, x, y, v.0 << 8, v.1 << 8);
    let cb = crate::field::area_obj(w, obj).and_then(|o| match o.kind {
        ObjKind::Breakable { on_break, .. } => on_break,
        _ => None,
    });
    if let Some(run) = cb {
        w.events.push(Event::Field(FieldEvent::Run {
            run,
            cause: Cause::Break { obj, x, y },
            slot: 0,
        }));
        let frame = w.frame + 1;
        if let Some(f) = w.field.as_mut() {
            f.busy_pre_pass = frame;
            // run before the objects update, like a queued event: the enemies run their update in its last frame
            // (segment sb 21-11)
            f.queued_event = true;
            f.queued_at = frame;
            f.break_event = frame;
            f.break_cb = true;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// loot (the script host calls these for the drop natives)

/// the loot position / velocity (`0x03001010` / `0x03001018`) the next drops use
pub fn set_loot(w: &mut World, x: i32, y: i32, vx: i32, vy: i32) {
    if let Some(f) = w.field.as_mut() {
        f.loot = (x, y, vx, vy);
    }
}

fn scatter(w: &mut World, vx: i32, vy: i32) -> (i32, i32) {
    let a = w.rng.rand(256) - 128;
    let b = w.rng.rand(256) - 128;
    let mut x = vx + ((a * vy) >> 8);
    let mut y = vy + ((b * vx) >> 8);
    x = (x * (w.rng.rand(64) + 0xE0)) >> 8;
    y = (y * (w.rng.rand(64) + 0xE0)) >> 8;
    (x, y)
}

/// `drop_zenie(size)` (`0x08018346`): a coin at the loot position (5 RNG draws)
pub fn drop_zenie(w: &mut World, size: i32) -> Option<EntityId> {
    let loot = w.field.as_ref()?.loot;
    drop_zenie_from(w, loot, size)
}

/// `drop_zenie` at a given loot block (position, velocity) - a link fight's prize (no field: `player::link_prize`)
pub fn drop_zenie_from(w: &mut World, loot: (i32, i32, i32, i32), size: i32) -> Option<EntityId> {
    let (lx, ly, lvx, lvy) = loot;
    let size = size.clamp(0, 3);
    // no coin (and no draws) when the object tiles are short: room < 0x3000 (`0x08018356`)
    if crate::vram::room_of(w).is_some_and(|r| r < 0x3000) {
        return None;
    }
    let phase = w.rng.next();
    let (vx, vy) = scatter(w, lvx, lvy);
    let big = size & 1 == 1;
    let (sw, bw, bh, yoff) = if big { (16, 10, 4, -6) } else { (8, 4, 2, -7) };
    let (x, y) = (lx << 8, ly << 8);
    let x0 = (x >> 8) - sw / 2;
    let y0 = (y >> 8) + yoff;
    let mut p = Pickup {
        kind: PickupKind::Zenie(size),
        x,
        y,
        vx,
        vy,
        z: 0,
        vz: 0x80,
        t: 0,
        mv: 0,
        landed: false,
        phase,
        bx: [x0, y0, x0 + bw, y0 + bh],
        solid: None,
        spawn: None,
        held: None,
        spin: 0,
    };
    if w.field.as_ref().is_some_and(|f| f.pre_update) {
        p.spawn = Some((w.frame + 1, p.coin_rect()));
    }
    let id = w.add(Body::Pickup(Box::new(p)));
    // its tiles (def +0x10: 0x40 small / 0x100 big) and its shadow's shared graphic (def +2/+3: (4, 2) / (10, 4);
    // `0x08018550` -> `0x080184C0`; segment sb 20-6 f155)
    crate::vram::coin_created(w, id, big);
    // the first physics step in the frame it spawns: the world update gives it when it is created before that
    // update (deferred calls, drop scripts at the frame's start), else now
    let pre = w.field.as_ref().is_some_and(|f| f.pre_update);
    if !pre {
        if let Some(k) = w.find(id) {
            pickup_update(w, k);
        }
    }
    // (the spin's counter is one ahead of the age from the start: the pixel board over 494 recorded segments, with
    // the draw check's rule - `coin_drawn` - 149 sampled frames more exact, 5 fewer)
    if let Some(k) = w.find(id) {
        if let Body::Pickup(p) = &mut w.ents[k].body {
            p.spin += 1;
        }
    }
    Some(id)
}

/// `drop_item(i)` / `drop_equipment(e)` (`0x0800D30E`): item >= 0 supply, < 0 equipment (4 RNG draws)
pub fn drop_item(w: &mut World, item: i32) -> Option<EntityId> {
    let loot = w.field.as_ref()?.loot;
    drop_item_from(w, loot, item)
}

/// `drop_item` at a given loot block (`drop_zenie_from`)
pub fn drop_item_from(w: &mut World, loot: (i32, i32, i32, i32), item: i32) -> Option<EntityId> {
    let (lx, ly, lvx, lvy) = loot;
    let (vx, vy) = scatter(w, lvx, lvy);
    let kind = if item >= 0 {
        PickupKind::Supply(item)
    } else {
        PickupKind::Equipment(-item)
    };
    let (x, y) = ((lx - 8) << 8, (ly - 16) << 8);
    let p = Pickup {
        kind,
        x,
        y,
        vx,
        vy,
        z: 0x2000,
        vz: 0x800,
        t: 0,
        mv: 0,
        landed: false,
        phase: 0,
        bx: [lx - 8, ly - 16, lx + 8, ly],
        solid: None,
        spawn: None,
        held: None,
        spin: 0,
    };
    let id = w.add(Body::Pickup(Box::new(p)));
    // its icon's tiles (the item's icon size, 0x100 for most: real run 4-1 tile r002_006+11,000 f104, block (135, 4);
    // item 69 0x400: tour_desert_c f3445, (402, 16)) and the 10 x 4 shadow, like a big coin
    crate::vram::item_drop_created(w, id, crate::scene::item_icon_bytes(item));
    if !w.field.as_ref().is_some_and(|f| f.pre_update) {
        if let Some(k) = w.find(id) {
            pickup_update(w, k);
        }
    }
    Some(id)
}

// ---------------------------------------------------------------------------------------------------------------
// pickups

/// a coin's move of one frame (0x0801809E: the move counter +0x2C runs 0..59 - sixty moves, then it lies)
fn coin_fly(w: &World, p: &mut Pickup) {
    if p.t + p.mv <= 60 {
        let (nx, ny) = (p.x + p.vx, p.y + p.vy);
        let (ddx, ddy) = ((nx >> 8) - (p.x >> 8), (ny >> 8) - (p.y >> 8));
        // 0x0801809E: the step is kept only when it reaches a new pixel (and the box there is free); a step
        // within the pixel is dropped, so a coin slower than 1 px per frame that starts on a whole pixel never
        // moves (story playthrough: 3 of the fighters' 4 coins stay where they dropped)
        if (ddx != 0 || ddy != 0)
            && walk_free(
                w,
                [p.bx[0] + ddx, p.bx[1] + ddy, p.bx[2] + ddx, p.bx[3] + ddy],
            )
        {
            p.x = nx;
            p.y = ny;
            p.bx = [p.bx[0] + ddx, p.bx[1] + ddy, p.bx[2] + ddx, p.bx[3] + ddy];
        }
        p.vz -= 4;
        p.z += p.vz;
        if p.z <= 0 {
            p.z = -p.z;
            p.vz = 0x80;
        }
    } else {
        p.z = 0;
        p.landed = true;
    }
}

/// a coin's move in a frame an event holds: the flight goes on, the age does not count
fn coin_move(w: &mut World, i: usize) {
    let Body::Pickup(p) = &w.ents[i].body else {
        return;
    };
    if !matches!(p.kind, PickupKind::Zenie(_)) || p.landed {
        return;
    }
    let mut p = (**p).clone();
    p.mv += 1;
    p.held = Some((w.frame, p.x, p.y, p.bx, p.z, p.vz));
    coin_fly(w, &mut p);
    if let Body::Pickup(q) = &mut w.ents[i].body {
        **q = p;
    }
}

/// the coins' moves of a frame whose object pass an inline event holds (session.rs)
pub fn held_moves(w: &mut World) {
    if let Some(f) = w.field.as_mut() {
        f.coins_moved = true;
    }
    for i in 0..w.ents.len() {
        if !w.ents[i].removed && matches!(w.ents[i].body, Body::Pickup(_)) {
            coin_move(w, i);
        }
    }
}

/// the coin's draw check (0x0801809E, after its flight's move): not on the blink's hidden frames (age over 480, the
/// frame counter's bit 2), and only when its sprite / shadow rect overlaps the camera - the draw (0x08018284) counts
/// the spin, so a frame without it leaves the spin behind the age
fn coin_drawn(w: &World, p: &Pickup) -> bool {
    if p.t > 0x1E0 && w.g.clock(w.frame) & 4 != 0 {
        return false;
    }
    let PickupKind::Zenie(s) = p.kind else { return true };
    let (sw, sh) = if s & 1 == 1 { (16, 16) } else { (8, 8) };
    let (sx, sy) = (p.x >> 8, (p.y >> 8) - (p.z >> 8));
    let r = [p.bx[0].min(sx - sw / 2), p.bx[1].min(sy - sh), p.bx[2].max(sx + sw / 2), p.bx[3].max(sy)];
    let c = &w.cameras[0].rect;
    r[0] < c.x1 && c.x0 < r[2] && r[1] < c.y1 && c.y0 < r[3]
}

pub fn pickup_update(w: &mut World, i: usize) {
    // the pass a held event interrupted completes in the frame the event ends (`Field::completing`): the game's
    // event returned at the top of that frame, so the coins' move of the frame is their normal update - not the held
    // pass's move and another one (segment tour_world_sub_fight f1,041: the group's last kill, s506's prop_show from the
    // on-defeat callback - a big coin collected where it was the frame before; earth_3_3_4's coins: their age)
    if w.field.as_ref().is_some_and(|f| f.completing) {
        if let Body::Pickup(p) = &mut w.ents[i].body {
            if let Some((_, x, y, bx, z, vz)) = p.held.take().filter(|h| h.0 + 1 == w.frame) {
                p.x = x;
                p.y = y;
                p.bx = bx;
                p.z = z;
                p.vz = vz;
                p.mv -= 1;
                p.landed = false;
            }
        }
    }
    // an event holding the field stops the pickups (story playthrough: a coin's age does not run during the level-up
    // popup, so it outlives the 600-frame limit by the popup's length) - but not a coin's flight: its sixty moves
    // (counter +0x2C) run in every frame, the age (+0x24) only in the objects' own pass (Diablo Desert Express
    // 11-24: the coins of two robbers fly on and land under their event s373 and the scene after it)
    if w.field
        .as_ref()
        .is_some_and(|f| f.busy || f.held || f.drop_held)
    {
        // (one move per frame: the pass an inline event's end completes comes after the held frame's moves)
        if w.field
            .as_ref()
            .is_some_and(|f| f.drop_held || !f.coins_moved)
        {
            coin_move(w, i);
            // (the held frame's draw goes on, and counts the spin)
            {
                if let Body::Pickup(p) = &w.ents[i].body {
                    if matches!(p.kind, PickupKind::Zenie(_)) && coin_drawn(w, p) {
                        if let Body::Pickup(p) = &mut w.ents[i].body {
                            p.spin += 1;
                        }
                    }
                }
            }
        }
        return;
    }
    // (a link fight's item-get holds them the same way: the coins' age stops, their moves run on - link recording
    // dojo_ig7 f3,600 .. f3,661: age 20 throughout, the moves counter 21 .. 60)
    if w.link_get.is_some() {
        coin_move(w, i);
        if let Body::Pickup(p) = &mut w.ents[i].body {
            p.spin += 1;
        }
        return;
    }
    let Body::Pickup(p) = &w.ents[i].body else {
        return;
    };
    let mut p = (**p).clone();
    // the frame's trigger check (after the player's update, before this one) sees the coin where it was
    if matches!(p.kind, PickupKind::Zenie(_)) {
        p.spawn = Some((w.frame, p.coin_rect()));
    }
    p.t += 1;
    match p.kind {
        PickupKind::Zenie(_) => {
            coin_fly(w, &mut p);
            // (a frame without its draw: the spin's counter stays - `coin_drawn`)
            if w.players.len() == 1 && !coin_drawn(w, &p) {
                p.spin -= 1;
            }
            // (the age counts its creation frame's update; the coin goes when it reaches 601 - MEASURED real run 20-6 r062_001
            // f21,213 .. f21,814: age 600 at f21,813, still alive)
            if p.t > 600 {
                w.ents[i].removed = true;
            }
        }
        PickupKind::Supply(_) | PickupKind::Equipment(_) | PickupKind::MapItem { .. }
            if !p.landed =>
        {
            let (nx, ny) = (p.x + p.vx, p.y + p.vy);
            let (ddx, ddy) = ((nx >> 8) - (p.x >> 8), (ny >> 8) - (p.y >> 8));
            // collision sub-box inside the 16x16 icon (`0x0800DB64` from the icon's pixels: content `sub_box`)
            let sb = sub_box(w, p.kind);
            let sub = [
                p.bx[0] + sb[0] + ddx,
                p.bx[1] + sb[1] + ddy,
                p.bx[0] + sb[2] + ddx,
                p.bx[1] + sb[3] + ddy,
            ];
            if ddx == 0 && ddy == 0 || walk_free(w, sub) {
                p.x = nx;
                p.y = ny;
                p.bx = [p.bx[0] + ddx, p.bx[1] + ddy, p.bx[2] + ddx, p.bx[3] + ddy];
            }
            p.vz -= 0x80;
            p.z += p.vz;
            if p.z < 0 {
                p.z = (-p.z) >> 1;
                p.vz = (-p.vz) >> 1;
                if p.vz < 0x200 {
                    p.z = 0;
                    p.landed = true;
                }
            }
        }
        _ => {}
    }
    if let Body::Pickup(q) = &mut w.ents[i].body {
        **q = p;
    }
}

/// the coins' touch triggers sit at the end of the trigger list (scanned first): the first coin (newest first) whose
/// rect holds the point is collected; true = a trigger fired this frame
pub fn coin_check(w: &mut World, px: i32, py: i32, slot: u8) -> bool {
    for k in (0..w.ents.len()).rev() {
        if w.ents[k].removed {
            continue;
        }
        let Body::Pickup(p) = &w.ents[k].body else {
            continue;
        };
        // the coin's touch trigger (rect = its box grown by 19 / 11 / 4 / 4, moved with it) holds from its creation
        // (story playthrough: a coin dropped next to the player is collected in its first frame, in the air)
        if !matches!(p.kind, PickupKind::Zenie(_)) {
            continue;
        }
        // (a link fight's prize: only from its age 60 - link recording dojo_b3, the winner stands on the two coins for
        // their 39 first frames and takes them when back at frame 91; dojo_ig7: coins whose flight ended during a
        // supply's hold, age 20, are taken when their age reaches 60, not before)
        if w.opts.link && p.t <= 60 {
            continue;
        }
        let r = match p.spawn {
            Some((f, r)) if f == w.frame => r,
            _ => p.coin_rect(),
        };
        if r[0] <= px && px < r[2] && r[1] <= py && py < r[3] {
            collect(w, k, slot);
            return true;
        }
    }
    false
}

/// collect pickup entity `k`: coins add their value (SFX 44); equipment goes into the list at once, supplies count
/// after the 60-frame item-get; map items restore the grids and run their event after the item-get
pub fn collect(w: &mut World, k: usize, slot: u8) {
    let Body::Pickup(p) = &w.ents[k].body else {
        return;
    };
    let (kind, solid) = (p.kind, p.solid);
    w.ents[k].removed = true;
    if let Some(s) = solid {
        if let Some(c) = w.col.as_mut() {
            c.mark_both(s, 1);
        }
    }
    let mut then = None;
    let item = match kind {
        PickupKind::Zenie(size) => {
            // the touch trigger destroys the coin object at once: its tiles and its shadow's reference go in this
            // frame, not at the next pass's reap (whole-run cut 497,351 +154: the game's block (423, 4) free in the
            // frame the coin is touched)
            crate::vram::entity_gone(w, k);
            w.story
                .add_zenie(COIN_VALUE[size.clamp(0, 3) as usize] as i64);
            w.sfx(44, 64);
            w.events
                .push(Event::Field(FieldEvent::Collected { kind, slot }));
            return;
        }
        PickupKind::Supply(i) => i,
        PickupKind::Equipment(e) => -e,
        PickupKind::MapItem { obj, item } => {
            then = crate::field::area_obj(w, obj).and_then(|o| match o.kind {
                ObjKind::Item { on_pickup, .. } => on_pickup.map(|cb| (cb, Cause::Pickup { obj })),
                _ => None,
            });
            item
        }
    };
    let supply = if item >= 0 {
        Some(item)
    } else {
        w.story.add_equipment(-item);
        None
    };
    w.sfx(45, 64);
    // the item object goes at once (its tiles free before the icon's: tour_castle_ship_48 f167, the icon at (332, 4))
    crate::vram::entity_gone(w, k);
    // the item's icon object (as `show_item_get`'s: the icon's size)
    crate::vram::acquire(w, crate::vram::Owner::GetIcon, crate::scene::item_icon_bytes(item));
    // the hold pose: raw anim 14, cel 1, facing kept (state 0x0804985C)
    if let Some(pi) = w.player_idx(slot as usize) {
        crate::player::script_pose(w, pi, 13, 1);
    }
    if let Some(f) = w.field.as_mut() {
        f.item_get = Some(ItemGet {
            left: 61,
            slot,
            supply,
            then,
            item: Some(item),
        });
    }
    w.events
        .push(Event::Field(FieldEvent::Collected { kind, slot }));
}

/// A link fight's item-get: the prize's supply picked up with A (the field's presentation, `ItemGet`, in a battle).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LinkGet {
    /// frames left (61, the field's)
    pub left: i32,
    pub slot: u8,
    pub supply: Option<i32>,
    /// the objects the interrupted pass had not reached: their update comes at the end
    pub rest: Vec<EntityId>,
}

/// a link fight's A press that hit nobody (`player::melee`): the interact message to the objects last to first - a
/// dropped supply under the facing's box is picked up as in the field: the item goes, the fighter holds it up (raw
/// 14, cel 1, its tick kept) and the fight holds 61 frames (link recording dojo_c9 f6,981: the winner's A by the
/// prize's supply - no regeneration, no round clock, the knocked-down fighter's animation runs on)
pub fn link_interact(w: &mut World, pi: usize) -> bool {
    if w.link_get.is_some() {
        return false;
    }
    let (px, py, facing) = {
        let a = &w.pl_ref(pi).a;
        (a.px(), a.py(), a.facing)
    };
    let me = w.ents[pi].id;
    let slot = w.players.iter().position(|&id| id == me).unwrap_or(0) as u8;
    let b = crate::fieldnpc::TALK_BOX[facing as usize & 3];
    let bx = [px + b[0], py + b[1], px + b[2], py + b[3]];
    let ov = |r: [i32; 4]| r[0] < bx[2] && bx[0] < r[2] && r[1] < bx[3] && bx[1] < r[3];
    for k in (0..w.ents.len()).rev() {
        if w.ents[k].removed {
            continue;
        }
        let Body::Pickup(p) = &w.ents[k].body else { continue };
        if !p.by_a() || !ov(p.rect()) {
            continue;
        }
        let supply = match p.kind {
            PickupKind::Supply(s) => Some(s),
            _ => None,
        };
        w.ents[k].removed = true;
        crate::vram::entity_gone(w, k);
        w.sfx(45, 64);
        let tick = w.pl_ref(pi).ap.tick;
        crate::player::script_pose(w, pi, 13, 1);
        crate::player::set_tick(w, pi, tick);
        w.link_get = Some(LinkGet { left: 61, slot, supply, rest: Vec::new() });
        return true;
    }
    false
}

/// the link item-get's frame (after the object pass, as the field's `item_get_step`): the count; at 0 the fighter
/// goes idle (SetAnim(0)); a frame later the objects the pass had not reached get their update and the supply counts
pub fn link_get_step(w: &mut World) {
    let Some(g) = w.link_get.as_mut() else { return };
    g.left -= 1;
    if g.left > 0 {
        return;
    }
    if g.left == 0 {
        let slot = g.slot as usize;
        if let Some(pi) = w.player_idx(slot) {
            crate::player::set_idle(w, pi);
        }
        return;
    }
    let g = w.link_get.take().unwrap();
    for id in g.rest.into_iter().rev() {
        let Some(k) = w.find(id).filter(|&k| !w.ents[k].removed) else { continue };
        w.update_entity(k);
    }
    if let Some(s) = g.supply {
        w.story.add_supply(s, 1);
    }
}

/// the item-get presentation's frame (blocking: the players get no input, no trigger check); at its end the supply
/// counts and the map item's event runs
pub fn item_get_step(w: &mut World) {
    let Some(f) = w.field.as_mut() else { return };
    let Some(g) = f.item_get.as_mut() else { return };
    g.left -= 1;
    if g.left > 0 {
        return;
    }
    if g.left == 0 {
        // the presentation ends: the player leaves the hold pose (SetAnim(0)); the count and the event follow a frame later
        let slot = g.slot as usize;
        crate::vram::release(w, crate::vram::Owner::GetIcon);
        if let Some(pi) = w.player_idx(slot) {
            if !crate::player::resume_it(w, pi) {
                crate::player::set_idle(w, pi);
            }
        }
        return;
    }
    let g = f.item_get.take().unwrap();
    // the presentation's loop returns into the object pass it interrupted: the objects that pass had not reached (older
    // than the player) get their full update now, after this frame's state-only tick of the loop (emulator, Nataday
    // 15-3 dr4_006 f301: the enemy's tick from lr 0x0800F5E1, the presentation's state tick, then from 0x0800EF29,
    // the actor update - its AI moves it; objects newer than the player get the state tick only)
    let rest = std::mem::take(&mut f.item_get_rest);
    if g.then.is_none() {
        for id in rest.into_iter().rev() {
            let Some(k) = w.find(id).filter(|&k| !w.ents[k].removed) else { continue };
            match w.ents[k].body {
                Body::Enemy(_) => crate::enemy::update(w, k),
                Body::Npc(_) => crate::scene::npc_update(w, k),
                _ => {}
            }
        }
    }
    if let Some(s) = g.supply {
        w.story.add_supply(s, 1);
    }
    if let Some((run, cause)) = g.then {
        w.events.push(Event::Field(FieldEvent::Run {
            run,
            cause,
            slot: g.slot,
        }));
        // the map item's event runs inside the presentation's caller (the player's update): the frame it ends in
        // completes the interrupted object pass (4-30: the scientist thinks in the frame s197's text box closes)
        if let Some(f) = w.field.as_mut().filter(|f| f.host) {
            f.held_event = true;
        }
    }
}

pub fn map_sprite_update(w: &mut World, i: usize) {
    if let Body::MapSprite(m) = &mut w.ents[i].body {
        m.t += 1;
    }
}

/// a dropped item's collision sub-box relative to its icon (content; default the lower half)
fn sub_box(w: &World, kind: PickupKind) -> [i32; 4] {
    let fd = w.data.field.as_ref();
    let b = match kind {
        PickupKind::Supply(i) => fd.and_then(|f| f.supply_box.get(&i).copied()),
        PickupKind::Equipment(e) => fd.and_then(|f| f.equip_box.get(&e).copied()),
        PickupKind::MapItem { item, .. } if item >= 0 => {
            fd.and_then(|f| f.supply_box.get(&item).copied())
        }
        PickupKind::MapItem { item, .. } => fd.and_then(|f| f.equip_box.get(&-item).copied()),
        _ => None,
    };
    b.unwrap_or([0, 8, 16, 16])
}

/// a breakable's debris chip (vt 0x08049F38, display only: 8x8 `effects/debris`, 32 frames)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Debris {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub t: i32,
    /// its place in the world list: the entity id the next object got when it was made (newer than every entity
    /// below it); 0 = unknown (imported: the pass's end)
    #[serde(default)]
    pub stamp: u32,
}

/// `0x0800C912`: 4 chips from the box centre - 4 px, velocities (96, 0) (-96, 0) (0, 64) (0, -64) (24.8), no RNG
fn spawn_debris(f: Option<&mut Box<crate::field::Field>>, cx: i32, cy: i32, stamp: u32) {
    let Some(f) = f else { return };
    for (vx, vy) in [(96, 0), (-96, 0), (0, 64), (0, -64)] {
        f.debris.push(Debris {
            x: (cx - 4) << 8,
            y: (cy - 4) << 8,
            vx,
            vy,
            t: 0,
            stamp,
        });
    }
}

/// the chips on their last update end at their place in the object pass (last to first): before the update of the
/// first entity older than them (`id` below their stamp; 0 = at the pass's end) - their graphic reference goes
/// before an older object's allocation (tour_castle_ship_21 f190: the debris graphic freed, then the picked-up item,
/// then its icon allocated) and after a newer one's (tour_desert_d f3188: a later puff deleted first)
pub fn debris_end_before(w: &mut World, id: u32) {
    if w.field
        .as_ref()
        .is_some_and(|f| f.busy && f.busy_pre_pass == w.frame && f.busy_since + 1 == w.frame)
    {
        return;
    }
    let steps = 1 + w.field.as_ref().is_some_and(|f| f.break_owed == w.frame) as i32;
    let Some(f) = w.field.as_mut() else { return };
    let ends = |d: &Debris| d.t + steps >= 33 && (id == 0 || d.stamp > id);
    let gone = f.debris.iter().filter(|d| ends(d)).count();
    f.debris.retain(|d| !ends(d));
    for _ in 0..gone {
        crate::vram::keyed_release(w, crate::vram::KEY_DEBRIS);
    }
}

/// the chips' motion (update 0x0800C4A6: the velocity + (1, 2) - right and down, whatever its sign - then the position
/// by it; emulator, HFIL 18-10 f187: the first hit's chips 25 updates on at vx 121 / -71, vy 114 / -14)
pub fn debris_step(w: &mut World) {
    // (the frame a break's callback event takes: no object is updated, see fx::update)
    if w.field
        .as_ref()
        .is_some_and(|f| f.busy && f.busy_pre_pass == w.frame && f.busy_since + 1 == w.frame)
    {
        return;
    }
    let twice = w.field.as_ref().is_some_and(|f| f.break_owed == w.frame);
    let Some(f) = w.field.as_mut() else { return };
    for _ in 0..1 + twice as i32 {
        for d in f.debris.iter_mut() {
            // (a chip made in this frame's pass is past the pass: it moves from the next frame on - emulator, 18-10
            // f162 / f163: 0x20 updates left and still at the centre, then 0x1F one step out)
            if d.t > 0 {
                d.vx += 1;
                d.vy += 2;
                d.x += d.vx;
                d.y += d.vy;
            }
            d.t += 1;
        }
    }
    // a chip object lives 32 updates, the first one in the frame after the hit (its shared tiles: segments sb
    // 21-11 f252, and sb 21-5 f569 with the held frame)
    let gone = f.debris.iter().filter(|d| d.t >= 33).count();
    f.debris.retain(|d| d.t < 33);
    for _ in 0..gone {
        crate::vram::keyed_release(w, crate::vram::KEY_DEBRIS);
    }
}
