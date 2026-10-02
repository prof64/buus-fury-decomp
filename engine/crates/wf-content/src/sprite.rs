//! `sprites/<id>/sprite.json` and `effects/<id>/effect.json` (CONTENT.md §3).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{image, Error, Packs, Result};

pub const DIRS: [&str; 4] = ["down", "up", "left", "right"];

#[derive(Clone, Debug, Deserialize)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub anchor: Anchor,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Step {
    pub frame: u32,
    pub duration_frames: u32,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub attach: Option<[i32; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Anim {
    #[serde(default)]
    pub raw_index: Option<i32>,
    #[serde(rename = "loop", default)]
    pub looping: bool,
    #[serde(default)]
    pub timing_source: Option<String>,
    pub directions: BTreeMap<String, Vec<Step>>,
}

impl Anim {
    /// Steps for a direction index (0..3 = down, up, left, right); falls back to `any`, then to any direction.
    pub fn steps(&self, dir: u8) -> &[Step] {
        DIRS.get(dir as usize)
            .and_then(|d| self.directions.get(*d))
            .or_else(|| self.directions.get("any"))
            .or_else(|| self.directions.values().next())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Total length of one pass in frames.
    pub fn length(&self, dir: u8) -> u32 {
        self.steps(dir).iter().map(|s| s.duration_frames.max(1)).sum()
    }

    /// The step shown `t` frames after the animation started. Looping animations wrap; others hold the last
    /// step.
    pub fn step_at(&self, dir: u8, t: u32, looping: bool) -> Option<(usize, &Step)> {
        let steps = self.steps(dir);
        if steps.is_empty() {
            return None;
        }
        let total = self.length(dir);
        let mut t = if looping && total > 0 { t % total } else { t };
        for (i, s) in steps.iter().enumerate() {
            let d = s.duration_frames.max(1);
            if t < d {
                return Some((i, s));
            }
            t -= d;
        }
        Some((steps.len() - 1, steps.last().unwrap()))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpriteDef {
    pub id: String,
    #[serde(default)]
    pub sprite_index: Option<i32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    pub sheet: String,
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub animations: BTreeMap<String, Anim>,
    #[serde(default)]
    pub body_box: BTreeMap<String, [i32; 4]>,
    #[serde(default)]
    pub footprint: Option<[i32; 2]>,
    #[serde(default)]
    pub shadow: Option<String>,
    #[serde(default)]
    pub attachment: Option<serde_json::Value>,
}

impl SpriteDef {
    /// `key` = `sprites/<id>` or `effects/<id>`.
    pub fn load(packs: &Packs, key: &str) -> Result<SpriteDef> {
        let file = if key.starts_with("effects/") { "effect.json" } else { "sprite.json" };
        packs.json(&format!("{key}/{file}"))
    }

    pub fn anim(&self, name: &str) -> Option<&Anim> {
        self.animations.get(name)
    }

    /// Animation by the engine's raw index (`raw_index`).
    pub fn anim_by_raw(&self, raw: i32) -> Option<(&str, &Anim)> {
        self.animations.iter().find(|(_, a)| a.raw_index == Some(raw)).map(|(k, a)| (k.as_str(), a))
    }
}

/// A sprite definition with its decoded sheet.
pub struct Sheet {
    pub def: SpriteDef,
    pub image: image::Rgba,
}

impl Sheet {
    pub fn load(packs: &Packs, key: &str) -> Result<Sheet> {
        let def = SpriteDef::load(packs, key)?;
        let png = packs.read(&format!("{key}/{}", def.sheet)).ok_or_else(|| Error(format!("{key}: missing {}", def.sheet)))?;
        let image = image::decode_rgba(&png).map_err(|e| Error(format!("{key}: {e}")))?;
        Ok(Sheet { def, image })
    }
}
