// Shadow Brute's scripted AI nodes (ai/state_2000..2003.json). Each function is a node of the behaviour tree:
//   ModCondition  `call: "hooks/ai/brute.rhai:fn"` -> fn(s) returns true / false (its child runs while true)
//   ModAction     -> fn(s) returns 0 while running, 1 when done; s.f = frames since the node started (1, 2, ...)
// `s` holds the enemy (s.x, s.y, s.hp, s.max_hp, s.facing, s.vars[0..7]), its target player (s.target, s.tx, s.ty,
// s.tdx, s.tdy, s.tdist, s.thp) and the node's `args` (s.args). Effects are commands run after the function returns.
// s.vars[0] = the phase (0 = first, 1 = second), set by the roar.

fn phase_two_due(s) {
    s.vars[0] == 0 && s.hp * 2 < s.max_hp
}

fn roar(s) {
    let len = s.args[0];
    if s.f == 1 {
        set_var(0, 1);
        hittable(false);
        anim("guard");
        flash(len);
        shake(len, 70);
        sfx(54);
        heal(s.max_hp / 8);
        return 0;
    }
    if s.f >= len {
        hittable(true);
        goto_state(8193);
        return 1;
    }
    0
}

// fire args[0] homing orbs in a fan aimed at the player (args[1] = spread across the aim, 1/256), then 24 frames
fn volley(s) {
    if s.f == 1 {
        face_target();
        anim("attack1");
        let n = s.args[0];
        let ax = s.tdx;
        let ay = s.tdy;
        if s.target == 0 { ax = dir_x(s.facing); ay = dir_y(s.facing); }
        // the aim as a 256-long vector, the fan across it
        let l = isqrt(ax * ax + ay * ay);
        if l == 0 { l = 1; }
        let ux = ax * 256 / l;
        let uy = ay * 256 / l;
        for k in 0..n {
            let off = (2 * k - (n - 1)) * s.args[1] / 2;
            fire_dir("brute_orb", ux * 2 - uy * off / 256, uy * 2 + ux * off / 256);
        }
    }
    s.f >= 24
}

fn back_to_phase(s) {
    goto_state(8192 + s.vars[0]);
    true
}
