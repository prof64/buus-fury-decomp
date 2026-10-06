//! Runtime quality of the app's frame loop (headless, muted): the whole real run from power-on (the whole-run
//! master's keys, one step per game iteration, as `features`) driven through `Control` + `App` the way the native
//! shell drives them, and per host frame:
//!
//! * `step`   - `Control::step` (the sim, the replay recorder, the rewind timeline's snapshots, autosave requests)
//! * `render` - `Control::render` at the window size (view + wf-render's picture at the auto zoom)
//! * `blit`   - `wf_app::blit_scaled` of the picture to the window (the native softbuffer present's CPU part)
//! * `audio`  - the audio engine (`AudioEngine::render`) fed the frame's audio events, in device-sized callbacks at
//!   48 kHz; each callback's time against its own buffer's duration (an underrun is a callback slower than that)
//! * heap allocations (count, bytes) per phase, by a counting global allocator
//!
//! Report: percentiles, worst frames (with the map), frames over the 16.74 ms budget, allocation totals.
//!
//!   cargo run --release -p wf-app --example pacing -- MASTER.jsonl --power-on POWER_ON.json [--steps N]
//!       [--size 1920x1080] [--mode pc|gba] [--buffer 512] [--top 12]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

use wf_app::audio::AudioEngine;
use wf_app::control::Control;
use wf_app::hotkeys::Hotkeys;
use wf_app::story::StorySim;
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_app::App;
use wf_app::wf_render::ViewMode;

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size() as u64, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(n as u64, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn allocs() -> (u64, u64) {
    (ALLOCS.load(Relaxed), BYTES.load(Relaxed))
}

fn power_on(packs: &Packs, power: &Option<serde_json::Value>) -> StorySim {
    let data = wf_app::battle::load_data(packs).expect("data");
    let cfg = wf_core::field::FieldConfig { world: 1, area: 1, rng: wf_core::battle::RngInit::State(0, 0), ..Default::default() };
    let mut session = wf_core::session::Session::new_field(data, cfg);
    session.world.rng = wf_core::rng::Rng::new(0, 0);
    session.world.opts.no_lag = true;
    wf_core::menu::title::open_title_boot(&mut session.world);
    if let Some(v) = power {
        let hex = v["driver"].as_str().unwrap_or("");
        let dram: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap_or(0)).collect();
        let n = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
        if session.world.data.sound.is_some() {
            wf_core::sndsim::import(&mut session.world, &dram, n("pos"), n("last"), Some(n("phase")));
        }
    }
    let mut s = StorySim::with_session(packs, session).expect("story sim");
    s.skip_area_banner();
    s.session.world.opts.no_lag = true;
    s
}

#[derive(Default, Clone)]
struct Frame {
    step: f64,
    render: f64,
    blit: f64,
    audio: f64,
    alloc_n: u64,
    alloc_b: u64,
    step_n: u64,
    render_n: u64,
    view: (u32, u32, u32),
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let master = args.first().expect("MASTER.jsonl").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let num = |name: &str, d: u64| opt(name).and_then(|v| v.parse::<u64>().ok()).unwrap_or(d);
    let limit = num("--steps", u64::MAX);
    let top = num("--top", 12) as usize;
    // --render-every N: the picture (render + blit + audio) every Nth frame only, the step every frame
    let every = num("--render-every", 1);
    let buffer = num("--buffer", 512) as usize;
    let (ww, wh) = opt("--size")
        .and_then(|s| s.split_once('x').map(|(a, b)| (a.parse().unwrap_or(1920), b.parse().unwrap_or(1080))))
        .unwrap_or((1920u32, 1080u32));
    let power: Option<serde_json::Value> = opt("--power-on").map(|p| serde_json::from_str(&std::fs::read_to_string(p).expect("power-on")).expect("json"));

    let mut inputs: Vec<u16> = vec![];
    let mut prev_lf = 0i64;
    for l in BufReader::new(std::fs::File::open(&master).expect("master")).lines() {
        let v: serde_json::Value = serde_json::from_str(&l.expect("line")).expect("row");
        let lf = v["lf"].as_i64().unwrap_or(0);
        if lf != prev_lf {
            inputs.push(v["keys"].as_u64().unwrap_or(0) as u16);
        }
        prev_lf = lf;
        if inputs.len() as u64 >= limit {
            break;
        }
    }
    let rom = wf_rom::testutil::rom().expect("rom");
    let (c, _) = wf_extract::extract_rom(&rom, &Default::default(), &mut |_| {});
    let packs = Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap();
    let bank = wf_app::wf_content::audio::load_bank(&packs).expect("bank");
    let bank: &'static _ = Box::leak(Box::new(bank));
    let mut sim = power_on(&packs, &power);
    // the recorder / rewind timeline run as in the app (Control needs a story start to record)
    sim.start = Some(serde_json::json!({"power_on": true, "seed": 0}));
    let mut app = App::new(packs, Box::new(sim));
    app.mode = if opt("--mode").as_deref() == Some("gba") { ViewMode::Gba { scale: 0 } } else { ViewMode::Pc { zoom: 0 } };
    let mut ctl = Control::new(&mut app, Hotkeys::default(), 0);
    let mut eng = AudioEngine::new(bank, 48_000);
    let mut abuf = vec![0f32; buffer * 2];
    let mut win = vec![0u32; ww as usize * wh as usize];
    let budget = 1000.0 / wf_view::GBA_FPS;
    let buf_ms = buffer as f64 * 1000.0 / 48_000.0;
    // the audio callbacks owed so far (48 kHz device, `buffer` frames per callback, paced by the frames' wall time)
    let mut owed = 0.0f64;
    let mut frames: Vec<Frame> = Vec::with_capacity(inputs.len());
    let mut maps: Vec<String> = Vec::with_capacity(inputs.len());
    let (mut cb_max, mut cb_slow, mut cbs) = (0.0f64, 0u64, 0u64);
    let (mut a_alloc_n, mut a_alloc_b) = (0u64, 0u64);
    let t_all = Instant::now();
    println!("{} steps, window {ww}x{wh}, mode {:?}, audio buffer {buffer} frames ({buf_ms:.2} ms)", inputs.len(), app.mode);
    // --from K: steps before K run untimed (the recorder / timeline as usual), the report covers K ..
    let from = num("--from", 0) as usize;
    let hash_on = args.iter().any(|a| a == "--hash");
    let mut pic_hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (k, &keys) in inputs.iter().enumerate() {
        if k < from {
            if !ctl.step(&mut app, &[keys]) {
                break;
            }
            app.take_audio();
            continue;
        }
        let mut fr = Frame::default();
        let (n0, b0) = allocs();
        let t = Instant::now();
        if !ctl.step(&mut app, &[keys]) {
            break;
        }
        fr.step = t.elapsed().as_secs_f64() * 1e3;
        let (n1, _) = allocs();
        fr.step_n = n1 - n0;
        if every > 1 && (frames.len() as u64) % every != 0 {
            // (sampled: this frame's picture is not made - render / blit 0)
            fr.alloc_n = fr.step_n;
            for e in app.take_audio() {
                eng.push(e);
            }
            maps.push(String::new());
            frames.push(fr);
            continue;
        }
        let t = Instant::now();
        let f = ctl.render(&mut app, 0, ww, wh);
        fr.render = t.elapsed().as_secs_f64() * 1e3;
        fr.view = (f.view_w, f.view_h, f.scale);
        // --hash: every picture made goes into one FNV-1a hash (a renderer change's before / after check)
        if hash_on {
            for &c in &app.renderer.fb[..(f.view_w * f.view_h) as usize] {
                pic_hash = (pic_hash ^ c as u64).wrapping_mul(0x0100_0000_01b3);
            }
        }
        let (n2, _) = allocs();
        fr.render_n = n2 - n1;
        // (the machine is shared: render / blit are pure, so each is timed twice and the faster one kept - a
        // scheduling stall shows as a difference between the two)
        let t = Instant::now();
        let _ = ctl.render(&mut app, 0, ww, wh);
        fr.render = fr.render.min(t.elapsed().as_secs_f64() * 1e3);
        let (n2b, _) = allocs();
        let t = Instant::now();
        wf_app::blit_scaled(&app.renderer.fb, f.view_w as usize, f.view_h as usize, f.scale as usize, &mut win, ww as usize, wh as usize);
        fr.blit = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        wf_app::blit_scaled(&app.renderer.fb, f.view_w as usize, f.view_h as usize, f.scale as usize, &mut win, ww as usize, wh as usize);
        fr.blit = fr.blit.min(t.elapsed().as_secs_f64() * 1e3);
        let (n1, b1) = allocs();
        fr.alloc_n = n1 - n0 - (n2b - n2);
        fr.alloc_b = b1 - b0;
        // audio: this frame's events, then the callbacks one frame of wall time owes
        let (n0, b0) = allocs();
        for e in app.take_audio() {
            eng.push(e);
        }
        owed += 48_000.0 / wf_view::GBA_FPS / buffer as f64;
        while owed >= 1.0 {
            owed -= 1.0;
            let t = Instant::now();
            eng.render(&mut abuf);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            fr.audio += ms;
            cbs += 1;
            cb_max = cb_max.max(ms);
            if ms > buf_ms * 0.5 {
                cb_slow += 1;
            }
        }
        let (n1, b1) = allocs();
        a_alloc_n += n1 - n0;
        a_alloc_b += b1 - b0;
        maps.push(app.view.map.as_ref().map(|m| m.id.clone()).unwrap_or_default());
        frames.push(fr);
    }
    // the rewind timeline's capture (every 30 frames, Control::step): the snapshot and its compression, best of 20
    {
        let best = |f: &mut dyn FnMut()| (0..20).map(|_| { let t = Instant::now(); f(); t.elapsed().as_secs_f64() * 1e3 }).fold(f64::MAX, f64::min);
        let mut len = (0, 0);
        let snap = best(&mut || len.0 = app.sim.snapshot().map(|v| v.len()).unwrap_or(0));
        let bytes = app.sim.snapshot().unwrap_or_default();
        let (a0, _) = allocs();
        let _ = app.sim.snapshot();
        let (a1, _) = allocs();
        let lz = best(&mut || len.1 = wf_app::wf_replay::lz::compress(&bytes).len());
        println!("timeline capture at the end: snapshot {snap:.3} ms ({} B, {} allocations) + compress {lz:.3} ms ({} B)", len.0, a1 - a0, len.1);
    }
    // the autosave's serialization of the whole session's recording (Control::advance runs it every 30 s of play)
    let t = Instant::now();
    ctl.finalize(&app);
    println!("autosave of the {}-frame recording: {:.2} ms", frames.len(), t.elapsed().as_secs_f64() * 1e3);
    if hash_on {
        println!("picture hash {pic_hash:016x}");
    }
    let n = frames.len().max(1);
    let wall = t_all.elapsed().as_secs_f64();
    println!("ran {} frames in {wall:.1} s ({:.0} fps unpaced)", frames.len(), frames.len() as f64 / wall);
    // (step: every frame; the rest: the frames whose picture was made)
    let col = |f: fn(&Frame) -> f64, all: bool| -> Vec<f64> { frames.iter().filter(|x| all || x.render > 0.0).map(f).collect() };
    for (name, f) in [
        ("step", (|x: &Frame| x.step) as fn(&Frame) -> f64),
        ("render", |x: &Frame| x.render),
        ("blit", |x: &Frame| x.blit),
        ("audio", |x: &Frame| x.audio),
        ("total", |x: &Frame| x.step + x.render + x.blit + x.audio),
    ] {
        let mut v = col(f, name == "step");
        let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
        println!(
            "{name:7} mean {mean:6.3}  p50 {:6.3}  p99 {:6.3}  p99.9 {:6.3}  max {:7.3} ms",
            pct(&mut v, 0.5),
            pct(&mut v, 0.99),
            pct(&mut v, 0.999),
            pct(&mut v, 1.0)
        );
    }
    let over: Vec<usize> = (0..frames.len()).filter(|&k| { let x = &frames[k]; x.step + x.render + x.blit + x.audio > budget }).collect();
    let half: usize = frames.iter().filter(|x| x.step + x.render + x.blit + x.audio > budget * 0.5).count();
    println!("frames over the {budget:.2} ms budget: {} ({:.4} %), over half of it: {half}", over.len(), over.len() as f64 * 100.0 / n as f64);
    let tot_n: u64 = frames.iter().map(|x| x.alloc_n).sum();
    let tot_b: u64 = frames.iter().map(|x| x.alloc_b).sum();
    println!(
        "  of which step {:.1} / frame, render {:.1} / frame",
        frames.iter().map(|x| x.step_n).sum::<u64>() as f64 / n as f64,
        frames.iter().map(|x| x.render_n).sum::<u64>() as f64 / n as f64
    );
    let mut an: Vec<f64> = frames.iter().map(|x| x.alloc_n as f64).collect();
    println!(
        "heap allocations (step + render + blit): {:.1} per frame ({:.1} KB / frame), p50 {} p99 {} max {}",
        tot_n as f64 / n as f64,
        tot_b as f64 / n as f64 / 1024.0,
        pct(&mut an, 0.5),
        pct(&mut an, 0.99),
        pct(&mut an, 1.0)
    );
    println!(
        "audio: {cbs} callbacks of {buffer} frames, max {cb_max:.3} ms of a {buf_ms:.2} ms buffer, {cb_slow} over half of it; {:.2} allocations / callback ({:.1} B)",
        a_alloc_n as f64 / cbs.max(1) as f64,
        a_alloc_b as f64 / cbs.max(1) as f64
    );
    let mut idx: Vec<usize> = (0..frames.len()).collect();
    idx.sort_by(|&a, &b| {
        let t = |k: usize| frames[k].step + frames[k].render + frames[k].blit + frames[k].audio;
        t(b).partial_cmp(&t(a)).unwrap()
    });
    println!("worst frames:");
    for &k in idx.iter().take(top) {
        let x = &frames[k];
        println!(
            "  step {:8} {:7.3} ms = step {:6.3} render {:6.3} blit {:6.3} audio {:6.3}  allocs {:6} ({:8} B)  view {}x{} x{}  map {}",
            k + from,
            x.step + x.render + x.blit + x.audio,
            x.step,
            x.render,
            x.blit,
            x.audio,
            x.alloc_n,
            x.alloc_b,
            x.view.0,
            x.view.1,
            x.view.2,
            maps[k]
        );
    }
    // the worst step / render frames per map (the heavy scenes)
    let mut per: std::collections::BTreeMap<&str, (f64, f64, usize)> = Default::default();
    for (k, x) in frames.iter().enumerate() {
        let e = per.entry(maps[k].as_str()).or_insert((0.0, 0.0, 0));
        e.0 = e.0.max(x.step);
        e.1 = e.1.max(x.render + x.blit);
        e.2 += 1;
    }
    let mut pv: Vec<_> = per.into_iter().collect();
    pv.sort_by(|a, b| (b.1 .1 + b.1 .0).partial_cmp(&(a.1 .1 + a.1 .0)).unwrap());
    println!("heaviest maps (max step / max render+blit, frames):");
    for (m, (s, r, c)) in pv.iter().take(top) {
        println!("  {m:24} step {s:6.3}  render+blit {r:6.3}  ({c} frames)");
    }
}
