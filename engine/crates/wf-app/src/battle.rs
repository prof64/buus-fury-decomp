//! The real simulation: `wf-core`'s battle `Session` (it implements `wf_view::Sim`) fed from the same layered
//! content packs as the renderer, so mods apply to both.

use std::sync::Arc;

use wf_content::Packs;
pub use wf_core::battle::{BattleConfig, FighterSetup, Phase, RngInit};
use wf_core::content::{ContentSource, GameData};
use wf_core::session::Session;

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

/// Game data for the core, loaded once per content set.
pub fn game_data(packs: &Packs) -> Result<Arc<GameData>, String> {
    GameData::load(&PacksSource(packs)).map(Arc::new).map_err(|e| e.to_string())
}

/// A battle session (`arena` names / enemy ids as in `wf_core::battle`, e.g. arena `forest`, enemy `t68`).
pub fn session(packs: &Packs, cfg: BattleConfig) -> Result<Session, String> {
    Ok(Session::new(game_data(packs)?, cfg))
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
