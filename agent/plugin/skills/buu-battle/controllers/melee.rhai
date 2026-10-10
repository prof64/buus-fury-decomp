// Melee brawler: punch whatever is in range, turn toward a target in reach, sidestep hostile shots about to land,
// block when a hit is imminent and we cannot punch first, otherwise follow obs.suggest.attack (the attack macro's
// path to a punching spot of the nearest enemy).
// Works for regular groups, Dabura, Janemba / Super Janemba.
let p = obs.player;
if obs.enemies.len() == 0 { return ""; }
let e = obs.enemies[0];
if !p.can_act {
    // keep blocking through a block, otherwise nothing
    return if p.state == "block" && "danger" in obs { "R" } else { "" };
}
// 1. a shot / hand about to land: step across its path
if "threats" in obs {
    for t in obs.threats {
        if "eta" in t && t.eta < 16 {
            let vx = t.vel[0]; let vy = t.vel[1];
            if vx == 0 && vy == 0 {
                // a stationary hazard (Janemba's hand) under us: step off sideways
                return if p.pos[0] > t.pos[0] { "RIGHT" } else { "LEFT" };
            }
            if vx.abs() >= vy.abs() { return if p.pos[1] > t.pos[1] { "DOWN" } else { "UP" }; }
            return if p.pos[0] > t.pos[0] { "RIGHT" } else { "LEFT" };
        }
    }
}
// 2. punch on every other frame (a press acts on the frame A goes down)
if "melee" in e {
    if e.melee == "in_range" {
        // the forecast says a hit lands before our punch would: block instead (frontal blocks take ~1/16)
        if "danger" in obs && obs.danger.hit_in <= 3 && type_of(obs.danger.if_blocking) == "string" { return "R"; }
        mem.a = !(mem.a == true);
        return if mem.a { "A" } else { "" };
    }
    return e.melee.sub_string(5).to_upper();
}
// 3. a hit is coming and nobody is in reach: block if that saves HP
if "danger" in obs && obs.danger.hit_in <= 6 {
    let b = obs.danger.if_blocking;
    if type_of(b) == "string" || b.hp < obs.danger.hp / 2 { return "R"; }
}
// 4. approach: the attack macro's suggestion (path on the walk grid to a punching spot of the nearest enemy)
if "suggest" in obs { return obs.suggest.attack; }
let side = if p.pos[0] < e.pos[0] { -1 } else { 1 };
toward(p.pos, [e.pos[0] + side * 14, e.pos[1]])
