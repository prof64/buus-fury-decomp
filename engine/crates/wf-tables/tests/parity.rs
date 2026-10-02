//! Parity with the research JSON (the Python extractors' output in data/buu/ and data/battle/buu_ai_extra.json):
//! the content files written by `wf_tables::extract` are reassembled into the reference structures and compared
//! value by value. Skips when the ROM or a reference file is missing.

use serde_json::Value;
use wf_rom::{content::Content, testutil};
use wf_tables::diff::json_diff;

fn reference(name: &str) -> Option<Value> {
    let p = testutil::repo_root().join("data").join(name);
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Documented differences (engine/docs/tables.md "Parity"): (file, path, why).
const EXCLUSIONS: &[(&str, &str, &str)] = &[(
    "buu/enemy_ai.json",
    ".enemies[132].name",
    "reference was generated from an older enemies.json without the ROM-text name \"Uub\" for type 0x84; \
     enemies.json itself has it",
)];

fn check(name: &str, ours: &Value) -> usize {
    let Some(r) = reference(name) else {
        eprintln!("{name}: reference missing, skipped");
        return 0;
    };
    let d: Vec<String> = json_diff(ours, &r, 40)
        .into_iter()
        .filter(|x| {
            let ex = EXCLUSIONS.iter().find(|(f, p, _)| *f == name && x.starts_with(&format!("{p}:")));
            if let Some((_, p, why)) = ex {
                eprintln!("{name}: excluded {p} ({why})");
            }
            ex.is_none()
        })
        .collect();
    if d.is_empty() {
        eprintln!("{name}: identical");
    } else {
        eprintln!("{name}: {} differences (first 40):", d.len());
        for x in &d {
            eprintln!("  {x}");
        }
    }
    d.len()
}

#[test]
fn parity_from_content_files() {
    let Some(rom) = testutil::rom() else {
        eprintln!("ROM missing, skipped");
        return;
    };
    let mut out = Content::new();
    let t0 = std::time::Instant::now();
    wf_tables::extract(&rom, &mut out).unwrap();
    eprintln!("{} table files in {:.2} s", out.files.len(), t0.elapsed().as_secs_f64());
    let docs = wf_tables::content::reassemble(&|p| out.json(p));
    assert_eq!(docs.len(), 11);
    let mut bad = 0;
    for (name, doc) in &docs {
        bad += check(name, doc);
    }
    assert_eq!(bad, 0, "parity differences (see stderr)");
    // the string table is complete
    let s = out.json("text/strings.json").unwrap();
    assert!(s.as_array().unwrap().len() > 1500);
}
