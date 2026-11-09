//! `cargo run --release -p wf-app --example ai_tree -- 798 ..`: an AI state's behaviour tree as our data has it
//! (node index, kind, address, params, kids) - for reading an audit's `ai` column.
use wf_app::wf_content::{DirSource, FileSource, Packs};

fn main() {
    let repo = wf_rom::testutil::repo_root();
    let content = repo.join("engine/target/tmp/wf-import-segment-content");
    let packs = Packs::new(vec![Box::new(DirSource(content)) as Box<dyn FileSource>]).unwrap();
    let d = wf_app::battle::load_data(&packs).expect("data");
    for a in std::env::args().skip(1) {
        let Ok(st) = a.parse::<u16>() else { continue };
        for (name, map) in [("state", &d.states), ("aux", &d.aux)] {
            if let Some(t) = map.get(&st) {
                println!("{name} {st}:");
                for (k, n) in t.nodes.iter().enumerate() {
                    println!("  {k}: {n:?}");
                }
            }
        }
    }
}
