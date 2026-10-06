//! Link play in the app (engine/docs/netcode.md "Link play: playing it"): two headless title apps
//! ([`wf_app::link::LinkSim`]) on an in-process network with latency, jitter and loss. Scripted closed-loop inputs drive
//! both through the game's own title (START, Multiplayer, Dojo / Trading Post, the "Waiting for a friend..." box) and the
//! link screens into a Dojo match and a trade: both peers confirm the same match (frame, checksum, inputs), the prize
//! lands once in the winner's slot file; the trade conserves supplies / zenie across both slot files; a drop
//! mid-match / mid-trade ends in "Disconnected" with nothing applied. Headless, muted; needs the ROM (roms/buu.gba)
//! or WF_CONTENT.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use wf_app::link::{title_wait, LinkConfig, LinkKind, LinkSim};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::content::GameData;
use wf_core::input::{A, DOWN, LEFT, RIGHT, START, UP};
use wf_core::menu::save::{SaveData, Saves};
use wf_core::menu::title::TPage;
use wf_core::menu::Screen;
use wf_core::story::Story;
use wf_net::dojo::{DojoMode, DojoRules, Ending};
use wf_net::lobby::Role;
use wf_net::netsim::Clock;
use wf_net::transport::{Conditions, SimNet};
use wf_view::Sim;

fn content_dir() -> Option<PathBuf> {
    static D: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    D.get_or_init(extract).clone()
}

fn extract() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("WF_CONTENT") {
        return Some(PathBuf::from(d));
    }
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wf-app-link-content");
    if wf_extract::cache::fresh(&dir, &[]) {
        return Some(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    let rom = wf_rom::testutil::rom()?;
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    for (p, d) in &c.files {
        let rel = p.strip_prefix("base/").unwrap_or(p);
        let f = dir.join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, d).unwrap();
    }
    std::fs::write(dir.join(".complete"), wf_extract::cache::stamp(&[])).unwrap();
    Some(dir)
}

fn packs(d: &Path) -> Packs {
    Packs::new(vec![Box::new(DirSource(d.to_path_buf())) as Box<dyn FileSource>]).unwrap()
}

const PRIZE: usize = 21;

fn rules() -> DojoRules {
    // short rounds and at most 3 keep the test quick; the ROM gives no item, the test turns the prize on to check
    // it is paid once
    DojoRules { round_frames: 2400, between_frames: 60, max_rounds: 3, prize: Some(PRIZE), ..Default::default() }
}

/// a save whose party is the given characters (index, level), with supplies and zenie to trade
fn save(d: &GameData, party: &[(usize, i32)], supplies: &[(usize, u8)], zenie: u32) -> SaveData {
    let mut story = Story::new_game(d);
    for c in story.chars.iter_mut() {
        c.flags &= !1;
    }
    for &(i, lv) in party {
        story.chars[i].flags |= 1;
        story.chars[i].rec.level = lv;
        story.chars[i].rec.str_ += lv / 4;
        story.chars[i].rec.end += lv / 5;
    }
    story.current = party[0].0 as i32;
    story.supplies.iter_mut().for_each(|n| *n = 0);
    story.equipment.clear();
    for &(i, n) in supplies {
        story.supplies[i] = n;
    }
    story.zenie = zenie;
    SaveData { story, world: 2, area: 1, variant: 1, x: 100, y: 100, flag: false }
}

fn slot_bytes(sd: &SaveData) -> Vec<u8> {
    let mut s = Saves::default();
    s.set(0, Some(sd.clone()));
    s.slot_bytes(0)
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wf-link-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// one player: its app (the title with link play), its save folder, its scripted closed-loop driver
struct Player {
    sim: LinkSim,
    dir: PathBuf,
    host: bool,
    kind: LinkKind,
    /// the driver's own memory
    downs: u32,
    proposed: bool,
    /// steps in the current link phase
    in_phase: u32,
    last_page: Option<TPage>,
    /// the host's Character Mode
    mode: DojoMode,
    last_phase: &'static str,
    /// whether a flow has started (the title afterwards = done)
    started: bool,
    k: u64,
}

fn player(
    p: &Packs,
    net: &SimNet,
    clock: &Clock,
    name: &str,
    host: bool,
    kind: LinkKind,
    sd: &SaveData,
    endpoints: Rc<RefCell<u32>>,
) -> Player {
    let dir = tmp(name);
    let saves = dir.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    let bytes = slot_bytes(sd);
    std::fs::write(saves.join("slot1.sav"), &bytes).unwrap();
    let inner = wf_app::launcher::title_sim(p, &[bytes], 7).unwrap();
    let net = net.clone();
    let id = name.to_string();
    let cfg = LinkConfig {
        connect: Box::new(move |_k| {
            *endpoints.borrow_mut() += 1;
            Ok((Box::new(net.endpoint(&id)), if host { Role::Host } else { Role::Player }))
        }),
        clock: clock.clone(),
        link_id: format!("link-{name}"),
        name: name.into(),
        saves_dir: Some(saves),
        rules: rules(),
        seed: 11,
    };
    Player { sim: LinkSim::new(inner, cfg), dir, host, kind, downs: 0, proposed: false, in_phase: 0, last_page: None, mode: DojoMode::InGame, last_phase: "", started: false, k: 0 }
}

impl Player {
    /// the button this frame: presses on every 4th frame (held 2), the choice from what is shown
    fn input(&mut self) -> u16 {
        self.k += 1;
        let ph = self.sim.phase();
        if ph != self.last_phase {
            self.last_phase = ph;
            self.in_phase = 0;
            self.downs = 0;
        }
        self.in_phase += 1;
        if ph == "match" {
            // P1 (the host) walks in and punches, the guest stands still
            return if self.host { RIGHT | if self.k % 12 < 2 { A } else { 0 } } else { 0 };
        }
        if self.k % 4 >= 2 {
            return 0;
        }
        let want = self.decide(ph);
        if want & (DOWN | UP | LEFT | RIGHT) != 0 {
            self.downs += 1;
        }
        want
    }

    fn decide(&mut self, ph: &str) -> u16 {
        match ph {
            "title" => {
                if self.started {
                    return 0;
                }
                let w = &self.sim.inner.session.world;
                let t = w.menu.stack.iter().rev().find_map(|s| match s {
                    Screen::Title(t) => Some(t),
                    _ => None,
                });
                let Some(t) = t else { return START };
                if self.last_page != Some(t.page) {
                    self.last_page = Some(t.page);
                    self.downs = 0;
                }
                match t.page {
                    TPage::Title => START,
                    // Single Player / Multiplayer: down once
                    TPage::Mode => {
                        if self.downs < 1 {
                            DOWN
                        } else {
                            A
                        }
                    }
                    // Dojo / Trading Post (one down)
                    TPage::Multi => {
                        if t.wait.is_some() {
                            0
                        } else if self.kind == LinkKind::Trade && self.downs < 1 {
                            DOWN
                        } else {
                            A
                        }
                    }
                    _ => 0,
                }
            }
            "waiting" => {
                self.started = true;
                0
            }
            "saves" => A,
            "chars" => {
                let (_, picked, _) = self.sim.setup_state().unwrap();
                if picked {
                    0
                } else {
                    A
                }
            }
            "setup" => {
                let (row, _, s) = self.sim.setup_state().unwrap();
                if self.host {
                    // the mode, Rounds: 1, then Start Fight
                    if row == 0 && s.mode != self.mode {
                        RIGHT
                    } else if row == 0 {
                        DOWN
                    } else if row == 1 && s.rounds != 1 {
                        LEFT
                    } else if row == 1 {
                        DOWN
                    } else {
                        A
                    }
                } else if s.rounds == 1 && s.mode == self.mode {
                    // the guest goes on once the host's setup shows
                    A
                } else {
                    0
                }
            }
            "trade" => {
                let (sel, picking, btn, counts) = self.sim.trade_cursor().unwrap();
                let (accepted, _, friend) = self.sim.trade_state().unwrap();
                let n = counts.len();
                // offer: 2 of the first row (a supply), 30 zenie (the last row)
                let target = |k: usize| if k == 0 { 2 } else if k == n - 1 { 30 } else { 0 };
                if let Some(k) = (0..n).find(|&k| counts[k] != target(k)) {
                    if picking && sel == k {
                        return if counts[k] < target(k) { UP } else { DOWN };
                    }
                    if picking {
                        return A;
                    }
                    return if sel == k { A } else if sel < k { DOWN } else { UP };
                }
                if picking {
                    return A;
                }
                if sel < n {
                    return DOWN;
                }
                if !self.proposed {
                    if btn != 0 {
                        return LEFT;
                    }
                    self.proposed = true;
                    return A;
                }
                if friend.is_some_and(|o| !o.is_empty()) && !accepted {
                    return if btn != 1 { RIGHT } else { A };
                }
                0
            }
            // the result / TRADE COMPLETE / Disconnected: shown a while, then A
            "result" | "complete" | "message" | "nosaves" => {
                if self.in_phase > 40 {
                    A
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn done(&self) -> bool {
        self.started && self.sim.phase() == "title"
    }
    fn slot(&self) -> SaveData {
        wf_app::trade::read_slot(&self.dir.join("saves/slot1.sav")).expect("slot file")
    }
}

struct Run {
    packs: Packs,
    renderer: wf_render::Renderer,
    /// the link pages drawn (each once, when it first shows; `LINK_SHOTS=dir` saves them as PPM)
    drawn: Vec<String>,
    net: SimNet,
    t: Rc<Cell<f64>>,
    a: Player,
    b: Player,
}

fn run(name: &str, kind: LinkKind, sa: &SaveData, sb: &SaveData, seed: u64) -> Option<Run> {
    let dir = content_dir()?;
    let p = packs(&dir);
    let net = SimNet::new(Conditions { latency_ms: 80.0, jitter_ms: 20.0, loss: 0.05 }, seed);
    let t = Rc::new(Cell::new(0.0f64));
    let tc = t.clone();
    let clock: Clock = Rc::new(move || tc.get());
    let eps = Rc::new(RefCell::new(0));
    let a = player(&p, &net, &clock, &format!("{name}-host"), true, kind, sa, eps.clone());
    let b = player(&p, &net, &clock, &format!("{name}-guest"), false, kind, sb, eps);
    Some(Run { packs: p, renderer: wf_render::Renderer::new(), drawn: Vec::new(), net, t, a, b })
}

impl Run {
    fn step(&mut self) {
        self.t.set(self.t.get() + 1000.0 / 59.7275);
        let mut ev = Vec::new();
        let ia = self.a.input();
        self.a.sim.step(&[ia], &mut ev);
        let ib = self.b.input();
        self.b.sim.step(&[ib], &mut ev);
        // every link page renders (the game's window, fonts and cursor over the title picture)
        for (who, p) in [("host", &self.a), ("guest", &self.b)] {
            let ph = p.sim.phase();
            let key = format!("{who}-{ph}");
            if ph == "title" || ph == "closing" || self.drawn.contains(&key) {
                continue;
            }
            let mut v = wf_view::RenderView::default();
            p.sim.view(&mut v);
            assert!(v.menu.is_some() || ph == "match" || ph == "waiting", "{key}: a menu view");
            let px = self.renderer.render(&self.packs, &v, 0, 240, 160).to_vec();
            let colours: std::collections::BTreeSet<u32> = px.iter().copied().collect();
            assert!(colours.len() > 8, "{key}: drawn");
            if let Ok(d) = std::env::var("LINK_SHOTS") {
                let mut out = b"P6\n240 160\n255\n".to_vec();
                for c in &px {
                    out.extend([(c >> 16) as u8, (c >> 8) as u8, *c as u8]);
                }
                let _ = std::fs::create_dir_all(&d);
                std::fs::write(Path::new(&d).join(format!("{key}.ppm")), out).unwrap();
            }
            self.drawn.push(key);
        }
    }
    /// steps until `f` holds (panics after `max`)
    fn until(&mut self, max: u32, what: &str, f: impl Fn(&Run) -> bool) {
        for _ in 0..max {
            if f(self) {
                return;
            }
            self.step();
            if std::env::var("LINK_DEBUG").is_ok() && self.a.k % 500 == 0 {
                for p in [&self.a, &self.b] {
                    if let Some(s) = p.sim.dojo() {
                        eprintln!("{} f{} c{} {:?} r{}", p.k, s.frame(), s.confirmed(), s.game.st.phase, s.game.st.round);
                    } else if let Some(c) = p.sim.trade_cursor() {
                        eprintln!("{} trade {:?} {:?} {:?}", p.k, c, p.sim.trade_state(), p.sim.trade_rows());
                    } else {
                        eprintln!("{} {}", p.k, p.sim.phase());
                    }
                }
            }
        }
        panic!("{what}: host {} / guest {} ({:?} / {:?})", self.a.sim.phase(), self.b.sim.phase(), self.a.sim.last, self.b.sim.last);
    }
}

#[test]
fn dojo_match_between_two_apps() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    let sa = save(&d, &[(0, 90)], &[], 0);
    let sb = save(&d, &[(4, 60)], &[], 0);
    let Some(mut r) = run("dojo", LinkKind::Dojo, &sa, &sb, 3) else { return };
    // through the title into the room, the screens, the match
    r.until(3000, "both on the link screens", |r| r.a.sim.phase() != "title" && r.b.sim.phase() != "title" && r.a.started && r.b.started);
    r.until(3000, "the match", |r| r.a.sim.dojo().is_some() && r.b.sim.dojo().is_some());
    // the match runs to its end on both
    r.until(20_000, "the ending", |r| r.a.sim.record.is_some() && r.b.sim.record.is_some());
    let (ra, rb) = (r.a.sim.record.clone().unwrap(), r.b.sim.record.clone().unwrap());
    assert_eq!((ra.frame, ra.checksum), (rb.frame, rb.checksum), "both confirm the same match");
    assert_eq!(ra.inputs, rb.inputs);
    assert!(ra.frame > 60);
    match (&ra.ending, &rb.ending) {
        (Ending::Won { .. }, Ending::Lost) | (Ending::Lost, Ending::Won { .. }) | (Ending::Draw, Ending::Draw) => {}
        e => panic!("endings {e:?}"),
    }
    // back to the title's Multiplayer page on both
    r.until(2000, "back at the title", |r| r.a.done() && r.b.done());
    for p in [&r.a, &r.b] {
        assert!(title_wait(&p.sim.inner.session.world).is_none(), "the link box closed");
    }
    // the prize once, into the winner's slot file and its running title's slot
    let won = |e: &Ending| matches!(e, Ending::Won { .. });
    for (p, e, s0) in [(&r.a, &ra.ending, &sa), (&r.b, &rb.ending, &sb)] {
        let want = s0.story.supplies[PRIZE] as u32 + u32::from(won(e));
        assert_eq!(p.slot().story.supplies[PRIZE] as u32, want, "prize in the slot file");
        let live = p.sim.inner.session.world.saves.slot(0).unwrap();
        assert_eq!(live.story.supplies[PRIZE] as u32, want, "prize in the title's slot");
    }
    // (more steps: nothing more is paid)
    for _ in 0..200 {
        r.step();
    }
    for (p, e, s0) in [(&r.a, &ra.ending, &sa), (&r.b, &rb.ending, &sb)] {
        assert_eq!(p.slot().story.supplies[PRIZE] as u32, s0.story.supplies[PRIZE] as u32 + u32::from(won(e)));
    }
    let _ = r.net.stats();
}

#[test]
fn dojo_disconnect_mid_match() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    let sa = save(&d, &[(0, 90)], &[], 0);
    let sb = save(&d, &[(4, 60)], &[], 0);
    let Some(mut r) = run("dojo-drop", LinkKind::Dojo, &sa, &sb, 5) else { return };
    // Balanced: no KO before the drop
    r.a.mode = DojoMode::Balanced;
    r.b.mode = DojoMode::Balanced;
    r.until(6000, "the match", |r| r.a.sim.dojo().is_some_and(|s| s.frame() > 120) && r.b.sim.dojo().is_some());
    // the guest's network dies
    r.net.kill(&format!("{}", "dojo-drop-guest"));
    r.b.sim.drop_link();
    r.until(3000, "host: Disconnected", |r| r.a.sim.ending().is_some() || r.a.sim.record.is_some());
    assert_eq!(r.a.sim.record.as_ref().map(|x| x.ending.clone()), Some(Ending::Disconnected));
    r.until(2000, "host back at the title", |r| r.a.done());
    assert_eq!(r.a.slot(), sa, "no prize after a drop");
}

#[test]
fn trade_between_two_apps() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    // supplies 0..: the host has 5 of supply 0, the guest 4 of supply 1
    let sa = save(&d, &[(0, 20)], &[(0, 5)], 500);
    let sb = save(&d, &[(4, 20)], &[(1, 4)], 100);
    let Some(mut r) = run("trade", LinkKind::Trade, &sa, &sb, 9) else { return };
    r.until(6000, "the trade screens", |r| r.a.sim.phase() == "trade" && r.b.sim.phase() == "trade");
    r.until(6000, "TRADE COMPLETE on both", |r| {
        let ok = |p: &Player| p.sim.phase() == "complete" || (p.started && p.sim.last.as_deref() == Some("complete"));
        ok(&r.a) && ok(&r.b)
    });
    r.until(2000, "back at the title", |r| r.a.done() && r.b.done());
    let (a1, b1) = (r.a.slot(), r.b.slot());
    // 2 of the first supply and 30 zenie each way
    assert_eq!(a1.story.supplies[0], 3);
    assert_eq!(a1.story.supplies[1], 2);
    assert_eq!(b1.story.supplies[0], 2);
    assert_eq!(b1.story.supplies[1], 2);
    assert_eq!(a1.story.zenie, 500);
    assert_eq!(b1.story.zenie, 100);
    // conserved
    for i in 0..sa.story.supplies.len() {
        let before = sa.story.supplies[i] as u32 + sb.story.supplies[i] as u32;
        assert_eq!(a1.story.supplies[i] as u32 + b1.story.supplies[i] as u32, before, "supply {i}");
    }
    assert_eq!(a1.story.zenie + b1.story.zenie, sa.story.zenie + sb.story.zenie);
    // the running titles have the traded saves
    assert_eq!(r.a.sim.inner.session.world.saves.slot(0), Some(&a1));
    assert_eq!(r.b.sim.inner.session.world.saves.slot(0), Some(&b1));
}

#[test]
fn trade_disconnect_before_commit() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    let sa = save(&d, &[(0, 20)], &[(0, 5)], 500);
    let sb = save(&d, &[(4, 20)], &[(1, 4)], 100);
    let Some(mut r) = run("trade-drop", LinkKind::Trade, &sa, &sb, 13) else { return };
    r.until(6000, "the trade screens", |r| r.a.sim.phase() == "trade" && r.b.sim.phase() == "trade");
    // the guest leaves before anyone accepted
    r.net.leave("trade-drop-guest");
    r.b.sim.drop_link();
    r.until(3000, "host leaves after Disconnected", |r| r.a.done());
    assert_eq!(r.a.slot(), sa, "nothing applied");
    assert_eq!(r.b.slot(), sb, "nothing applied");
}
