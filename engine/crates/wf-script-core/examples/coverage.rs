//! Print the native → core binding table (markdown) used in engine/docs/script-host.md.
//! `cargo run -p wf-script-core --example coverage`
use wf_script::natives;
use wf_script_core::binding::{binding, Where};

fn main() {
    let mut count = std::collections::BTreeMap::new();
    println!("| id | native | kind | where | system / note |");
    println!("|---|---|---|---|---|");
    for n in 0..natives::NATIVE_COUNT as u8 {
        let b = binding(n);
        let kind = natives::info(n).and_then(|i| i.kind.clone()).unwrap_or_default();
        let at = match b.at {
            Where::Vm => "VM",
            Where::Core => "core",
            Where::Story => "story",
            Where::Scene => "scene",
            Where::Field => "field",
            Where::Partial => "partial",
        };
        *count.entry(at).or_insert(0) += 1;
        println!("| 0x{n:02X} | `{}` | {kind} | {at} | {} |", natives::name(n), b.note.replace('|', "\\|"));
    }
    println!();
    println!("Counts: {}", count.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
}
