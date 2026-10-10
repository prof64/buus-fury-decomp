//! Survey: load every event and script, check stack-depth consistency, decompile, print stats.
//! `cargo run -p wf-script --example survey [--dis <slug>]`
use wf_script::{decompile, natives, ScriptSet};

fn main() {
    let rom = wf_rom::testutil::rom().expect("roms/buu.gba");
    let set = ScriptSet::load_all(&rom);
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 && args[1] == "--dis" {
        let sc = set.scripts.values().find(|s| s.id.slug() == args[2]).expect("no such script");
        for ins in sc.insns.values() {
            println!("{:04x}: {}", ins.off, wf_script::bytecode::format_insn(ins, &natives::name));
        }
        return;
    }
    let nodes: usize = set.events.values().map(|e| e.entries.len()).sum();
    println!("events {} entries {} scripts {} errors {}", set.events.len(), nodes, set.scripts.len(), set.errors.len());
    for (id, e) in &set.errors {
        println!("  load error {id:?}: {e}");
    }
    let (mut ok, mut st, mut bad) = (0, 0, 0);
    for sc in set.scripts.values() {
        match decompile::decompile(sc, &sc.id.slug()) {
            Ok(d) => {
                ok += 1;
                if matches!(d.func.body, wf_script::ir::Body::Structured(_)) {
                    st += 1;
                } else {
                    println!("  unstructured {}: {:?}", sc.id.slug(), d.structure_error);
                }
            }
            Err(e) => {
                bad += 1;
                println!("  decompile error {}: {}", sc.id.slug(), e);
            }
        }
    }
    println!("decompiled {ok} (structured {st}), failed {bad}");
}
