//! Actor machinery shared by the player and the enemies: 24.8 positions, the collision footprint (pos .. pos +
//! header shadow w/h, obj+0x19C), the per-direction body (hurt) box (0x0800FE78), height offset (obj+0x176), and
//! the time-driven sprite animation enemies use (engine anim number n = raw n + 1).

use crate::content::{AnimRef, GameData};
use crate::damage::CombatStats;
use crate::geom::{Dir, Rect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor {
    pub x: i32,
    pub y: i32,
    /// height offset in px (negative = up)
    pub z: i32,
    pub facing: Dir,
    pub stats: CombatStats,
    pub blocking: bool,
    pub hittable: bool,
    pub foot_w: i32,
    pub foot_h: i32,
    pub body: [[i32; 4]; 4],
    /// sprite index
    pub sprite: Option<i32>,
    /// animation index in the sprite
    pub anim: Option<u16>,
    pub anim_t: i32,
    pub anim_loop: Option<bool>,
    pub flash: i32,
    /// cosmetic opacity in 1/256
    pub alpha: i32,
}

impl Default for Actor {
    fn default() -> Self {
        Actor {
            x: 0,
            y: 0,
            z: 0,
            facing: 0,
            stats: CombatStats::default(),
            blocking: false,
            hittable: true,
            foot_w: 16,
            foot_h: 8,
            body: [
                [0, -22, 16, 8],
                [0, -22, 16, 8],
                [0, -23, 16, 7],
                [0, -23, 16, 7],
            ],
            sprite: None,
            anim: None,
            anim_t: 0,
            anim_loop: None,
            flash: 0,
            alpha: 256,
        }
    }
}

impl Actor {
    #[inline]
    pub fn px(&self) -> i32 {
        self.x >> 8
    }
    #[inline]
    pub fn py(&self) -> i32 {
        self.y >> 8
    }
    pub fn body_box(&self) -> Rect {
        let b = self.body[self.facing as usize & 3];
        Rect::new(
            self.px() + b[0],
            self.py() + b[1] + self.z,
            self.px() + b[2],
            self.py() + b[3] + self.z,
        )
    }
    /// the body box at the ground position (no height)
    pub fn body_box_ground(&self) -> Rect {
        let b = self.body[self.facing as usize & 3];
        Rect::new(
            self.px() + b[0],
            self.py() + b[1],
            self.px() + b[2],
            self.py() + b[3],
        )
    }
    pub fn foot_box(&self) -> Rect {
        Rect::new(
            self.px(),
            self.py(),
            self.px() + self.foot_w,
            self.py() + self.foot_h,
        )
    }
    pub fn center(&self) -> (i32, i32) {
        self.body_box().center()
    }
    pub fn foot_center(&self) -> (i32, i32) {
        (
            self.px() + (self.foot_w >> 1),
            self.py() + (self.foot_h >> 1),
        )
    }
    /// footprint-rect centre ((x0 + x1) >> 1 of the footprint)
    pub fn fc(&self) -> (i32, i32) {
        (
            (self.px() * 2 + self.foot_w) >> 1,
            (self.py() * 2 + self.foot_h) >> 1,
        )
    }
    pub fn set_sprite(&mut self, d: &GameData, s: Option<i32>) {
        self.sprite = s;
        if let Some(sp) = s.and_then(|i| d.sprite(i)) {
            self.body = sp.body;
            self.foot_w = sp.foot.0;
            self.foot_h = sp.foot.1;
        }
    }
    /// set by engine anim number / name (first that exists, then 'idle')
    pub fn set_anim(
        &mut self,
        d: &GameData,
        refs: &[AnimRef],
        restart: bool,
        looped: Option<bool>,
    ) {
        let Some(sp) = self.sprite.and_then(|i| d.sprite(i)) else {
            return;
        };
        let Some(a) = sp.pick(refs) else { return };
        if Some(a) != self.anim || restart {
            self.anim_t = 0;
        }
        self.anim = Some(a);
        self.anim_loop = looped;
    }
    pub fn anim_name<'a>(&self, d: &'a GameData) -> Option<&'a str> {
        let sp = d.sprite(self.sprite?)?;
        sp.anims.get(self.anim? as usize).map(|a| a.name.as_str())
    }
    pub fn anim_raw(&self, d: &GameData) -> Option<i32> {
        let sp = d.sprite(self.sprite?)?;
        sp.anims.get(self.anim? as usize).map(|a| a.raw)
    }
    pub fn anim_length(&self, d: &GameData) -> i32 {
        let Some(sp) = self.sprite.and_then(|i| d.sprite(i)) else {
            return 0;
        };
        match self.anim.and_then(|a| sp.anims.get(a as usize)) {
            Some(a) => a.length(self.facing) as i32,
            None => 0,
        }
    }
    pub fn anim_done(&self, d: &GameData) -> bool {
        self.anim.is_none() || self.anim_t >= self.anim_length(d)
    }
    /// the step index shown (obj+0x11)
    pub fn anim_frame(&self, d: &GameData) -> i32 {
        let Some(sp) = self.sprite.and_then(|i| d.sprite(i)) else {
            return 0;
        };
        let Some(a) = self.anim.and_then(|a| sp.anims.get(a as usize)) else {
            return 0;
        };
        let looped = self.anim_loop.unwrap_or(a.looped);
        a.step_index_at(self.facing, self.anim_t, looped)
            .map(|i| i as i32)
            .unwrap_or(0)
    }
    pub fn tick_anim(&mut self) {
        self.anim_t += 1;
        if self.flash > 0 {
            self.flash -= 1;
        }
    }
}
