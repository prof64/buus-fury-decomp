//! The field game: area loading from content, spawn conditions, exits and transitions, pickups, breakables,
//! story state, determinism and snapshots. Emulator ground truth (data/field/*.json) is replayed in field_lockstep.rs.
mod common;
use wf_core::battle::RngInit;
use wf_core::field::{AreaId, FieldConfig, FieldEvent};
use wf_core::input;
use wf_core::session::Session;
use wf_core::world::{Body, Event};

fn session(world: i32, area: i32, pos: (i32, i32)) -> Session {
    Session::new_field(common::field_data(), FieldConfig { world, area, pos: Some(pos), rng: RngInit::Seed(7), ..Default::default() })
}

fn count(s: &Session, kind: &str) -> usize {
    s.world.ents.iter().filter(|e| e.kind() == kind).count()
}

fn area(s: &Session) -> AreaId {
    s.world.field.as_ref().unwrap().area
}

#[test]
fn loads_area_objects_by_condition() {
    let s = session(8, 3, (100, 192));
    assert_eq!(area(&s), AreaId { world: 8, area: 3, variant: 1 });
    // Southwest Forest: 4 Majin Fighters while flag 163 is clear, the script-conditioned ones need 163 && !164
    let enemies = s.world.ents.iter().filter(|e| matches!(&e.body, Body::Enemy(n) if n.ty == 68)).count();
    assert_eq!(enemies, 4);
    assert!(s.world.field.as_ref().unwrap().triggers.len() >= 3, "map header exits");
    let mut st = s.world.story.clone();
    st.set_flag(163, true);
    let s2 = Session::new_field(common::field_data(), FieldConfig { world: 8, area: 3, pos: Some((100, 192)), story: Some(st), rng: RngInit::Seed(7), ..Default::default() });
    let e2: Vec<i32> = s2.world.ents.iter().filter_map(|e| if let Body::Enemy(n) = &e.body { Some(n.ty) } else { None }).collect();
    assert!(!e2.contains(&68), "flag 163 set removes the Majin Fighters: {e2:?}");
    assert!(e2.contains(&21), "the inline condition get_flag(163) && not_flag(164) spawns type 21: {e2:?}");
}

#[test]
fn exit_moves_to_destination_area() {
    // 8-3's top exit: rect (64, 0)..(113, 17) -> 8-6 at (72, 364)
    let mut s = session(8, 3, (80, 40));
    let mut entered = None;
    for _ in 0..200 {
        let ev = s.step(&[input::UP]).to_vec();
        for e in ev {
            if let Event::Field(FieldEvent::AreaEnter { area, .. }) = e {
                entered = Some(area);
            }
        }
        if entered.is_some() {
            break;
        }
    }
    assert_eq!(entered.map(|a| (a.world, a.area)), Some((8, 6)));
    let pi = s.world.player_idx(0).unwrap();
    let a = &s.world.pl_ref(pi).a;
    assert_eq!((a.px(), a.py()), (72, 364));
}

#[test]
fn deterministic_with_snapshots() {
    let keys = |f: u32| [0u16, input::UP, input::LEFT, input::RIGHT, input::DOWN, input::A, 0, input::B][((f / 13).wrapping_mul(2654435761) >> 9) as usize % 8];
    let mut a = session(8, 3, (200, 300));
    let mut b = session(8, 3, (200, 300));
    let mut hashes = vec![];
    let mut snap = None;
    let mut bytes = None;
    for f in 0..900 {
        if f == 300 {
            snap = Some(a.snapshot());
            bytes = Some(a.snapshot_bytes());
        }
        a.step(&[keys(f)]);
        b.step(&[keys(f)]);
        assert_eq!(a.hash(), b.hash(), "frame {f}");
        hashes.push(a.hash());
    }
    let mut c = session(8, 3, (200, 300));
    c.restore(snap.as_ref().unwrap());
    let mut d = session(1, 1, (8, 167));
    assert!(d.restore_bytes(bytes.as_ref().unwrap()));
    for f in 300..900 {
        c.step(&[keys(f)]);
        d.step(&[keys(f)]);
        assert_eq!(c.hash(), hashes[f as usize], "clone restore frame {f}");
        assert_eq!(d.hash(), hashes[f as usize], "bytes restore frame {f}");
    }
}

#[test]
fn story_state_basics() {
    let s = session(8, 3, (100, 192));
    let st = &s.world.story;
    assert!(st.chars[0].in_party());
    assert_eq!(st.chars.len(), 9);
    assert!(st.chars[0].rec.max_hp > 0);
}
