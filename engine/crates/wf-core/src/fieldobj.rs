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
    spawn_debris(w.field.as_mut(), cx, cy);
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
    let id = w.ents[k].id;
    let rate = 0x1000 + w.rng.rand(0x1000);
    crate::fx::explosion(w, (bx[0] + bx[2]) >> 1, (bx[1] + bx[3]) >> 1, rate);
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
                Deferred::Break { at, .. } => *at <= now,
            });
            f.deferred = keep;
            due
        }
        None => return,
    };
    for d in due {
        match d {
            Deferred::Break { entity, .. } => break_done(w, entity),
        }
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
    // the bound map prop goes with it (it was still drawn: frontend objects_breakable_drops_8_1)
    if let Some(p) = w
        .field
        .as_mut()
        .and_then(|f| f.props.get_mut(prop.max(0) as usize))
    {
        p.visible = false;
    }
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
    let (lx, ly, lvx, lvy) = w.field.as_ref()?.loot;
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
    if !w.field.as_ref().is_some_and(|f| f.pre_update) {
        if let Some(k) = w.find(id) {
            pickup_update(w, k);
        }
    }
    Some(id)
}

/// `drop_item(i)` / `drop_equipment(e)` (`0x0800D30E`): item >= 0 supply, < 0 equipment (4 RNG draws)
pub fn drop_item(w: &mut World, item: i32) -> Option<EntityId> {
    let (lx, ly, lvx, lvy) = w.field.as_ref()?.loot;
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
    };
    let id = w.add(Body::Pickup(Box::new(p)));
    // its icon's tiles (0x100 bytes) and the 10 x 4 shadow, like a big coin (real run 4-1 tile r002_006+11,000 f104:
    // the drop's object takes block (135, 4) in the game's allocator)
    crate::vram::coin_created(w, id, true);
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

pub fn pickup_update(w: &mut World, i: usize) {
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
        if let Some(pi) = w.player_idx(slot) {
            if !crate::player::resume_it(w, pi) {
                crate::player::set_idle(w, pi);
            }
        }
        return;
    }
    let g = f.item_get.take().unwrap();
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
}

/// `0x0800C912`: 4 chips from the box centre - 4 px, velocities (96, 0) (-96, 0) (0, 64) (0, -64) (24.8), no RNG
fn spawn_debris(f: Option<&mut Box<crate::field::Field>>, cx: i32, cy: i32) {
    let Some(f) = f else { return };
    for (vx, vy) in [(96, 0), (-96, 0), (0, 64), (0, -64)] {
        f.debris.push(Debris {
            x: (cx - 4) << 8,
            y: (cy - 4) << 8,
            vx,
            vy,
            t: 0,
        });
    }
}

/// the chips' motion (+1 / +2 per frame on the velocity, INFERRED as acceleration away from the centre and down)
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
            d.x += d.vx;
            d.y += d.vy;
            d.vx += d.vx.signum();
            d.vy += 2;
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
