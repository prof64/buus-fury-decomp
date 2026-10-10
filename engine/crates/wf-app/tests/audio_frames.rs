//! The app's audio engine plays each game frame's sound requests at that frame's driver update, as the game does
//! (one update per frame, ~267.5 samples): the device asks for samples in bursts of several frames, and the
//! requests of a burst must not all start in its first update. Headless, muted (nothing reaches a device).

use wf_app::audio::{AudioEngine, Resample};
use wf_app::wf_content::{FileSource, MemSource, Packs};
use wf_audio::driver::{Driver, MIX_RATE};
use wf_view::{AudioEvent, GBA_FPS};

fn packs() -> Option<Packs> {
    let rom = wf_rom::testutil::rom()?;
    let opts = wf_extract::Options { only: ["audio"].map(String::from).to_vec() };
    let (c, _) = wf_extract::extract_rom(&rom, &opts, &mut |_| {});
    Some(Packs::new(vec![Box::new(MemSource(c.files)) as Box<dyn FileSource>]).unwrap())
}

#[test]
fn each_frames_requests_start_at_its_own_update() {
    let Some(p) = packs() else {
        eprintln!("no ROM: skipped");
        return;
    };
    let bank: &'static _ = Box::leak(Box::new(wf_app::wf_content::audio::load_bank(&p).expect("bank")));
    // frames 0 .. 12: a punch sound on frames 1, 2 and 5, a hit on 2, a stop on 9 (requests a frame or two apart)
    let frames: Vec<Vec<AudioEvent>> = (0..12)
        .map(|f| match f {
            1 | 5 => vec![AudioEvent::PlaySfx { id: 20, volume: 64, pan: 0 }],
            2 => vec![AudioEvent::PlaySfx { id: 20, volume: 64, pan: 0 }, AudioEvent::PlaySfx { id: 48, volume: 64, pan: 0 }],
            9 => vec![AudioEvent::StopSfx(20)],
            _ => vec![],
        })
        .collect();
    // the game: one driver update per frame with that frame's requests
    let mut d = Driver::new(bank);
    let mut want: Vec<(i8, i8)> = Vec::new();
    let mut acc = 0.0f64;
    for g in &frames {
        for e in g {
            match *e {
                AudioEvent::PlaySfx { id, volume, pan } => {
                    d.play_sfx(id, volume, pan);
                }
                AudioEvent::StopSfx(id) => d.stop_sfx(id),
                _ => {}
            }
        }
        acc += MIX_RATE / GBA_FPS;
        let n = wf_app::audio::update_samples(&mut acc);
        assert!(n == 256 || n == 272, "an update is whole 16-sample blocks: {n}");
        d.render_into(&mut want, n);
    }
    // the app: every frame's requests handed over at once (one device burst), then the samples asked for at the
    // driver's own rate (no resampling)
    let mut a = AudioEngine::new(bank, MIX_RATE.round() as u32);
    a.resample = Resample::Hold;
    // (three frames handed over per device callback of nearly three frames' samples - a 50 ms buffer; three updates
    // are at least 784 samples)
    let mut got: Vec<(i8, i8)> = Vec::new();
    let mut out = vec![0.0f32; 2 * 760];
    for burst in frames.chunks(3) {
        for g in burst {
            a.push_frame(g.clone());
        }
        a.render(&mut out);
        got.extend(out.chunks_exact(2).map(|c| ((c[0] * 128.0).round() as i8, (c[1] * 128.0).round() as i8)));
    }
    let n = got.len().min(want.len());
    if let Some(i) = (0..n).find(|&i| got[i] != want[i]) {
        eprintln!("first difference at sample {i}: {:?} vs {:?}; nonzero want from {:?}", &got[i..(i + 4).min(n)], &want[i..(i + 4).min(n)], want.iter().position(|s| *s != (0, 0)));
    }
    // (the device rate rounded to an integer: the engine's step drifts by 1e-5 per sample - none over 3,000)
    assert_eq!(&got[..n], &want[..n], "the requests did not start at their frames' updates");
}
