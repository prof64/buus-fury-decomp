p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/engine/crates/wf-core/src/menu/mod.rs'
s = open(p).read()
a = s[s.index("/// pause-menu entry timing (captures"):s.index("/// what the top screen's step asks for")]
b = '''/// pause-menu timing (captures `re/buu/menu/capture.py` status_open_close, frame 0 = START):
/// * frames 1..16: the fade-out mode (one iteration per frame, level 16k; frame 16 is black), the world frozen;
/// * frames 17..20 black: the world save, the page's mode enter and its first draw (CPU time; frame 20 shows the
///   VRAM copy mid-scanout, not modelled: black here), the page at frame 21, the first input read at 22;
/// * the closing iteration still shows the page; then 24 black frames (the exit stub's iteration + the world
///   restore), the 16-frame fade-in (levels 240..0), the world runs again the frame after.
pub const FADE_STEP: u16 = 16;
pub const ENTER_FRAMES: u16 = 5;
pub const LEAVE_FRAMES: u16 = 25;

/// One video frame of an active menu (called instead of the world update).
pub fn step(w: &mut World) {
    let held = w.input[0].held;
    match w.menu.phase {
        Phase::Idle => {}
        Phase::FadeOut { level } => {
            // the fade mode's iteration reads the input (edges of the first menu read are relative to it)
            w.menu.held = held;
            w.story.play_frames += 1;
            let l = level + FADE_STEP;
            w.menu.phase = if l >= 256 { Phase::Enter { t: 0 } } else { Phase::FadeOut { level: l } };
        }
        Phase::Enter { t } => {
            if t + 1 >= ENTER_FRAMES {
                w.menu.phase = Phase::Run;
                // the first update draws during the black frames: on screen now, no lag after it
                redraw(w, true);
            } else {
                w.menu.phase = Phase::Enter { t: t + 1 };
            }
        }
        Phase::Run => {
            if w.menu.lag > 0 {
                w.menu.lag -= 1;
                if w.menu.lag == 0 {
                    // the long update ends with the VRAM copy: the new surface shows in its last frame
                    if let Some(p) = w.menu.pending.take() {
                        w.menu.shown = Some(p);
                    }
                }
                return;
            }
            tick(w);
        }
        Phase::Leave { t } => {
            if t == 0 {
                // the exit stub's iteration (counts a logic frame)
                w.story.play_frames += 1;
            }
            if t + 1 >= LEAVE_FRAMES {
                w.menu.phase = Phase::FadeIn { level: 256 - FADE_STEP };
                w.story.play_frames += 1;
            } else {
                w.menu.phase = Phase::Leave { t: t + 1 };
            }
        }
        Phase::FadeIn { level } => {
            w.story.play_frames += 1;
            let l = level.saturating_sub(FADE_STEP);
            if l == 0 {
                w.menu.phase = Phase::Idle;
                w.menu.shown = None;
            } else {
                w.menu.phase = Phase::FadeIn { level: l };
            }
        }
    }
}

/// one iteration of the mode loop, from the input read: input, step (widgets, page logic), then the next
/// iteration's update (redraw if dirty; its CPU time becomes lag frames)
fn tick(w: &mut World) {
    let held = w.input[0].held;
    let m = &mut w.menu;
    m.pressed = held & !m.held;
    m.released = m.held & !held;
    m.held = held;
    m.ticks = m.ticks.wrapping_add(1);
    w.story.play_frames += 1;
    step_screen(w);
    if w.menu.phase == Phase::Run {
        redraw(w, false);
    }
}

/// update(): the mode's counters, then a redraw of the top screen when its container is not drawn. A redraw
/// costing n frames leaves n - 1 lag frames and the surface on screen in the last of them.
fn redraw(w: &mut World, first: bool) {
    let Some(mut top) = w.menu.stack.pop() else { return };
    top.update(w);
    if !top.container_ref().drawn {
        top.before_draw(w);
        top.container().drawn = true;
        let cost = if first { 1 } else { top.redraw_cost().max(1) };
        if cost == 1 {
            w.menu.shown = Some(Box::new(top.clone()));
            w.menu.pending = None;
        } else {
            w.menu.pending = Some(Box::new(top.clone()));
            w.menu.lag = cost - 1;
        }
    }
    w.menu.stack.push(top);
}

'''
s = s.replace(a, b)
s = s.replace("""    /// what is on screen: the state of the top screen at its last copy to VRAM
    pub shown: Option<Box<Screen>>,""", """    /// what is on screen: the state of the top screen at its last copy to VRAM
    pub shown: Option<Box<Screen>>,
    /// a redraw in progress (on screen when its lag frames end)
    pub pending: Option<Box<Screen>>,""")
s = s.replace("""fn close(w: &mut World) {
    w.menu.stack.clear();
    w.menu.phase = Phase::Leave { t: 0 };
    w.menu.lag = 0;""", """fn close(w: &mut World) {
    w.menu.stack.clear();
    w.menu.phase = Phase::Leave { t: 0 };
    w.menu.lag = 0;
    w.menu.pending = None;""")
s = s.replace("""    match w.menu.phase {
        Phase::Run => {}
        Phase::Enter { .. } | Phase::Leave { .. } => return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() }),
        _ => return None,
    }""", """    match w.menu.phase {
        // the closing iteration still shows the page
        Phase::Run | Phase::Leave { t: 0 } => {}
        Phase::Enter { .. } | Phase::Leave { .. } => return Some(MenuView { backdrop: 0, palette: vec![(0, 0)], ..Default::default() }),
        _ => return None,
    }""")
open(p, 'w').write(s)
print('ok')
