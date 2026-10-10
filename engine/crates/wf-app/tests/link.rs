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

use wf_app::link::{title_wait, LinkBot, LinkConfig, LinkKind, LinkSim};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::content::GameData;
use wf_core::menu::save::{SaveData, Saves};
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

/// one player: its app (the title with link play, the scripted player inside), its save folder
struct Player {
    sim: LinkSim,
    dir: PathBuf,
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
    let _ = std::fs::write(saves.join("slot1.sav"), &bytes);
    *endpoints.borrow_mut() += 0;
    open(p, net, clock, &dir, &format!("link-{name}"), name, host, kind, endpoints)
}

/// a player's app from its folder: the save slot read from disk through the title's own startup (as the native
/// shell's `--link`: `link_title_app`), its link id; connections as endpoint `endpoint` (then `endpoint#k`)
#[allow(clippy::too_many_arguments)]
fn open(p: &Packs, net: &SimNet, clock: &Clock, dir: &Path, link_id: &str, endpoint: &str, host: bool, kind: LinkKind, endpoints: Rc<RefCell<u32>>) -> Player {
    let saves = dir.join("saves");
    let bytes = std::fs::read(saves.join("slot1.sav")).unwrap();
    let inner = wf_app::launcher::title_sim(p, &[bytes], 7).unwrap();
    let net = net.clone();
    let base = endpoint.to_string();
    let n = Rc::new(Cell::new(0u32));
    let cfg = LinkConfig {
        connect: Box::new(move |_k| {
            *endpoints.borrow_mut() += 1;
            let k = n.get();
            n.set(k + 1);
            let id = if k == 0 { base.clone() } else { format!("{base}#{k}") };
            Ok((Box::new(net.endpoint(&id)), if host { Role::Host } else { Role::Player }))
        }),
        clock: clock.clone(),
        link_id: link_id.to_string(),
        name: endpoint.into(),
        saves_dir: Some(saves),
        rules: rules(),
        seed: 11,
        bot: Some(LinkBot::new(kind, host)),
        report: None,
    };
    Player { sim: LinkSim::new(inner, cfg), dir: dir.to_path_buf() }
}

impl Player {
    fn done(&self) -> bool {
        self.sim.bot().is_some_and(|b| b.done(&self.sim))
    }
    fn started(&self) -> bool {
        self.sim.bot().is_some_and(|b| b.started)
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
    /// the guest's app is running (a test kills it and restarts it from its folder)
    b_alive: bool,
}

fn run(name: &str, kind: LinkKind, sa: &SaveData, sb: &SaveData, seed: u64) -> Option<Run> {
    run_on(name, kind, sa, sb, seed, Conditions { latency_ms: 80.0, jitter_ms: 20.0, loss: 0.05 })
}

fn run_on(name: &str, kind: LinkKind, sa: &SaveData, sb: &SaveData, seed: u64, cond: Conditions) -> Option<Run> {
    let dir = content_dir()?;
    let p = packs(&dir);
    let net = SimNet::new(cond, seed);
    let t = Rc::new(Cell::new(0.0f64));
    let tc = t.clone();
    let clock: Clock = Rc::new(move || tc.get());
    let eps = Rc::new(RefCell::new(0));
    let a = player(&p, &net, &clock, &format!("{name}-host"), true, kind, sa, eps.clone());
    let b = player(&p, &net, &clock, &format!("{name}-guest"), false, kind, sb, eps);
    Some(Run { packs: p, renderer: wf_render::Renderer::new(), drawn: Vec::new(), net, t, a, b, b_alive: true })
}

impl Run {
    fn step(&mut self) {
        self.t.set(self.t.get() + 1000.0 / 59.7275);
        let mut ev = Vec::new();
        self.a.sim.step(&[0], &mut ev);
        if self.b_alive {
            self.b.sim.step(&[0], &mut ev);
        }
        // every link page renders (the game's window, fonts and cursor over the title picture)
        for (who, p) in [("host", &self.a), ("guest", &self.b)].into_iter().take(if self.b_alive { 2 } else { 1 }) {
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
            // (a page shows from its first redraw, as the game's: its first frame can still be the one before)
            if colours.len() <= 8 {
                continue;
            }
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
            if std::env::var("LINK_DEBUG").is_ok() && self.a.sim.frame() % 500 == 0 {
                for p in [&self.a, &self.b] {
                    if let Some(s) = p.sim.dojo() {
                        eprintln!("{} f{} c{} {:?} r{}", p.sim.frame(), s.frame(), s.confirmed(), s.game.st.phase, s.game.st.round);
                    } else if let Some(c) = p.sim.trade_cursor() {
                        eprintln!("{} trade {:?} {:?} {:?}", p.sim.frame(), c, p.sim.trade_state(), p.sim.trade_rows());
                    } else {
                        eprintln!("{} {}", p.sim.frame(), p.sim.phase());
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
    r.until(3000, "both on the link screens", |r| r.a.sim.phase() != "title" && r.b.sim.phase() != "title" && r.a.started() && r.b.started());
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
    r.a.sim.bot_mut().unwrap().mode = DojoMode::Balanced;
    r.b.sim.bot_mut().unwrap().mode = DojoMode::Balanced;
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
        let ok = |p: &Player| p.sim.phase() == "complete" || (p.started() && p.sim.last.as_deref() == Some("complete"));
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

/// the arrow prompts the trade list shows (picture, frame) in a player's current view
fn arrows(p: &Player) -> Vec<(String, u16)> {
    let mut v = wf_view::RenderView::default();
    p.sim.view(&mut v);
    let mut out = Vec::new();
    for l in v.menu.iter().flat_map(|m| m.layers.iter()) {
        for op in &l.ops {
            if let wf_view::MenuOp::Image { path, frame, .. } = op {
                if path.ends_with("buttons/up.png") || path.ends_with("buttons/down.png") {
                    out.push((path.rsplit('/').next().unwrap_or("").to_string(), *frame));
                }
            }
        }
    }
    out.sort();
    out
}

#[test]
fn trade_list_scrolls_with_arrows() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    // eight supplies + zenie: nine rows, five shown
    let many: Vec<(usize, u8)> = (0..8).map(|i| (i, 3)).collect();
    let sa = save(&d, &[(0, 20)], &many, 500);
    let sb = save(&d, &[(4, 20)], &[(1, 4)], 100);
    let Some(mut r) = run("trade-scroll", LinkKind::Trade, &sa, &sb, 21) else { return };
    r.until(6000, "the trade screens", |r| r.a.sim.phase() == "trade" && r.b.sim.phase() == "trade");
    let n = r.a.sim.trade_cursor().unwrap().3.len();
    assert!(n > 5, "a long list ({n} rows)");
    // at the top: up greyed, down live
    assert_eq!(arrows(&r.a), vec![("down.png".into(), 0), ("up.png".into(), 2)], "top of the list");
    // the scripted player walks down to the zenie row (the last): the list has scrolled
    r.until(3000, "the last row", |r| r.a.sim.trade_cursor().is_some_and(|c| c.0 == n - 1));
    assert_eq!(arrows(&r.a), vec![("down.png".into(), 2), ("up.png".into(), 0)], "bottom of the list");
    // drawn: the prompts' pixels at their place (the layer at (8, 8) + (98, 44) / (98, 94))
    let mut v = wf_view::RenderView::default();
    r.a.sim.view(&mut v);
    let px = r.renderer.render(&r.packs, &v, 0, 240, 160).to_vec();
    for (x0, y0) in [(106usize, 52usize), (106, 102)] {
        let colours: std::collections::BTreeSet<u32> =
            (0..16).flat_map(|dy| (0..16).map(move |dx| (dx, dy))).map(|(dx, dy)| px[(y0 + dy) * 240 + x0 + dx]).collect();
        assert!(colours.len() > 2, "an arrow drawn at ({x0}, {y0})");
    }
    // a short list shows none (the guest: two rows)
    assert!(arrows(&r.b).is_empty(), "no arrows on a short list");
    // and the trade still completes
    r.until(6000, "TRADE COMPLETE on both", |r| {
        let ok = |p: &Player| p.sim.phase() == "complete" || (p.started() && p.sim.last.as_deref() == Some("complete"));
        ok(&r.a) && ok(&r.b)
    });
}

/// the trade journal's records next to a player's slot: (id, state)
fn journal(dir: &Path) -> Vec<(String, String)> {
    let Ok(b) = std::fs::read(dir.join("saves/slot1.sav.trade.json")) else { return Vec::new() };
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    v["records"]
        .as_object()
        .map(|m| m.iter().map(|(k, r)| (k.clone(), r["state"].as_str().unwrap_or("").to_string())).collect())
        .unwrap_or_default()
}

/// both accepted; the guest's app dies when its journal reaches `at` (between Prepared and Committed, or between
/// Committed and Done), its network with it; the host goes on alone; the guest restarts from its slot + journal
/// through the title's startup, goes back into the Trading Post (the host's scripted player comes back too), and the
/// trade settles: applied exactly once on both sides, nothing lost, the screens in between show the trade in doubt
/// (the locked save's "Disconnected") and, when the guest commits only after its restart, TRADE COMPLETE
fn trade_crash_at(at: &str, seed: u64) {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    let sa = save(&d, &[(0, 20)], &[(0, 5)], 500);
    let sb = save(&d, &[(4, 20)], &[(1, 4)], 100);
    let name = format!("trade-crash-{}", at.to_lowercase());
    let Some(mut r) = run(&name, LinkKind::Trade, &sa, &sb, seed) else { return };
    r.until(6000, "the trade screens", |r| r.a.sim.phase() == "trade" && r.b.sim.phase() == "trade");
    if at == "Prepared" {
        // the guest accepts last: it agrees and prepares on its own accept, before the host has heard it - a frame
        // window between its Prepared and its Committed (accepting first, both prepare in one exchange and the guest
        // prepares and commits in the same tick)
        r.b.sim.bot_mut().unwrap().accept = false;
        r.until(6000, "the host accepted", |r| r.b.sim.trade_state().is_some_and(|(_, friend_accepted, _)| friend_accepted));
        r.b.sim.bot_mut().unwrap().accept = true;
    }
    r.until(6000, "the guest's journal at the crash point", |r| journal(&r.b.dir).iter().any(|(_, s)| s == at));
    // the crash: the guest's app and its network are gone
    let gdir = r.b.dir.clone();
    r.net.kill(&format!("{name}-guest"));
    r.b_alive = false;
    assert_eq!(journal(&gdir).len(), 1);
    // the host's scripted player: no new trade, and it comes back if it left
    {
        let b = r.a.sim.bot_mut().unwrap();
        b.repeat = true;
        b.accept = false;
        b.first = 0;
        b.zenie = 0;
    }
    // the host alone: the trade in doubt (Prepared: locked, "Disconnected") or already TRADE COMPLETE (it committed)
    let mut host_saw = std::collections::BTreeSet::new();
    for _ in 0..900 {
        r.step();
        let ph = r.a.sim.phase();
        host_saw.insert(match r.a.sim.trade_away() {
            Some(true) => "trade in doubt".to_string(),
            _ => ph.to_string(),
        });
    }
    assert!(host_saw.contains("trade in doubt") || host_saw.contains("complete"), "the host saw {host_saw:?}");
    // the guest's app restarts from its folder (slot + journal) through the title's startup
    let eps = Rc::new(RefCell::new(0));
    let clock = {
        let t = r.t.clone();
        Rc::new(move || t.get()) as Clock
    };
    r.b = open(&r.packs, &r.net, &clock, &gdir, &format!("link-{name}-guest"), &format!("{name}-guest-r"), false, LinkKind::Trade, eps);
    {
        let b = r.b.sim.bot_mut().unwrap();
        b.accept = false;
        b.first = 0;
        b.zenie = 0;
    }
    r.b_alive = true;
    let mut guest_saw = std::collections::BTreeSet::new();
    let settled = |dir: &Path| {
        let j = journal(dir);
        j.len() == 1 && (j[0].1 == "Committed" || j[0].1 == "Done")
    };
    let mut k = 0;
    // (Prepared: the guest commits after its restart - TRADE COMPLETE on its screen; Committed: it marks the trade
    // Done once it hears the host)
    let guest_done = |dir: &Path| journal(dir).first().is_some_and(|(_, s)| s == "Done");
    while !(guest_saw.contains("trade in doubt")
        && settled(&r.a.dir)
        && settled(&gdir)
        && (if at == "Prepared" { guest_saw.contains("complete") } else { guest_done(&gdir) }))
    {
        r.step();
        guest_saw.insert(match r.b.sim.trade_away() {
            Some(true) => "trade in doubt".to_string(),
            _ => r.b.sim.phase().to_string(),
        });
        k += 1;
        assert!(k < 20_000, "not settled: host {:?} {:?} / guest {:?} {:?}, guest saw {guest_saw:?}", r.a.sim.phase(), journal(&r.a.dir), r.b.sim.phase(), journal(&gdir));
    }
    // the restarted guest showed the trade in doubt (the save locked) before it settled
    assert!(guest_saw.contains("trade in doubt"), "the guest saw {guest_saw:?}");
    // applied exactly once on both sides, nothing lost: the same trade id, both slot files as after one trade
    let (ja, jb) = (journal(&r.a.dir), journal(&gdir));
    assert_eq!(ja[0].0, jb[0].0, "one trade");
    let (a1, b1) = (r.a.slot(), r.b.slot());
    assert_eq!((a1.story.supplies[0], a1.story.supplies[1], a1.story.zenie), (3, 2, 500));
    assert_eq!((b1.story.supplies[0], b1.story.supplies[1], b1.story.zenie), (2, 2, 100));
    for i in 0..sa.story.supplies.len() {
        assert_eq!(a1.story.supplies[i] as u32 + b1.story.supplies[i] as u32, sa.story.supplies[i] as u32 + sb.story.supplies[i] as u32, "supply {i}");
    }
    assert_eq!(a1.story.zenie + b1.story.zenie, sa.story.zenie + sb.story.zenie);
    // and nothing more happens (a later step applies nothing again)
    for _ in 0..600 {
        r.step();
    }
    assert_eq!((r.a.slot(), r.b.slot()), (a1, b1));
}

#[test]
fn trade_crash_between_prepare_and_commit() {
    trade_crash_at("Prepared", 31);
}

#[test]
fn trade_crash_between_commit_and_done() {
    trade_crash_at("Committed", 37);
}

/// The trade's last confirmation (Committed -> Done) under randomized networks: per seed a latency 0..250 ms, jitter
/// 0..120 ms (reordering) and loss 0..35 %; both scripted players leave TRADE COMPLETE as early as the screen lets
/// them. Both journals must end the session with the trade Done - a side that left while its friend had not yet
/// heard its commit left the friend Committed (the rare stall of the two-process runs, 1 in 14). Seeds:
/// `TRADE_STRESS=N` (default 12; the stall hunt ran 240)
#[test]
fn trade_stress_both_reach_done() {
    let Some(dir) = content_dir() else { return };
    let d = wf_app::battle::load_data(&packs(&dir)).unwrap();
    let sa = save(&d, &[(0, 20)], &[(0, 5)], 500);
    let sb = save(&d, &[(4, 20)], &[(1, 4)], 100);
    let n: u64 = std::env::var("TRADE_STRESS").ok().and_then(|v| v.parse().ok()).unwrap_or(12);
    let mut bad = Vec::new();
    for seed in 0..n {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        let mut rnd = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 1000) as f64 / 1000.0
        };
        let cond = Conditions { latency_ms: 250.0 * rnd(), jitter_ms: 120.0 * rnd(), loss: 0.35 * rnd() };
        let Some(mut r) = run_on(&format!("stress{seed}"), LinkKind::Trade, &sa, &sb, 1000 + seed, cond) else { return };
        r.until(12000, "the trade screens", |r| r.a.sim.phase() == "trade" && r.b.sim.phase() == "trade");
        r.until(12000, "TRADE COMPLETE on both", |r| {
            let ok = |p: &Player| p.sim.phase() == "complete" || (p.started() && p.sim.last.as_deref() == Some("complete"));
            ok(&r.a) && ok(&r.b)
        });
        r.until(4000, "back at the title", |r| r.a.done() && r.b.done());
        let (ja, jb) = (journal(&r.a.dir), journal(&r.b.dir));
        let done = |j: &[(String, String)]| !j.is_empty() && j.iter().all(|(_, s)| s == "Done");
        if !done(&ja) || !done(&jb) {
            bad.push(format!("seed {seed} {:.0} ms / {:.0} / {:.2}: host {ja:?} guest {jb:?}", cond.latency_ms, cond.jitter_ms, cond.loss));
        }
        let (a1, b1) = (r.a.slot(), r.b.slot());
        assert_eq!(a1.story.zenie + b1.story.zenie, sa.story.zenie + sb.story.zenie, "seed {seed}: zenie conserved");
        for i in 0..sa.story.supplies.len() {
            assert_eq!(
                a1.story.supplies[i] as u32 + b1.story.supplies[i] as u32,
                sa.story.supplies[i] as u32 + sb.story.supplies[i] as u32,
                "seed {seed}: supply {i}"
            );
        }
    }
    assert!(bad.is_empty(), "{} of {n} sessions left a trade not Done:\n{}", bad.len(), bad.join("\n"));
}
