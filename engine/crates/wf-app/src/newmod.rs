//! `buus-fury --new-mod NAME [--dir PARENT]`: a new mod folder that is valid and playable out of the box, as a
//! starting point (engine/docs/modding.md). It holds one example of each kind of change, all small:
//!
//! * `pack.json` - the mod's id (= NAME), name, format version, `depends: ["base"]`
//! * `README.md` - what each file does and the next steps (check, play, the guide's sections)
//! * `characters/goku.patch.json` - a plain-file merge patch: Goku's starting END 26 -> 36 (Status page)
//! * `scripts/events/s048.text.json` - a dialogue override: King Yemma's first line (Snakeway's office, 1-3)
//! * `world/areas/1-1+<NAME>.json` + `scripts/events/sNNN.rhai` - an NPC appended to Snakeway (1-1, the first area
//!   of a New Game, by the road east of where Goku starts) whose Rhai talk event says hello and sets a story flag
//!
//! The event number (600..899) and the flag (900..1143) come from a hash of NAME so two scaffolds rarely collide;
//! `--modcheck` reports a clash if they do.

use std::path::{Path, PathBuf};

/// the sample NPC: Snakeway 1-1, above the road east of Goku's start (144, 152); script sprite 65 (Chiaotzu)
pub const SAMPLE_AREA: (i32, i32) = (1, 1);
pub const SAMPLE_NPC_POS: (i32, i32) = (200, 128);
/// the patched value: Goku's new-game END (base 26)
pub const SAMPLE_END: i32 = 36;
/// the overridden dialogue: King Yemma's first line in s048 (node 1)
pub const SAMPLE_TEXT_EVENT: u32 = 48;

/// what `scaffold` made
#[derive(Debug, Clone)]
pub struct NewMod {
    pub dir: PathBuf,
    pub event: u32,
    pub flag: u32,
    pub files: Vec<String>,
}

fn fnv(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// a mod id: 1..48 of `a-z 0-9 - _`, starting with a letter or digit
pub fn valid_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 48
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(format!(
            "'{name}' is not a valid mod name: use 1..48 lowercase letters, digits, '-' or '_', starting with a letter or digit (e.g. my-first-mod)"
        ))
    }
}

/// the event / flag numbers a mod named `name` gets
pub fn numbers(name: &str) -> (u32, u32) {
    let h = fnv(name);
    (600 + h % 300, 900 + (h / 300) % 244)
}

fn title(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write the new mod `parent/name/`. Refuses a bad name and an existing folder (nothing is overwritten).
pub fn scaffold(parent: &Path, name: &str) -> Result<NewMod, String> {
    valid_name(name)?;
    let dir = parent.join(name);
    if dir.exists() {
        return Err(format!("{} already exists - pick another name or remove it first (nothing was written)", dir.display()));
    }
    let (event, flag) = numbers(name);
    let disp = title(name);
    let (ax, aa) = SAMPLE_AREA;
    let (nx, ny) = SAMPLE_NPC_POS;
    let mut files: Vec<(String, String)> = vec![];
    files.push((
        "pack.json".into(),
        format!(
            "{{\n  \"id\": \"{name}\",\n  \"name\": \"{disp}\",\n  \"format_version\": 1,\n  \"depends\": [\"base\"]\n}}\n"
        ),
    ));
    files.push((
        "characters/goku.patch.json".into(),
        format!("{{\n  \"new_game\": {{\n    \"END\": {SAMPLE_END}\n  }}\n}}\n"),
    ));
    files.push((
        format!("scripts/events/s{SAMPLE_TEXT_EVENT:03}.text.json"),
        format!("{{\n  \"nodes\": {{\n    \"1\": \"Next soul, please! (This line comes from the {name} mod.)\"\n  }}\n}}\n"),
    ));
    files.push((
        format!("world/areas/{ax}-{aa}+{name}.json"),
        format!(
            r#"{{
  "key": "{ax}-{aa}",
  "add_objects": [
    {{
      "type": "npc",
      "comment": "a sample NPC by the road east of where Goku starts; talking to him (A) runs event s{event:03}",
      "condition": {{ "always": true }},
      "pos": [{nx}, {ny}],
      "sprite_index": 65,
      "talk": {{ "event": "s{event:03}" }},
      "behaviour": []
    }}
  ]
}}
"#
        ),
    ));
    files.push((
        format!("scripts/events/s{event:03}.rhai"),
        format!(
            r#"// Event {event} (mod: {name}): the sample NPC in Snakeway ({ax}-{aa}). Talk to him with A.
// An event is a list of nodes: `say` shows a text box (box = 8 + portrait index: 19 = Chiaotzu), `code` calls
// nN_<entry>() (a returned number jumps to that node), `end` finishes. Flags for mods: 900..1143.
fn event() {{
    [
        #{{ node: 0, code: "n0_", entry: "main", next: 1 }},
        #{{ node: 1, say: "Hi! I'm from the {name} mod. Edit scripts/events/s{event:03}.rhai to change what I say.", box: 19, next: 2 }},
        #{{ node: 2, code: "n2_", entry: "main", next: 4 }},
        #{{ node: 3, say: "Hello again! (story flag {flag} is set)", box: 19, next: 4 }},
        #{{ node: 4, end: 0 }},
    ]
}}

// node 0: met before -> node 3
fn n0_main() {{
    if get_flag({flag}) != 0 {{
        return 3;
    }}
}}

// node 2: remember the meeting
fn n2_main() {{
    set_flag({flag});
}}
"#
        ),
    ));
    files.push(("README.md".into(), readme(name, &disp, event, flag)));
    // write: everything or nothing
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    for (p, s) in &files {
        let f = dir.join(p);
        let r = f.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| std::fs::write(&f, s));
        if let Err(e) = r {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("cannot write {}: {e}", f.display()));
        }
    }
    Ok(NewMod { dir, event, flag, files: files.into_iter().map(|f| f.0).collect() })
}

fn readme(name: &str, disp: &str, event: u32, flag: u32) -> String {
    format!(
        r#"# {disp}

A new mod made by `buus-fury --new-mod {name}`. It already works: each file below is one small example of a kind of
change. Edit them, delete what you do not need, add more (the guide: `engine/docs/modding.md`).

| file | what it does | guide |
|---|---|---|
| `pack.json` | the mod's id, name and what it loads after | §1 |
| `characters/goku.patch.json` | a merge patch: Goku starts a New Game with END {SAMPLE_END} (base 26) - see the Status page | §2, §5c |
| `scripts/events/s{SAMPLE_TEXT_EVENT:03}.text.json` | replaces one line of an existing event: King Yemma's first line (Snakeway, his office 1-3) | §5c |
| `world/areas/1-1+{name}.json` | adds an NPC to Snakeway (1-1, the first area of a New Game), by the road east of where Goku starts | §5, §5d |
| `scripts/events/s{event:03}.rhai` | his talk event in Rhai: a text box, then story flag {flag} is set (talk again: another line) | §5d, §6 |

## Next steps

1. **Check it:** `buus-fury --modcheck path/to/{name}` - prints `OK`, or each problem with the file and a hint.
2. **Play it:** put the folder in your mods folder (`<data dir>/wf-buu/content/mods/`, macOS
   `~/Library/Application Support/wf-buu/content/mods/`) or run `buus-fury --mod path/to/{name}`; start a New Game
   and walk right from where Goku stands - talk to the NPC with A.
3. **Headless (no window, no sound):** `buus-fury --headless --mute --mod path/to/{name} --story 1-1 --frames 120
   --shot out.ppm`.
4. **Change things:** numbers in the JSON files, text in the `.rhai` / `.text.json` files; run `--modcheck` again.

Numbers this mod uses: event {event}, story flag {flag} (picked from the mod's name; mods use events 600+ and flags
900..1143 - change them if `--modcheck` reports a clash with another mod).
"#
    )
}
