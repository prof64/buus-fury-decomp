// Homing Volley (skill 25): hold B to gather orbs, release to fire them in a fan; the orbs home in on enemies
// (projectiles/homing_orb.json: speed, acceleration, turn rate, life, damage).
//
// Hooks get `s`: s.t (frames since the start), s.held, s.released (frame of the B release, -1 while held),
// s.facing, s.ep, s.vars[0..7] (kept between frames, written with set_var), s.target / s.tdist (nearest enemy), ...
// They run to completion and only *ask* for effects (fire, sfx, spend, ...): the engine executes them afterwards,
// so s.vars shows this frame's values until the hook returns.

// (functions are the script's constants: hooks see only their argument, no global variables)
fn max_orbs() { 3 }
fn gather() { 12 }          // frames per extra orb
fn extra_cost() { 6 }       // EP per extra orb

fn on_start(s) {
    set_var(0, 1);           // orbs gathered
    set_var(1, 0);           // frame the volley left (0 = not yet)
    anim_loop("special_charge");
    sfx(3);                  // the charge hum
}

fn on_frame(s) {
    if s.held == 1 && s.t < 150 {
        // one more orb every gather() frames while EP lasts
        if s.t % gather() == 0 && s.vars[0] < max_orbs() && s.ep >= extra_cost() {
            set_var(0, s.vars[0] + 1);
            spend(extra_cost());
            sfx(47);
        }
        return 0;
    }
    if s.vars[1] == 0 {
        // the release (or 150 frames held): the orbs leave in a fan around the facing direction
        sfx_stop(3);
        anim("kiblast");
        let fx = dir_x(s.facing) * 256;
        let fy = dir_y(s.facing) * 256;
        let n = s.vars[0];
        for k in 0..n {
            let spread = (2 * k - (n - 1)) * 110;       // -110 / 0 / +110 across the facing direction
            fire_dir("homing_orb", fx - dir_y(s.facing) * spread, fy + dir_x(s.facing) * spread);
        }
        sfx(4);
        set_var(1, s.t);
        return 0;
    }
    // 14 frames of recovery, then the skill ends (result 1)
    s.t >= s.vars[1] + 14
}

fn on_end(s) {
    sfx_stop(3);
}
