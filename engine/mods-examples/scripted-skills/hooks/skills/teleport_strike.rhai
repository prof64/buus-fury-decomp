// Teleport Strike (skill 26): vanish (not hittable), reappear behind the nearest enemy facing it, one heavy blow.
// s.target = the nearest enemy (0 = none), s.tx / s.ty its footprint position, s.tdx / s.tdy the vector to it.

fn on_start(s) {
    hittable(false);
    flash(6);
    sfx(53);
}

fn on_frame(s) {
    if s.t == 6 {
        if s.target != 0 {
            // the side of the target away from us, 22 px (horizontal) / 14 px (vertical) past it
            let nx = s.tx;
            let ny = s.ty;
            let f = 0;
            if abs(s.tdx) >= abs(s.tdy) {
                if s.tdx > 0 { nx = s.tx + 22; f = 2; } else { nx = s.tx - 22; f = 3; }
            } else {
                if s.tdy > 0 { ny = s.ty + 14; f = 1; } else { ny = s.ty - 14; f = 0; }
            }
            teleport(nx, ny);
            face(f);
        }
        hittable(true);
        sfx(53);
    }
    if s.t == 9 {
        anim("attack1");
        strike(26, 26, 2048);     // a 26 x 26 box in front of the body, x2.0 damage
        sfx(50);
    }
    s.t >= 26
}

fn on_end(s) {
    hittable(true);
}
