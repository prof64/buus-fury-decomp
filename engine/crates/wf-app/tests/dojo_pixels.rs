//! The link Dojo's fight screen vs the real game's, frame by frame (headless, muted; skipped without the ROM).
//!
//! Recordings: `data/link/dojo_NAME.json` with a `video` list (tools/link/dojo_record.py `--video DIR`: per video frame
//! of the host - and with `video_b` of the guest - [video frame, the logic frame counter, sha1 of the 240x160 RGB
//! picture's first 16 hex digits]); the pictures themselves stay in `work/link_fight/NAME/` (ROM-derived, git-ignored).
//! The match is replayed as `tests/dojo_link.rs` does in wf-net (the app's `DojoGame`, advanced with the recorded
//! keys from the fighters' creation), and after each logic frame its picture on that side (`DojoGame::view`, the app's
//! match view -> wf-render, 240x160) is compared with the game's picture of that logic frame (its last video frame:
//! the app recreates no lag frames). Every compared frame must be exact, or torn as the tear rule allows (core.md:
//! the game writes while the screen is drawn - mostly the guest, whose frames the link's waits shift): the last lines
//! (from 150) still our previous picture; the HUD's / the friend's bars still the frame before's (a hit's frame); the
//! lines above or below a split with the frame before's BG scroll; our previous picture down to a line; those
//! together; a tear through the OBJs locked in the recording (`torn`, hex masks). Each of those is checked against our own pictures
//! and counted; a picture the rule would call torn (differing from both neighbours next to a lag frame) that none of
//! them explains is counted apart and printed.
//! `DOJO_PIX_OUT=<dir>` writes emulator | ours | diff strips of differing frames, `DOJO_PIX_ONLY=name` one recording,
//! `DOJO_PIX_LOG=1` prints every differing / torn frame; `DOJO_PIX_SPR=<lf>` that frame's sprites and shots,
//! `DOJO_PIX_CAM=<lf>-<lf>` the cameras / shake / bars / pickups over a range, `DOJO_PIX_HP=1` the fighters' HP.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha1::{Digest, Sha1};
use wf_app::wf_content::{DirSource, FileSource, Packs};
use wf_core::content::GameData;
use wf_net::dojo::{balanced_roster, fighter_setup, match_config, DojoChar, DojoGame, DojoMode, DojoRules, DojoSetup};
use wf_net::game::Game;
use wf_render::Renderer;
use wf_view::RenderView;

fn repo() -> PathBuf {
    wf_rom::testutil::repo_root()
}

fn content_dir() -> Option<PathBuf> {
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

fn load_png(p: &Path) -> Option<Vec<u32>> {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).ok()?));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let ch = info.color_type.samples();
    Some(buf.chunks(ch).take(240 * 160).map(|c| 0xFF00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect())
}

fn save(p: &Path, imgs: &[&[u32]]) {
    let (w, h) = (240 * imgs.len(), 160);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for im in imgs {
            for x in 0..240 {
                let c = im[y * 240 + x];
                rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
        }
    }
    let f = std::fs::File::create(p).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

/// the recorder's picture hash: sha1 of the RGB bytes, first 16 hex digits
fn hash(fb: &[u32]) -> String {
    let mut h = Sha1::new();
    let mut rgb = Vec::with_capacity(240 * 160 * 3);
    for &c in fb.iter().take(240 * 160) {
        rgb.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
    }
    h.update(&rgb);
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// wf-net tests/dojo_link.rs `rec_char`: a fighter's record as the game made it -> the Dojo character
fn rec_char(hex: &str, d: &GameData) -> DojoChar {
    let r: Vec<u8> = (0..hex.len() / 2).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap()).collect();
    let u16le = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]) as i32;
    let u32le = |o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let form = r[0x14] as i32;
    let character = d.form(form).map(|f| f.char_).unwrap_or(0);
    let f = d.fighters.iter().find(|f| f.character == character && f.fusion.is_none()).expect("fighter");
    let n = (r[0x0C] as usize).min(16);
    DojoChar {
        name: f.name.clone(),
        character,
        fighter: f.id.clone(),
        form,
        level: wf_core::progression::level_of(u32le(0x24) as i64, &d.xp_table),
        stats: [r[0x28] as i32, r[0x29] as i32, r[0x2A] as i32, r[0x2B] as i32],
        equip: [0, 1, 2, 3].map(|k| (u32le(0x3C + 4 * k) & 0x0FFF_FFFF) as i32),
        max_hp: u16le(0x18),
        max_ep: u16le(0x1C),
        skills: (0..n).map(|k| r[0x0D + k] as i32).collect(),
        sel: r[0x15] as usize,
        skill_levels: (0..n).map(|k| r[0x2E + 2 * k] as i32).collect(),
    }
}

struct Side {
    slot: u8,
    /// the game's picture hash per logic frame (its last video frame) and that video frame
    want: std::collections::BTreeMap<i64, (String, i64)>,
    /// every video frame's hash (the tear rule)
    seq: Vec<(i64, i64, String)>,
    dir: PathBuf,
}

/// (frames compared, exact, tears, first differing logic frames)
fn run(packs: &Packs, data: &Arc<GameData>, name: &str) -> Vec<(u8, usize, usize, usize, Vec<i64>, usize)> {
    let p = repo().join(format!("data/link/dojo_{name}.json"));
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    let v = |x: &serde_json::Value| x.as_i64().unwrap();
    let rows: Vec<&serde_json::Value> = doc["rows"].as_array().unwrap().iter().skip_while(|r| r["p"].as_array().is_none_or(|p| p.is_empty())).collect();
    // each round's fighters made: the host's logic frame (the first row; then 64 updates after each round's result
    // - `DojoRules::between_frames` + the creation's own)
    let mut made_a = vec![v(&rows[0]["lf"])];
    for k in 1..rows.len() {
        if rows[k]["wins"] != rows[k - 1]["wins"] {
            made_a.push(v(&rows[k]["lf"]) + 64);
        }
    }
    let mut sides = vec![];
    for (slot, key, sub) in [(0u8, "video", ""), (1, "video_b", "_b")] {
        let Some(list) = doc[key].as_array() else { continue };
        // (the guest's counter runs apart from the host's: its frames go to the host's logic frames by each round's
        // creation - the guest's entries say whether its fighters exist, `[.., timer, wins, fighters]`)
        let mut seq: Vec<(i64, i64, String)> = list.iter().map(|e| (v(&e[0]), v(&e[1]), e[2].as_str().unwrap().to_string())).collect();
        if slot == 1 {
            // the guest's creation counters per round (its round timer's first 180 less the 125 updates of the round's
            // start - its fighters may be made in a long frame), and its frames renumbered into the host's logic frames
            let mut made_b = vec![];
            let mut prev: Option<&serde_json::Value> = None;
            for e in list {
                let start = e[3].as_i64() == Some(180) && e[5].as_i64() == Some(1);
                if start && prev.is_none_or(|p| p[3].as_i64() != Some(180) || p[5].as_i64() != Some(1)) {
                    made_b.push(v(&e[1]) - 125);
                }
                prev = Some(e);
            }
            let mut out = vec![];
            for (vf, lf, h) in seq {
                let Some(r) = made_b.iter().rposition(|&m| lf >= m) else { continue };
                if let Some(a0) = made_a.get(r) {
                    out.push((vf, lf - made_b[r] + a0, h));
                }
            }
            seq = out;
        }
        let mut want = std::collections::BTreeMap::new();
        for (vf, lf, h) in &seq {
            want.insert(*lf, (h.clone(), *vf));
        }
        sides.push(Side { slot, want, seq, dir: repo().join(format!("work/link_fight/{name}{sub}")) });
    }
    let r = DojoRules::default();
    let pick = |k: usize| doc["pick"].get(k).and_then(|x| x.as_u64()).unwrap_or(0) as usize;
    let roster = balanced_roster();
    let rounds = doc["rounds"].as_u64().unwrap_or(3) as u8;
    let mode = if doc["mode"].as_str() == Some("ingame") { DojoMode::InGame } else { DojoMode::Balanced };
    let ch = |k: usize| match mode {
        DojoMode::InGame => rec_char(doc["recs"][k].as_str().unwrap(), data),
        DojoMode::Balanced => roster[pick(k)].clone(),
    };
    let cfg = match_config(fighter_setup(&ch(0), mode, &r, data), fighter_setup(&ch(1), mode, &r, data), 0);
    let mut g = DojoGame::new(data.clone(), cfg, DojoSetup { mode, rounds }, r);
    let out = std::env::var("DOJO_PIX_OUT").ok().map(PathBuf::from);
    if let Some(o) = &out {
        std::fs::create_dir_all(o).unwrap();
    }
    let log = std::env::var("DOJO_PIX_LOG").is_ok();
    let off: i64 = std::env::var("DOJO_PIX_OFF").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut renderer = Renderer::new();
    let mut res: Vec<(u8, usize, usize, usize, Vec<i64>, usize)> = sides.iter().map(|s| (s.slot, 0, 0, 0, vec![], 0)).collect();
    let mut written = 0;
    // per side: the last picture still to judge (its logic frame, the game's hash and video frame, the tear rule, ours,
    // what the match was doing) - judged once the next picture is known (the guest's scanout tear)
    #[allow(clippy::type_complexity)]
    let mut pend: Vec<Option<(i64, String, i64, bool, Vec<u32>, String, RenderView, bool)>> = vec![None; sides.len()];
    let mut last: Vec<Option<(Vec<u32>, RenderView)>> = vec![None; sides.len()];
    let mut tear_renderer = Renderer::new();
    let torn = doc["torn"].clone();
    let mut judge = |si: usize, p: (i64, String, i64, bool, Vec<u32>, String, RenderView, bool), prev: Option<&(Vec<u32>, RenderView)>, res: &mut Vec<(u8, usize, usize, usize, Vec<i64>, usize)>| {
        let (lf, h, vf, tear, ours, what, view, long) = p;
        let side = &sides[si];
        let e = &mut res[si];
        e.1 += 1;
        if hash(&ours) == h {
            e.2 += 1;
            return;
        }
        // a scanout tear at the bottom: the game's last lines (at most 10) still show the previous frame (the frame's
        // VRAM / palette copy ran into the picture's end - the guest, whose updates the link's waits shift): our
        // picture with that band of our previous one
        if let Some((n, pv)) = prev {
            // a long iteration's picture (a lag frame next to it): still the one before, whole
            if long && hash(n) == h {
                if log {
                    eprintln!("dojo_{name} side {} lf {lf}: a long iteration, our previous picture", side.slot);
                }
                e.3 += 1;
                return;
            }
            // the picture's first line still the frame before's (the OBJs' new places written as the scanout begins)
            if {
                let mut c = ours.clone();
                c[..240].copy_from_slice(&n[..240]);
                hash(&c) == h
            } {
                if log {
                    eprintln!("dojo_{name} side {} lf {lf}: the first line the frame before's", side.slot);
                }
                e.3 += 1;
                return;
            }
            if let Some(y0) = (150..160).rev().find(|&y0| {
                let mut c = ours.clone();
                c[y0 * 240..].copy_from_slice(&n[y0 * 240..]);
                hash(&c) == h
            }) {
                if log {
                    eprintln!("dojo_{name} side {} lf {lf}: bottom tear from line {y0}", side.slot);
                }
                e.3 += 1;
                return;
            }
            // our previous picture down to a line, the new one below (the OAM / scroll copy while the screen is drawn)
            if let Some(y0) = (1..160).find(|&y0| {
                let mut c = ours.clone();
                c[..y0 * 240].copy_from_slice(&n[..y0 * 240]);
                hash(&c) == h
            }) {
                if log {
                    eprintln!("dojo_{name} side {} lf {lf}: our previous picture to line {y0}", side.slot);
                }
                e.3 += 1;
                return;
            }
            // the bars' tiles still the frame before's (drawn into the tiles in the frame's draw pass, after the
            // scanout passed the HUD: a hit's frame - both sides show the same logic frame's bars differently)
            let mut bars = view.clone();
            for (b, o) in bars.hud.iter_mut().zip(&pv.hud) {
                (b.hp, b.hp_max, b.ep, b.ep_max) = (o.hp, o.hp_max, o.ep, o.ep_max);
            }
            for (b, o) in bars.boss_bars.iter_mut().zip(&pv.boss_bars) {
                b.fill = o.fill;
            }
            let bars_pic = tear_renderer.render(packs, &bars, 0, 240, 160).to_vec();
            if hash(&bars_pic) == h {
                if log {
                    eprintln!("dojo_{name} side {} lf {lf}: the bars of the frame before", side.slot);
                }
                e.3 += 1;
                return;
            }
            // the BG scroll still the frame before's on the lines above or below a split (the tear rule, core.md:
            // the registers written while the screen is drawn)
            let mut sc = view.clone();
            sc.cameras = pv.cameras.clone();
            sc.map = pv.map.clone();
            let scroll_pic = tear_renderer.render(packs, &sc, 0, 240, 160).to_vec();
            for (a, b, tag) in [(&ours, &scroll_pic, "new over old"), (&scroll_pic, &ours, "old over new"), (&bars_pic, &scroll_pic, "new (old bars) over old")] {
                if let Some(y0) = (0..=160).find(|&y0| {
                    let mut c = a.clone();
                    c[y0 * 240..].copy_from_slice(&b[y0 * 240..]);
                    hash(&c) == h
                }) {
                    if log {
                        eprintln!("dojo_{name} side {} lf {lf}: scroll tear ({tag}) at line {y0}", side.slot);
                    }
                    e.3 += 1;
                    return;
                }
            }
            // the OBJs still the frame before's (the OAM copy after the scanout: where they were on the screen then)
            // over this frame's BG
            {
                let mut ob = view.clone();
                let d = match (view.cameras.first(), pv.cameras.first()) {
                    (Some(c), Some(o)) => (c.x - o.x, c.y - o.y),
                    _ => (0, 0),
                };
                ob.sprites = pv.sprites.clone();
                for sp in ob.sprites.iter_mut() {
                    sp.x += d.0;
                    sp.y += d.1;
                }
                ob.damage_numbers = pv.damage_numbers.clone();
                for dn in ob.damage_numbers.iter_mut() {
                    dn.x += d.0;
                    dn.y += d.1;
                }
                let pic = tear_renderer.render(packs, &ob, 0, 240, 160).to_vec();
                let hit = if hash(&pic) == h {
                    Some(160)
                } else {
                    (150..160).rev().find(|&y1| {
                        let mut c = pic.clone();
                        c[y1 * 240..].copy_from_slice(&n[y1 * 240..]);
                        hash(&c) == h
                    })
                };
                if let Some(y1) = hit {
                    if log {
                        eprintln!("dojo_{name} side {} lf {lf}: torn - the OBJs of the frame before, our previous picture from line {y1}", side.slot);
                    }
                    e.3 += 1;
                    return;
                }
            }
            // those together: the bars and the scroll each old or new above and below one split, and the last lines
            // still our previous picture (the guest in a moving camera's frame with a hit)
            let mut both = bars.clone();
            both.cameras = pv.cameras.clone();
            both.map = pv.map.clone();
            let both_pic = tear_renderer.render(packs, &both, 0, 240, 160).to_vec();
            let bases = [(&ours, "new"), (&bars_pic, "old bars"), (&scroll_pic, "old scroll"), (&both_pic, "old bars and scroll")];
            for (a, ta) in bases {
                for (b, tb) in bases {
                    for y0 in 0..=160usize {
                        if y0 == 0 && !std::ptr::eq(a, b) {
                            continue;
                        }
                        let mut c = a.clone();
                        c[y0 * 240..].copy_from_slice(&b[y0 * 240..]);
                        let hit = if hash(&c) == h {
                            Some(160)
                        } else {
                            (150..160).rev().find(|&y1| {
                                let mut d = c.clone();
                                d[y1 * 240..].copy_from_slice(&n[y1 * 240..]);
                                hash(&d) == h
                            })
                        };
                        if let Some(y1) = hit {
                            if log {
                                eprintln!("dojo_{name} side {} lf {lf}: torn - {ta} to line {y0}, {tb} below, our previous picture from line {y1}", side.slot);
                            }
                            e.3 += 1;
                            return;
                        }
                        if std::ptr::eq(a, b) {
                            break;
                        }
                    }
                }
            }
        }
        // a tear through the OBJs (the OAM copy while the screen is drawn): an older picture down to a line, then a few
        // lines where each pixel is that picture's or the new one's, the new picture below - locked in the recording
        // as `torn: {side: {lf: [kind, first mixed line, hex masks of the older picture's pixels ..]}}`; the older
        // picture: "p" our previous one, "o" the previous frame's OBJs over this frame's BG, "ob" that with the
        // previous bars (`DOJO_PIX_LOCK=1` prints the lock of such a frame from the emulator's picture)
        if let Some((n, pv)) = prev {
            let mut older = |kind: &str| -> Vec<u32> {
                if kind == "p" {
                    return n.clone();
                }
                let mut ob = view.clone();
                let d = match (view.cameras.first(), pv.cameras.first()) {
                    (Some(c), Some(o)) => (c.x - o.x, c.y - o.y),
                    _ => (0, 0),
                };
                ob.sprites = pv.sprites.clone();
                for sp in ob.sprites.iter_mut() {
                    sp.x += d.0;
                    sp.y += d.1;
                }
                ob.damage_numbers = pv.damage_numbers.clone();
                for dn in ob.damage_numbers.iter_mut() {
                    dn.x += d.0;
                    dn.y += d.1;
                }
                if kind == "ob" {
                    for (b, o) in ob.hud.iter_mut().zip(&pv.hud) {
                        (b.hp, b.hp_max, b.ep, b.ep_max) = (o.hp, o.hp_max, o.ep, o.ep_max);
                    }
                    for (b, o) in ob.boss_bars.iter_mut().zip(&pv.boss_bars) {
                        b.fill = o.fill;
                    }
                }
                tear_renderer.render(packs, &ob, 0, 240, 160).to_vec()
            };
            if let Some(l) = torn[&side.slot.to_string()][&lf.to_string()].as_array() {
                let n = older(l[0].as_str().unwrap_or("p"));
                let y0 = l[1].as_u64().unwrap_or(0) as usize;
                let mut c = ours.clone();
                c[..y0 * 240].copy_from_slice(&n[..y0 * 240]);
                for (j, m) in l[2..].iter().enumerate() {
                    let m = m.as_str().unwrap_or("").as_bytes();
                    for x in 0..240 {
                        let nib = m.get(x / 4).map_or(0, |&ch| (ch as char).to_digit(16).unwrap_or(0));
                        if nib >> (3 - x % 4) & 1 == 1 {
                            c[(y0 + j) * 240 + x] = n[(y0 + j) * 240 + x];
                        }
                    }
                }
                if hash(&c) == h {
                    if log {
                        eprintln!("dojo_{name} side {} lf {lf}: torn through the OBJs at line {y0} (locked)", side.slot);
                    }
                    e.3 += 1;
                    return;
                }
            }
            if std::env::var("DOJO_PIX_LOCK").is_ok() {
                if let Some(emu) = load_png(&side.dir.join(format!("{vf:06}.png"))) {
                    for kind in ["p", "o", "ob"] {
                        let n = older(kind);
                        let line = |p: &[u32], y: usize| p[y * 240..(y + 1) * 240].to_vec();
                        // (the last line that is not the new picture's, and the first that is not the older one's)
                        let last_old = (0..160).rev().find(|&y| line(&emu, y) != line(&ours, y));
                        let first_new = (0..160).find(|&y| line(&emu, y) != line(&n, y));
                        let (Some(lo), Some(fnw)) = (last_old, first_new) else { continue };
                        let fnw = fnw.min(lo + 1);
                        let either = (fnw..=lo).all(|y| (0..240).all(|x| emu[y * 240 + x] == n[y * 240 + x] || emu[y * 240 + x] == ours[y * 240 + x]));
                        if lo + 1 - fnw <= 4 && either {
                            let masks: Vec<String> = (fnw..=lo)
                                .map(|y| (0..60).map(|q| format!("{:x}", (0..4).fold(0, |a, b| a << 1 | u32::from(emu[y * 240 + q * 4 + b] != ours[y * 240 + q * 4 + b])))).collect::<String>())
                                .collect();
                            eprintln!("LOCK {name} {} {lf} {kind} {fnw} {}", side.slot, masks.join(" "));
                            break;
                        }
                    }
                }
            }
        }
        if tear {
            e.5 += 1;
            return;
        }
        if e.4.len() < 4000 {
            e.4.push(lf);
        }
        if log {
            eprintln!("dojo_{name} side {} lf {lf} video {vf}: differs ({what})", side.slot);
        }
        if let Some(o) = &out {
            if written < 400 {
                if let Some(emu) = load_png(&side.dir.join(format!("{vf:06}.png"))) {
                    let diff: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(format!("{name}_{}_{lf}.png", side.slot)), &[&emu, &ours, &diff]);
                    written += 1;
                }
            }
        }
    };
    // `lost` (dojo_record.py `--reset-b-at`): the host's last fight logic frame before the link was lost - it stands on
    // it while it waits, then the link error's script (s001) fades that picture out: 16 steps of the script's
    // palette scale, to black
    let lost = doc["lost"].as_i64();
    let mut lost_prev: Vec<Option<Vec<u32>>> = vec![None; sides.len()];
    let mut faded = 0u16;
    for (k, row) in rows.iter().enumerate() {
        // (the script's first iteration still shows the fight's next picture: its OBJs are the last update's, a
        // frame late as always - the picture that then fades)
        if let Some(l) = lost.filter(|&l| v(&row["lf"]) >= l) {
            let lf = v(&row["lf"]);
            for (si, side) in sides.iter().enumerate() {
                if let Some(p) = pend[si].take() {
                    let keep = (p.4.clone(), p.6.clone());
                    judge(si, p, last[si].as_ref(), &mut res);
                    last[si] = Some(keep);
                }
                if si == 0 {
                    g.link_lost();
                    g.lost_step();
                }
                let Some((h, vf)) = side.want.get(&lf) else { continue };
                let mut nv = RenderView::default();
                g.view(side.slot, &mut nv);
                let ours = renderer.render(packs, &nv, 0, 240, 160).to_vec();
                res[si].1 += 1;
                let mut level = (hash(&ours) == *h).then_some((lf - l) as u16);
                // (... or with its first line still the frame before's, as in the fight)
                if let (None, Some(pv)) = (level, lost_prev[si].as_ref()) {
                    let mut c = ours.clone();
                    c[..240].copy_from_slice(&pv[..240]);
                    if hash(&c) == *h {
                        res[si].3 += 1;
                        res[si].2 = res[si].2.wrapping_sub(1);
                        level = Some((lf - l) as u16);
                    }
                }
                lost_prev[si] = Some(ours.clone());
                if let (None, Some(o), Some(emu)) = (level, &out, load_png(&side.dir.join(format!("{vf:06}.png")))) {
                    let diff: Vec<u32> = emu.iter().zip(&ours).map(|(a, b)| if a == b { 0xFF00_0000 } else { 0xFFFF_00FF }).collect();
                    save(&o.join(format!("{name}_{}_{lf}.png", side.slot)), &[&emu, &ours, &diff]);
                }
                match level {
                    Some(lv) => {
                        if log {
                            eprintln!("dojo_{name} side {} lf {lf}: the lost link's fade, level {lv} / 16 ({} after the last fight frame)", side.slot, lf - l);
                        }
                        faded = lv * 16;
                        let _ = &faded;
                        res[si].2 += 1;
                    }
                    None => res[si].4.push(lf),
                }
            }
            if faded >= 256 {
                break;
            }
            continue;
        }
        let keys: Vec<u16> = row["keys"].as_array().unwrap().iter().map(|x| v(x) as u16).collect();
        if k > 0 {
            let gap = v(&row["lf"]) - v(&rows[k - 1]["lf"]);
            for _ in 1..gap {
                g.advance(&keys, true);
            }
            g.advance(&keys, true);
        }
        let lf = v(&row["lf"]);
        // (to the match's fade-out's end: then the link's menus, not the match)
        if g.st.over_at.is_some_and(|o| g.st.frame >= o + 64) {
            break;
        }
        if std::env::var("DOJO_PIX_HP").is_ok() {
            let hp: Vec<i32> = g.s.world.players.iter().map(|&id| g.s.world.pl_ref(g.s.world.find(id).unwrap()).rec.hp).collect();
            eprintln!("HP lf {lf} {hp:?} frame {}", g.s.world.frame);
        }
        for (si, side) in sides.iter().enumerate() {
            let mut view = RenderView::default();
            g.view(side.slot, &mut view);
            if std::env::var("DOJO_PIX_SPR").ok().and_then(|f| f.parse::<i64>().ok()) == Some(lf) {
                for e in g.s.world.ents.iter().filter(|e| !e.removed) {
                    match &e.body {
                        wf_core::world::Body::Fx(f) => eprintln!("   ent {} fx {}", e.id, format!("{f:?}").chars().take(300).collect::<String>()),
                        wf_core::world::Body::Shot(f) => eprintln!("   ent {} shot {}", e.id, format!("{:?}", f.kind).chars().take(200).collect::<String>()),
                        _ => {}
                    }
                }
                eprintln!("lf {lf} side {}: sprites {:?} numbers {:?}", side.slot, view.sprites.iter().map(|s| (s.entity, s.sheet.clone(), s.priority, s.x, s.y, format!("{:?}", s.effect))).collect::<Vec<_>>(), view.damage_numbers);
            }
            let ours = renderer.render(packs, &view, 0, 240, 160).to_vec();
            // (`DOJO_PIX_DUMP=<lf>-<lf>` with `DOJO_PIX_OUT`: our pictures of those frames)
            if let (Some(o), Ok(r)) = (std::env::var("DOJO_PIX_OUT").ok(), std::env::var("DOJO_PIX_DUMP")) {
                let r: Vec<i64> = r.split('-').filter_map(|x| x.parse().ok()).collect();
                if r.len() == 2 && (r[0]..=r[1]).contains(&lf) {
                    save(&PathBuf::from(o).join(format!("ours_{name}_{}_{lf}.png", side.slot)), &[&ours]);
                }
            }
            if let Some(p) = pend[si].take() {
                let keep = (p.4.clone(), p.6.clone());
                judge(si, p, last[si].as_ref(), &mut res);
                last[si] = Some(keep);
            }
            let Some((h, vf)) = side.want.get(&(lf + off)) else { continue };
            // the tear rule (menu_parity): the game's picture differs from both neighbours and they from each other
            let i = side.seq.iter().position(|e| e.0 == *vf).unwrap();
            // - only by a long iteration (a lag frame next to it: the logic counter held over a video frame)
            let long = (i > 0 && side.seq[i - 1].1 == side.seq[i].1) || (i + 1 < side.seq.len() && side.seq[i + 1].1 == side.seq[i].1) || (i > 1 && side.seq[i - 2].1 == side.seq[i - 1].1);
            let tear = long && i > 0 && i + 1 < side.seq.len() && side.seq[i].2 != side.seq[i - 1].2 && side.seq[i].2 != side.seq[i + 1].2 && side.seq[i - 1].2 != side.seq[i + 1].2;
            if std::env::var("DOJO_PIX_CAM").is_ok_and(|r| r.split('-').map(|x| x.parse::<i64>().unwrap_or(0)).collect::<Vec<_>>().windows(2).any(|w| (w[0]..=w[1]).contains(&lf))) {
                let c = &g.s.world.cameras[side.slot as usize];
                eprintln!("lf {lf} side {} camera {:?} fine {:?} view {:?}", side.slot, c.rect, (c.x, c.y), view.cameras);
                eprintln!("   hud {:?} bars {:?} recs {:?}", view.hud.iter().map(|h| (h.hp, h.hp_max, h.ep, h.ep_max)).collect::<Vec<_>>(), view.boss_bars, g.s.world.players.iter().map(|&id| { let p = g.s.world.pl_ref(g.s.world.find(id).unwrap()); (p.rec.hp, p.rec.max_hp, p.hp(), p.ep(), p.regen_timer, p.hp_regen_acc) }).collect::<Vec<_>>());
                eprintln!("   shake off {:?} shown {:?} ticked {} frame {} shakes {} numbers {:?}", g.s.world.g.shake_off, g.s.world.g.shake_shown, g.s.world.g.shakes_ticked, g.s.world.frame, g.s.world.g.shakes.len(), view.damage_numbers);
                eprintln!("   sprites {:?} players {:?}", view.sprites.iter().map(|d| (d.entity, d.sheet.clone(), d.x, d.y)).collect::<Vec<_>>(), g.s.world.players);
                let w = &g.s.world;
                let pk: Vec<_> = w.ents.iter().filter(|e| !e.removed).filter_map(|e| match &e.body {
                    wf_core::world::Body::Pickup(p) => Some((e.id, p.bx, p.x >> 8, p.y >> 8, p.t)),
                    _ => None,
                }).collect();
                let pp: Vec<_> = w.battle.popups.iter().map(|p| (p.0, p.1, p.2, p.4 >> 3, p.7)).collect();
                eprintln!("   pickups {pk:?} popups {pp:?} next_id {} frame {} fighters {:?}", w.next_id, w.frame, w.players.iter().map(|&id| { let a = &w.pl_ref(w.find(id).unwrap()).a; (a.x >> 8, a.y >> 8, a.foot_h) }).collect::<Vec<_>>());
            }

            pend[si] = Some((lf, h.clone(), *vf, tear, ours, format!("{:?} t {} down {}", g.st.phase, g.st.t, g.st.down_for), view, long));
        }
    }
    for (si, p) in pend.into_iter().enumerate() {
        if let Some(p) = p {
            judge(si, p, last[si].as_ref(), &mut res);
        }
    }
    res
}

#[test]
fn dojo_fight_screen_matches_the_game() {
    let Some(dir) = content_dir() else {
        eprintln!("no ROM: skipped");
        return;
    };
    let packs = Packs::new(vec![Box::new(DirSource(dir)) as Box<dyn FileSource>]).unwrap();
    let data = wf_app::battle::load_data(&packs).expect("data");
    let mut names: Vec<String> = std::fs::read_dir(repo().join("data/link"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str()?.strip_prefix("dojo_")?.strip_suffix(".json").map(String::from))
        .collect();
    names.sort();
    if let Ok(o) = std::env::var("DOJO_PIX_ONLY") {
        names.retain(|n| *n == o);
    }
    let mut bad = vec![];
    for name in &names {
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(repo().join(format!("data/link/dojo_{name}.json"))).unwrap()).unwrap();
        if doc["video"].is_null() {
            continue;
        }
        for (slot, n, exact, tears, diffs, torn) in run(&packs, &data, name) {
            // (the differing logic frames as ranges)
            let mut rg: Vec<(i64, i64)> = vec![];
            for &f in &diffs {
                match rg.last_mut() {
                    Some(r) if r.1 + 1 == f => r.1 = f,
                    _ => rg.push((f, f)),
                }
            }
            eprintln!("dojo_{name} side {slot}: {exact} / {n} pictures exact, {tears} torn as the tear rule allows, {torn} more torn frames skipped; differing {}: {:?}", diffs.len(), &rg[..rg.len().min(40)]);
            if !diffs.is_empty() {
                bad.push(format!("dojo_{name} side {slot}"));
            }
        }
    }
    assert!(bad.is_empty(), "pictures differ: {bad:?}");
}
