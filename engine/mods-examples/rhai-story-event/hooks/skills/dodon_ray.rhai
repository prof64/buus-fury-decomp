// Dodon Ray (skill 27): a short wind-up, then one fast ray in the facing direction (projectiles/dodon_ray.json:
// speed, life, pierce, damage). Holding B longer fires a second ray if the EP lasts.
//
// `s` is a snapshot (s.t = frames since the start, s.held, s.ep, s.vars ...); the calls below only *ask* for
// effects, which the engine runs after the hook returns. Constants are functions: hooks see nothing but `s`.

fn wind_up() { 6 }
fn second_cost() { 4 }

fn on_start(s) {
    set_var(0, 0);          // rays fired
    sfx(47);                // the gathering ki
}

fn on_frame(s) {
    if s.t == wind_up() {
        anim("kiblast");
        fire("dodon_ray");
        sfx(20);
        set_var(0, 1);
    }
    // still holding B ten frames later: a second ray (4 more EP)
    if s.t == wind_up() + 10 && s.held == 1 && s.ep >= second_cost() {
        spend(second_cost());
        fire("dodon_ray");
        sfx(20);
        set_var(0, 2);
    }
    s.t >= wind_up() + 22   // true ends the skill
}
