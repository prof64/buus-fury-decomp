//! The contract between a deterministic simulation (`wf-core`'s World, or the frontend's demo world) and the
//! frontend (`wf-render`, `wf-app`, the native and web shells). See `engine/docs/frontend.md`.
//!
//! * The frontend owns the clock: it calls [`Sim::step`] once per game frame (59.7275 Hz) with every player's
//!   buttons. That is the only way the simulation state changes.
//! * After a step the frontend asks for a [`RenderView`] ([`Sim::view`], `&self`): plain data — which map, which
//!   sprite frames where, per-player cameras and HUD values. Rendering never touches simulation state, so the
//!   same state can be rendered any number of times (rewind / replay scrubbing / spectators / split screen).
//! * Side effects that are not state (sound, rumble, ...) leave the step as [`SimEvent`]s. They are a pure
//!   function of (state, inputs), so a replay reproduces them; the frontend may drop them while fast-forwarding.
//!
//! No dependencies, so `wf-core` can implement [`Sim`] without pulling in any frontend code.

/// GBA KEYINPUT bit layout (bit set = pressed; the hardware register is active-low, this is not).
pub mod buttons {
    pub const A: u16 = 1 << 0;
    pub const B: u16 = 1 << 1;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const RIGHT: u16 = 1 << 4;
    pub const LEFT: u16 = 1 << 5;
    pub const UP: u16 = 1 << 6;
    pub const DOWN: u16 = 1 << 7;
    pub const R: u16 = 1 << 8;
    pub const L: u16 = 1 << 9;
    pub const DPAD: u16 = RIGHT | LEFT | UP | DOWN;
    /// Names in bit order (for bindings UIs / config files).
    pub const NAMES: [&str; 10] = ["A", "B", "Select", "Start", "Right", "Left", "Up", "Down", "R", "L"];
}

/// One player's buttons for one frame (see [`buttons`]).
pub type Buttons = u16;

/// GBA screen = the game's logical camera size.
pub const GBA_W: i32 = 240;
pub const GBA_H: i32 = 160;
/// Frames per second of the original hardware (16.78 MHz / 280 896 cycles per frame).
pub const GBA_FPS: f64 = 16_777_216.0 / 280_896.0;

/// Everything the frontend needs to draw one frame. Built by [`Sim::view`]; contains no references into the
/// simulation. Reuse one instance across frames ([`RenderView::clear`]) to avoid allocations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderView {
    /// Simulation frame number this view was taken at.
    pub frame: u64,
    /// The map shown (BG layers) — `None` = backdrop only.
    pub map: Option<MapView>,
    /// OBJ layer: every sprite to draw this frame, in OAM order (index 0 is drawn on top among equal
    /// priorities — the renderer draws the list back to front).
    pub sprites: Vec<SpriteDraw>,
    /// One logical 240x160 camera per local player (index = player slot). Each local viewport is centred on
    /// its player's camera; zoomed-out views show more around it, the game logic only ever sees this rectangle.
    pub cameras: Vec<CameraView>,
    /// HUD values per player slot.
    pub hud: Vec<HudView>,
    /// Screen brightness fade: 0 = normal, 16 = black (GBA BLDY semantics, applied after composition).
    pub fade_black: u8,
    /// The dialogue box, if one is open (drawn above everything, like the game's priority-0 OBJs).
    pub dialog: Option<DialogView>,
}

/// An open dialogue box (`ui/dialog.json`): the renderer word-wraps `text` with the game's line fitter, pages it
/// by the style's line count and draws page `page`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DialogView {
    /// Text as the game stores it: `\n` new line, `\u{b}` page break, `\u{8}` + char(n) = colour n.
    pub text: String,
    /// Box style (0 = 4 lines, 1 = 2 lines).
    pub style: u8,
    pub page: u16,
    /// Box placement (the text's position code `!` top, `@` middle, `#` bottom).
    pub position: DialogPos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DialogPos {
    Top,
    Middle,
    #[default]
    Bottom,
}

impl RenderView {
    pub fn clear(&mut self) {
        self.frame = 0;
        self.map = None;
        self.sprites.clear();
        self.cameras.clear();
        self.hud.clear();
        self.fade_black = 0;
        self.dialog = None;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapView {
    /// Content id of the map (`maps/<id>/map.json`), e.g. `area_08_003`.
    pub id: String,
    /// Frames since the map was loaded: drives tile animations and wrap-layer drift exactly like the game.
    pub anim_frame: u32,
    /// Screen shake added to every BG scroll (px), like the game's shake offset.
    pub shake: (i32, i32),
}

/// Which pixels of a sprite sheet to draw.
#[derive(Clone, Debug, PartialEq)]
pub enum FrameSel {
    /// A frame index of the sheet (`frames[i]` in sprite.json / effect.json) with the step's flip.
    Frame { index: u16, flip: bool },
    /// Let the renderer resolve the animation step from content timing: `anim` name, direction
    /// (`down`/`up`/`left`/`right`/`any`), `t` = frames since the animation started.
    Anim { anim: String, dir: u8, t: u32, looping: bool },
}

/// One OBJ draw. Coordinates are world pixels of the entity position (the frame's anchor lands there).
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteDraw {
    /// Simulation entity id (debug overlays / picking; not used for drawing).
    pub entity: u32,
    /// Content key of the sheet: `sprites/<id>` or `effects/<id>`.
    pub sheet: String,
    pub frame: FrameSel,
    pub x: i32,
    pub y: i32,
    /// GBA OBJ priority 0..3: drawn above BGs of the same or a higher number, below BGs with a lower number
    /// (entities use 3, so BG0 tree tops at priority 2 cover them, see docs/frontend.md).
    pub priority: u8,
    /// Extra horizontal mirror on top of the step's own flip (GBA OBJ h-flip around the anchor).
    pub mirror: bool,
    pub effect: SpriteEffect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpriteEffect {
    #[default]
    None,
    /// Hit flash: every opaque pixel white.
    White,
    /// GBA semi-transparent OBJ: alpha blended over what is below with the map's EVA/EVB.
    SemiTransparent,
    /// Explicit blend `eva/16 * sprite + evb/16 * below`.
    Blend { eva: u8, evb: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CameraView {
    /// Top-left of the 240x160 logical camera in world pixels.
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HudView {
    pub name: String,
    pub level: u32,
    pub hp: i32,
    pub hp_max: i32,
    pub ep: i32,
    pub ep_max: i32,
    /// Super Saiyan / SS3 gauges (character record +0x1E / +0x20), drawn in the EP bar.
    pub ss: i32,
    pub ss3: i32,
    /// Experience line steps 0..72 (`0x0801363E(player, 72)`).
    pub xp72: u8,
    /// Selected skill id (icon `icons/skills/skill_NN.png`), `None` = empty slot.
    pub skill: Option<u8>,
    /// Use the `_alt` icon (not enough EP for the skill).
    pub skill_alt: bool,
    /// The game hides the HUD during dialogue / cut-scenes.
    pub hidden: bool,
}

/// Non-state outputs of a step, in emission order.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    Audio(AudioEvent),
    /// Free-form, for debugging tools.
    Debug(String),
}

/// Commands for the sound driver (`wf-audio` semantics: 15 music tracks + 16 SFX voices, an SFX request is
/// dropped when all 16 voices are busy).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioEvent {
    PlaySong(u16),
    StopSong,
    /// `volume` 0..64, `pan` -64..64.
    PlaySfx { id: u8, volume: u8, pan: i8 },
    StopSfx(u8),
    StopAllSfx,
    /// Options-menu volumes 0..8 (game defaults: music 4, SFX 6).
    SetMasterVolumes { music: u8, sfx: u8 },
}

impl AudioEvent {
    /// Compact 4-word encoding for crossing thread / wasm boundaries (kind, a, b, c).
    pub fn encode(self) -> [i32; 4] {
        match self {
            AudioEvent::PlaySong(id) => [1, id as i32, 0, 0],
            AudioEvent::StopSong => [2, 0, 0, 0],
            AudioEvent::PlaySfx { id, volume, pan } => [3, id as i32, volume as i32, pan as i32],
            AudioEvent::StopSfx(id) => [4, id as i32, 0, 0],
            AudioEvent::StopAllSfx => [5, 0, 0, 0],
            AudioEvent::SetMasterVolumes { music, sfx } => [6, music as i32, sfx as i32, 0],
        }
    }
    pub fn decode(w: [i32; 4]) -> Option<AudioEvent> {
        Some(match w[0] {
            1 => AudioEvent::PlaySong(w[1] as u16),
            2 => AudioEvent::StopSong,
            3 => AudioEvent::PlaySfx { id: w[1] as u8, volume: w[2] as u8, pan: w[3] as i8 },
            4 => AudioEvent::StopSfx(w[1] as u8),
            5 => AudioEvent::StopAllSfx,
            6 => AudioEvent::SetMasterVolumes { music: w[1] as u8, sfx: w[2] as u8 },
            _ => return None,
        })
    }
}

/// A deterministic simulation the frontend can drive. `wf-core`'s World implements this (directly or through a
/// thin adapter in `wf-app`); so does the frontend's demo world.
pub trait Sim {
    /// Advance one frame. `inputs[p]` = player p's buttons (missing players = 0). Events are appended.
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>);
    /// Describe the current state for drawing. Must not change the state.
    fn view(&self, out: &mut RenderView);
    /// Frame counter (number of steps taken).
    fn frame(&self) -> u64;
    /// Hash of the full state (desync checks / replays); 0 = not implemented.
    fn state_hash(&self) -> u64 {
        0
    }
    /// Serialized state for save states / rewind / rollback; `None` = not supported.
    fn snapshot(&self) -> Option<Vec<u8>> {
        None
    }
    /// Restore a [`Sim::snapshot`]. Returns false if unsupported or invalid.
    fn restore(&mut self, _snapshot: &[u8]) -> bool {
        false
    }
    /// Number of player slots this simulation reads from `inputs`.
    fn players(&self) -> usize {
        1
    }
    /// The concrete simulation, for tools that understand it (training overlays, ghosts); `None` = opaque.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}
