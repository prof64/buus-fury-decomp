//! Format + timeline behaviour on a toy deterministic sim (no content needed). The real-battle tests live in
//! `wf-app/tests/replay.rs`.

use wf_replay::*;
use wf_view::{RenderView, Sim, SimEvent};

#[derive(Clone)]
struct Toy {
    frame: u64,
    x: u64,
    acc: Vec<u8>,
}

impl Sim for Toy {
    fn step(&mut self, inputs: &[u16], _: &mut Vec<SimEvent>) {
        self.frame += 1;
        let i = inputs.first().copied().unwrap_or(0) as u64;
        self.x = self.x.wrapping_mul(6364136223846793005).wrapping_add(i + 1);
        self.acc[(self.x % 4096) as usize] ^= 1;
    }
    fn view(&self, _: &mut RenderView) {}
    fn frame(&self) -> u64 {
        self.frame
    }
    fn state_hash(&self) -> u64 {
        fnv64(&self.snapshot().unwrap())
    }
    fn snapshot(&self) -> Option<Vec<u8>> {
        let mut v = self.frame.to_le_bytes().to_vec();
        v.extend(self.x.to_le_bytes());
        v.extend(&self.acc);
        Some(v)
    }
    fn restore(&mut self, s: &[u8]) -> bool {
        self.frame = u64::from_le_bytes(s[..8].try_into().unwrap());
        self.x = u64::from_le_bytes(s[8..16].try_into().unwrap());
        self.acc = s[16..].to_vec();
        true
    }
}

fn toy() -> Toy {
    Toy { frame: 0, x: 1, acc: vec![0; 4096] }
}

fn header() -> Header {
    Header { sim: "toy".into(), players: 1, hash_interval: 60, config: serde_json::json!({"seed": 1}), ..Default::default() }
}

fn input(f: u64) -> u16 {
    if (f / 40) % 3 == 0 { 0x10 } else if f % 97 < 5 { 1 } else { 0 }
}

#[test]
fn record_verify_round_trip() {
    let mut s = toy();
    let mut rec = Recorder::new(header(), 0, s.state_hash(), None);
    let mut ev = Vec::new();
    for f in 0..3600 {
        let i = [input(f)];
        s.step(&i, &mut ev);
        rec.record(&i, s.frame(), || s.state_hash());
    }
    rec.finish(s.state_hash());
    let bytes = rec.replay.to_bytes();
    println!("toy replay: 3600 frames -> {} bytes", bytes.len());
    assert!(bytes.len() < 2000);
    let r = Replay::from_bytes(&bytes).unwrap();
    assert_eq!(r, {
        let mut x = rec.replay.clone();
        x.header.frames = 3600;
        x.header.final_hash = format!("{:016x}", s.state_hash());
        x
    });
    let rep = verify(&r, &mut toy());
    assert!(rep.ok, "{rep:?}");
    assert_eq!(rep.final_hash, s.state_hash());

    // a tampered input diverges at the next checkpoint
    let mut bad = r.clone();
    let mut l = InputLog::new(1, 0);
    for (f, x) in r.inputs.frames().enumerate() {
        l.push(&[if f == 1000 { x[0] ^ 2 } else { x[0] }]);
    }
    bad.inputs = l;
    let rep = verify(&bad, &mut toy());
    let d = rep.divergence.unwrap();
    assert_eq!((d.last_good, d.frame), (960, 1020));

    // JSON interchange, and the agent server's format
    let j = serde_json::to_vec(&r.to_json()).unwrap();
    assert_eq!(Replay::from_any(&j).unwrap(), r);
    assert_eq!(Replay::from_any(&bytes).unwrap(), r);
    let agent = serde_json::json!({"format": "wf-agent-replay/1", "config": {"seed": 1}, "slots": 1,
        "inputs": [[[16], 40], [[0], 2]], "frames": 42, "hash": format!("{:016x}", 99u64), "outcome": ""});
    let a = Replay::from_any(agent.to_string().as_bytes()).unwrap();
    assert_eq!((a.inputs.len(), a.inputs.get(39), a.checkpoints.clone()), (42, Some(&[16u16][..]), vec![(42, 99)]));

    // checksum catches corruption
    let mut c = bytes.clone();
    c[40] ^= 1;
    assert!(Replay::from_bytes(&c).is_err());
}

#[test]
fn timeline_seek_and_rewind() {
    let mut log = InputLog::new(1, 0);
    let mut s = toy();
    let mut t = Timeline::new(TimelineConfig { interval: 30, window: 600, sparse_every: 240, ..Default::default() });
    t.capture(&s);
    let mut hashes = vec![s.state_hash()];
    let mut ev = Vec::new();
    for f in 0..3000 {
        let i = [input(f)];
        log.push(&i);
        s.step(&i, &mut ev);
        t.on_frame(&s);
        hashes.push(s.state_hash());
    }
    // the dense window is pruned to sparse snapshots before it
    assert!(t.len() < 3000 / 30);
    for target in [2999, 2500, 17, 0, 1234, 2990, 3000, 100] {
        assert_eq!(t.seek(&mut s, &log, target), target);
        assert_eq!(s.state_hash(), hashes[target as usize], "seek {target}");
    }
    // rewind then continue with the same inputs == uninterrupted
    t.seek(&mut s, &log, 2000);
    for f in 2000..3000 {
        s.step(&[input(f)], &mut ev);
    }
    assert_eq!(s.state_hash(), hashes[3000]);
}

#[test]
fn save_state_round_trip() {
    let mut s = toy();
    let mut ev = Vec::new();
    let mut rec = Recorder::new(header(), 0, s.state_hash(), None);
    for f in 0..500 {
        s.step(&[input(f)], &mut ev);
        rec.record(&[input(f)], s.frame(), || s.state_hash());
    }
    let st = SaveState {
        header: StateHeader { sim: "toy".into(), frame: s.frame(), hash: format!("{:016x}", s.state_hash()), ..Default::default() },
        snapshot: s.snapshot().unwrap(),
        replay: Some(rec.replay.clone()),
    };
    let b = st.to_bytes();
    let back = SaveState::from_bytes(&b).unwrap();
    assert_eq!(back.snapshot, st.snapshot);
    assert_eq!(back.replay.unwrap().inputs, rec.replay.inputs);
    assert_eq!(SaveState::peek_header(&b).unwrap().frame, 500);
}
