//! The real simulation: `wf-core`'s battle `Session` (it implements `wf_view::Sim`) fed from the same layered
//! content packs as the renderer, so mods apply to both.

use std::sync::Arc;

use wf_content::Packs;
pub use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::{ContentSource, GameData};
use wf_core::session::Session;
use wf_view::{Buttons, RenderView, Sim, SimEvent};

/// `wf-core`'s content trait over the merged packs (overrides / merge patches / deletes already applied).
pub struct PacksSource<'a>(pub &'a Packs);

impl ContentSource for PacksSource<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read(path)
    }
    fn list(&self) -> Vec<String> {
        self.0.list()
    }
}

/// Game data for the core, loaded once per content set (also registers modded fighters for the lobby menu).
pub fn game_data(packs: &Packs) -> Result<Arc<GameData>, String> {
    let d = GameData::load(&PacksSource(packs)).map(Arc::new).map_err(|e| e.to_string())?;
    crate::netmenu::set_roster(&d);
    Ok(d)
}

/// A battle session (`arena` names / enemy ids as in `wf_core::battle`, e.g. arena `forest`, enemy `t68`).
pub fn session(packs: &Packs, cfg: BattleConfig) -> Result<Session, String> {
    Ok(Session::new(game_data(packs)?, cfg))
}

/// `wf-core`'s session as the app's `Sim`: delegates everything to the core's own `Sim` impl and exposes the
/// concrete session (`Sim::as_any`) to the tools that read core state (training overlays, ghosts, replays).
pub struct CoreSim(pub Session);

impl CoreSim {
    /// The core session behind an app sim, if it is one.
    pub fn of(sim: &dyn Sim) -> Option<&Session> {
        sim.as_any()?.downcast_ref::<CoreSim>().map(|c| &c.0)
    }
}

impl Sim for CoreSim {
    fn step(&mut self, inputs: &[Buttons], events: &mut Vec<SimEvent>) {
        Sim::step(&mut self.0, inputs, events)
    }
    fn view(&self, out: &mut RenderView) {
        Sim::view(&self.0, out)
    }
    fn frame(&self) -> u64 {
        Sim::frame(&self.0)
    }
    fn state_hash(&self) -> u64 {
        Sim::state_hash(&self.0)
    }
    fn snapshot(&self) -> Option<Vec<u8>> {
        Sim::snapshot(&self.0)
    }
    fn restore(&mut self, s: &[u8]) -> bool {
        Sim::restore(&mut self.0, s)
    }
    fn players(&self) -> usize {
        Sim::players(&self.0)
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

/// Sim kind written into replays / save states of a wf-core battle.
pub const SIM_KIND: &str = "wf-core/battle";

/// What a recording of these packs depends on (engine build, extractor, ROM, mods + their bytes).
pub fn content_id(packs: &Packs) -> wf_replay::ContentId {
    let base = packs.base();
    let rom_sha1 = packs.packs[0]
        .src
        .read("pack.json")
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["source_rom"]["sha1"].as_str().map(String::from))
        .unwrap_or_default();
    let mut h = 0u64;
    let mut mods = Vec::new();
    for p in &packs.packs[1..] {
        mods.push(p.info.id.clone());
        h = wf_replay::fnv64_update(h ^ 0x9E37_79B9_7F4A_7C15, p.info.id.as_bytes());
        let mut files = p.src.list();
        files.sort();
        for f in files {
            h = wf_replay::fnv64_update(h, f.as_bytes());
            h = wf_replay::fnv64_update(h, &p.src.read(&f).unwrap_or_default());
        }
    }
    wf_replay::ContentId {
        engine: concat!("wf-core ", env!("CARGO_PKG_VERSION")).into(),
        extractor: base.generated_by.clone(),
        rom_sha1,
        mods,
        mods_hash: format!("{h:016x}"),
    }
}

/// Short title for listings: `forest: t68 x2 (goku)`.
pub fn title(cfg: &BattleConfig) -> String {
    let enemies: Vec<String> = cfg.phases.iter().map(|p| if p.count > 1 { format!("{} x{}", p.enemy, p.count) } else { p.enemy.clone() }).collect();
    let fighters: Vec<&str> = cfg.players.iter().map(|f| f.fighter.as_str()).collect();
    format!("{}: {} ({})", cfg.arena, enemies.join(", "), fighters.join("+"))
}

/// Seed of a config (for listings).
pub fn seed(cfg: &BattleConfig) -> u64 {
    match cfg.rng {
        RngInit::Seed(s) => s as u64,
        RngInit::State(a, b) => ((a as u64) << 32) | b as u64,
    }
}

/// `BattleConfig` from simple CLI / URL parameters.
pub fn config(arena: &str, enemy: &str, count: u32, fighter: &str, form: Option<i32>, level: Option<i32>, players: usize, seed: u32) -> BattleConfig {
    let mut f = FighterSetup { fighter: fighter.into(), ..Default::default() };
    if let Some(v) = form {
        f.form = v;
    }
    if let Some(v) = level {
        f.level = v;
    }
    BattleConfig {
        arena: arena.into(),
        players: vec![f; players.max(1)],
        phases: vec![Phase { enemy: enemy.into(), count: count.max(1) as i32 }],
        rng: RngInit::Seed(seed),
        xp: true,
    }
}
