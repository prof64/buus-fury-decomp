//! `wf-extract`: the "bring your own ROM" extractor. ROM bytes in, the game's base content out as plain
//! files (see `engine/CONTENT.md`). No emulator, no file system: the result is an in-memory [`Content`]
//! tree, written to disk by `wf-cli` or handed to JavaScript by `wf-wasm`.

pub mod annotations;
pub mod effects;
pub mod fx_extra;
pub mod misc;
pub mod sheet;
pub mod sprites;

pub use wf_rom::{content::Content, Error, Rom};

/// Categories, in extraction order. `--only` / the wasm API select a subset by these names.
pub const CATEGORIES: &[&str] =
    &["tables", "sprites", "effects", "portraits", "items", "maps", "audio", "ui", "world", "scripts"];

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Restrict to these categories (empty = all).
    pub only: Vec<String>,
}

impl Options {
    fn wants(&self, c: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|o| o == c)
    }
}

/// Timing of one category, for the report.
#[derive(Clone, Debug)]
pub struct Timing {
    pub category: &'static str,
    pub millis: f64,
    pub files: usize,
}

/// Verify the ROM and extract. Paths in the returned tree are relative to the `base/` content pack.
pub fn extract(rom_bytes: Vec<u8>, opts: &Options) -> Result<(Content, Vec<Timing>), Error> {
    let rom = Rom::new(rom_bytes)?;
    Ok(extract_rom(&rom, opts, &mut |_| {}))
}

/// Extract from an already verified ROM. `progress` is called with each category name before it runs.
pub fn extract_rom(rom: &Rom, opts: &Options, progress: &mut dyn FnMut(&str)) -> (Content, Vec<Timing>) {
    let mut out = Content::new();
    let mut timings = Vec::new();
    // effects exclude every frame used by a sprite definition
    let mut known = None;
    for &cat in CATEGORIES {
        if !opts.wants(cat) {
            continue;
        }
        progress(cat);
        let before = out.files.len();
        let t0 = now_ms();
        let r = match cat {
            "tables" => wf_tables::extract(rom, &mut out),
            "maps" => wf_maps::extract(rom, &mut out),
            "audio" => wf_audio::extract(rom, &mut out),
            "ui" => wf_ui::extract(rom, &mut out),
            // after portraits (speaker names) and maps (Tiled object layers)
            "world" => wf_world::extract(rom, &mut out),
            "scripts" => wf_script::content::extract(rom, &mut out),
            "sprites" => {
                known = Some(sprites::known_frames(&sprites::extract(rom, &mut out)));
                Ok(())
            }
            "effects" => {
                let k = known.take().unwrap_or_else(|| sprites::known_frames(&sprites::all_defs(rom)));
                effects::extract(rom, &mut out, &k);
                Ok(())
            }
            "portraits" => {
                misc::portraits(rom, &mut out, &annotations::Names::load());
                Ok(())
            }
            "items" => {
                misc::item_icons(rom, &mut out);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = r {
            out.warn(format!("{cat}: {e}"));
        }
        timings.push(Timing { category: cat, millis: now_ms() - t0, files: out.files.len() - before });
    }
    write_manifest(rom, &mut out, &timings);
    (out, timings)
}

fn write_manifest(rom: &Rom, out: &mut Content, timings: &[Timing]) {
    let files: serde_json::Map<String, serde_json::Value> = CATEGORIES
        .iter()
        .map(|c| (c.to_string(), serde_json::json!(timings.iter().find(|t| t.category == *c).map(|t| t.files))))
        .collect();
    let warnings = out.warnings.clone();
    // every file of the pack, for loaders that cannot list directories (HTTP)
    let list: Vec<&String> = out.files.keys().collect();
    let list = serde_json::json!(list);
    out.put_json("files.json", &list);
    out.put_json(
        "pack.json",
        &serde_json::json!({
            "id": "base",
            "name": "Dragon Ball Z: Buu's Fury (base game)",
            "format_version": 1,
            "format_doc": "engine/CONTENT.md",
            "generated_by": concat!("wf-extract ", env!("CARGO_PKG_VERSION")),
            "source_rom": {"title": "Dragon Ball Z - Buu's Fury (USA)", "sha1": rom.sha1()},
            "files_per_category": files,
            "warnings": warnings,
        }),
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    0.0
}
