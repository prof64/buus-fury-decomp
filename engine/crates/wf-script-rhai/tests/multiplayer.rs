//! Multiplayer script API: per-player handlers (`who`), `player(n)`, per-player effects, queue rules, and
//! rollback of a multi-step handler. Runs without the ROM (a mod script only).
use std::collections::BTreeMap;
use wf_script::testhost::{Effect, TestHost};
use wf_script_rhai::{Runtime, Status};

const MOD: &str = r#"
// co-op chest: everyone who opens it gets a potion; the opener also gets a capsule
fn on_open(who) {
    let n = player_count();
    for i in 1..=n {
        give_item_to(i, 3);              // queued per-player action
    }
    give_item_to(who, 41);
    set_flag(900);                       // shared story flag
    return next(1);
}
fn step_1(result) {
    // query after the queue drained: legal
    if player_alive(who()) != 0 {
        set_player_hp(who(), 100);
    }
    face(player(who()), 0);
    say_to(who(), 8, 1201);
}
// misuse: a query behind queued actions must be rejected
fn bad(who) {
    give_item_to(who, 1);
    player_hp(who);
}
"#;

fn runtime(players: i32) -> (Runtime, TestHost) {
    let h = TestHost::new(3);
    h.0.borrow_mut().players = players;
    let mut files = BTreeMap::new();
    files.insert("scripts/mods/chest.rhai".to_string(), MOD.to_string());
    let rt = Runtime::new(&files, Box::new(h.clone())).unwrap();
    (rt, h)
}

fn finish(rt: &mut Runtime, h: &TestHost, mut st: Result<Status, String>) -> Result<(), String> {
    while let Status::Blocked = st.clone()? {
        st = rt.resume(h.take_pending());
    }
    st.map(|_| ())
}

#[test]
fn per_player_handler_effects() {
    let (mut rt, h) = runtime(3);
    rt.start_handler("chest", "on_open", 2, [0; 4]);
    let st = rt.run();
    finish(&mut rt, &h, st).unwrap();
    let ext: Vec<(String, Vec<i32>, i32)> = h
        .trace()
        .into_iter()
        .filter_map(|e| match e {
            Effect::Ext { name, args, who } => Some((name, args, who)),
            _ => None,
        })
        .collect();
    let names: Vec<&str> = ext.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(&names[..4], ["give_item_to"; 4]);
    assert_eq!(ext[0].1, vec![1, 3]);
    assert_eq!(ext[2].1, vec![3, 3]);
    assert_eq!(ext[3].1, vec![2, 41]);
    assert!(ext.iter().all(|e| e.2 == 2), "who is carried to the host");
    assert!(names.contains(&"say_to"));
    // face(player(2), 0): player 2's actor id from the host
    assert!(h.trace().iter().any(|e| matches!(e, Effect::Native { native: 0x53, args, .. } if args == &vec![0x202, 0])));
}

#[test]
fn query_behind_queued_action_is_rejected() {
    let (mut rt, _h) = runtime(2);
    rt.start_handler("chest", "bad", 1, [0; 4]);
    let e = rt.run().unwrap_err();
    assert!(e.contains("return next"), "{e}");
}

#[test]
fn rollback_mid_handler() {
    let (mut rt, h) = runtime(4);
    rt.start_handler("chest", "on_open", 3, [0; 4]);
    let st = rt.run().unwrap();
    assert_eq!(st, Status::Blocked);
    let snap = (rt.state(), h.0.borrow().clone(), rt.globals());
    let st = rt.resume(h.take_pending());
    finish(&mut rt, &h, st).unwrap();
    let a = h.trace();
    rt.restore(snap.0.clone());
    *h.0.borrow_mut() = snap.1.clone();
    rt.set_globals(snap.2.clone());
    let st = rt.resume(h.take_pending());
    finish(&mut rt, &h, st).unwrap();
    assert_eq!(a, h.trace());
    // the snapshot is plain data
    let j = serde_json::to_string(&snap.0).unwrap();
    assert!(j.contains("\"who\":3"));
}
