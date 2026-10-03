p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/menu/minigame.rs'
s = open(p).read()
def rep(a, b):
    global s
    assert a in s, a
    s = s.replace(a, b, 1)
rep("counter: w.menu.ticks, watch: None }", "counter: w.story.play_frames as u32, watch: None }")
rep('''        Minigame::base(Kind::Punch, Gauge { x: 0x48, y: 0x80, value: BAR_MAX, peak: 0, vel''', '''        Minigame::base(Kind::Punch, Gauge { x: 0x48, y: 0x80, value: BAR_MAX, peak: -1, vel''')
rep('''    /// `0x080309F4(mode)`: the full gauge at (72, 128), the marker sweeping''', '''    /// `0x080309F4(mode)`: the full gauge at (72, 128), no marker until the first update, the marker sweeping''')
rep('''        self.counter = w.menu.ticks;
        if let Some(wt) = &self.watch {
            // the mode's step returns the verdict: non-zero ends the mode
            return if wt.verdict != 0 { Action::Pop(wt.verdict) } else { Action::None };
        }''', '''        self.counter = self.counter.wrapping_add(1);
        if let Some(wt) = &self.watch {
            // the mode's step returns the verdict: non-zero ends the mode
            let v = wt.verdict;
            return if v != 0 { self.finish(w, v) } else { Action::None };
        }''')
rep('''                let r = match self.kind {
                    Kind::Punch => self.g.peak,
                    _ => 0,
                };
                return Action::Pop(r);''', '''                let r = match self.kind {
                    Kind::Punch => self.g.peak,
                    _ => 0,
                };
                return self.finish(w, r);''')
rep('''    /// the object's pre-draw (vt +0xC''', '''    /// the mode ends in this iteration (no update after it): the screen shows the OAM of the last update
    fn finish(&mut self, w: &mut World, r: i32) -> Action {
        if let Some(wt) = self.watch.as_mut() {
            wt.oam_shown = wt.oam_last;
        } else {
            self.oam_shown = self.oam_last;
        }
        w.menu.shown = Some(Box::new(Screen::Minigame(self.clone())));
        Action::Pop(r)
    }

    /// the object's pre-draw (vt +0xC''')
rep('''    /// the mode loop's frame counter (`0x03000F94`, the punch marker's blink)
    pub counter: u32,''', '''    /// the mode loop's frame counter (`0x03000F94` since boot; the punch marker's blink): the remake has no boot
    /// counter, it starts from the play time (a harness passes the capture's)
    pub counter: u32,''')
rep('''        "punch" => Minigame::punch(w, v["mode"].as_i64().unwrap_or(0) as i32),
        "watch_this"''', '''        "punch" => {
            let mut m = Minigame::punch(w, v["mode"].as_i64().unwrap_or(0) as i32);
            if let Some(c) = v["counter"].as_u64() {
                m.counter = c as u32;
            }
            m
        }
        "watch_this"''')
open(p, 'w').write(s)

p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu/scenarios_minigame.py'
s = open(p).read()
a = '''def finish_result(frames, extra):
    """the native's push (stack top) after the script ended; Watch This: the sequence it drew"""
'''
assert a in s
s = s.replace(a, a + '''    if frames and extra['open'].get('kind') == 'punch':
        # the boot frame counter at frame 0 (the punch marker blinks with it)
        extra['open']['counter'] = frames[0]['lf']
''')
open(p, 'w').write(s)
print('ok')
