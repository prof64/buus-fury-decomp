p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/menu/minigame.rs'
s = open(p).read()
a = s[s.index("/// Watch This! (`0x08027DB0(level)`)"):s.index("/// `run_power_struggle` (0xC6)")]
b = r'''/// what the 160x64 surface holds (`+0x34`, the box picture `0x08071758` + text + button icons)
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Surf {
    /// string id of the bold outlined line at (80, 28), 0 = none
    pub text: u16,
    /// button icon per slot (x 20 + 24 i, y 44): 0 A, 1 B, 2 RIGHT, 3 LEFT, 4 UP, 5 DOWN; -1 none
    pub icons: [i8; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WState {
    /// `0x0802812C`: the box zooms in (+0x14 = 1..8)
    Open,
    /// `0x08028034`: the icons appear one by one, then disappear, every `period` frames
    Show,
    /// `0x08027FB8`: the player's five presses
    Input,
    /// `0x08027F74`: 16 frames
    Wait,
    /// `0x08027EEE`: "Fusion-Ha!" blinking (32 frames)
    Blink,
    /// `0x08027EB0`: the box zooms out, then the verdict
    Close,
}

/// Watch This! (`watch_this_game(level)` 0xA7: mode vt `0x0804A3E8`, game object `0x08028144` vt `0x0804C9C8`).
/// The construction draws 5 x rand(6) (the buttons to repeat); the state runs in the mode's *update* (before the
/// VBlank wait), so it reads the input of the previous iteration. Inputs: any of A B RIGHT LEFT UP DOWN pressed;
/// the button = the lowest pressed bit (SELECT / START count as A / B: `b >= 2 ? b - 2`). Win = the sequence or
/// the fixed code UP B LEFT DOWN A (`0x0874EA60`); the mode ends with 2 (win) / 1, the native pushes code - 1.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Watch {
    pub level: i32,
    /// +0x20: frames per icon = (8 - level) * 3
    pub period: i32,
    pub seq: [u8; 5],
    pub input: [u8; 5],
    pub state: WState,
    /// +0x14 zoom step (0 = not drawn), +0x18 counter, +0x1C countdown, +0x10 verdict
    pub k: i32,
    pub i: i32,
    pub cd: i32,
    pub verdict: i32,
    /// the drawing surface and what the OBJ tiles hold (uploaded copies; `None` = the plain box)
    pub surf: Surf,
    pub vram: Surf,
    /// OAM: the zoom step of the previous / last draw
    pub oam_shown: i32,
    pub oam_last: i32,
}

/// the code that always wins (`0x0874EA60`): UP B LEFT DOWN A
pub const WATCH_CODE: [u8; 5] = [4, 1, 3, 5, 0];
/// the icons' pictures (`0x0874EA68`: the button prompts' first frames)
const ICONS: [&str; 6] = ["ui/buttons/a.png", "ui/buttons/b.png", "ui/buttons/right.png", "ui/buttons/left.png", "ui/buttons/up.png", "ui/buttons/down.png"];

impl Watch {
    /// `0x08028144`: the box surface (uploaded), five rand(6), "Watch This!" drawn (not uploaded yet)
    pub fn new(level: i32, seq: [u8; 5]) -> Watch {
        let none = Surf { text: 0, icons: [-1; 5] };
        Watch {
            level,
            period: (8 - level) * 3,
            seq,
            input: [0; 5],
            state: WState::Open,
            k: 0,
            i: 0,
            cd: 0,
            verdict: 0,
            surf: Surf { text: 148, icons: [-1; 5] },
            vram: none,
            oam_shown: 0,
            oam_last: 0,
        }
    }
    /// the five rand(6) of the construction, in the game's order
    pub fn roll(rng: &mut crate::rng::Rng) -> [u8; 5] {
        let mut s = [0u8; 5];
        for x in s.iter_mut() {
            *x = rng.rand(6) as u8;
        }
        s
    }
    fn upload(&mut self) {
        self.vram = self.surf.clone();
    }
    fn reset_box(&mut self, text: u16) {
        self.surf = Surf { text, icons: [-1; 5] };
    }
    /// the game object's update (the state function at +0x30), once per mode iteration
    pub fn update(&mut self, pressed: u16) {
        match self.state {
            WState::Open => {
                self.k += 1;
                if self.k >= 8 {
                    self.state = WState::Show;
                    self.i = 0;
                    self.cd = 1;
                }
            }
            WState::Show => {
                self.cd -= 1;
                if self.cd != 0 {
                    return;
                }
                if self.i >= 10 {
                    self.reset_box(149);
                    self.upload();
                    self.state = WState::Input;
                    self.i = 0;
                    return;
                }
                if self.i >= 5 {
                    // 0x0803F8E2: the box picture copied back over the icon (index 0 skipped)
                    self.surf.icons[(self.i - 5) as usize] = -1;
                } else {
                    self.surf.icons[self.i as usize] = self.seq[self.i as usize] as i8;
                }
                self.upload();
                self.i += 1;
                self.cd = self.period;
            }
            WState::Input => {
                if pressed & (A | B | RIGHT | LEFT | UP | DOWN) == 0 {
                    return;
                }
                // 0x08040322: the lowest pressed bit
                let mut b = pressed.trailing_zeros() as i32;
                if b >= 2 {
                    b -= 2;
                }
                let k = self.i as usize;
                self.input[k] = b as u8;
                self.surf.icons[k] = b as i8;
                self.i += 1;
                self.upload();
                if self.i == 5 {
                    self.state = WState::Wait;
                    self.i = 0;
                }
            }
            WState::Wait => {
                self.i += 1;
                if self.i >= 16 {
                    self.reset_box(150);
                    self.upload();
                    self.state = WState::Blink;
                    self.i = 0;
                }
            }
            WState::Blink => {
                self.i += 1;
                if self.i >= 32 {
                    self.state = WState::Close;
                }
                if self.i & 7 == 0 {
                    if self.i & 8 != 0 {
                        self.vram = Surf { text: 0, icons: [-1; 5] };
                    } else {
                        self.upload();
                    }
                }
            }
            WState::Close => {
                self.k -= 1;
                if self.k == 0 {
                    self.verdict = if self.input == WATCH_CODE || self.input == self.seq { 2 } else { 1 };
                }
            }
        }
    }
    fn before_draw(&mut self) {
        self.oam_shown = self.oam_last;
        self.oam_last = self.k;
    }
    /// `0x08028224`: three affine OBJs (64x64, 64x64, 32x64; tiles of the 160x64 surface) at y 48, PA = PD =
    /// 2048 / k, x = (40k - 88(k - 8)) / 8, (104k - 88(k - 8)) / 8, (168k - 104(k - 8)) / 8
    fn view(&self, _w: &World, d: &MenuData) -> MenuView {
        let mut v = MenuView { overlay: true, ..Default::default() };
        let k = self.oam_shown;
        if k <= 0 {
            return v;
        }
        let t = &d.text;
        let mut cv = Canvas::new();
        cv.image("ui/images/dialog_box_0.png", 0, 160, 64, 0, 0);
        if self.vram.text != 0 {
            let s = t.s(self.vram.text as usize);
            let m = t.font(MenuFont::Bold).measure_n(s, s.len());
            cv.ops.push(MenuOp::TextOutline { font: MenuFont::Bold, x: 80 - ((m + 1) >> 1), y: 28, colour: 2, outline: 1, clip: [0, 0, 160, 64], text: s.to_vec() });
        }
        for (i, &ic) in self.vram.icons.iter().enumerate() {
            if ic >= 0 {
                cv.image(ICONS[ic as usize], 0, 16, 16, 20 + 24 * i as i32, 44);
            }
        }
        let tr = |n: i32| if n < 0 { -((-n) / 8) } else { n / 8 };
        let x0 = tr(40 * k - 88 * (k - 8));
        let x1 = tr(104 * k - 88 * (k - 8));
        let x2 = tr(168 * k - 104 * (k - 8));
        cv.ops.push(MenuOp::AffineObjs { pieces: vec![[0, 0, 64, 64, x0, 48], [64, 0, 64, 64, x1, 48], [128, 0, 32, 64, x2, 48]], pa: 2048 / k });
        v.layers.push(MenuLayer { w: 240, h: 160, x: 0, y: 0, priority: 0, ops: cv.ops });
        v
    }
}

/// `watch_this_game(level)` (0xA7): draws the five rand(6) from `World.rng` now; the native pushes
/// `World.menu.result.code - 1` (1 = repeated, 0 = failed)
pub fn open_watch(w: &mut World, level: i32) {
    let held = w.input[0].held;
    let seq = Watch::roll(&mut w.rng);
    let mut s = Minigame::base(Kind::Watch, Gauge::default(), w);
    s.watch = Some(Watch::new(level, seq));
    open_overlay(w, s, held);
}

'''
s = s.replace(a, b)
# Page impl: update hook and watch view / step
s = s.replace('''    fn before_draw(&mut self, w: &World) {
        if let Some(wt) = self.watch.as_mut() {
            wt.before_draw();
            return;
        }''', '''    fn update(&mut self, w: &World) {
        if let Some(wt) = self.watch.as_mut() {
            wt.update(w.menu.pressed);
        }
    }
    fn before_draw(&mut self, w: &World) {
        if let Some(wt) = self.watch.as_mut() {
            wt.before_draw();
            return;
        }''')
s = s.replace('''        if let Some(wt) = self.watch.as_mut() {
            return match wt.step(w) {
                Some(r) => Action::Pop(r),
                None => Action::None,
            };
        }''', '''        if let Some(wt) = &self.watch {
            // the mode's step returns the verdict: non-zero ends the mode
            return if wt.verdict != 0 { Action::Pop(wt.verdict) } else { Action::None };
        }''')
s = s.replace('''    if let Some(Screen::Minigame(mut m)) = w.menu.stack.pop() {
        m.before_draw(w);''', '''    if let Some(Screen::Minigame(mut m)) = w.menu.stack.pop() {
        // the first iteration's update runs in the native's frame (Watch This: its state; then the draw)
        m.update(w);
        m.before_draw(w);''')
s = s.replace('''        "punch" => Minigame::punch(w, v["mode"].as_i64().unwrap_or(0) as i32),
        _ => return None,
    };''', '''        "punch" => Minigame::punch(w, v["mode"].as_i64().unwrap_or(0) as i32),
        "watch_this" => {
            // the sequence: the capture's (the remake's harness world has another RNG state); else rolled from a
            // copy of the world's RNG
            let mut seq = Watch::roll(&mut w.rng.clone());
            if let Some(a) = v["seq"].as_array() {
                for (k, x) in a.iter().take(5).enumerate() {
                    seq[k] = x.as_u64().unwrap_or(0) as u8;
                }
            }
            let mut m = Minigame::base(Kind::Watch, Gauge::default(), w);
            m.watch = Some(Watch::new(v["level"].as_i64().unwrap_or(0) as i32, seq));
            m
        }
        _ => return None,
    };''')
open(p, 'w').write(s)
print('ok')
